use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr, slice,
};

use lindelion_plugin_shell::vst3::{copy_wstring, len_wstring};
use lindelion_sample_library::{
    FileSampleLibrary, LibraryPaths, SampleLibrary, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    PARAMETERS, ResonatorSynthPatch, ResonatorTelemetry, parameter_binding,
    parameter_binding_by_index, parameter_binding_index, patch_io, patch_parameter_plain_value,
};

use super::{
    DEFAULT_LIBRARY_DIR, DEFAULT_PITCH_BEND_RANGE_SEMITONES, PITCH_BEND_PARAMETER_ID,
    PITCH_BEND_PARAMETER_INDEX, ResonatorPluginMessage, VST3_PARAMETER_COUNT, editor,
    read_plugin_state_from_stream,
};

pub(super) struct ResonatorVst3Controller {
    pub(super) values: Cell<[f64; VST3_PARAMETER_COUNT]>,
    pub(super) handler: Cell<*mut IComponentHandler>,
    pub(super) editor_summary: RefCell<EditorPatchSummary>,
    pub(super) patch: RefCell<ResonatorSynthPatch>,
    peer: Cell<*mut IConnectionPoint>,
    telemetry: Cell<EditorTelemetry>,
    library_samples: RefCell<Vec<SampleMetadata>>,
}

impl Class for ResonatorVst3Controller {
    type Interfaces = (IEditController, IMidiMapping, IConnectionPoint);
}

impl ResonatorVst3Controller {
    pub(super) const CID: TUID = uid(0x15C8B012, 0xF4B64F5E, 0x93D9AA38, 0x69383E3B);

    pub(super) fn new() -> Self {
        Self {
            values: Cell::new(default_parameter_values()),
            handler: Cell::new(ptr::null_mut()),
            editor_summary: RefCell::new(EditorPatchSummary::from_patch(
                &crate::ResonatorSynthPatch::default(),
            )),
            patch: RefCell::new(ResonatorSynthPatch::default()),
            peer: Cell::new(ptr::null_mut()),
            telemetry: Cell::new(EditorTelemetry::default()),
            library_samples: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameter_index(id) else {
            return kInvalidArgument;
        };
        let mut values = self.values.get();
        values[index] = if normalized.is_finite() {
            normalized.clamp(0.0, 1.0)
        } else {
            default_parameter_values()[index]
        };
        self.values.set(values);
        if id != PITCH_BEND_PARAMETER_ID
            && let Some(parameter) = parameter_by_id(id)
        {
            let plain = parameter.range.denormalize(values[index] as f32);
            crate::apply_parameter_plain_for_controller(&mut self.patch.borrow_mut(), id, plain);
            self.editor_summary
                .replace(EditorPatchSummary::from_patch_and_library(
                    &self.patch.borrow(),
                    &self.library_samples.borrow(),
                ));
        }
        kResultOk
    }

    pub(super) fn editor_summary(&self) -> EditorPatchSummary {
        self.editor_summary.borrow().clone()
    }

    pub(super) fn telemetry(&self) -> EditorTelemetry {
        self.telemetry.get()
    }

    fn replace_patch_mirror(&self, patch: ResonatorSynthPatch) {
        self.values.set(parameter_values_from_patch(&patch));
        self.patch.replace(patch);
        self.editor_summary
            .replace(EditorPatchSummary::from_patch_and_library(
                &self.patch.borrow(),
                &self.library_samples.borrow(),
            ));
    }

    fn notify_parameter_values_changed(&self) {
        let Some(handler) = (unsafe { ComRef::from_raw(self.handler.get()) }) else {
            return;
        };
        unsafe {
            handler.restartComponent(RestartFlags_::kParamValuesChanged);
        }
    }

    fn send_patch_to_processor(&self) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultFalse;
        };
        let payload_patch = processor_patch_from_controller_patch(&self.patch.borrow());
        let Ok(payload) = patch_io::to_toml_string(&payload_patch) else {
            return kResultFalse;
        };
        let message = ResonatorPluginMessage::patch_update(payload.into_bytes()).into_com_message();
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }

    pub(super) fn request_telemetry(&self) {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return;
        };
        let message = ResonatorPluginMessage::telemetry_request().into_com_message();
        if let Some(message) = message.to_com_ptr::<IMessage>() {
            unsafe {
                peer.notify(message.as_ptr());
            }
        }
    }

    pub(super) fn save_patch_to_path(&self, path: &Path) -> Result<(), patch_io::PatchIoError> {
        patch_io::save_patch(path, &self.patch.borrow())
    }

    pub(super) fn load_patch_from_path(
        &self,
        path: &Path,
    ) -> Result<tresult, patch_io::PatchIoError> {
        let mut patch = patch_io::load_patch(path)?;
        resolve_patch_samples_for_loaded_path(&mut patch, path);
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        Ok(self.send_patch_to_processor())
    }

    pub(super) fn export_patch_bundle(&self, directory: &Path) -> io::Result<PathBuf> {
        export_patch_bundle(directory, &self.patch.borrow())
    }

    pub(super) fn refresh_library(&self) -> io::Result<()> {
        let samples = open_default_sample_library()
            .map_err(io::Error::other)?
            .list_samples()
            .map_err(io::Error::other)?;
        self.library_samples.replace(samples);
        self.editor_summary
            .replace(EditorPatchSummary::from_patch_and_library(
                &self.patch.borrow(),
                &self.library_samples.borrow(),
            ));
        Ok(())
    }

    pub(super) fn ingest_sample(&self, path: PathBuf) -> io::Result<SampleReference> {
        let mut library = open_default_sample_library().map_err(io::Error::other)?;
        let metadata = library.ingest(path).map_err(io::Error::other)?;
        let reference = metadata.reference.clone();
        self.refresh_library()?;
        Ok(reference)
    }

    pub(super) fn assign_library_sample_to_slot(
        &self,
        sample_index: usize,
        slot_index: usize,
    ) -> tresult {
        let Some(metadata) = self.library_samples.borrow().get(sample_index).cloned() else {
            return kInvalidArgument;
        };
        self.assign_sample_reference_to_slot(metadata.reference, slot_index)
    }

    pub(super) fn assign_sample_reference_to_slot(
        &self,
        reference: SampleReference,
        slot_index: usize,
    ) -> tresult {
        let mut patch = self.patch.borrow().clone();
        ensure_excitation_slot(&mut patch, slot_index);
        if let Some(slot) = patch.excitation_slots.get_mut(slot_index) {
            slot.sample = Some(reference);
        }
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        self.send_patch_to_processor()
    }

    pub(super) fn clear_slot(&self, slot_index: usize) -> tresult {
        let mut patch = self.patch.borrow().clone();
        ensure_excitation_slot(&mut patch, slot_index);
        if let Some(slot) = patch.excitation_slots.get_mut(slot_index) {
            slot.sample = None;
        }
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        self.send_patch_to_processor()
    }
}

impl IConnectionPointTrait for ResonatorVst3Controller {
    unsafe fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.set(other);
        kResultOk
    }

    unsafe fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        if self.peer.get() == other {
            self.peer.set(ptr::null_mut());
        }
        kResultOk
    }

    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        let message = match ResonatorPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };
        match message {
            ResonatorPluginMessage::TelemetryResponse(payload) => {
                let Some(telemetry) = decode_telemetry(&payload) else {
                    return kResultFalse;
                };
                self.telemetry.set(telemetry);
                kResultOk
            }
            ResonatorPluginMessage::PatchUpdate(_) | ResonatorPluginMessage::TelemetryRequest => {
                kNotImplemented
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct EditorTelemetry {
    pub(super) left_peak: f32,
    pub(super) right_peak: f32,
    pub(super) left_rms: f32,
    pub(super) right_rms: f32,
    pub(super) active_voices: f32,
}

impl From<ResonatorTelemetry> for EditorTelemetry {
    fn from(value: ResonatorTelemetry) -> Self {
        Self {
            left_peak: value.left_peak,
            right_peak: value.right_peak,
            left_rms: value.left_rms,
            right_rms: value.right_rms,
            active_voices: value.active_voices as f32,
        }
    }
}

pub(super) fn encode_telemetry(telemetry: ResonatorTelemetry) -> String {
    format!(
        "{:.8},{:.8},{:.8},{:.8},{}",
        telemetry.left_peak,
        telemetry.right_peak,
        telemetry.left_rms,
        telemetry.right_rms,
        telemetry.active_voices
    )
}

pub(super) fn decode_telemetry(payload: &[u8]) -> Option<EditorTelemetry> {
    let text = std::str::from_utf8(payload).ok()?;
    let mut parts = text.split(',');
    let left_peak = finite_telemetry(parts.next()?.parse().ok()?);
    let right_peak = finite_telemetry(parts.next()?.parse().ok()?);
    let left_rms = finite_telemetry(parts.next()?.parse().ok()?);
    let right_rms = finite_telemetry(parts.next()?.parse().ok()?);
    let active_voices = parts.next()?.parse::<f32>().ok()?.clamp(0.0, 64.0);
    if parts.next().is_some() {
        return None;
    }
    Some(EditorTelemetry {
        left_peak,
        right_peak,
        left_rms,
        right_rms,
        active_voices,
    })
}

fn finite_telemetry(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 64.0)
    } else {
        0.0
    }
}

pub(super) fn default_library_paths() -> LibraryPaths {
    LibraryPaths::from_root(default_library_root())
}

fn default_library_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Music")
        .join(DEFAULT_LIBRARY_DIR)
}

fn open_default_sample_library()
-> Result<FileSampleLibrary, lindelion_sample_library::SampleLibraryError> {
    FileSampleLibrary::open(default_library_paths())
}

fn ensure_excitation_slot(patch: &mut ResonatorSynthPatch, slot_index: usize) {
    let target_len = (slot_index + 1).min(crate::dsp::MAX_EXCITATION_LAYERS);
    while patch.excitation_slots.len() < target_len {
        patch
            .excitation_slots
            .push(crate::ExcitationSlot::default());
    }
}

fn processor_patch_from_controller_patch(patch: &ResonatorSynthPatch) -> ResonatorSynthPatch {
    let mut patch = patch.clone();
    let paths = default_library_paths();
    for slot in &mut patch.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        if reference.last_known_path.is_relative() {
            let candidate = paths.root.join(&reference.last_known_path);
            if candidate.exists() {
                reference.last_known_path = candidate;
            }
        }
    }
    patch
}

fn resolve_patch_samples_for_loaded_path(patch: &mut ResonatorSynthPatch, patch_path: &Path) {
    let Some(patch_dir) = patch_path.parent() else {
        return;
    };
    let default_root = default_library_root();
    for slot in &mut patch.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        if reference.last_known_path.is_absolute() {
            continue;
        }
        let relative = reference.last_known_path.clone();
        for candidate in [patch_dir.join(&relative), default_root.join(&relative)] {
            if candidate.exists() {
                reference.last_known_path = candidate;
                break;
            }
        }
    }
}

fn export_patch_bundle(directory: &Path, patch: &ResonatorSynthPatch) -> io::Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let samples_dir = directory.join("Samples");
    fs::create_dir_all(&samples_dir)?;

    let mut exported = patch.clone();
    let default_root = default_library_root();
    for slot in &mut exported.excitation_slots {
        let Some(reference) = slot.sample.as_mut() else {
            continue;
        };
        let source = if reference.last_known_path.is_absolute() {
            reference.last_known_path.clone()
        } else {
            default_root.join(&reference.last_known_path)
        };
        if !source.is_file() {
            continue;
        }
        let filename = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("sample.wav");
        let target = samples_dir.join(filename);
        fs::copy(&source, &target)?;
        reference.last_known_path = PathBuf::from("Samples").join(filename);
    }

    let patch_path = directory.join(format!("{}.toml", sanitize_patch_filename(&exported.name)));
    patch_io::save_patch(&patch_path, &exported)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    Ok(patch_path)
}

fn sanitize_patch_filename(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => '-',
            character => character,
        })
        .collect::<String>()
        .trim()
        .to_string();
    if sanitized.is_empty() {
        "Untitled".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EditorPatchSummary {
    pub(super) patch_name: String,
    pub(super) slots: [EditorSlotSummary; 4],
    pub(super) library_samples: Vec<EditorSampleSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EditorSampleSummary {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) preview: Vec<EditorWaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct EditorWaveformPoint {
    pub(super) min: f32,
    pub(super) max: f32,
    pub(super) rms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EditorSlotSummary {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) sample_backed: bool,
    pub(super) pitch_track: bool,
    pub(super) looping: bool,
}

impl EditorPatchSummary {
    pub(super) fn from_patch(patch: &crate::ResonatorSynthPatch) -> Self {
        Self::from_patch_and_library(patch, &[])
    }

    fn from_patch_and_library(
        patch: &crate::ResonatorSynthPatch,
        samples: &[SampleMetadata],
    ) -> Self {
        Self {
            patch_name: patch.name.clone(),
            slots: std::array::from_fn(|index| {
                let slot = patch.excitation_slots.get(index);
                EditorSlotSummary::from_slot(index, slot)
            }),
            library_samples: samples
                .iter()
                .map(EditorSampleSummary::from_metadata)
                .collect(),
        }
    }
}

impl EditorSampleSummary {
    fn from_metadata(metadata: &SampleMetadata) -> Self {
        let duration = metadata.duration_ms as f32 / 1_000.0;
        let detail = format!(
            "{duration:.2}s  {} Hz  {}ch",
            metadata.sample_rate, metadata.channels
        );
        Self {
            label: metadata.filename.clone(),
            detail,
            preview: waveform_points(&metadata.waveform_preview),
        }
    }
}

fn waveform_points(preview: &SampleWaveformPreview) -> Vec<EditorWaveformPoint> {
    preview
        .points
        .iter()
        .map(|point| EditorWaveformPoint {
            min: point.min,
            max: point.max,
            rms: point.rms,
        })
        .collect()
}

impl EditorSlotSummary {
    fn from_slot(index: usize, slot: Option<&crate::ExcitationSlot>) -> Self {
        let Some(slot) = slot else {
            return Self::empty(index);
        };

        let Some(reference) = slot.sample.as_ref() else {
            return if index == 0 {
                Self::builtin(slot)
            } else {
                Self::empty(index)
            };
        };

        let filename = reference
            .last_known_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Sample");
        Self {
            label: format!("Slot {}", index + 1),
            detail: filename.to_string(),
            sample_backed: true,
            pitch_track: slot.pitch_track,
            looping: slot.looping,
        }
    }

    fn builtin(slot: &crate::ExcitationSlot) -> Self {
        Self {
            label: "Slot 1".to_string(),
            detail: "Built-in pluck".to_string(),
            sample_backed: false,
            pitch_track: slot.pitch_track,
            looping: slot.looping,
        }
    }

    fn empty(index: usize) -> Self {
        Self {
            label: format!("Slot {}", index + 1),
            detail: "Empty layer".to_string(),
            sample_backed: false,
            pitch_track: false,
            looping: false,
        }
    }
}

impl IPluginBaseTrait for ResonatorVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for ResonatorVst3Controller {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultFalse;
        };
        let Ok(payload) = std::str::from_utf8(&plugin_state.payload) else {
            return kResultFalse;
        };
        let Ok(patch) = patch_io::from_toml_str(payload) else {
            return kResultFalse;
        };

        self.replace_patch_mirror(patch);
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        VST3_PARAMETER_COUNT as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;
        if param_index as usize == PITCH_BEND_PARAMETER_INDEX {
            info.id = PITCH_BEND_PARAMETER_ID;
            copy_wstring("Pitch Bend", &mut info.title);
            copy_wstring("Pitch", &mut info.shortTitle);
            copy_wstring("st", &mut info.units);
            info.stepCount = 0;
            info.defaultNormalizedValue = 0.5;
            info.unitId = 0;
            info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate
                | ParameterInfo_::ParameterFlags_::kIsHidden;
            return kResultOk;
        }

        let Some(binding) = parameter_binding_by_index(param_index as usize) else {
            return kInvalidArgument;
        };
        let parameter = binding.info();

        info.id = parameter.id.0;
        copy_wstring(parameter.name, &mut info.title);
        copy_wstring(parameter.name, &mut info.shortTitle);
        copy_wstring(parameter.units, &mut info.units);
        info.stepCount = parameter.step_count.map_or(0, |steps| steps as i32);
        info.defaultNormalizedValue = parameter.range.normalize(parameter.range.default) as f64;
        info.unitId = 0;
        info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate;
        kResultOk
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value_normalized: f64,
        string: *mut String128,
    ) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        if id == PITCH_BEND_PARAMETER_ID {
            copy_wstring(
                &format_plain_value(pitch_bend_plain_from_normalized(value_normalized) as f32),
                &mut *string,
            );
            return kResultOk;
        }

        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        copy_wstring(&format_parameter_plain_value(id, plain), &mut *string);
        kResultOk
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value_normalized: *mut f64,
    ) -> tresult {
        if string.is_null() || value_normalized.is_null() {
            return kInvalidArgument;
        }
        let len = len_wstring(string as *const TChar);
        let Ok(text) = String::from_utf16(slice::from_raw_parts(string as *const u16, len)) else {
            return kInvalidArgument;
        };
        let Ok(value) = text.trim().parse::<f32>() else {
            return kInvalidArgument;
        };
        if id == PITCH_BEND_PARAMETER_ID {
            *value_normalized = pitch_bend_normalized_from_plain(value);
            return kResultOk;
        }

        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        *value_normalized = parameter.range.normalize(value) as f64;
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        if id == PITCH_BEND_PARAMETER_ID {
            return pitch_bend_plain_from_normalized(value_normalized);
        }

        parameter_by_id(id)
            .map(|parameter| parameter.range.denormalize(value_normalized as f32) as f64)
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        if id == PITCH_BEND_PARAMETER_ID {
            return pitch_bend_normalized_from_plain(plain_value as f32);
        }

        normalized_parameter_value(id, plain_value as f32)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        let Some(index) = parameter_index(id) else {
            return 0.0;
        };
        self.values.get()[index]
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        self.set_value(id, value)
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        self.handler.set(handler);
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        editor::create_editor_view(self)
    }
}

impl IMidiMappingTrait for ResonatorVst3Controller {
    unsafe fn getMidiControllerAssignment(
        &self,
        busIndex: i32,
        channel: i16,
        midiControllerNumber: CtrlNumber,
        id: *mut u32,
    ) -> tresult {
        if id.is_null() {
            return kInvalidArgument;
        }

        if busIndex == 0
            && (0..=15).contains(&channel)
            && midiControllerNumber == ControllerNumbers_::kPitchBend as CtrlNumber
        {
            *id = PITCH_BEND_PARAMETER_ID;
            return kResultTrue;
        }

        kResultFalse
    }
}

pub(super) fn default_parameter_values() -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = [0.0; VST3_PARAMETER_COUNT];
    for (index, parameter) in PARAMETERS.iter().enumerate() {
        values[index] = parameter.range.normalize(parameter.range.default) as f64;
    }
    values[PITCH_BEND_PARAMETER_INDEX] = 0.5;
    values
}

pub(super) fn parameter_values_from_patch(
    patch: &crate::ResonatorSynthPatch,
) -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = default_parameter_values();
    for binding in (0..PARAMETERS.len()).filter_map(parameter_binding_by_index) {
        let parameter = binding.info();
        if let Some(plain) = patch_parameter_plain_value(patch, parameter.id.0)
            && let Some(index) = parameter_index(parameter.id.0)
        {
            values[index] = parameter.range.normalize(plain) as f64;
        }
    }
    values
}

pub(super) fn parameter_index(id: u32) -> Option<usize> {
    if id == PITCH_BEND_PARAMETER_ID {
        return Some(PITCH_BEND_PARAMETER_INDEX);
    }

    parameter_binding_index(id)
}

fn parameter_by_id(id: u32) -> Option<&'static lindelion_plugin_shell::ParameterInfo> {
    parameter_binding(id).map(|binding| {
        let info = binding.info();
        PARAMETERS
            .iter()
            .find(|parameter| parameter.id == info.id)
            .expect("binding info should be mirrored in PARAMETERS")
    })
}

pub(super) fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    parameter_binding(id)
        .map(|binding| binding.info().range.normalize(plain) as f64)
        .unwrap_or(0.0)
}

pub(super) fn pitch_bend_plain_from_normalized(normalized: f64) -> f64 {
    let normalized = if normalized.is_finite() {
        normalized
    } else {
        0.5
    };
    (normalized.clamp(0.0, 1.0) * 2.0 - 1.0) * f64::from(DEFAULT_PITCH_BEND_RANGE_SEMITONES)
}

pub(super) fn pitch_bend_normalized_from_plain(plain: f32) -> f64 {
    let plain = plain.clamp(
        -DEFAULT_PITCH_BEND_RANGE_SEMITONES,
        DEFAULT_PITCH_BEND_RANGE_SEMITONES,
    );
    f64::from((plain / DEFAULT_PITCH_BEND_RANGE_SEMITONES + 1.0) * 0.5)
}

pub(super) fn format_parameter_plain_value(parameter_id: u32, value: f32) -> String {
    parameter_binding(parameter_id)
        .map(|binding| binding.format_plain_value(value))
        .unwrap_or_else(|| format_plain_value(value))
}

fn format_plain_value(value: f32) -> String {
    if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}
