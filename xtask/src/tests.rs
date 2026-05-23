use super::*;

#[test]
fn lamath_bundle_spec_uses_instrument_metadata() {
    let lamath = BundleSpec::from_plugin("lamath").expect("lamath bundle spec");

    assert_eq!(lamath.bundle_name, "Lamath");
    assert_eq!(lamath.executable_name, "Lamath");
    assert_eq!(lamath.bundle_identifier, "com.ahara.lamath");
    assert_eq!(lamath.library_stem, "lamath");
    assert_eq!(lamath.sub_categories, &["Instrument", "Synth"][..]);
}

#[test]
fn glirdir_bundle_spec_uses_effect_metadata() {
    let glirdir = BundleSpec::from_plugin("glirdir").expect("glirdir bundle spec");

    assert_eq!(glirdir.bundle_name, "Glirdir");
    assert_eq!(glirdir.executable_name, "Glirdir");
    assert_eq!(glirdir.bundle_identifier, "com.ahara.glirdir");
    assert_eq!(glirdir.library_stem, "glirdir");
    assert_eq!(glirdir.sub_categories, &["Fx"][..]);
}

#[test]
fn glirdir_module_info_uses_effect_metadata() {
    let spec = BundleSpec::from_plugin("glirdir").expect("glirdir bundle spec");
    let module_info = module_info(&spec);

    assert!(module_info.contains(r#""Name": "Glirdir""#));
    assert!(module_info.contains(
        r#""Sub Categories": [
        "Fx"
      ]"#
    ));
    assert!(module_info.contains("7C2E2B8AB1C44F0DA6F924276C9E0D5B"));
    assert!(module_info.contains("0D0466D253E446E58E90CF1325B5E241"));
    assert!(!module_info.contains(r#""Instrument""#));
}
