//! Editor host — open and manage plugin editor windows (multiple at once).
//!
//! Galad's live UI opens editors against the already-pooled plugin instance so single-component
//! plugins (Cenedril, Lúmedir, Calóma) expose the processor state the audio thread is actually
//! driving. The path-loading helper remains for the standalone `galad editor` diagnostic command.
//! Windows-only; cross-compile-verified.

use std::path::Path;
use std::sync::Arc;

use vst3::ComPtr;
use vst3::Steinberg::Vst::IHostApplication;

use super::editor_controller::EditorController;
use super::editor_window::EditorWindow;
use super::host_context::HostContext;
use super::instance::{HostError, PluginInstance};
use super::module::{LoadedModule, load_module};

/// Ownership kept only for editors opened by path (the live UI keeps instances/modules in the pool).
struct OwnedEditorPlugin {
    _instance: PluginInstance,
    _module: LoadedModule,
}

/// A single open editor. Field order is the teardown order: window → controller → owned plugin.
struct OpenEditor {
    window: EditorWindow,
    _controller: Arc<EditorController>,
    _live_instance: Option<Arc<PluginInstance>>,
    _live_module: Option<Arc<LoadedModule>>,
    _owned: Option<OwnedEditorPlugin>,
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
        let controller = Arc::new(EditorController::new(
            module.factory(),
            &instance,
            &self.host,
        )?);
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
            _live_instance: None,
            _live_module: None,
            _owned: Some(OwnedEditorPlugin {
                _instance: instance,
                _module: module,
            }),
        });
        crate::diagnostics::log(format!(
            "editor-host: open done editors={}",
            self.editors.len()
        ));
        Ok(())
    }

    /// Open an editor window for an already-live plugin instance.
    pub fn open_instance(
        &mut self,
        controller: Arc<EditorController>,
        instance: Arc<PluginInstance>,
        module: Arc<LoadedModule>,
        title: &str,
    ) -> Result<(), HostError> {
        crate::diagnostics::log(format!("editor-host: open_instance begin title={title:?}"));
        crate::diagnostics::log("editor-host: create_view begin");
        let view = controller.create_view().ok_or(HostError::NoController)?;
        crate::diagnostics::log("editor-host: create_view done");
        crate::diagnostics::log("editor-host: EditorWindow::open begin");
        let window = EditorWindow::open(view, title, self.quit_on_last_close)?;
        crate::diagnostics::log("editor-host: EditorWindow::open done");
        self.editors.push(OpenEditor {
            window,
            _controller: controller,
            _live_instance: Some(instance),
            _live_module: Some(module),
            _owned: None,
        });
        crate::diagnostics::log(format!(
            "editor-host: open_instance done editors={}",
            self.editors.len()
        ));
        Ok(())
    }

    /// Whether no editors are open.
    pub fn is_empty(&self) -> bool {
        self.editors.is_empty()
    }

    /// Apply any pending plugin-requested resizes across all open editors.
    pub fn apply_resizes(&mut self) {
        self.prune_closed();
        for editor in &self.editors {
            editor.window.apply_pending_resize();
        }
    }

    fn prune_closed(&mut self) {
        let before = self.editors.len();
        self.editors.retain(|editor| editor.window.is_open());
        let after = self.editors.len();
        if after != before {
            crate::diagnostics::log(format!(
                "editor-host: pruned closed editors before={before} after={after}"
            ));
        }
    }
}

impl Default for EditorHost {
    fn default() -> Self {
        Self::new()
    }
}
