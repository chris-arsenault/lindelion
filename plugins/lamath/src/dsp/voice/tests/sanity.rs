use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms, rms_difference};

use super::{bright_modal_patch, impulse, render_left, test_patch};
use crate::{
    FilterMode, ModalConfig, ModalPreset, ResonatorConfig, ResonatorRouting, WaveguideConfig,
};

#[test]
fn resonator_routing_parameter_combinations_emit_sane_output() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let resonators = resonator_sanity_cases();
    let routing_cases = [
        (
            "parallel_equal",
            ResonatorRouting::Parallel {
                mix_a: 0.5,
                mix_b: 0.5,
            },
        ),
        (
            "parallel_b_heavy",
            ResonatorRouting::Parallel {
                mix_a: 0.15,
                mix_b: 0.85,
            },
        ),
        ("body_color", body_color_routing()),
    ];
    let output_cases = [
        OutputSanityCase {
            label: "open_dry",
            filter_mode: FilterMode::LowPass,
            filter_cutoff: 20_000.0,
            filter_resonance: 0.0,
            saturation_drive: 0.0,
        },
        OutputSanityCase {
            label: "focused_band",
            filter_mode: FilterMode::BandPass,
            filter_cutoff: 2_500.0,
            filter_resonance: 0.4,
            saturation_drive: 0.0,
        },
        OutputSanityCase {
            label: "driven_open",
            filter_mode: FilterMode::LowPass,
            filter_cutoff: 18_000.0,
            filter_resonance: 0.0,
            saturation_drive: 0.6,
        },
    ];

    for (routing_label, routing) in routing_cases {
        for resonator_a in resonators {
            for resonator_b in resonators {
                for output in output_cases {
                    let mut patch = test_patch(routing);
                    patch.resonator_a = resonator_a.config;
                    patch.resonator_b = resonator_b.config;
                    patch.output.filter_mode = output.filter_mode;
                    patch.output.filter_cutoff = output.filter_cutoff;
                    patch.output.filter_resonance = output.filter_resonance;
                    patch.output.saturation_drive = output.saturation_drive;
                    patch.output.master_gain_db = 0.0;

                    let rendered = render_left(sample_rate, &patch, &excitation);
                    let context = format!(
                        "{routing_label} / A={} / B={} / {}",
                        resonator_a.label, resonator_b.label, output.label
                    );

                    assert_sane_render(&rendered, &context);
                }
            }
        }
    }
}

#[test]
fn body_color_modal_pair_has_nominal_headroom_at_unity_master_gain() {
    let sample_rate = 48_000.0;
    let mut patch = bright_modal_patch();
    patch.resonator_b = ResonatorConfig::Modal(ModalConfig {
        mode_count: 64,
        preset: ModalPreset::GenericStrike,
        decay_global: 1.0,
        decay_tilt: 0.0,
        brightness: 1.0,
        ..ModalConfig::default()
    });
    patch.routing = body_color_routing();
    patch.output.master_gain_db = 0.0;
    let excitation = impulse(64);
    let rendered = render_left(sample_rate, &patch, &excitation);
    let peak = peak_abs(&rendered);
    let energy = rms(&rendered);

    assert_all_finite(&rendered);
    assert!(peak < 2.0, "body-color modal pair peak={peak}");
    assert!(energy < 0.2, "body-color modal pair rms={energy}");
    assert!(energy > 0.000_001, "body-color modal pair rms={energy}");
}

#[test]
fn modal_modal_series_renders_through_body_color_guard() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let mut series = bright_modal_patch();
    series.resonator_b = ResonatorConfig::Modal(ModalConfig {
        mode_count: 64,
        preset: ModalPreset::GenericStrike,
        decay_global: 1.0,
        decay_tilt: 0.0,
        brightness: 1.0,
        ..ModalConfig::default()
    });
    series.routing = ResonatorRouting::Series {
        mix_a: 1.0,
        mix_b: 1.0,
    };
    let mut body_color = series.clone();
    body_color.routing = body_color_routing();

    let series_render = render_left(sample_rate, &series, &excitation);
    let body_color_render = render_left(sample_rate, &body_color, &excitation);
    let difference = rms_difference(&series_render, &body_color_render);

    assert_sane_render(&series_render, "modal-modal series guard");
    assert!(
        difference < 0.000_000_01,
        "modal-modal series should render as body color; diff={difference}",
    );
}

#[test]
fn body_color_resonator_a_materially_changes_resonator_b_excitation() {
    let sample_rate = 48_000.0;
    let excitation = impulse(64);
    let mut marimba = test_patch(body_color_routing());
    let mut bell = marimba.clone();
    marimba.resonator_a = ResonatorConfig::Modal(ModalConfig {
        mode_count: 32,
        preset: ModalPreset::Marimba,
        decay_global: 0.7,
        brightness: 0.35,
        ..ModalConfig::default()
    });
    bell.resonator_a = ResonatorConfig::Modal(ModalConfig {
        mode_count: 96,
        preset: ModalPreset::Bell,
        decay_global: 2.0,
        brightness: 1.0,
        ..ModalConfig::default()
    });

    let marimba_render = render_left(sample_rate, &marimba, &excitation);
    let bell_render = render_left(sample_rate, &bell, &excitation);
    let difference = rms_difference(&marimba_render, &bell_render);

    assert!(rms(&marimba_render) > 0.000_001);
    assert!(rms(&bell_render) > 0.000_001);
    assert!(
        difference > 0.000_01,
        "body-color A choice did not materially alter B excitation; diff={difference}"
    );
}

fn assert_sane_render(samples: &[f32], context: &str) {
    assert_all_finite(samples);
    let peak = peak_abs(samples);
    let energy = rms(samples);
    assert!(peak < 8.0, "{context}: peak={peak}");
    assert!(energy < 0.8, "{context}: rms={energy}");
    assert!(energy > 0.000_000_01, "{context}: rms={energy}");
}

fn body_color_routing() -> ResonatorRouting {
    ResonatorRouting::BodyColor {
        mix_a: 1.0,
        mix_b: 1.0,
    }
}

#[derive(Clone, Copy)]
struct ResonatorSanityCase {
    label: &'static str,
    config: ResonatorConfig,
}

#[derive(Clone, Copy)]
struct OutputSanityCase {
    label: &'static str,
    filter_mode: FilterMode,
    filter_cutoff: f32,
    filter_resonance: f32,
    saturation_drive: f32,
}

fn resonator_sanity_cases() -> [ResonatorSanityCase; 4] {
    [
        ResonatorSanityCase {
            label: "modal_marimba",
            config: ResonatorConfig::Modal(ModalConfig {
                mode_count: 32,
                preset: ModalPreset::Marimba,
                decay_global: 0.7,
                decay_tilt: 0.75,
                brightness: 0.35,
                ..ModalConfig::default()
            }),
        },
        ResonatorSanityCase {
            label: "modal_bright_bell",
            config: ResonatorConfig::Modal(ModalConfig {
                mode_count: 96,
                preset: ModalPreset::Bell,
                inharmonicity: 0.6,
                decay_global: 2.0,
                decay_tilt: 0.0,
                brightness: 1.0,
                ..ModalConfig::default()
            }),
        },
        ResonatorSanityCase {
            label: "waveguide_damped",
            config: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.72,
                loop_filter_cutoff: 4_000.0,
                loop_filter_resonance: 0.2,
                loop_nonlinearity: 0.0,
                boundary_reflection: 0.45,
                ..WaveguideConfig::default()
            }),
        },
        ResonatorSanityCase {
            label: "waveguide_hot",
            config: ResonatorConfig::Waveguide(WaveguideConfig {
                loop_gain: 0.995,
                loop_filter_cutoff: 18_000.0,
                loop_filter_resonance: 0.95,
                loop_nonlinearity: 0.6,
                boundary_reflection: 0.95,
                ..WaveguideConfig::default()
            }),
        },
    ]
}
