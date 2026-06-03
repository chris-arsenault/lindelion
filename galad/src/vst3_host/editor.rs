//! Editor host — open and manage plugin editor windows (multiple at once). Each open editor keeps its
//! module, instance, controller, and window alive together, torn down in the right order on drop.
//! Windows-only; cross-compile-verified.

use std::path::Path;

use vst3::ComPtr;
use vst3::Steinberg::Vst::IHostApplication;

use super::editor_controller::EditorController;
use super::editor_window::EditorWindow;
use super::host_context::HostContext;
use super::instance::{HostError, PluginInstance};
use super::module::{LoadedModule, load_module};

/// A single open editor. Field order is the teardown order: window → controller → instance → module.
struct OpenEditor {
    window: EditorWindow,
    _controller: EditorController,
    _instance: PluginInstance,
    _module: LoadedModule,
}

/// Hosts one or more plugin editor windows.
pub struct EditorHost {
    host: ComPtr<IHostApplication>,
    editors: Vec<OpenEditor>,
    quit_on_last_close: bool,
}

impl EditorHost {
    /// A new, empty editor host.
    pub fn new() -> Self {
        EditorHost {
            host: HostContext::new()
                .to_com_ptr::<IHostApplication>()
                .expect("host exposes IHostApplication"),
            editors: Vec::new(),
            quit_on_last_close: false,
        }
    }

    /// A new editor host whose message loop exits when its last editor window closes.
    pub fn with_quit_on_last_close() -> Self {
        EditorHost {
            quit_on_last_close: true,
            ..Self::new()
        }
    }

    /// Load the plugin at `path` and open its editor window.
    pub fn open(&mut self, path: &Path) -> Result<(), HostError> {
        crate::diagnostics::log(format!("editor-host: open begin path={}", path.display()));
        crate::diagnostics::log("editor-host: load_module begin");
        let module = load_module(path)?;
        crate::diagnostics::log("editor-host: load_module done");
        crate::diagnostics::log("editor-host: PluginInstance::from_factory begin");
        let instance = PluginInstance::from_factory(module.factory(), &self.host)?;
        crate::diagnostics::log("editor-host: PluginInstance::from_factory done");
        crate::diagnostics::log("editor-host: EditorController::new begin");
        let controller = EditorController::new(module.factory(), &instance, &self.host)?;
        crate::diagnostics::log("editor-host: EditorController::new done");
        crate::diagnostics::log("editor-host: create_view begin");
        let view = controller.create_view().ok_or(HostError::NoController)?;
        crate::diagnostics::log("editor-host: create_view done");
        crate::diagnostics::log("editor-host: EditorWindow::open begin");
        let window =
            EditorWindow::open(view, &path.display().to_string(), self.quit_on_last_close)?;
        crate::diagnostics::log("editor-host: EditorWindow::open done");
        self.editors.push(OpenEditor {
            window,
            _controller: controller,
            _instance: instance,
            _module: module,
        });
        crate::diagnostics::log(format!(
            "editor-host: open done editors={}",
            self.editors.len()
        ));
        Ok(())
    }

    /// Whether no editors are open.
    pub fn is_empty(&self) -> bool {
        self.editors.is_empty()
    }

    /// Apply any pending plugin-requested resizes across all open editors.
    pub fn apply_resizes(&self) {
        for editor in &self.editors {
            editor.window.apply_pending_resize();
        }
    }
}

impl Default for EditorHost {
    fn default() -> Self {
        Self::new()
    }
}
