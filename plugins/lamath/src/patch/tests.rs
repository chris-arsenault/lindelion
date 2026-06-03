use super::*;

#[test]
fn default_patch_uses_transparent_sample_driver() {
    assert_eq!(DriverConfig::default(), DriverConfig::Sample);
    assert_eq!(ResonatorSynthPatch::default().driver, DriverConfig::Sample);
}

#[test]
fn tube_resonator_forces_reed_driver_per_slot() {
    // ADR-0032: a Tube is a wind resonator — a struck/impulse driver (Sample/Pick) or the Bow
    // on a Tube renders silence, so any non-reed driver on a Tube normalizes to the reed wind
    // driver. The driver is per-resonator, so a co-resident non-Tube waveguide keeps its own.
    let tube = ResonatorConfig::Waveguide(WaveguideConfig {
        style: WaveguideStyle::Tube,
        ..WaveguideConfig::default()
    });
    let string = ResonatorConfig::Waveguide(WaveguideConfig {
        style: WaveguideStyle::String,
        ..WaveguideConfig::default()
    });

    // Every struck driver on a Tube becomes Reed; an existing Reed is left as-is.
    for struck in [
        DriverConfig::Sample,
        DriverConfig::Pick(PickConfig::default()),
        DriverConfig::Bow(BowConfig::default()),
    ] {
        assert!(matches!(
            wind_normalized_driver(tube, struck),
            DriverConfig::Reed(_)
        ));
    }
    let reed = DriverConfig::Reed(ReedConfig::default());
    assert_eq!(wind_normalized_driver(tube, reed), reed);

    // Non-Tube resonators keep their driver untouched.
    assert_eq!(
        wind_normalized_driver(string, DriverConfig::Sample),
        DriverConfig::Sample
    );

    // Per-slot on a whole patch: Tube in A forces driver A to Reed; co-resident String in B
    // keeps its Sample driver.
    let mut patch = ResonatorSynthPatch {
        resonator_a: tube,
        resonator_b: string,
        driver: DriverConfig::Sample,
        driver_b: DriverConfig::Sample,
        ..ResonatorSynthPatch::default()
    };
    patch.normalize_drivers_for_resonator_models();
    assert!(matches!(patch.driver, DriverConfig::Reed(_)));
    assert_eq!(patch.driver_b, DriverConfig::Sample);
}

#[test]
fn default_shared_body_is_off_with_bottom_octave_damp_range() {
    // M0: shared-body mode ships defeated; the damp/choke key-switch range defaults
    // to the bottom MIDI octave C-1..B-1 (0..=11), below an 88-key piano.
    assert_eq!(
        ResonatorSynthPatch::default().shared_body,
        SharedBodyConfig {
            enabled: false,
            damp_key_low: 0,
            damp_key_high: 11,
        }
    );
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
fn default_contact_is_transparent_and_balance_is_alive() {
    // M9 contact stays at its transparent identity (spread/contact-time 0). The
    // source↔body balance, by contrast, defaults to 0.5 (M11 P10) so a default String
    // is dynamically alive; the patch and registry both source it from `default_*`.
    let contact = ContactConfig::default();
    assert_eq!(contact.spread, 0.0);
    assert_eq!(contact.contact_time, 0.0);
    assert_eq!(ResonatorSynthPatch::default().contact, contact);
    assert_eq!(
        WaveguideConfig::default().source_body_balance,
        default_source_body_balance()
    );
    assert_eq!(default_source_body_balance(), 0.5);
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
