use std::ffi::c_void;
use std::sync::Arc;

#[cfg(target_os = "windows")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use lindelion_ui::caloma_vizia::{CALOMA_EDITOR_HEIGHT, CALOMA_EDITOR_WIDTH, CalomaControlSurface};
use vst3::{ComWrapper, Steinberg::*};

use super::CalomaVst3Plugin;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(CALOMA_EDITOR_WIDTH, CALOMA_EDITOR_HEIGHT);

pub(super) fn create_editor_view(plugin: &CalomaVst3Plugin) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        CalomaEditorView::new(plugin.control_surface()),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct CalomaEditorView {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    surface: Arc<dyn CalomaControlSurface>,
    #[cfg(target_os = "windows")]
    editor: RefCell<Option<lindelion_ui::caloma_vizia::CalomaViziaEditor>>,
}

impl CalomaEditorView {
    fn new(surface: Arc<dyn CalomaControlSurface>) -> Self {
        Self {
            surface,
            #[cfg(target_os = "windows")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for CalomaEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "windows")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let host = lindelion_ui::caloma_vizia::CalomaEditorHost::new(Arc::clone(&self.surface));
            *editor = Some(unsafe {
                lindelion_ui::caloma_vizia::CalomaViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::caloma_vizia::CalomaEditorSize {
                        width: size.right - size.left,
                        height: size.bottom - size.top,
                    },
                )
            });
            kResultOk
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = parent;
            let _ = size;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        #[cfg(target_os = "windows")]
        {
            self.editor.borrow_mut().take();
        }
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_editor_view_returns_non_null_plug_view() {
        let plugin = CalomaVst3Plugin::new();
        let view = create_editor_view(&plugin);
        assert!(!view.is_null());
        // Reclaim the leaked COM reference created by `into_raw` so the test allocates nothing net.
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }
}
