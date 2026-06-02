use std::ffi::c_void;

#[cfg(target_os = "windows")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use lindelion_ui::cenedril_vizia::{CENEDRIL_EDITOR_HEIGHT, CENEDRIL_EDITOR_WIDTH};
use vst3::{ComWrapper, Steinberg::*};

use super::CenedrilVst3Processor;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(CENEDRIL_EDITOR_WIDTH, CENEDRIL_EDITOR_HEIGHT);

pub(super) fn create_editor_view(controller: &CenedrilVst3Processor) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        CenedrilEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct CenedrilEditorView {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    controller: *const CenedrilVst3Processor,
    #[cfg(target_os = "windows")]
    editor: RefCell<Option<lindelion_ui::cenedril_vizia::CenedrilViziaEditor>>,
}

impl CenedrilEditorView {
    fn new(controller: &CenedrilVst3Processor) -> Self {
        Self {
            controller,
            #[cfg(target_os = "windows")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for CenedrilEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "windows")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let component = unsafe { &*self.controller };
            let source: std::sync::Arc<dyn lindelion_ui::cenedril_vizia::SpectrogramSource> =
                std::sync::Arc::new(crate::analysis::CenedrilFrameSource::new(
                    component.frame_ring(),
                    component.sample_rate(),
                ));
            let host = lindelion_ui::cenedril_vizia::CenedrilEditorHost::new(source);
            *editor = Some(unsafe {
                lindelion_ui::cenedril_vizia::CenedrilViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::cenedril_vizia::CenedrilEditorSize {
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
        let controller = CenedrilVst3Processor::new();
        let view = create_editor_view(&controller);
        assert!(!view.is_null());
        // Reclaim the leaked COM reference created by `into_raw` so the test allocates nothing net.
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }
}
