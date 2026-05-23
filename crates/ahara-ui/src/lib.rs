#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveformPoint {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

impl WaveformPoint {
    pub fn silence() -> Self {
        Self {
            min: 0.0,
            max: 0.0,
            rms: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveformPreview {
    pub sample_rate: u32,
    pub points: Vec<WaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PadId(pub u8);

impl PadId {
    pub fn new(index: u8) -> Option<Self> {
        if (1..=16).contains(&index) {
            Some(Self(index))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    Pad,
    Chromatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    SavePatch,
    LoadPatch,
    ExportPatchWithSamples,
    OpenLibrary,
    SelectExcitationSlot(PadId),
    LoadSelectedExcitationSlot,
    LoadExcitationSlot(PadId),
    ClearSelectedExcitationSlot,
    ClearExcitationSlot(PadId),
    RedetectSlices,
    TuneSelectedSlice,
    TuneAllSlices,
    SnapAllSlicesToScale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiCommandState {
    pub selected_slot: Option<PadId>,
    pub last_command: Option<UiCommand>,
}

impl Default for UiCommandState {
    fn default() -> Self {
        Self {
            selected_slot: PadId::new(1),
            last_command: None,
        }
    }
}

impl UiCommandState {
    pub fn dispatch(&mut self, command: UiCommand) {
        let command = self.resolve(command);
        match command {
            UiCommand::SelectExcitationSlot(slot)
            | UiCommand::LoadExcitationSlot(slot)
            | UiCommand::ClearExcitationSlot(slot) => {
                self.selected_slot = Some(slot);
            }
            _ => {}
        }
        self.last_command = Some(command);
    }

    fn resolve(&self, command: UiCommand) -> UiCommand {
        match command {
            UiCommand::LoadSelectedExcitationSlot => {
                UiCommand::LoadExcitationSlot(self.selected_slot.unwrap_or(PadId(1)))
            }
            UiCommand::ClearSelectedExcitationSlot => {
                UiCommand::ClearExcitationSlot(self.selected_slot.unwrap_or(PadId(1)))
            }
            command => command,
        }
    }
}

pub fn command_label(command: Option<UiCommand>) -> &'static str {
    match command {
        Some(UiCommand::SavePatch) => "Patch save requested",
        Some(UiCommand::LoadPatch) => "Patch load requested",
        Some(UiCommand::ExportPatchWithSamples) => "Export requested",
        Some(UiCommand::OpenLibrary) => "Library requested",
        Some(UiCommand::SelectExcitationSlot(_)) => "Slot selected",
        Some(UiCommand::LoadSelectedExcitationSlot) => "Slot load requested",
        Some(UiCommand::LoadExcitationSlot(_)) => "Slot load requested",
        Some(UiCommand::ClearSelectedExcitationSlot) => "Slot clear requested",
        Some(UiCommand::ClearExcitationSlot(_)) => "Slot clear requested",
        Some(UiCommand::RedetectSlices) => "Slice detection requested",
        Some(UiCommand::TuneSelectedSlice) => "Slice tuning requested",
        Some(UiCommand::TuneAllSlices) => "Tune all requested",
        Some(UiCommand::SnapAllSlicesToScale) => "Scale snap requested",
        None => "Ready",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_state_tracks_selected_excitation_slot() {
        let mut state = UiCommandState::default();
        let slot = PadId::new(3).unwrap();

        state.dispatch(UiCommand::LoadExcitationSlot(slot));

        assert_eq!(state.selected_slot, Some(slot));
        assert_eq!(
            state.last_command,
            Some(UiCommand::LoadExcitationSlot(slot))
        );
        assert_eq!(command_label(state.last_command), "Slot load requested");
    }

    #[test]
    fn selected_slot_commands_resolve_to_current_slot() {
        let mut state = UiCommandState::default();
        let slot = PadId::new(4).unwrap();

        state.dispatch(UiCommand::SelectExcitationSlot(slot));
        state.dispatch(UiCommand::ClearSelectedExcitationSlot);

        assert_eq!(state.selected_slot, Some(slot));
        assert_eq!(
            state.last_command,
            Some(UiCommand::ClearExcitationSlot(slot))
        );
    }
}
