//! M11 energy-bus calibration and gain-staging measurement tests for the voice.
//! Split out of `voice/tests.rs` to keep that file within the 600-line size cap.

use super::super::{Voice, VoiceTrigger};
use super::{impulse, test_patch};
use crate::{
    MeshConfig, ModalConfig, OutputConfig, ResonatorConfig, ResonatorRouting, ResonatorSynthPatch,
    WaveguideConfig, WaveguideStyle,
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
