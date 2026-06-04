//! Lamath Cymbal editor surface.
//!
//! The plugin owns the physical model and sample loading. The UI crate only exposes a sparse
//! parameter and audio-slot-list boundary that compiles on every target; the Vizia view itself is
//! target-gated.

use std::sync::Arc;

use crate::audio_file_slot::AudioFileSlotListHost;

pub const LAMATH_CYMBAL_EDITOR_WIDTH: i32 = 560;
pub const LAMATH_CYMBAL_EDITOR_HEIGHT: i32 = 420;

#[derive(Debug, Clone, Copy)]
pub struct LamathCymbalEditorSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LamathCymbalKnob {
    pub id: u32,
    pub label: &'static str,
    pub units: &'static str,
    pub normalized: f32,
    pub plain: f32,
}

pub trait LamathCymbalControlSurface: Send + Sync {
    fn knobs(&self) -> Vec<LamathCymbalKnob>;
    fn set_knob_normalized(&self, id: u32, normalized: f32);
}

#[derive(Clone)]
pub struct LamathCymbalEditorHost {
    pub controls: Arc<dyn LamathCymbalControlSurface>,
    pub strikers: AudioFileSlotListHost,
}

impl LamathCymbalEditorHost {
    pub fn new(
        controls: Arc<dyn LamathCymbalControlSurface>,
        strikers: AudioFileSlotListHost,
    ) -> Self {
        Self { controls, strikers }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use platform::LamathCymbalViziaEditor;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListSurface, AudioFileSlotListView, AudioFileSlotView,
    };
    use std::path::Path;

    struct StubControls;

    impl LamathCymbalControlSurface for StubControls {
        fn knobs(&self) -> Vec<LamathCymbalKnob> {
            vec![LamathCymbalKnob {
                id: 1,
                label: "Damping",
                units: "",
                normalized: 0.5,
                plain: 0.5,
            }]
        }

        fn set_knob_normalized(&self, _id: u32, _normalized: f32) {}
    }

    struct StubSlot;

    impl AudioFileSlotListSurface for StubSlot {
        fn slot_list_view(&self) -> AudioFileSlotListView {
            AudioFileSlotListView {
                selected: AudioFileSlotId(0),
                slots: vec![AudioFileSlotView::default(); 4],
            }
        }

        fn select_slot(&self, _slot: AudioFileSlotId) {}

        fn load_audio_file(&self, _slot: AudioFileSlotId, _path: &Path) {}

        fn clear_audio_file(&self, _slot: AudioFileSlotId) {}
    }

    #[test]
    fn host_exposes_sparse_knob_and_striker_boundaries() {
        let host = LamathCymbalEditorHost::new(
            Arc::new(StubControls),
            AudioFileSlotListHost::new(Arc::new(StubSlot)),
        );
        assert_eq!(host.controls.knobs().len(), 1);
        assert_eq!(host.strikers.surface.slot_list_view().slots.len(), 4);
    }
}
