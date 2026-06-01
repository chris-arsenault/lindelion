use super::bundle::{BundleSpec, module_info};
use super::{CLIPPY_ARGS, TEST_ARGS};

/// Returns true if `args` contains `--exclude` immediately followed by `pkg`.
fn excludes_package(args: &[&str], pkg: &str) -> bool {
    args.windows(2).any(|pair| pair == ["--exclude", pkg])
}

#[test]
fn ci_clippy_excludes_galad_host() {
    assert!(
        excludes_package(CLIPPY_ARGS, "galad"),
        "make ci clippy must exclude the Windows-only `galad` host (ADR-0022)"
    );
}

#[test]
fn ci_test_excludes_galad_host() {
    assert!(
        excludes_package(TEST_ARGS, "galad"),
        "make ci test must exclude the Windows-only `galad` host (ADR-0022)"
    );
}

#[test]
fn lamath_bundle_spec_uses_instrument_metadata() {
    let lamath = BundleSpec::from_plugin("lamath").expect("lamath bundle spec");

    assert_eq!(lamath.metadata.bundle_name, "Lamath");
    assert_eq!(lamath.metadata.executable_name, "Lamath");
    assert_eq!(lamath.metadata.bundle_identifier, "com.ahara.lamath");
    assert_eq!(lamath.metadata.library_stem, "lamath");
    assert_eq!(
        lamath.metadata.module_sub_categories,
        &["Instrument", "Synth"][..]
    );
}

#[test]
fn glirdir_bundle_spec_uses_effect_metadata() {
    let glirdir = BundleSpec::from_plugin("glirdir").expect("glirdir bundle spec");

    assert_eq!(glirdir.metadata.bundle_name, "Glirdir");
    assert_eq!(glirdir.metadata.executable_name, "Glirdir");
    assert_eq!(glirdir.metadata.bundle_identifier, "com.ahara.glirdir");
    assert_eq!(glirdir.metadata.library_stem, "glirdir");
    assert_eq!(glirdir.metadata.module_sub_categories, &["Fx"][..]);
}

#[test]
fn linnod_bundle_spec_uses_sampler_instrument_metadata() {
    let linnod = BundleSpec::from_plugin("linnod").expect("linnod bundle spec");

    assert_eq!(linnod.metadata.bundle_name, "Linnod");
    assert_eq!(linnod.metadata.executable_name, "Linnod");
    assert_eq!(linnod.metadata.bundle_identifier, "com.ahara.linnod");
    assert_eq!(linnod.metadata.library_stem, "linnod");
    assert_eq!(
        linnod.metadata.module_sub_categories,
        &["Instrument", "Sampler"][..]
    );
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

#[test]
fn linnod_module_info_uses_sampler_metadata() {
    let spec = BundleSpec::from_plugin("linnod").expect("linnod bundle spec");
    let module_info = module_info(&spec);

    assert!(module_info.contains(r#""Name": "Linnod""#));
    assert!(module_info.contains(
        r#""Sub Categories": [
        "Instrument",
        "Sampler"
      ]"#
    ));
    assert!(module_info.contains("8EDB8B287BC44EDCA13D9D832A84A152"));
    assert!(module_info.contains("34F2E7B18D9C4D56B6D7281962D1C0AA"));
}
