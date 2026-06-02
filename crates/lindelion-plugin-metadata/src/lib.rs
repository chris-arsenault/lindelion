pub use lindelion_plugin_shell::vst3::Vst3BundleMetadata;

pub const LAMATH_VST3_BUNDLE_METADATA: Vst3BundleMetadata = Vst3BundleMetadata {
    package: "lamath",
    bundle_name: "Lamath",
    executable_name: "Lamath",
    bundle_identifier: "com.ahara.lamath",
    library_stem: "lamath",
    windows_runtime_dlls: &[],
    vst3_sub_categories: "Instrument|Synth",
    module_sub_categories: &["Instrument", "Synth"],
    processor_cid: [0x4B410E03, 0x80AD49B6, 0x9B7D5479, 0xF4A9B0D1],
    controller_cid: [0x15C8B012, 0xF4B64F5E, 0x93D9AA38, 0x69383E3B],
    controller_name: "Lamath Controller",
    single_component: false,
};

pub const GLIRDIR_VST3_BUNDLE_METADATA: Vst3BundleMetadata = Vst3BundleMetadata {
    package: "glirdir",
    bundle_name: "Glirdir",
    executable_name: "Glirdir",
    bundle_identifier: "com.ahara.glirdir",
    library_stem: "glirdir",
    windows_runtime_dlls: &[],
    vst3_sub_categories: "Fx",
    module_sub_categories: &["Fx"],
    processor_cid: [0x7C2E2B8A, 0xB1C44F0D, 0xA6F92427, 0x6C9E0D5B],
    controller_cid: [0x0D0466D2, 0x53E446E5, 0x8E90CF13, 0x25B5E241],
    controller_name: "Glirdir Controller",
    single_component: false,
};

pub const LINNOD_VST3_BUNDLE_METADATA: Vst3BundleMetadata = Vst3BundleMetadata {
    package: "linnod",
    bundle_name: "Linnod",
    executable_name: "Linnod",
    bundle_identifier: "com.ahara.linnod",
    library_stem: "linnod",
    windows_runtime_dlls: &[],
    vst3_sub_categories: "Instrument|Sampler",
    module_sub_categories: &["Instrument", "Sampler"],
    processor_cid: [0x8EDB8B28, 0x7BC44EDC, 0xA13D9D83, 0x2A84A152],
    controller_cid: [0x34F2E7B1, 0x8D9C4D56, 0xB6D72819, 0x62D1C0AA],
    controller_name: "Linnod Controller",
    single_component: false,
};

pub const CENEDRIL_VST3_BUNDLE_METADATA: Vst3BundleMetadata = Vst3BundleMetadata {
    package: "cenedril",
    bundle_name: "Cenedril",
    executable_name: "Cenedril",
    bundle_identifier: "com.ahara.cenedril",
    library_stem: "cenedril",
    windows_runtime_dlls: &[],
    vst3_sub_categories: "Fx",
    module_sub_categories: &["Fx"],
    processor_cid: [0xCE9ED713, 0x1A5B4C20, 0x8F3D6E94, 0xB2470FA1],
    controller_cid: [0xCE9EDC72, 0x6D8E4F31, 0xA1B05C28, 0x73E2941D],
    controller_name: "Cenedril Controller",
    single_component: true,
};

pub const CALOMA_VST3_BUNDLE_METADATA: Vst3BundleMetadata = Vst3BundleMetadata {
    package: "caloma",
    bundle_name: "Caloma",
    executable_name: "Caloma",
    bundle_identifier: "com.ahara.caloma",
    library_stem: "caloma",
    windows_runtime_dlls: &["DirectML.dll"],
    vst3_sub_categories: "Fx",
    module_sub_categories: &["Fx"],
    processor_cid: [0xCA10A001, 0x5E1F4C32, 0x8B3D6E95, 0xA2470FB2],
    controller_cid: [0xCA10A0C2, 0x6D9E4F33, 0xB1C05D29, 0x73E2952E],
    controller_name: "Caloma Controller",
    single_component: true,
};

pub fn metadata_for_package(package: &str) -> Option<Vst3BundleMetadata> {
    match package {
        "lamath" => Some(LAMATH_VST3_BUNDLE_METADATA),
        "glirdir" => Some(GLIRDIR_VST3_BUNDLE_METADATA),
        "linnod" => Some(LINNOD_VST3_BUNDLE_METADATA),
        "cenedril" => Some(CENEDRIL_VST3_BUNDLE_METADATA),
        "caloma" => Some(CALOMA_VST3_BUNDLE_METADATA),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_lookup_covers_bundleable_plugins() {
        assert_eq!(
            metadata_for_package("lamath").unwrap(),
            LAMATH_VST3_BUNDLE_METADATA
        );
        assert_eq!(
            metadata_for_package("glirdir").unwrap(),
            GLIRDIR_VST3_BUNDLE_METADATA
        );
        assert_eq!(
            metadata_for_package("linnod").unwrap(),
            LINNOD_VST3_BUNDLE_METADATA
        );
        assert_eq!(
            metadata_for_package("cenedril").unwrap(),
            CENEDRIL_VST3_BUNDLE_METADATA
        );
        assert_eq!(
            metadata_for_package("caloma").unwrap(),
            CALOMA_VST3_BUNDLE_METADATA
        );
        assert!(metadata_for_package("unknown").is_none());
    }

    #[test]
    fn caloma_declares_directml_runtime_dependency() {
        assert_eq!(
            CALOMA_VST3_BUNDLE_METADATA.windows_runtime_dlls,
            &["DirectML.dll"]
        );
    }
}
