//! M11 energy-bus calibration and gain-staging measurement tests for the voice.
//! Split out of `voice/tests.rs` to keep that file within the 600-line size cap.

use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};

use super::super::{Voice, VoiceTrigger};
use super::{impulse, test_patch};
use crate::{
    BowConfig, DriverConfig, MeshConfig, ModalConfig, OutputConfig, ResonatorConfig,
    ResonatorRouting, ResonatorSynthPatch, WaveguideConfig, WaveguideStyle,
};

/// M11 P1: the per-stage gain-staging taps are wired and report finite,
/// audible levels through a held-note render. The objective whole-path level
/// targets are P9; this only guards that the measurement infrastructure exists
/// and is sane. Heavy enough to gate behind `integration-tests`.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn stage_peaks_report_finite_audible_levels() {
    let sample_rate = 48_000.0;
    let patch = test_patch(ResonatorRouting::Parallel {
        mix_a: 1.0,
        mix_b: 0.0,
    });
    let excitation = impulse(256);
    let mut voice = Voice::new(sample_rate);
    let mut left = vec![0.0; 24_000];
    let mut right = vec![0.0; 24_000];

    voice.trigger(VoiceTrigger::new(60, 1.0, &excitation, sample_rate, &patch));
    voice.render_add(&mut left, &mut right);

    let peaks = voice.stage_peaks();
    assert!(
        peaks.iter().all(|peak| peak.is_finite()),
        "stage peaks must be finite: {peaks:?}"
    );
    assert!(
        peaks[0] > 0.0 && peaks[1] > 0.0,
        "excitation and resonator stages must be audible: {peaks:?}"
    );
}

fn single_resonator_patch(resonator_a: ResonatorConfig) -> ResonatorSynthPatch {
    ResonatorSynthPatch {
        resonator_a,
        resonator_b: ResonatorConfig::Modal(ModalConfig {
            mode_count: 1,
            ..ModalConfig::default()
        }),
        routing: ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0,
        },
        output: OutputConfig {
            filter_cutoff: 20_000.0,
            filter_resonance: 0.0,
            saturation_drive: 0.0,
            master_gain_db: 0.0,
            master_pan: 0.0,
            ..OutputConfig::default()
        },
        ..ResonatorSynthPatch::default()
    }
}

/// M11 P8 energy-reference calibration. The per-family energy references
/// (`STRING_TENSION_ENERGY_REF`, `STEEPEN_ENERGY_REF`, `GEOMETRIC_ENERGY_REF`, the
/// balance/radiation refs) decide where real playing lands on each dynamic effect's
/// drive curve. They were originally uncalibrated first-principles guesses ~15× too
/// high, so a full-velocity note drove the effects to <1% of their range (inaudible).
/// This pins the actual measured per-voice energy bus to the references' neighborhood:
/// a full-velocity note must peak near the reference (so squared effects reach ≈0.7
/// drive, linear effects ≈0.8), and a soft note must be clearly gentler (the dynamic
/// range the nonlinearities act over). If the resonator output level drifts away from
/// these references, this fails — flagging that the references need re-tuning.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn dynamic_effect_energy_references_track_real_playing() {
    let sample_rate = 48_000.0;
    let excitation = impulse(256);
    // (label, resonator, reference the family's dynamic effects normalise against —
    // mirrors the per-module ENERGY_REF consts; keep in sync if those change.)
    let families: [(&str, ResonatorConfig, f32); 3] = [
        (
            "String",
            ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::String,
                ..WaveguideConfig::default()
            }),
            0.012,
        ),
        (
            "Tube",
            ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::Tube,
                ..WaveguideConfig::default()
            }),
            0.005,
        ),
        ("Mesh", ResonatorConfig::Mesh(MeshConfig::default()), 0.013),
    ];
    // Each family's primary dynamic effect (String tension, Tube steepening, Mesh
    // geometric coupling) uses the squared drive `(energy/ref)^2`, clamped to [0, 1].
    let drive = |energy: f32, energy_ref: f32| ((energy / energy_ref).powi(2)).clamp(0.0, 1.0);
    for (label, resonator, energy_ref) in families {
        let patch = single_resonator_patch(resonator);
        let peak_energy = |velocity: f32| -> f32 {
            let mut voice = Voice::new(sample_rate);
            voice.trigger(VoiceTrigger::new(
                57,
                velocity,
                &excitation,
                sample_rate,
                &patch,
            ));
            let mut max_energy = 0.0_f32;
            for _ in 0..24_000 {
                voice.process_sample();
                max_energy = max_energy.max(voice.measured_energy());
            }
            max_energy
        };
        let loud_drive = drive(peak_energy(1.0), energy_ref);
        let soft_drive = drive(peak_energy(0.3), energy_ref);
        // Full-velocity playing reaches the effect's active range — at least halfway up
        // the drive curve (an energy-adding effect like the Tube's bloom saturates here,
        // which is the intended maximum, not a failure).
        assert!(
            loud_drive > 0.5,
            "{label}: full-velocity drive {loud_drive} should reach the effect's active range (ref {energy_ref})"
        );
        // Soft playing stays low on the curve — the squared nonlinearity concentrates
        // on hard hits, so there is a wide dynamic range for the effect to act over.
        assert!(
            soft_drive < 0.25 && soft_drive < loud_drive * 0.5,
            "{label}: soft drive {soft_drive} should be clearly gentler than loud {loud_drive}"
        );
    }
}

/// M11 P9 step 1: the bow driver self-oscillates a *sustained but sane* tone. P8
/// found a held bow blowing up to energy-bus RMS ~8.7 (vs ~0.01 for a pluck) — the
/// `BOW_INJECTION_GAIN`/`BOW_OUTPUT_LIMIT` were far too hot. After taming, a held
/// bowed String must still build and hold a steady limit cycle (it does not decay
/// like a freed pluck) while staying bounded near the other sustained-driver scale.
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn bowed_string_sustains_while_held_but_bounded() {
    let sample_rate = 48_000.0;
    let mut patch = single_resonator_patch(ResonatorConfig::Waveguide(WaveguideConfig {
        style: WaveguideStyle::String,
        ..WaveguideConfig::default()
    }));
    patch.driver = DriverConfig::Bow(BowConfig::default());

    let excitation = impulse(256);
    let mut voice = Voice::new(sample_rate);
    voice.trigger(VoiceTrigger::new(57, 1.0, &excitation, sample_rate, &patch));
    let mut out = vec![0.0; 96_000];
    let mut max_energy = 0.0_f32;
    for sample in &mut out {
        *sample = voice.process_sample();
        max_energy = max_energy.max(voice.measured_energy());
    }
    assert_all_finite(&out);

    // Bounded: the limit cycle never runs away — a locked bow self-oscillates to a
    // steady ~0.3 energy (a forte sustained tone, ~26x lower than the P8 ~8.7 blow-up;
    // a self-oscillator's locked cycle cannot go arbitrarily low without un-locking),
    // and the output stays in range.
    assert!(
        max_energy < 0.5,
        "held bow should stay a bounded limit cycle, not the P8 runaway: max_energy={max_energy}"
    );
    assert!(
        peak_abs(&out) < 4.0,
        "bow peak ran away: {}",
        peak_abs(&out)
    );

    // Sustains: a steady limit cycle, not a decaying ring — the late window holds
    // most of the mid-render level (a freed pluck would have decayed substantially).
    let mid_rms = rms(&out[24_000..36_000]);
    let late_rms = rms(&out[84_000..96_000]);
    assert!(
        late_rms > 0.005,
        "bow died out instead of sustaining: late_rms={late_rms}"
    );
    assert!(
        late_rms > mid_rms * 0.7,
        "bow tail should hold a steady limit cycle: mid={mid_rms} late={late_rms}"
    );
}

/// M11 P9 step 2: per-family output makeup levels and balances the families. The
/// resonator families emerge ~38 dB apart (Modal ≈ −1.7 dBFS peak, String/Tube/Mesh
/// ≈ −36/−28/−40 dBFS) and the waveguides run ~30 dB too quiet. The per-family makeup
/// (output-side of the energy tap) must bring every family's single full-velocity
/// voice to a healthy ≈ −6 dBFS peak, within ±3 dB of each other. Peak-matched (the
/// families' crest factors differ too much to RMS-match without clipping a pluck).
#[cfg_attr(
    not(feature = "integration-tests"),
    ignore = "see make test-integration"
)]
#[test]
fn per_family_output_makeup_levels_and_balances_families() {
    let sample_rate = 48_000.0;
    let excitation = impulse(256);
    let families: [(&str, ResonatorConfig); 4] = [
        ("Modal", ResonatorConfig::Modal(ModalConfig::default())),
        (
            "String",
            ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::String,
                ..WaveguideConfig::default()
            }),
        ),
        (
            "Tube",
            ResonatorConfig::Waveguide(WaveguideConfig {
                style: WaveguideStyle::Tube,
                ..WaveguideConfig::default()
            }),
        ),
        ("Mesh", ResonatorConfig::Mesh(MeshConfig::default())),
    ];
    // Healthy, balanced band: −9…−3 dBFS peak (≈ −6 dBFS ± 3 dB) — guarantees every
    // family is at a usable level and within ±3 dB of the others.
    for (label, resonator) in families {
        let patch = single_resonator_patch(resonator);
        let mut voice = Voice::new(sample_rate);
        voice.trigger(VoiceTrigger::new(57, 1.0, &excitation, sample_rate, &patch));
        let mut out = vec![0.0; 48_000];
        for sample in &mut out {
            *sample = voice.process_sample();
        }
        assert_all_finite(&out);
        let final_peak = voice.stage_peaks()[3];
        assert!(
            (0.35..=0.71).contains(&final_peak),
            "{label}: single-voice final peak {final_peak} should sit in the healthy −9…−3 dBFS band"
        );
    }
}
