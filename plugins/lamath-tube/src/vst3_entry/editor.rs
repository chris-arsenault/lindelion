use std::ffi::c_void;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::{path::Path, sync::Arc};

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_ui::audio_file_slot::AudioFileSlotListHost;
use lindelion_ui::lamath_tube_vizia::{LAMATH_TUBE_EDITOR_HEIGHT, LAMATH_TUBE_EDITOR_WIDTH};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_ui::{
    audio_file_slot::{AudioFileSlotId, AudioFileSlotListSurface, AudioFileSlotListView},
    lamath_tube_vizia::{
        LamathTubeControlSurface, LamathTubeEditorHost, LamathTubeKnob, LamathTubeModelSwitch,
        LamathTubeSwitchId,
    },
};
use vst3::{ComWrapper, Steinberg::*};

use super::LamathTubeVst3Processor;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(LAMATH_TUBE_EDITOR_WIDTH, LAMATH_TUBE_EDITOR_HEIGHT);

pub(super) fn create_editor_view(controller: &LamathTubeVst3Processor) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        LamathTubeEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct LamathTubeEditorView {
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    controller: *const LamathTubeVst3Processor,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    editor: std::cell::RefCell<Option<lindelion_ui::lamath_tube_vizia::LamathTubeViziaEditor>>,
}

impl LamathTubeEditorView {
    fn new(controller: &LamathTubeVst3Processor) -> Self {
        Self {
            controller,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            editor: std::cell::RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for LamathTubeEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let surface = Arc::new(EditorSurface {
                controller: self.controller,
            });
            let controls: Arc<dyn LamathTubeControlSurface> = surface.clone();
            let articulations: Arc<dyn AudioFileSlotListSurface> = surface;
            let host =
                LamathTubeEditorHost::new(controls, AudioFileSlotListHost::new(articulations));
            *editor = Some(unsafe {
                lindelion_ui::lamath_tube_vizia::LamathTubeViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::lamath_tube_vizia::LamathTubeEditorSize {
                        width: size.right - size.left,
                        height: size.bottom - size.top,
                    },
                )
            });
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
    controller: *const LamathTubeVst3Processor,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
unsafe impl Send for EditorSurface {}
#[cfg(any(target_os = "macos", target_os = "windows"))]
unsafe impl Sync for EditorSurface {}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl EditorSurface {
    fn component(&self) -> &LamathTubeVst3Processor {
        unsafe { &*self.controller }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl LamathTubeControlSurface for EditorSurface {
    fn knobs(&self) -> Vec<LamathTubeKnob> {
        self.component().editor_knobs()
    }

    fn set_knob_normalized(&self, id: u32, normalized: f32) {
        self.component().set_editor_parameter(id, normalized);
    }

    fn model_switches(&self) -> Vec<LamathTubeModelSwitch> {
        self.component().model_switches()
    }

    fn set_model_switch(&self, id: LamathTubeSwitchId, enabled: bool) {
        self.component().set_model_switch(id, enabled);
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl AudioFileSlotListSurface for EditorSurface {
    fn slot_list_view(&self) -> AudioFileSlotListView {
        self.component().articulation_slot_list_view()
    }

    fn select_slot(&self, slot: AudioFileSlotId) {
        self.component().select_articulation_slot(slot.0);
    }

    fn load_audio_file(&self, slot: AudioFileSlotId, path: &Path) {
        self.component().load_excitation_from_path(slot.0, path);
    }

    fn clear_audio_file(&self, slot: AudioFileSlotId) {
        self.component().clear_excitation(slot.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_editor_view_returns_non_null_plug_view() {
        let controller = LamathTubeVst3Processor::new();
        let view = create_editor_view(&controller);
        assert!(!view.is_null());
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }
}
