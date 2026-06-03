use super::*;

#[test]
fn default_patch_is_dual_modal() {
    let patch = ResonatorSynthPatch::default();

    assert_eq!(patch.resonator_a.preset, ModalPreset::Marimba);
    assert_eq!(patch.resonator_b.preset, ModalPreset::Bell);
    assert!(matches!(
        patch.routing,
        ResonatorRouting::Parallel {
            mix_a: 1.0,
            mix_b: 0.0
        }
    ));
}

#[test]
fn routing_normalization_preserves_series_and_body_color_modes() {
    assert!(matches!(
        normalize_routing(ResonatorRouting::Series {
            mix_a: 1.2,
            mix_b: -1.0,
        }),
        ResonatorRouting::Series {
            mix_a: 1.0,
            mix_b: 0.0
        }
    ));
    assert!(matches!(
        normalize_routing(ResonatorRouting::BodyColor {
            mix_a: f32::NAN,
            mix_b: 0.25,
        }),
        ResonatorRouting::BodyColor {
            mix_a: 1.0,
            mix_b: 0.25
        }
    ));
}

#[test]
fn surrounding_defaults_are_defeated() {
    assert_eq!(
        ResonatorSynthPatch::default().surrounding,
        SurroundingConfig::default()
    );
    assert_eq!(SurroundingConfig::default().mechanical_noise, 0.0);
    assert_eq!(SurroundingConfig::default().radiation_brightness, 0.0);
    assert_eq!(SurroundingConfig::default().sympathetic, 0.0);
}
