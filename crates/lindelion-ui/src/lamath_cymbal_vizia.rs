//! Lamath Cymbal editor surface.
//!
//! The plugin owns the physical model and sample loading. The UI crate only exposes a sparse
//! parameter and audio-slot-list boundary that compiles on every target; the Vizia view itself is
//! target-gated.

use std::sync::Arc;

use crate::audio_file_slot::AudioFileSlotListHost;

pub const LAMATH_CYMBAL_EDITOR_WIDTH: i32 = 700;
pub const LAMATH_CYMBAL_EDITOR_HEIGHT: i32 = 560;

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

/// A named cymbal voice exposed to the editor's Basic tab. The plugin owns the parameter values; the
/// UI only needs a label and a one-line description to render the picker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LamathCymbalPreset {
    pub name: &'static str,
    pub description: &'static str,
}

/// Audio-thread health, surfaced so a player can tell a bad-sounding voice apart from a dropout: if
/// the load is low and the sound is still wrong, it's the voice, not the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LamathCymbalPerformance {
    /// Smoothed processing time as a fraction of the realtime block budget (`1.0` == the whole
    /// budget; `> 1.0` means the block overran and the host will drop audio).
    pub load: f32,
    /// Sticky worst-case load since the last reset.
    pub peak_load: f32,
    /// Count of blocks whose processing time exceeded the realtime budget (guaranteed dropouts).
    pub xruns: u32,
}

pub trait LamathCymbalControlSurface: Send + Sync {
    fn knobs(&self) -> Vec<LamathCymbalKnob>;
    fn set_knob_normalized(&self, id: u32, normalized: f32);
    /// The ordered list of selectable voices for the Basic-tab picker.
    fn presets(&self) -> Vec<LamathCymbalPreset>;
    /// Load the voice at `index`, leaving the striker slots and loaded samples untouched.
    fn apply_preset(&self, index: usize);
    /// The index of the voice whose parameters match the current patch, if any.
    fn active_preset(&self) -> Option<usize>;
    /// A snapshot of audio-thread load for the editor's performance indicator.
    fn performance(&self) -> LamathCymbalPerformance;
    /// Clear the peak-load hold and the dropout counter.
    fn reset_performance(&self);
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

        fn presets(&self) -> Vec<LamathCymbalPreset> {
            vec![LamathCymbalPreset {
                name: "Default",
                description: "Stub voice",
            }]
        }

        fn apply_preset(&self, _index: usize) {}

        fn active_preset(&self) -> Option<usize> {
            Some(0)
        }

        fn performance(&self) -> LamathCymbalPerformance {
            LamathCymbalPerformance {
                load: 0.25,
                peak_load: 0.4,
                xruns: 0,
            }
        }

        fn reset_performance(&self) {}
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
        assert_eq!(host.controls.presets().len(), 1);
        assert_eq!(host.controls.active_preset(), Some(0));
        assert_eq!(host.controls.performance().xruns, 0);
        assert_eq!(host.strikers.surface.slot_list_view().slots.len(), 4);
    }
}
