// M11 whole-instrument calibration battery. Drives the full `ResonatorSynth` per
// resonator family at several dynamics and reports objective metrics (loudness, peak,
// fundamental pitch) that the M11 calibration steps assert against. Heavy multi-second
// renders, so gated behind the `integration-tests` feature (run via
// `make test-integration`) and kept out of the fast `make ci` path. Included into the
// `plugin_tests` module, so the shared render helpers (`RenderedClip`, `rms`,
// `peak_abs`, `set_patch_for_test`) resolve from module scope.

use lindelion_dsp_utils::analysis::estimate_f0_autocorrelation;

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

/// Render one family/velocity clip through the full synth.
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

#[derive(Debug, Clone, Copy)]
struct FamilyMetrics {
    /// Loudness over the onset window, in dBFS (`20·log10(rms)`).
    loudness_db: f32,
    /// Peak over the whole clip (headroom / clipping margin).
    peak: f32,
    /// Estimated fundamental, if the family is harmonic enough to track.
    fundamental_hz: Option<f32>,
}

/// Measure the objective metrics over the loudness window of a rendered clip.
fn measure_family(clip: &RenderedClip, sample_rate: f32) -> FamilyMetrics {
    let window_len = ((sample_rate * CALIBRATION_WINDOW_SECONDS) as usize).min(clip.left.len());
    let window = &clip.left[..window_len];
    let loudness = rms(window).max(1.0e-9);
    FamilyMetrics {
        loudness_db: 20.0 * loudness.log10(),
        peak: clip.peak,
        fundamental_hz: estimate_f0_autocorrelation(
            window,
            sample_rate,
            midi_note_to_hz(f32::from(CALIBRATION_NOTE)) * 0.5,
            midi_note_to_hz(f32::from(CALIBRATION_NOTE)) * 2.0,
        ),
    }
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
            let clip = render_family_clip(family, velocity, sample_rate, 2.0);
            assert_all_finite(&clip.left);
            assert_all_finite(&clip.right);
            let metrics = measure_family(&clip, sample_rate);
            assert!(
                metrics.loudness_db.is_finite() && clip.rms > 0.0,
                "{} @ vel {velocity} should render audible output (loudness_db={})",
                family.name(),
                metrics.loudness_db
            );
            assert!(
                metrics.peak < 8.0,
                "{} @ vel {velocity} peak should be bounded: {}",
                family.name(),
                metrics.peak
            );
            if let Some(fundamental) = metrics.fundamental_hz {
                assert!(
                    fundamental.is_finite() && fundamental > 0.0,
                    "{} @ vel {velocity} fundamental estimate should be sane: {fundamental}",
                    family.name()
                );
            }
        }
    }
}
