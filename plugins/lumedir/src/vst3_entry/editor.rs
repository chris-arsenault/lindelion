use std::ffi::c_void;

#[cfg(target_os = "windows")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use lindelion_ui::lumedir_vizia::{LUMEDIR_EDITOR_HEIGHT, LUMEDIR_EDITOR_WIDTH};
use vst3::{ComWrapper, Steinberg::*};

use super::LumedirVst3Controller;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(LUMEDIR_EDITOR_WIDTH, LUMEDIR_EDITOR_HEIGHT);

pub(super) fn create_editor_view(controller: &LumedirVst3Controller) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        LumedirEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct LumedirEditorView {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    controller: *const LumedirVst3Controller,
    #[cfg(target_os = "windows")]
    editor: RefCell<Option<lindelion_ui::lumedir_vizia::LumedirViziaEditor>>,
}

impl LumedirEditorView {
    fn new(controller: &LumedirVst3Controller) -> Self {
        Self {
            controller,
            #[cfg(target_os = "windows")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for LumedirEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "windows")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let host =
                lindelion_ui::lumedir_vizia::LumedirEditorHost::new(self.controller as usize);
            *editor = Some(unsafe {
                lindelion_ui::lumedir_vizia::LumedirViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::lumedir_vizia::LumedirEditorSize {
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
        let controller = LumedirVst3Controller::new();
        let view = create_editor_view(&controller);
        assert!(!view.is_null());
        // Reclaim the leaked COM reference created by `into_raw` so the test allocates nothing net.
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }
}
