//! UI commands — every action the UI can issue. `apply` performs the framework-neutral **state** part
//! of a command; the Windows controller (M6 Step 7) performs the **effectful** part (start/stop the
//! engine, open an editor, save/load to disk + rebuild the chain) and drives the matching state.

use std::path::PathBuf;

use super::state::{Dir, HostUiState};
use crate::session::DeviceRef;

/// A command emitted by the UI that has a **framework-neutral state effect**. Purely effectful
/// actions (open editor, save/load session) are not modelled here — the controller performs those
/// directly, since they need the engine/disk and have no neutral state to apply.
#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    SelectInput(DeviceRef),
    SelectOutput(DeviceRef),
    AddPlugin(PathBuf),
    RemovePlugin(usize),
    MoveSlot(usize, Dir),
    ToggleBypass(usize),
    Start,
    Stop,
}

/// Apply the state part of `cmd` to `state`.
pub fn apply(state: &mut HostUiState, cmd: &UiCommand) {
    match cmd {
        UiCommand::SelectInput(device) => state.select_input(device.clone()),
        UiCommand::SelectOutput(device) => state.select_output(device.clone()),
        UiCommand::AddPlugin(path) => state.add_slot(path.clone()),
        UiCommand::RemovePlugin(index) => state.remove_slot(*index),
        UiCommand::MoveSlot(index, dir) => state.move_slot(*index, *dir),
        UiCommand::ToggleBypass(index) => state.toggle_bypass(*index),
        UiCommand::Start => state.running = true,
        UiCommand::Stop => state.running = false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_toggle_maps_to_chain() {
        let mut state = HostUiState::default();
        apply(
            &mut state,
            &UiCommand::AddPlugin(PathBuf::from("/p/A.vst3")),
        );
        apply(
            &mut state,
            &UiCommand::AddPlugin(PathBuf::from("/p/B.vst3")),
        );
        apply(&mut state, &UiCommand::ToggleBypass(0));

        assert_eq!(state.chain.len(), 2);
        assert!(state.chain[0].bypassed);
    }

    #[test]
    fn start_stop_set_running() {
        let mut state = HostUiState::default();
        apply(&mut state, &UiCommand::Start);
        assert!(state.running);
        apply(&mut state, &UiCommand::Stop);
        assert!(!state.running);
    }

    #[test]
    fn move_slot_reorders() {
        let mut state = HostUiState::default();
        apply(
            &mut state,
            &UiCommand::AddPlugin(PathBuf::from("/p/A.vst3")),
        );
        apply(
            &mut state,
            &UiCommand::AddPlugin(PathBuf::from("/p/B.vst3")),
        );
        apply(&mut state, &UiCommand::MoveSlot(0, Dir::Down));

        assert_eq!(state.chain[0].path, PathBuf::from("/p/B.vst3"));
    }
}
