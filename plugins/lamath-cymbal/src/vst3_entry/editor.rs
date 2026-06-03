use std::ffi::c_void;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::{path::Path, sync::Arc};

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_ui::audio_file_slot::AudioFileSlotHost;
use lindelion_ui::lamath_cymbal_vizia::{LAMATH_CYMBAL_EDITOR_HEIGHT, LAMATH_CYMBAL_EDITOR_WIDTH};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_ui::{
    audio_file_slot::{AudioFileSlotSurface, AudioFileSlotView},
    lamath_cymbal_vizia::{LamathCymbalControlSurface, LamathCymbalEditorHost, LamathCymbalKnob},
};
use vst3::{ComWrapper, Steinberg::*};

use super::LamathCymbalVst3Processor;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(LAMATH_CYMBAL_EDITOR_WIDTH, LAMATH_CYMBAL_EDITOR_HEIGHT);

pub(super) fn create_editor_view(controller: &LamathCymbalVst3Processor) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        LamathCymbalEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct LamathCymbalEditorView {
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    controller: *const LamathCymbalVst3Processor,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    editor: std::cell::RefCell<Option<lindelion_ui::lamath_cymbal_vizia::LamathCymbalViziaEditor>>,
}

impl LamathCymbalEditorView {
    fn new(controller: &LamathCymbalVst3Processor) -> Self {
        Self {
            controller,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            editor: std::cell::RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for LamathCymbalEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let surface = Arc::new(EditorSurface {
                controller: self.controller,
            });
            let controls: Arc<dyn LamathCymbalControlSurface> = surface.clone();
            let excitation: Arc<dyn AudioFileSlotSurface> = surface;
            let host = LamathCymbalEditorHost::new(controls, AudioFileSlotHost::new(excitation));
            *editor = Some(unsafe {
                lindelion_ui::lamath_cymbal_vizia::LamathCymbalViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::lamath_cymbal_vizia::LamathCymbalEditorSize {
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
    controller: *const LamathCymbalVst3Processor,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
unsafe impl Send for EditorSurface {}
#[cfg(any(target_os = "macos", target_os = "windows"))]
unsafe impl Sync for EditorSurface {}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl EditorSurface {
    fn component(&self) -> &LamathCymbalVst3Processor {
        unsafe { &*self.controller }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl LamathCymbalControlSurface for EditorSurface {
    fn knobs(&self) -> Vec<LamathCymbalKnob> {
        self.component().editor_knobs()
    }

    fn set_knob_normalized(&self, id: u32, normalized: f32) {
        self.component().set_editor_parameter(id, normalized);
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl AudioFileSlotSurface for EditorSurface {
    fn slot_view(&self) -> AudioFileSlotView {
        self.component().excitation_slot_view()
    }

    fn load_audio_file(&self, path: &Path) {
        self.component().load_excitation_from_path(path);
    }

    fn clear_audio_file(&self) {
        self.component().clear_excitation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_editor_view_returns_non_null_plug_view() {
        let controller = LamathCymbalVst3Processor::new();
        let view = create_editor_view(&controller);
        assert!(!view.is_null());
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }
}
