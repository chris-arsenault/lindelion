use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::bundle::{BundleSpec, module_info};

const WINDOWS_ARCH_DIR: &str = "x86_64-win";
const WINDOWS_MODULEINFO_RELATIVE_PATH: &str = "Contents/Resources/moduleinfo.json";

fn windows_bundle_dir_name(spec: &BundleSpec) -> String {
    format!("{}.vst3", spec.metadata.bundle_name)
}

fn windows_binary_relative_path(spec: &BundleSpec) -> String {
    format!(
        "Contents/{WINDOWS_ARCH_DIR}/{}.vst3",
        spec.metadata.bundle_name
    )
}

/// Create a Windows VST3 bundle: the cdylib DLL is renamed to `<BundleName>.vst3` inside
/// `Contents/x86_64-win/`, with the platform-neutral `moduleinfo.json` under `Contents/Resources/`.
/// No `Info.plist`/`PkgInfo`/codesign — those are macOS-only.
pub(crate) fn create_windows_vst3_bundle(
    spec: &BundleSpec,
    bundle_dir: &Path,
    source_dll: &Path,
) -> io::Result<PathBuf> {
    let bundle = bundle_dir.join(windows_bundle_dir_name(spec));
    let binary = bundle.join(windows_binary_relative_path(spec));
    let module_info_path = bundle.join(WINDOWS_MODULEINFO_RELATIVE_PATH);

    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }
    if let Some(parent) = binary.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = module_info_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(source_dll, &binary)?;
    fs::write(module_info_path, module_info(spec))?;

    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cenedril_windows_layout_places_dll_and_moduleinfo() {
        let spec = BundleSpec::from_plugin("cenedril").expect("cenedril spec");
        assert_eq!(windows_bundle_dir_name(&spec), "Cenedril.vst3");
        assert_eq!(
            windows_binary_relative_path(&spec),
            "Contents/x86_64-win/Cenedril.vst3"
        );
        assert_eq!(
            WINDOWS_MODULEINFO_RELATIVE_PATH,
            "Contents/Resources/moduleinfo.json"
        );
    }

    #[test]
    fn cenedril_module_info_uses_effect_metadata() {
        let spec = BundleSpec::from_plugin("cenedril").expect("cenedril spec");
        let module_info = module_info(&spec);
        assert!(module_info.contains(r#""Name": "Cenedril""#));
        assert!(module_info.contains("CE9ED7131A5B4C208F3D6E94B2470FA1"));
        assert!(module_info.contains("CE9EDC726D8E4F31A1B05C2873E2941D"));
        assert!(module_info.contains(r#""Fx""#));
    }

    #[test]
    fn lumedir_windows_layout_places_dll_and_moduleinfo() {
        let spec = BundleSpec::from_plugin("lumedir").expect("lumedir spec");
        assert_eq!(windows_bundle_dir_name(&spec), "Lumedir.vst3");
        assert_eq!(
            windows_binary_relative_path(&spec),
            "Contents/x86_64-win/Lumedir.vst3"
        );
        assert_eq!(
            WINDOWS_MODULEINFO_RELATIVE_PATH,
            "Contents/Resources/moduleinfo.json"
        );
    }

    #[test]
    fn lumedir_module_info_uses_effect_metadata() {
        let spec = BundleSpec::from_plugin("lumedir").expect("lumedir spec");
        let module_info = module_info(&spec);
        assert!(module_info.contains(r#""Name": "Lumedir""#));
        assert!(module_info.contains("1AEDC0132B5C4D209E3F7A85C1582FB6"));
        assert!(module_info.contains("1AEDCD727E9F4A31B2C16D3984F3A52E"));
        assert!(module_info.contains(r#""Fx""#));
    }
}
