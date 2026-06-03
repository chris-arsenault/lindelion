//! Lamath Tube editor surface.

use std::sync::Arc;

use crate::audio_file_slot::AudioFileSlotListHost;

pub const LAMATH_TUBE_EDITOR_WIDTH: i32 = 640;
pub const LAMATH_TUBE_EDITOR_HEIGHT: i32 = 540;

#[derive(Debug, Clone, Copy)]
pub struct LamathTubeEditorSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LamathTubeKnob {
    pub id: u32,
    pub label: &'static str,
    pub units: &'static str,
    pub normalized: f32,
    pub plain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LamathTubeSwitchId {
    Reed,
    Bell,
    BoreSteepening,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LamathTubeModelSwitch {
    pub id: LamathTubeSwitchId,
    pub label: &'static str,
    pub enabled: bool,
    pub editable: bool,
}

pub trait LamathTubeControlSurface: Send + Sync {
    fn knobs(&self) -> Vec<LamathTubeKnob>;
    fn set_knob_normalized(&self, id: u32, normalized: f32);
    fn model_switches(&self) -> Vec<LamathTubeModelSwitch>;
    fn set_model_switch(&self, id: LamathTubeSwitchId, enabled: bool);
}

#[derive(Clone)]
pub struct LamathTubeEditorHost {
    pub controls: Arc<dyn LamathTubeControlSurface>,
    pub articulations: AudioFileSlotListHost,
}

impl LamathTubeEditorHost {
    pub fn new(
        controls: Arc<dyn LamathTubeControlSurface>,
        articulations: AudioFileSlotListHost,
    ) -> Self {
        Self {
            controls,
            articulations,
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use platform::LamathTubeViziaEditor;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListSurface, AudioFileSlotListView, AudioFileSlotView,
    };
    use std::path::Path;

    struct StubControls;

    impl LamathTubeControlSurface for StubControls {
        fn knobs(&self) -> Vec<LamathTubeKnob> {
            vec![LamathTubeKnob {
                id: 1,
                label: "Pressure",
                units: "",
                normalized: 0.5,
                plain: 0.5,
            }]
        }

        fn set_knob_normalized(&self, _id: u32, _normalized: f32) {}

        fn model_switches(&self) -> Vec<LamathTubeModelSwitch> {
            vec![LamathTubeModelSwitch {
                id: LamathTubeSwitchId::Reed,
                label: "Reed",
                enabled: true,
                editable: false,
            }]
        }

        fn set_model_switch(&self, _id: LamathTubeSwitchId, _enabled: bool) {}
    }

    struct StubSlots;

    impl AudioFileSlotListSurface for StubSlots {
        fn slot_list_view(&self) -> AudioFileSlotListView {
            AudioFileSlotListView {
                selected: AudioFileSlotId(0),
                slots: vec![AudioFileSlotView::default()],
            }
        }

        fn select_slot(&self, _slot: AudioFileSlotId) {}

        fn load_audio_file(&self, _slot: AudioFileSlotId, _path: &Path) {}

        fn clear_audio_file(&self, _slot: AudioFileSlotId) {}
    }

    #[test]
    fn host_exposes_sparse_knobs_switches_and_articulation_slots() {
        let host = LamathTubeEditorHost::new(
            Arc::new(StubControls),
            AudioFileSlotListHost::new(Arc::new(StubSlots)),
        );
        assert_eq!(host.controls.knobs().len(), 1);
        assert_eq!(
            host.controls.model_switches()[0].id,
            LamathTubeSwitchId::Reed
        );
        assert_eq!(host.articulations.surface.slot_list_view().slots.len(), 1);
    }
}
