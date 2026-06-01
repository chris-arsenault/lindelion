use super::*;

#[test]
fn default_patch_uses_transparent_sample_driver() {
    assert_eq!(DriverConfig::default(), DriverConfig::Sample);
    assert_eq!(ResonatorSynthPatch::default().driver, DriverConfig::Sample);
}

#[test]
fn patch_with_physical_driver_roundtrips_through_toml() {
    for driver in [
        DriverConfig::Pick(PickConfig::default()),
        DriverConfig::Reed(ReedConfig::default()),
    ] {
        let patch = ResonatorSynthPatch {
            driver,
            ..ResonatorSynthPatch::default()
        };
        let encoded = crate::patch_io::to_toml_string(&patch).expect("encode patch");
        let decoded = crate::patch_io::from_toml_str(&encoded).expect("decode patch");
        assert_eq!(decoded.driver, driver);
        assert_eq!(decoded, patch);
    }
}

#[test]
fn default_contact_and_balance_reproduce_pre_m9_behavior() {
    // M9 identity defaults: transparent contact (spread/contact-time 0) and a 0
    // balance depth, so a default patch renders exactly as before the milestone.
    let contact = ContactConfig::default();
    assert_eq!(contact.spread, 0.0);
    assert_eq!(contact.contact_time, 0.0);
    assert_eq!(ResonatorSynthPatch::default().contact, contact);
    assert_eq!(WaveguideConfig::default().source_body_balance, 0.0);
}

#[test]
fn patch_with_contact_and_balance_roundtrips_through_toml() {
    let patch = ResonatorSynthPatch {
        contact: ContactConfig {
            spread: 0.7,
            contact_time: 0.4,
        },
        resonator_b: ResonatorConfig::Waveguide(WaveguideConfig {
            source_body_balance: 0.6,
            ..WaveguideConfig::default()
        }),
        ..ResonatorSynthPatch::default()
    };
    let encoded = crate::patch_io::to_toml_string(&patch).expect("encode patch");
    let decoded = crate::patch_io::from_toml_str(&encoded).expect("decode patch");
    assert_eq!(decoded.contact, patch.contact);
    assert_eq!(decoded.resonator_b, patch.resonator_b);
    assert_eq!(decoded, patch);
}

#[test]
fn default_surrounding_is_defeated() {
    // M10 identity default: every surrounding-effect depth is 0 (defeated), so a
    // default patch is unchanged.
    let surrounding = SurroundingConfig::default();
    assert_eq!(surrounding.mechanical_noise, 0.0);
    assert_eq!(surrounding.radiation_brightness, 0.0);
    assert_eq!(surrounding.sympathetic, 0.0);
    assert_eq!(ResonatorSynthPatch::default().surrounding, surrounding);
}

#[test]
fn patch_with_surrounding_roundtrips_through_toml() {
    let patch = ResonatorSynthPatch {
        surrounding: SurroundingConfig {
            mechanical_noise: 0.6,
            radiation_brightness: 0.4,
            sympathetic: 0.3,
        },
        ..ResonatorSynthPatch::default()
    };
    let encoded = crate::patch_io::to_toml_string(&patch).expect("encode patch");
    let decoded = crate::patch_io::from_toml_str(&encoded).expect("decode patch");
    assert_eq!(decoded.surrounding, patch.surrounding);
    assert_eq!(decoded, patch);
}
