use super::{
    modal::ModalBankParams,
    render_metrics::{
        RenderExcitation, compare_render_metrics, render_metric_profile, render_modal_response,
        render_waveguide_response,
    },
    waveguide::{WaveguideParams, WaveguideStyle},
};
use crate::ModalPreset;
use lindelion_dsp_utils::analysis::assert_all_finite;

#[test]
fn ab_render_metrics_compare_waveguide_styles_against_modal_presets() {
    let sample_rate = 48_000.0;
    let sample_count = 24_000;

    for case in ab_render_cases() {
        let waveguide =
            render_waveguide_response(sample_rate, case.waveguide, sample_count, case.excitation);
        let modal = render_modal_response(sample_rate, case.modal, sample_count, case.excitation);
        let waveguide_profile = render_metric_profile(&waveguide, sample_rate, case.frequency_hz);
        let modal_profile = render_metric_profile(&modal, sample_rate, case.frequency_hz);
        let comparison = compare_render_metrics(&modal, &waveguide, sample_rate, case.frequency_hz);

        assert_all_finite(&waveguide);
        assert_all_finite(&modal);
        assert!(
            waveguide_profile.early.rms > 1.0e-8,
            "{} waveguide_profile={waveguide_profile:?}",
            case.name
        );
        assert!(
            modal_profile.early.rms > 1.0e-8,
            "{} modal_profile={modal_profile:?}",
            case.name
        );
        assert!(
            comparison.normalized_shape_difference > 0.03,
            "{} comparison={comparison:?}",
            case.name
        );
        assert!(
            comparison.early_centroid_delta_hz.is_finite()
                && comparison.high_frequency_ratio_delta.is_finite(),
            "{} comparison={comparison:?}",
            case.name
        );
        assert!(
            !waveguide_profile.harmonic_decay.is_empty()
                && !modal_profile.harmonic_decay.is_empty(),
            "{} waveguide_profile={waveguide_profile:?}, modal_profile={modal_profile:?}",
            case.name
        );
    }
}

fn ab_render_cases() -> [AbRenderCase; 3] {
    [
        AbRenderCase {
            name: "string_marimba_pluck",
            frequency_hz: 220.0,
            excitation: RenderExcitation::ShapedPluck,
            waveguide: WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: 220.0,
                loop_filter_cutoff: 12_000.0,
                loop_filter_resonance: 0.1,
                loop_gain: 0.965,
                loop_nonlinearity: 0.0,
                dispersion: 0.12,
                position_of_strike: 0.35,
                pickup_position: 0.62,
                boundary_reflection: 0.65,
            },
            modal: ModalBankParams {
                fundamental_hz: 220.0,
                mode_count: 64,
                preset: ModalPreset::Marimba,
                inharmonicity: 0.05,
                brightness: 0.62,
                decay_global: 1.2,
                decay_tilt: 0.35,
                position_of_strike: 0.35,
            },
        },
        AbRenderCase {
            name: "tube_woodblock_burst",
            frequency_hz: 196.0,
            excitation: RenderExcitation::SidechainBurst,
            waveguide: WaveguideParams {
                style: WaveguideStyle::Tube,
                frequency_hz: 196.0,
                loop_filter_cutoff: 7_500.0,
                loop_filter_resonance: 0.15,
                loop_gain: 0.95,
                loop_nonlinearity: 0.05,
                dispersion: 0.0,
                position_of_strike: 0.18,
                pickup_position: 0.72,
                boundary_reflection: 0.82,
            },
            modal: ModalBankParams {
                fundamental_hz: 196.0,
                mode_count: 48,
                preset: ModalPreset::Woodblock,
                inharmonicity: 0.18,
                brightness: 0.7,
                decay_global: 1.1,
                decay_tilt: 0.55,
                position_of_strike: 0.18,
            },
        },
        AbRenderCase {
            name: "dispersed_string_metal_bar",
            frequency_hz: 147.0,
            excitation: RenderExcitation::NoiseBurst,
            waveguide: WaveguideParams {
                style: WaveguideStyle::String,
                frequency_hz: 147.0,
                loop_filter_cutoff: 16_000.0,
                loop_filter_resonance: 0.05,
                loop_gain: 0.975,
                loop_nonlinearity: 0.02,
                dispersion: 0.78,
                position_of_strike: 0.28,
                pickup_position: 0.54,
                boundary_reflection: 0.65,
            },
            modal: ModalBankParams {
                fundamental_hz: 147.0,
                mode_count: 80,
                preset: ModalPreset::MetalBar,
                inharmonicity: 0.32,
                brightness: 0.68,
                decay_global: 1.0,
                decay_tilt: 0.25,
                position_of_strike: 0.28,
            },
        },
    ]
}

#[derive(Debug, Clone, Copy)]
struct AbRenderCase {
    name: &'static str,
    frequency_hz: f32,
    excitation: RenderExcitation,
    waveguide: WaveguideParams,
    modal: ModalBankParams,
}
