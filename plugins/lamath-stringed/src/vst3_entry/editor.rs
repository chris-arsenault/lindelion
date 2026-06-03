use std::ffi::c_void;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::{path::Path, sync::Arc};

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use lindelion_ui::lamath_stringed_vizia::{
    LAMATH_STRINGED_EDITOR_HEIGHT, LAMATH_STRINGED_EDITOR_WIDTH,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_ui::{
    audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListHost, AudioFileSlotListSurface, AudioFileSlotListView,
    },
    lamath_stringed_vizia::{
        LamathStringedBodyId, LamathStringedControlSurface, LamathStringedDriverId,
        LamathStringedEditorHost, LamathStringedKnob, LamathStringedModelSwitch,
        LamathStringedSwitchId,
    },
};
use vst3::{ComWrapper, Steinberg::*};

use super::LamathStringedVst3Processor;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(LAMATH_STRINGED_EDITOR_WIDTH, LAMATH_STRINGED_EDITOR_HEIGHT);

pub(super) fn create_editor_view(controller: &LamathStringedVst3Processor) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        LamathStringedEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct LamathStringedEditorView {
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    controller: *const LamathStringedVst3Processor,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    editor:
        std::cell::RefCell<Option<lindelion_ui::lamath_stringed_vizia::LamathStringedViziaEditor>>,
}

impl LamathStringedEditorView {
    fn new(controller: &LamathStringedVst3Processor) -> Self {
        Self {
            controller,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            editor: std::cell::RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for LamathStringedEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let surface = Arc::new(EditorSurface {
                controller: self.controller,
            });
            let controls: Arc<dyn LamathStringedControlSurface> = surface.clone();
            let slots: Arc<dyn AudioFileSlotListSurface> = surface;
            let host = LamathStringedEditorHost::new(controls, AudioFileSlotListHost::new(slots));
            let editor = unsafe {
                lindelion_ui::lamath_stringed_vizia::LamathStringedViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::lamath_stringed_vizia::LamathStringedEditorSize {
                        width: size.right - size.left,
                        height: size.bottom - size.top,
                    },
                )
            };
            self.editor.borrow_mut().replace(editor);
            kResultOk
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = parent;
            let _ = size;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.editor.borrow_mut().take();
        }
        kResultOk
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
struct EditorSurface {
    controller: *const LamathStringedVst3Processor,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
unsafe impl Send for EditorSurface {}
#[cfg(any(target_os = "macos", target_os = "windows"))]
unsafe impl Sync for EditorSurface {}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl EditorSurface {
    fn controller(&self) -> &LamathStringedVst3Processor {
        unsafe { &*self.controller }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl LamathStringedControlSurface for EditorSurface {
    fn knobs(&self) -> Vec<LamathStringedKnob> {
        self.controller().editor_knobs()
    }

    fn set_knob_normalized(&self, id: u32, normalized: f32) {
        self.controller().set_editor_parameter(id, normalized);
    }

    fn selected_driver(&self) -> LamathStringedDriverId {
        self.controller().selected_driver()
    }

    fn set_driver(&self, driver: LamathStringedDriverId) {
        self.controller().set_driver(driver);
    }

    fn selected_body(&self) -> LamathStringedBodyId {
        self.controller().selected_body()
    }

    fn set_body(&self, body: LamathStringedBodyId) {
        self.controller().set_body(body);
    }

    fn model_switches(&self) -> Vec<LamathStringedModelSwitch> {
        self.controller().model_switches()
    }

    fn set_model_switch(&self, id: LamathStringedSwitchId, enabled: bool) {
        self.controller().set_model_switch(id, enabled);
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl AudioFileSlotListSurface for EditorSurface {
    fn slot_list_view(&self) -> AudioFileSlotListView {
        self.controller().articulation_slot_list_view()
    }

    fn select_slot(&self, slot: AudioFileSlotId) {
        self.controller().select_articulation_slot(slot.0);
    }

    fn load_audio_file(&self, slot: AudioFileSlotId, path: &Path) {
        self.controller().load_excitation_from_path(slot.0, path);
    }

    fn clear_audio_file(&self, slot: AudioFileSlotId) {
        self.controller().clear_excitation(slot.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_editor_view_returns_non_null_plug_view() {
        let processor = LamathStringedVst3Processor::new();
        let view = create_editor_view(&processor);
        assert!(!view.is_null());
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }
}
