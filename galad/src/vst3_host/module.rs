//! Module loader — open a `.vst3` dynamic library and resolve its `GetPluginFactory` entry point
//! into a `ComPtr<IPluginFactory>`. Uses `libloading` (cross-platform: `LoadLibrary` on Windows).
//!
//! A real Windows VST3 module is the DLL at `<Bundle>.vst3/Contents/x86_64-win/<Bundle>.vst3` (the
//! DLL renamed `.vst3`; see `xtask/src/windows_bundle.rs`). The DLL only loads at runtime on Windows;
//! this code compiles on all platforms (the missing-file error path is exercised on Linux).

use std::path::{Path, PathBuf};

use libloading::{Library, Symbol};
use vst3::{ComPtr, Steinberg::*};

use super::instance::HostError;

/// VST3 module C entry points (Windows naming convention).
type GetPluginFactoryFn = unsafe extern "system" fn() -> *mut IPluginFactory;
type InitDllFn = unsafe extern "system" fn() -> bool;
type ExitDllFn = unsafe extern "system" fn() -> bool;

const WINDOWS_ARCH_DIR: &str = "x86_64-win";

/// A loaded `.vst3` module: its factory plus the owning library, kept alive for the factory's life.
pub struct LoadedModule {
    library: Library,
    factory: Option<ComPtr<IPluginFactory>>,
}

impl LoadedModule {
    /// The module's plugin factory.
    pub fn factory(&self) -> &ComPtr<IPluginFactory> {
        self.factory.as_ref().expect("factory present until drop")
    }
}

impl Drop for LoadedModule {
    fn drop(&mut self) {
        crate::diagnostics::log("vst3-module: drop begin");
        // Release the factory while the DLL is still mapped, then call ExitDll, then unload.
        self.factory.take();
        crate::diagnostics::log("vst3-module: factory released");
        unsafe {
            if let Ok(exit) = self.library.get::<ExitDllFn>(b"ExitDll\0") {
                let result = exit();
                crate::diagnostics::log(format!("vst3-module: ExitDll result={result}"));
            }
        }
        crate::diagnostics::log("vst3-module: drop end");
    }
}

/// Open the `.vst3` module at `path`, run `InitDll` (if present), and resolve `GetPluginFactory`.
pub fn load_module(path: &Path) -> Result<LoadedModule, HostError> {
    let module_path = resolve_vst3_module_path(path);
    unsafe {
        let library = Library::new(&module_path).map_err(|error| {
            if module_path == path {
                HostError::ModuleLoad(error.to_string())
            } else {
                HostError::ModuleLoad(format!(
                    "{} (resolved {} to {})",
                    error,
                    path.display(),
                    module_path.display()
                ))
            }
        })?;

        if let Ok(init) = library.get::<InitDllFn>(b"InitDll\0")
            && !init()
        {
            return Err(HostError::ModuleLoad("InitDll returned false".to_string()));
        }

        let raw = {
            let get_factory: Symbol<GetPluginFactoryFn> = library
                .get(b"GetPluginFactory\0")
                .map_err(|error| HostError::ModuleLoad(format!("GetPluginFactory: {error}")))?;
            get_factory()
        };

        let factory = ComPtr::from_raw(raw)
            .ok_or_else(|| HostError::ModuleLoad("GetPluginFactory returned null".to_string()))?;

        Ok(LoadedModule {
            library,
            factory: Some(factory),
        })
    }
}

fn resolve_vst3_module_path(path: &Path) -> PathBuf {
    resolve_vst3_module_path_with_dir_state(path, path.is_dir())
}

fn resolve_vst3_module_path_with_dir_state(path: &Path, is_dir: bool) -> PathBuf {
    if is_dir
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("vst3"))
        && let Some(file_name) = path.file_name()
    {
        return path.join("Contents").join(WINDOWS_ARCH_DIR).join(file_name);
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_a_missing_module_errors() {
        let result = load_module(Path::new("/nonexistent/galad-no-such.vst3"));
        assert!(matches!(result, Err(HostError::ModuleLoad(_))));
    }

    #[test]
    fn resolves_windows_vst3_bundle_dir_to_inner_module() {
        let resolved =
            resolve_vst3_module_path_with_dir_state(Path::new("/plugins/Caloma.vst3"), true);
        assert_eq!(
            resolved,
            PathBuf::from("/plugins/Caloma.vst3/Contents/x86_64-win/Caloma.vst3")
        );
    }

    #[test]
    fn leaves_exact_windows_vst3_module_path_unchanged() {
        let path = Path::new("/plugins/Caloma.vst3/Contents/x86_64-win/Caloma.vst3");
        let resolved = resolve_vst3_module_path_with_dir_state(path, false);
        assert_eq!(resolved, path);
    }
}
