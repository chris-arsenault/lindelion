use serde::{Deserialize, Serialize};

use super::{FIRST_PAD_MIDI_NOTE, LinnodPatch, PadEdit, SLICE_COUNT};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadAssignment {
    pub pad: PadId,
    pub slice_index: usize,
    pub midi_note: u8,
    #[serde(default)]
    pub choke_group: Option<ChokeGroupId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PadId(pub u8);

impl PadId {
    pub const fn new(index: u8) -> Option<Self> {
        if index >= 1 && index <= SLICE_COUNT as u8 {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn index(self) -> usize {
        self.0.saturating_sub(1) as usize
    }

    pub const fn sanitized(self) -> Self {
        if self.0 < 1 {
            Self(1)
        } else if self.0 > SLICE_COUNT as u8 {
            Self(SLICE_COUNT as u8)
        } else {
            self
        }
    }
}

impl Default for PadId {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChokeGroupId(pub u8);

impl ChokeGroupId {
    pub const fn new(index: u8) -> Option<Self> {
        if index >= 1 && index <= SLICE_COUNT as u8 {
            Some(Self(index))
        } else {
            None
        }
    }

    pub const fn sanitized(self) -> Self {
        if self.0 < 1 {
            Self(1)
        } else if self.0 > SLICE_COUNT as u8 {
            Self(SLICE_COUNT as u8)
        } else {
            self
        }
    }
}

pub fn default_pad_assignments() -> Vec<PadAssignment> {
    (1..=SLICE_COUNT)
        .map(|index| PadAssignment {
            pad: PadId(index as u8),
            slice_index: index - 1,
            midi_note: FIRST_PAD_MIDI_NOTE + index as u8 - 1,
            choke_group: None,
        })
        .collect()
}

pub fn normalize_pad_assignments(input: &[PadAssignment]) -> Vec<PadAssignment> {
    let mut pad_map = default_pad_assignments();
    for assignment in input {
        let pad = assignment.pad.sanitized();
        let index = pad.index();
        pad_map[index] = PadAssignment {
            pad,
            slice_index: assignment.slice_index.min(SLICE_COUNT - 1),
            midi_note: assignment.midi_note,
            choke_group: assignment.choke_group.map(ChokeGroupId::sanitized),
        };
    }
    pad_map
}

pub fn pad_assignment_for_note(pad_map: &[PadAssignment], note: u8) -> Option<PadAssignment> {
    pad_map
        .iter()
        .find(|assignment| assignment.midi_note == note)
        .map(normalize_pad_assignment)
}

pub fn slice_index_for_pad(pad_map: &[PadAssignment], pad: PadId) -> Option<usize> {
    let pad = pad.sanitized();
    pad_map
        .iter()
        .find(|assignment| assignment.pad.sanitized() == pad)
        .map(|assignment| assignment.slice_index.min(SLICE_COUNT - 1))
        .or_else(|| {
            default_pad_assignments()
                .get(pad.index())
                .map(|assignment| assignment.slice_index)
        })
}

impl LinnodPatch {
    pub fn apply_pad_edit(&mut self, pad: PadId, edit: PadEdit) -> bool {
        let pad = pad.sanitized();
        self.normalize_layout();
        let Some(assignment) = self
            .pad_map
            .iter_mut()
            .find(|assignment| assignment.pad.sanitized() == pad)
        else {
            return false;
        };
        match edit {
            PadEdit::ChokeGroup(group) => {
                assignment.choke_group = group.map(ChokeGroupId::sanitized);
            }
        }
        true
    }
}

fn normalize_pad_assignment(assignment: &PadAssignment) -> PadAssignment {
    PadAssignment {
        pad: assignment.pad.sanitized(),
        slice_index: assignment.slice_index.min(SLICE_COUNT - 1),
        midi_note: assignment.midi_note,
        choke_group: assignment.choke_group.map(ChokeGroupId::sanitized),
    }
}
