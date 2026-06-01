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
}

impl EditorHost {
    /// A new, empty editor host.
    pub fn new() -> Self {
        EditorHost {
            host: HostContext::new()
                .to_com_ptr::<IHostApplication>()
                .expect("host exposes IHostApplication"),
            editors: Vec::new(),
        }
    }

    /// Load the plugin at `path` and open its editor window.
    pub fn open(&mut self, path: &Path) -> Result<(), HostError> {
        let module = load_module(path)?;
        let instance = PluginInstance::from_factory(module.factory(), &self.host)?;
        let controller = EditorController::new(module.factory(), instance.component(), &self.host)?;
        let view = controller.create_view().ok_or(HostError::NoController)?;
        let window = EditorWindow::open(view, &path.display().to_string())?;
        self.editors.push(OpenEditor {
            window,
            _controller: controller,
            _instance: instance,
            _module: module,
        });
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
