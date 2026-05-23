use std::path::Path;

pub mod glirdir_vizia;
pub mod resonator_vizia;

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
    pub fn dispatch(&mut self, command: UiCommand) -> EditorCommandDispatch {
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
        EditorCommandDispatch {
            command,
            selected_slot: self.selected_slot,
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCommandDispatch {
    pub command: UiCommand,
    pub selected_slot: Option<PadId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditorCommandBus {
    state: UiCommandState,
}

impl EditorCommandBus {
    pub fn dispatch(&mut self, command: UiCommand) -> EditorCommandDispatch {
        self.state.dispatch(command)
    }

    pub const fn selected_slot(&self) -> Option<PadId> {
        self.state.selected_slot
    }

    pub const fn last_command(&self) -> Option<UiCommand> {
        self.state.last_command
    }

    pub const fn state(&self) -> &UiCommandState {
        &self.state
    }
}

pub trait PatchIoService {
    type Error;

    fn save_patch(&mut self, path: &Path) -> Result<(), Self::Error>;
    fn load_patch(&mut self, path: &Path) -> Result<(), Self::Error>;
    fn export_patch_with_samples(&mut self, directory: &Path) -> Result<(), Self::Error>;
}

pub trait SampleSlotService {
    type SampleReference;
    type Error;

    fn refresh_library(&mut self) -> Result<(), Self::Error>;
    fn ingest_sample(&mut self, path: &Path) -> Result<Self::SampleReference, Self::Error>;
    fn assign_library_sample_to_slot(
        &mut self,
        sample_index: usize,
        slot: PadId,
    ) -> Result<(), Self::Error>;
    fn assign_sample_to_slot(
        &mut self,
        reference: Self::SampleReference,
        slot: PadId,
    ) -> Result<(), Self::Error>;
    fn clear_slot(&mut self, slot: PadId) -> Result<(), Self::Error>;
}

pub trait TelemetryRequestService {
    type Error;

    fn request_telemetry(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EditorCommandContext<'a> {
    pub patch_save_path: Option<&'a Path>,
    pub patch_load_path: Option<&'a Path>,
    pub patch_export_directory: Option<&'a Path>,
    pub sample_path: Option<&'a Path>,
    pub selected_library_sample: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommandOutcome {
    Ignored,
    PatchSaved,
    PatchLoaded,
    PatchExported,
    LibraryRefreshed,
    SampleIngested,
    SlotAssigned,
    SlotCleared,
    TelemetryRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommandError<PatchError, SampleError> {
    Patch(PatchError),
    Sample(SampleError),
}

pub struct EditorCommandHandler;

impl EditorCommandHandler {
    pub fn handle<P, S>(
        command: UiCommand,
        context: EditorCommandContext<'_>,
        patch_io: &mut P,
        sample_slots: &mut S,
    ) -> Result<EditorCommandOutcome, EditorCommandError<P::Error, S::Error>>
    where
        P: PatchIoService,
        S: SampleSlotService,
    {
        match command {
            UiCommand::SavePatch => {
                let Some(path) = context.patch_save_path else {
                    return Ok(EditorCommandOutcome::Ignored);
                };
                patch_io
                    .save_patch(path)
                    .map(|()| EditorCommandOutcome::PatchSaved)
                    .map_err(EditorCommandError::Patch)
            }
            UiCommand::LoadPatch => {
                let Some(path) = context.patch_load_path else {
                    return Ok(EditorCommandOutcome::Ignored);
                };
                patch_io
                    .load_patch(path)
                    .map(|()| EditorCommandOutcome::PatchLoaded)
                    .map_err(EditorCommandError::Patch)
            }
            UiCommand::ExportPatchWithSamples => {
                let Some(directory) = context.patch_export_directory else {
                    return Ok(EditorCommandOutcome::Ignored);
                };
                patch_io
                    .export_patch_with_samples(directory)
                    .map(|()| EditorCommandOutcome::PatchExported)
                    .map_err(EditorCommandError::Patch)
            }
            UiCommand::OpenLibrary => {
                if let Some(path) = context.sample_path {
                    sample_slots
                        .ingest_sample(path)
                        .map(|_| EditorCommandOutcome::SampleIngested)
                        .map_err(EditorCommandError::Sample)
                } else {
                    sample_slots
                        .refresh_library()
                        .map(|()| EditorCommandOutcome::LibraryRefreshed)
                        .map_err(EditorCommandError::Sample)
                }
            }
            UiCommand::LoadExcitationSlot(slot) => {
                load_excitation_slot::<P::Error, S>(slot, context, sample_slots)
            }
            UiCommand::ClearExcitationSlot(slot) => sample_slots
                .clear_slot(slot)
                .map(|()| EditorCommandOutcome::SlotCleared)
                .map_err(EditorCommandError::Sample),
            UiCommand::SelectExcitationSlot(_)
            | UiCommand::LoadSelectedExcitationSlot
            | UiCommand::ClearSelectedExcitationSlot
            | UiCommand::RedetectSlices
            | UiCommand::TuneSelectedSlice
            | UiCommand::TuneAllSlices
            | UiCommand::SnapAllSlicesToScale => Ok(EditorCommandOutcome::Ignored),
        }
    }

    pub fn request_telemetry<T>(telemetry: &mut T) -> Result<EditorCommandOutcome, T::Error>
    where
        T: TelemetryRequestService,
    {
        telemetry
            .request_telemetry()
            .map(|()| EditorCommandOutcome::TelemetryRequested)
    }
}

fn load_excitation_slot<PatchError, S>(
    slot: PadId,
    context: EditorCommandContext<'_>,
    sample_slots: &mut S,
) -> Result<EditorCommandOutcome, EditorCommandError<PatchError, S::Error>>
where
    S: SampleSlotService,
{
    if let Some(sample_index) = context.selected_library_sample {
        match sample_slots.assign_library_sample_to_slot(sample_index, slot) {
            Ok(()) => return Ok(EditorCommandOutcome::SlotAssigned),
            Err(error) if context.sample_path.is_none() => {
                return Err(EditorCommandError::Sample(error));
            }
            Err(_) => {}
        }
    }

    let Some(path) = context.sample_path else {
        return Ok(EditorCommandOutcome::Ignored);
    };

    let reference = sample_slots
        .ingest_sample(path)
        .map_err(EditorCommandError::Sample)?;
    sample_slots
        .assign_sample_to_slot(reference, slot)
        .map(|()| EditorCommandOutcome::SlotAssigned)
        .map_err(EditorCommandError::Sample)
}

pub struct EditorCommandFloatAdapter;

impl EditorCommandFloatAdapter {
    const NONE: u16 = 0;
    const SAVE_PATCH: u16 = 1;
    const LOAD_PATCH: u16 = 2;
    const EXPORT_PATCH_WITH_SAMPLES: u16 = 3;
    const OPEN_LIBRARY: u16 = 4;
    const LOAD_SELECTED_EXCITATION_SLOT: u16 = 5;
    const CLEAR_SELECTED_EXCITATION_SLOT: u16 = 6;
    const REDETECT_SLICES: u16 = 40;
    const TUNE_SELECTED_SLICE: u16 = 41;
    const TUNE_ALL_SLICES: u16 = 42;
    const SNAP_ALL_SLICES_TO_SCALE: u16 = 43;
    const SELECT_EXCITATION_SLOT_BASE: u16 = 100;
    const LOAD_EXCITATION_SLOT_BASE: u16 = 200;
    const CLEAR_EXCITATION_SLOT_BASE: u16 = 300;

    pub fn encode(command: Option<UiCommand>) -> f32 {
        f32::from(match command {
            Some(UiCommand::SavePatch) => Self::SAVE_PATCH,
            Some(UiCommand::LoadPatch) => Self::LOAD_PATCH,
            Some(UiCommand::ExportPatchWithSamples) => Self::EXPORT_PATCH_WITH_SAMPLES,
            Some(UiCommand::OpenLibrary) => Self::OPEN_LIBRARY,
            Some(UiCommand::LoadSelectedExcitationSlot) => Self::LOAD_SELECTED_EXCITATION_SLOT,
            Some(UiCommand::ClearSelectedExcitationSlot) => Self::CLEAR_SELECTED_EXCITATION_SLOT,
            Some(UiCommand::SelectExcitationSlot(slot)) => {
                Self::SELECT_EXCITATION_SLOT_BASE + u16::from(slot.0)
            }
            Some(UiCommand::LoadExcitationSlot(slot)) => {
                Self::LOAD_EXCITATION_SLOT_BASE + u16::from(slot.0)
            }
            Some(UiCommand::ClearExcitationSlot(slot)) => {
                Self::CLEAR_EXCITATION_SLOT_BASE + u16::from(slot.0)
            }
            Some(UiCommand::RedetectSlices) => Self::REDETECT_SLICES,
            Some(UiCommand::TuneSelectedSlice) => Self::TUNE_SELECTED_SLICE,
            Some(UiCommand::TuneAllSlices) => Self::TUNE_ALL_SLICES,
            Some(UiCommand::SnapAllSlicesToScale) => Self::SNAP_ALL_SLICES_TO_SCALE,
            None => Self::NONE,
        })
    }

    pub fn decode(payload: f32) -> Option<UiCommand> {
        let code = integral_code(payload)?;
        match code {
            Self::NONE => None,
            Self::SAVE_PATCH => Some(UiCommand::SavePatch),
            Self::LOAD_PATCH => Some(UiCommand::LoadPatch),
            Self::EXPORT_PATCH_WITH_SAMPLES => Some(UiCommand::ExportPatchWithSamples),
            Self::OPEN_LIBRARY => Some(UiCommand::OpenLibrary),
            Self::LOAD_SELECTED_EXCITATION_SLOT => Some(UiCommand::LoadSelectedExcitationSlot),
            Self::CLEAR_SELECTED_EXCITATION_SLOT => Some(UiCommand::ClearSelectedExcitationSlot),
            Self::REDETECT_SLICES => Some(UiCommand::RedetectSlices),
            Self::TUNE_SELECTED_SLICE => Some(UiCommand::TuneSelectedSlice),
            Self::TUNE_ALL_SLICES => Some(UiCommand::TuneAllSlices),
            Self::SNAP_ALL_SLICES_TO_SCALE => Some(UiCommand::SnapAllSlicesToScale),
            code => decode_slot_command(code),
        }
    }
}

fn integral_code(payload: f32) -> Option<u16> {
    if !payload.is_finite() {
        return None;
    }

    let rounded = payload.round();
    if (payload - rounded).abs() > f32::EPSILON || !(0.0..=f32::from(u16::MAX)).contains(&rounded) {
        return None;
    }

    Some(rounded as u16)
}

fn decode_slot_command(code: u16) -> Option<UiCommand> {
    decode_slot(code, EditorCommandFloatAdapter::SELECT_EXCITATION_SLOT_BASE)
        .map(UiCommand::SelectExcitationSlot)
        .or_else(|| {
            decode_slot(code, EditorCommandFloatAdapter::LOAD_EXCITATION_SLOT_BASE)
                .map(UiCommand::LoadExcitationSlot)
        })
        .or_else(|| {
            decode_slot(code, EditorCommandFloatAdapter::CLEAR_EXCITATION_SLOT_BASE)
                .map(UiCommand::ClearExcitationSlot)
        })
}

fn decode_slot(code: u16, base: u16) -> Option<PadId> {
    let index = code.checked_sub(base)?;
    let index = u8::try_from(index).ok()?;
    PadId::new(index)
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
mod tests;
