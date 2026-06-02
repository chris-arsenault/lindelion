// Driver / contact / surrounding parameter-range tests. `include!`d into
// `parameters/tests.rs` so they share that test module's helpers and imports verbatim;
// split out only to keep `tests.rs` under the repository file-size limit.

#[test]
fn driver_type_selector_maps_to_patch_variant() {
    // Driver-type selector: label mapping, default 0 => Sample, and each index round-
    // trips to its `DriverConfig` variant. Bow (index 3) is the P10-exposed driver.
    let driver_type = parameter_binding(DRIVER_TYPE_PARAMETER_ID).expect("driver type binding");
    assert_eq!(driver_type.info().range.default, 0.0);
    for (index, label) in [(0.0, "Sample"), (1.0, "Pick"), (2.0, "Reed"), (3.0, "Bow")] {
        assert_eq!(driver_type.format_plain_value(index), label);
    }

    let mut patch = ResonatorSynthPatch::default();
    let variant_matches = |patch: &ResonatorSynthPatch, index: f32| match index as u8 {
        0 => matches!(patch.driver, DriverConfig::Sample),
        1 => matches!(patch.driver, DriverConfig::Pick(_)),
        2 => matches!(patch.driver, DriverConfig::Reed(_)),
        _ => matches!(patch.driver, DriverConfig::Bow(_)),
    };
    for index in [1.0, 2.0, 3.0, 0.0] {
        driver_type.apply_plain(&mut patch, index);
        assert!(variant_matches(&patch, index), "index {index} variant");
        assert!((driver_type.plain_value(&patch) - index).abs() < 0.001);
    }
}

#[test]
fn driver_continuous_parameters_expose_normalised_ranges() {
    for id in [
        DRIVER_PICK_HARDNESS_PARAMETER_ID,
        DRIVER_PICK_CONTACT_TIME_PARAMETER_ID,
        DRIVER_REED_PRESSURE_DEPTH_PARAMETER_ID,
        DRIVER_REED_STIFFNESS_PARAMETER_ID,
        DRIVER_REED_EMBOUCHURE_PARAMETER_ID,
        DRIVER_BOW_PRESSURE_DEPTH_PARAMETER_ID,
        DRIVER_BOW_SPEED_PARAMETER_ID,
        DRIVER_BOW_FRICTION_PARAMETER_ID,
    ] {
        let binding = parameter_binding(id).expect("driver parameter binding");
        let range = binding.info().range;
        assert!(range.min == 0.0 && range.max == 1.0);
        assert!((range.default - 0.5).abs() < 0.001);
        assert!(binding.info().name.contains("Driver"));
    }
}

#[test]
fn contact_and_balance_parameters_expose_identity_default_ranges() {
    // M9 contact controls span `0..1` with an identity default of 0 (the pre-M9
    // transparent values). The source↔body balance also spans `0..1`, but M11 P10
    // defaults it to 0.5 so the factory String is dynamically alive; the registry
    // default must equal the patch default (`default_source_body_balance`).
    for id in [CONTACT_SPREAD_PARAMETER_ID, CONTACT_TIME_PARAMETER_ID] {
        let binding = parameter_binding(id).expect("M9 contact binding");
        let range = binding.info().range;
        assert!(range.min == 0.0 && range.max == 1.0);
        assert_eq!(range.default, 0.0);
    }
    for id in [
        RESONATOR_A_SOURCE_BODY_BALANCE_PARAMETER_ID,
        RESONATOR_B_SOURCE_BODY_BALANCE_PARAMETER_ID,
    ] {
        let binding = parameter_binding(id).expect("source-body balance binding");
        let range = binding.info().range;
        assert!(range.min == 0.0 && range.max == 1.0);
        assert_eq!(range.default, crate::patch::default_source_body_balance());
    }

    // Each routes to its patch field: contact spread/time -> patch.contact, and the
    // per-slot source-body balance -> the slot's WaveguideConfig.
    let mut patch = ResonatorSynthPatch::default();
    parameter_binding(CONTACT_SPREAD_PARAMETER_ID)
        .unwrap()
        .apply_plain(&mut patch, 0.8);
    assert!((patch.contact.spread - 0.8).abs() < 0.001);

    patch.resonator_b = ResonatorConfig::Waveguide(WaveguideConfig::default());
    parameter_binding(RESONATOR_B_SOURCE_BODY_BALANCE_PARAMETER_ID)
        .unwrap()
        .apply_plain(&mut patch, 0.5);
    let ResonatorConfig::Waveguide(config) = patch.resonator_b else {
        panic!("resonator_b should be Waveguide");
    };
    assert!((config.source_body_balance - 0.5).abs() < 0.001);
}

#[test]
fn surrounding_parameters_expose_defeated_default_ranges() {
    // M10 surrounding-effect depths are `0..1` with a 0 (defeated) default, and each
    // routes to its `patch.surrounding` field.
    for id in [
        SURROUNDING_MECHANICAL_NOISE_PARAMETER_ID,
        SURROUNDING_RADIATION_BRIGHTNESS_PARAMETER_ID,
        SURROUNDING_SYMPATHETIC_PARAMETER_ID,
    ] {
        let binding = parameter_binding(id).expect("M10 surrounding binding");
        let range = binding.info().range;
        assert!(range.min == 0.0 && range.max == 1.0);
        assert_eq!(range.default, 0.0);
    }

    let mut patch = ResonatorSynthPatch::default();
    parameter_binding(SURROUNDING_SYMPATHETIC_PARAMETER_ID)
        .unwrap()
        .apply_plain(&mut patch, 0.7);
    assert!((patch.surrounding.sympathetic - 0.7).abs() < 0.001);
}
