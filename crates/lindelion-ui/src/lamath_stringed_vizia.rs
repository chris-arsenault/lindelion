//! Lamath Stringed editor surface.

use std::sync::Arc;

use crate::audio_file_slot::AudioFileSlotListHost;

pub const LAMATH_STRINGED_EDITOR_WIDTH: i32 = 680;
pub const LAMATH_STRINGED_EDITOR_HEIGHT: i32 = 560;

#[derive(Debug, Clone, Copy)]
pub struct LamathStringedEditorSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LamathStringedKnob {
    pub id: u32,
    pub label: &'static str,
    pub units: &'static str,
    pub normalized: f32,
    pub plain: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LamathStringedDriverId {
    None,
    #[default]
    Pick,
    Bow,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LamathStringedBodyId {
    Disabled,
    #[default]
    Guitar,
    Violin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LamathStringedSwitchId {
    BodyContact,
    BowDrive,
    Tension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LamathStringedModelSwitch {
    pub id: LamathStringedSwitchId,
    pub label: &'static str,
    pub enabled: bool,
    pub editable: bool,
}

pub trait LamathStringedControlSurface: Send + Sync {
    fn knobs(&self) -> Vec<LamathStringedKnob>;
    fn set_knob_normalized(&self, id: u32, normalized: f32);
    fn selected_driver(&self) -> LamathStringedDriverId;
    fn set_driver(&self, driver: LamathStringedDriverId);
    fn selected_body(&self) -> LamathStringedBodyId;
    fn set_body(&self, body: LamathStringedBodyId);
    fn model_switches(&self) -> Vec<LamathStringedModelSwitch>;
    fn set_model_switch(&self, id: LamathStringedSwitchId, enabled: bool);
}

#[derive(Clone)]
pub struct LamathStringedEditorHost {
    pub controls: Arc<dyn LamathStringedControlSurface>,
    pub articulations: AudioFileSlotListHost,
}

impl LamathStringedEditorHost {
    pub fn new(
        controls: Arc<dyn LamathStringedControlSurface>,
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
pub use platform::LamathStringedViziaEditor;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListSurface, AudioFileSlotListView, AudioFileSlotView,
    };
    use std::path::Path;

    struct Controls;

    impl LamathStringedControlSurface for Controls {
        fn knobs(&self) -> Vec<LamathStringedKnob> {
            vec![LamathStringedKnob {
                id: 1,
                label: "Brightness",
                units: "",
                normalized: 0.5,
                plain: 0.5,
            }]
        }

        fn set_knob_normalized(&self, _id: u32, _normalized: f32) {}

        fn selected_driver(&self) -> LamathStringedDriverId {
            LamathStringedDriverId::Pick
        }

        fn set_driver(&self, _driver: LamathStringedDriverId) {}

        fn selected_body(&self) -> LamathStringedBodyId {
            LamathStringedBodyId::Guitar
        }

        fn set_body(&self, _body: LamathStringedBodyId) {}

        fn model_switches(&self) -> Vec<LamathStringedModelSwitch> {
            vec![LamathStringedModelSwitch {
                id: LamathStringedSwitchId::BodyContact,
                label: "Body contact",
                enabled: true,
                editable: true,
            }]
        }

        fn set_model_switch(&self, _id: LamathStringedSwitchId, _enabled: bool) {}
    }

    struct Slots;

    impl AudioFileSlotListSurface for Slots {
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
    fn host_exposes_controls_and_articulation_slots() {
        let host = LamathStringedEditorHost::new(
            Arc::new(Controls),
            AudioFileSlotListHost::new(Arc::new(Slots)),
        );
        assert_eq!(host.controls.knobs()[0].label, "Brightness");
        assert_eq!(
            host.controls.selected_driver(),
            LamathStringedDriverId::Pick
        );
        assert_eq!(host.articulations.surface.slot_list_view().slots.len(), 1);
    }
}
