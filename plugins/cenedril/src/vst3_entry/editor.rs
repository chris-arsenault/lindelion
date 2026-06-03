use std::ffi::c_void;

#[cfg(target_os = "windows")]
use std::cell::RefCell;
#[cfg(target_os = "windows")]
use std::panic::{AssertUnwindSafe, catch_unwind};

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
            lindelion_ui::vizia_window::debug_log(format!(
                "cenedril-editor: attached begin parent=0x{:x} rect=({}, {}, {}, {})",
                parent as usize, size.left, size.top, size.right, size.bottom
            ));
            let mut editor = self.editor.borrow_mut();
            lindelion_ui::vizia_window::debug_log("cenedril-editor: editor borrow acquired");
            *editor = None;
            let result = catch_unwind(AssertUnwindSafe(|| {
                lindelion_ui::vizia_window::debug_log("cenedril-editor: component borrow begin");
                let component = unsafe { &*self.controller };
                lindelion_ui::vizia_window::debug_log("cenedril-editor: component borrow done");
                // One concrete source drains the shared ring; hand the editor both trait views of it
                // (magnitude + reassigned), so switching the view never double-drains.
                lindelion_ui::vizia_window::debug_log("cenedril-editor: frame source begin");
                let frame_source = std::sync::Arc::new(crate::analysis::CenedrilFrameSource::new(
                    component.frame_ring(),
                    component.meter(),
                    component.analysis(),
                    component.settings(),
                    component.sample_rate(),
                ));
                lindelion_ui::vizia_window::debug_log("cenedril-editor: frame source done");
                let source: std::sync::Arc<dyn lindelion_ui::cenedril_vizia::SpectrogramSource> =
                    frame_source.clone();
                let reassigned: std::sync::Arc<dyn lindelion_ui::cenedril_vizia::ReassignedSource> =
                    frame_source.clone();
                let meters: std::sync::Arc<dyn lindelion_ui::cenedril_vizia::MeterSource> =
                    frame_source.clone();
                let settings: std::sync::Arc<dyn lindelion_ui::cenedril_vizia::SettingsStore> =
                    frame_source;
                lindelion_ui::vizia_window::debug_log("cenedril-editor: trait arcs done");
                let host = lindelion_ui::cenedril_vizia::CenedrilEditorHost::new(
                    source, reassigned, meters, settings,
                );
                lindelion_ui::vizia_window::debug_log("cenedril-editor: host done");
                lindelion_ui::vizia_window::debug_log("cenedril-editor: vizia attach begin");
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
                lindelion_ui::vizia_window::debug_log("cenedril-editor: vizia attach done");
            }));
            match result {
                Ok(()) => {
                    lindelion_ui::vizia_window::debug_log("cenedril-editor: attached done");
                    kResultOk
                }
                Err(payload) => {
                    *editor = None;
                    let panic = if let Some(message) = payload.downcast_ref::<&str>() {
                        *message
                    } else if let Some(message) = payload.downcast_ref::<String>() {
                        message.as_str()
                    } else {
                        "unknown panic"
                    };
                    lindelion_ui::vizia_window::debug_log(format!(
                        "cenedril-editor: editor attach panic: {panic}"
                    ));
                    eprintln!("cenedril: editor attach panic: {panic}");
                    kResultFalse
                }
            }
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
            lindelion_ui::vizia_window::debug_log("cenedril-editor: removed begin");
            self.editor.borrow_mut().take();
            lindelion_ui::vizia_window::debug_log("cenedril-editor: removed done");
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
