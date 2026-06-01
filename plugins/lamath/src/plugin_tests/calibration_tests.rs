// M11 whole-instrument calibration battery. Drives the full `ResonatorSynth` per
// resonator family at several dynamics and reports objective metrics (loudness, peak,
// fundamental pitch) that the M11 calibration steps assert against. Heavy multi-second
// renders, so gated behind the `integration-tests` feature (run via
// `make test-integration`) and kept out of the fast `make ci` path. Included into the
// `plugin_tests` module, so the shared render helpers (`RenderedClip`, `rms`,
// `peak_abs`, `set_patch_for_test`) resolve from module scope.

use lindelion_dsp_utils::analysis::{
    attack_sustain_ratio, estimate_f0_autocorrelation, inharmonicity_ratios, partial_t60_seconds,
    spectral_centroid_trajectory,
};

/// Pitch the families are auditioned at across the battery (middle C).
const CALIBRATION_NOTE: u8 = 60;
/// The dynamics the loudness curve is measured at — soft, medium, hard (M11 step 1:
/// "cover peak, vel 100, vel 20, etc.").
const CALIBRATION_VELOCITIES: [u8; 3] = [20, 100, 127];
/// Loudness-measurement window length (seconds from the note onset): attack + early
/// sustain, where the families' levels are compared.
const CALIBRATION_WINDOW_SECONDS: f32 = 0.5;

#[derive(Debug, Clone, Copy)]
enum ResonatorFamily {
    Modal,
    String,
    Tube,
    Mesh,
}

impl ResonatorFamily {
    const ALL: [Self; 4] = [Self::Modal, Self::String, Self::Tube, Self::Mesh];

    fn name(self) -> &'static str {
        match self {
            Self::Modal => "Modal",
            Self::String => "String",
            Self::Tube => "Tube",
            Self::Mesh => "Mesh",
        }
    }

    fn resonator_config(self) -> ResonatorConfig {
        match self {
            Self::Modal => ResonatorConfig::Modal(ModalConfig::default()),
            Self::String => ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::String,
                ..WaveguideConfig::default()
            }),
            Self::Tube => ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::Tube,
                ..WaveguideConfig::default()
            }),
            Self::Mesh => ResonatorConfig::Mesh(MeshConfig::default()),
        }
    }
}

/// A neutral single-family patch: only resonator A sounds (parallel mix B = 0), the
/// output stage is transparent (0 dB master, no saturation, filter wide open), and the
/// dynamic-response surrounding effects are defeated — so the measured level is the
/// resonator family's own output.
fn family_patch(family: ResonatorFamily) -> ResonatorSynthPatch {
    let mut patch = ResonatorSynthPatch {
        resonator_a: family.resonator_config(),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        ..ResonatorSynthPatch::default()
    };
    patch.output.master_gain_db = 0.0;
    patch.output.saturation_drive = 0.0;
    patch.output.filter_cutoff = 20_000.0;
    patch
}

/// Render one held-note family/velocity clip through the full synth: a single
/// note-on at the start and **no note-off**, so the note is held for the whole
/// clip duration. This is the held-note render the voicing metrics measure
/// against (sustain, decay, tail darkening).
fn render_family_clip(
    family: ResonatorFamily,
    velocity: u8,
    sample_rate: f32,
    seconds: f32,
) -> RenderedClip {
    let block_size = 512;
    let total_blocks = ((sample_rate * seconds).ceil() as usize).div_ceil(block_size);
    let setup = ProcessSetup {
        sample_rate: f64::from(sample_rate),
        max_block_size: block_size,
        mode: ProcessMode::Realtime,
    };
    let mut synth = ResonatorSynth::default();
    let mut block_left = vec![0.0; block_size];
    let mut block_right = vec![0.0; block_size];
    let mut left = Vec::with_capacity(total_blocks * block_size);
    let mut right = Vec::with_capacity(total_blocks * block_size);

    synth.reset(setup);
    synth.set_patch_for_test(family_patch(family));

    for block in 0..total_blocks {
        let note_on = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: CALIBRATION_NOTE,
            velocity: f32::from(velocity) / 127.0,
        })];
        let events = if block == 0 { &note_on[..] } else { &[] };
        process_block(&mut synth, setup, &mut block_left, &mut block_right, events);
        left.extend_from_slice(&block_left);
        right.extend_from_slice(&block_right);
    }

    let rms = rms(&left).max(rms(&right));
    let peak = peak_abs(&left).max(peak_abs(&right));
    RenderedClip {
        left,
        right,
        rms,
        peak,
    }
}

/// Number of partials the inharmonicity ratio is reported for; unmeasured
/// partials are `f32::NAN`.
const INHARMONICITY_PARTIALS: usize = 4;

#[derive(Debug, Clone, Copy)]
struct FamilyMetrics {
    /// Loudness over the onset window, in dBFS (`20·log10(rms)`).
    loudness_db: f32,
    /// Peak over the whole clip (headroom / clipping margin).
    peak: f32,
    /// Estimated fundamental, if the family is harmonic enough to track.
    fundamental_hz: Option<f32>,
    /// T60 (s) of the fundamental partial, when the tail measurably decays.
    t60_seconds: Option<f32>,
    /// Measured / ideal frequency ratio per partial (`NAN` when unmeasurable).
    inharmonicity_ratios: [f32; INHARMONICITY_PARTIALS],
    /// Spectral centroid (Hz) at the first and last analysed window of the tail.
    centroid_endpoints: Option<(f32, f32)>,
    /// Onset-to-sustain energy ratio (`rms(attack)/rms(sustain)`).
    attack_sustain_ratio: Option<f32>,
}

/// Measure the objective voicing metrics of a held-note rendered clip: loudness
/// and peak over the onset window, plus the M11 tail metrics (fundamental, T60,
/// inharmonicity, centroid-over-time, attack/sustain) over the full clip.
fn measure_family(clip: &RenderedClip, sample_rate: f32) -> FamilyMetrics {
    let window_len = ((sample_rate * CALIBRATION_WINDOW_SECONDS) as usize).min(clip.left.len());
    let window = &clip.left[..window_len];
    let loudness = rms(window).max(1.0e-9);
    let nominal_hz = midi_note_to_hz(f32::from(CALIBRATION_NOTE));
    let fundamental_hz =
        estimate_f0_autocorrelation(window, sample_rate, nominal_hz * 0.5, nominal_hz * 2.0);
    let analysis_hz = fundamental_hz.unwrap_or(nominal_hz);

    // Non-overlapping windows keep the (O(n) per-window) T60 fit cheap over the
    // multi-second clip.
    let t60_seconds = partial_t60_seconds(&clip.left, sample_rate, analysis_hz, 4_096, 4_096);

    let inharmonicity_window = &clip.left[..clip.left.len().min(16_384)];
    let measured_ratios = inharmonicity_ratios(
        inharmonicity_window,
        sample_rate,
        analysis_hz,
        INHARMONICITY_PARTIALS,
        0.05,
    );
    let mut inharmonicity_ratios = [f32::NAN; INHARMONICITY_PARTIALS];
    for (slot, ratio) in inharmonicity_ratios.iter_mut().zip(measured_ratios) {
        *slot = ratio;
    }

    // `spectral_centroid_hz` is O(window²); a small window and a coarse hop keep
    // the trajectory cheap while still spanning onset→tail for the endpoints.
    let trajectory = spectral_centroid_trajectory(&clip.left, sample_rate, 2_048, 16_384);
    let centroid_endpoints = (trajectory.len() >= 2)
        .then(|| (*trajectory.first().unwrap(), *trajectory.last().unwrap()));

    let attack_sustain_ratio = attack_sustain_ratio(&clip.left, sample_rate, 0.03, 1.0, 0.2);

    FamilyMetrics {
        loudness_db: 20.0 * loudness.log10(),
        peak: clip.peak,
        fundamental_hz,
        t60_seconds,
        inharmonicity_ratios,
        centroid_endpoints,
        attack_sustain_ratio,
    }
}

/// Assert every metric of one rendered clip is finite, bounded, and (where the
/// metric is optional) sane when present. Targets per dimension belong to later
/// M11 phases; P1 only guards that the fixtures are computable and well-formed.
fn assert_family_metrics_sane(
    clip: &RenderedClip,
    metrics: &FamilyMetrics,
    label: &str,
    velocity: u8,
) {
    assert!(
        metrics.loudness_db.is_finite() && clip.rms > 0.0,
        "{label} @ vel {velocity} should render audible output (loudness_db={})",
        metrics.loudness_db
    );
    assert!(
        metrics.peak < 8.0,
        "{label} @ vel {velocity} peak should be bounded: {}",
        metrics.peak
    );
    assert!(
        metrics
            .fundamental_hz
            .is_none_or(|f| f.is_finite() && f > 0.0),
        "{label} @ vel {velocity} fundamental estimate should be sane: {:?}",
        metrics.fundamental_hz
    );
    // T60 is either unmeasurable (None — e.g. a tail too short to fit) or a finite
    // positive duration. Targets are P2; this only asserts sanity.
    assert!(
        metrics.t60_seconds.is_none_or(|t| t.is_finite() && t > 0.0),
        "{label} @ vel {velocity} T60 should be a sane duration: {:?}",
        metrics.t60_seconds
    );
    // Every measurable partial ratio is finite and within a sane band (target
    // stretch curves are P4).
    assert!(
        metrics
            .inharmonicity_ratios
            .iter()
            .filter(|ratio| ratio.is_finite())
            .all(|ratio| (0.5..=2.0).contains(ratio)),
        "{label} @ vel {velocity} partial ratios out of band: {:?}",
        metrics.inharmonicity_ratios
    );
    assert!(
        metrics
            .centroid_endpoints
            .is_none_or(|(first, last)| first.is_finite()
                && last.is_finite()
                && first > 0.0
                && last > 0.0),
        "{label} @ vel {velocity} centroid endpoints should be sane: {:?}",
        metrics.centroid_endpoints
    );
    assert!(
        metrics
            .attack_sustain_ratio
            .is_none_or(|r| r.is_finite() && r > 0.0),
        "{label} @ vel {velocity} attack/sustain ratio should be sane: {:?}",
        metrics.attack_sustain_ratio
    );
}

#[cfg_attr(not(feature = "integration-tests"), ignore = "see make test-integration")]
#[test]
fn calibration_battery_reports_sane_metrics_for_every_family_and_dynamic() {
    // The measurement battery underpinning M11: every resonator family renders audible,
    // finite, bounded output at every measured dynamic, and its metrics are computable.
    // This is the objective-fixture foundation; the loudness-MATCH target is M11 step 3.
    let sample_rate = 48_000.0;
    for family in ResonatorFamily::ALL {
        for velocity in CALIBRATION_VELOCITIES {
            // 3 s held note so the tail metrics (T60, centroid-over-time) have a
            // decay to measure.
            let clip = render_family_clip(family, velocity, sample_rate, 3.0);
            assert_all_finite(&clip.left);
            assert_all_finite(&clip.right);
            let metrics = measure_family(&clip, sample_rate);
            assert_family_metrics_sane(&clip, &metrics, family.name(), velocity);
        }
    }
}
