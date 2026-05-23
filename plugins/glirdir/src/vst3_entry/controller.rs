use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr, slice,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use lindelion_midi::{Scale, SnapMode, TimingGrid};
use lindelion_plugin_shell::vst3::{
    copy_wstring, len_wstring, read_plugin_state_from_stream, write_plugin_state_to_stream,
};
use lindelion_ui::glirdir_vizia::GlirdirEditorMidiDrag;
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    GlirdirPatch, PARAMETERS, apply_parameter_plain,
    midi_export::{MidiExportPayload, empty_midi_export},
    parameter_binding,
    patch::SyncMode,
    patch_io,
    sample_library::{SampleLibrarySavePayload, SampleLibrarySaveStatus},
};

use super::{GlirdirPluginMessage, GlirdirStatusPayload, VST3_PARAMETER_COUNT};

pub(super) const MAX_MIDI_DRAG_FILES: usize = 8;
static MIDI_DRAG_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct GlirdirVst3Controller {
    pub(super) values: Cell<[f64; VST3_PARAMETER_COUNT]>,
    pub(super) patch: RefCell<GlirdirPatch>,
    pub(super) status: Cell<GlirdirStatusPayload>,
    pub(super) sample_library_status: Cell<SampleLibrarySaveStatus>,
    pub(super) last_midi_export: RefCell<Option<MidiExportPayload>>,
    pub(super) midi_drag_files: RefCell<VecDeque<PathBuf>>,
    handler: Cell<*mut IComponentHandler>,
    peer: Cell<*mut IConnectionPoint>,
}

impl Class for GlirdirVst3Controller {
    type Interfaces = (IEditController, IConnectionPoint);
}

impl GlirdirVst3Controller {
    pub(super) const CID: TUID = uid(0x0D0466D2, 0x53E446E5, 0x8E90CF13, 0x25B5E241);

    pub(super) fn new() -> Self {
        Self {
            values: Cell::new(default_parameter_values()),
            patch: RefCell::new(GlirdirPatch::default()),
            status: Cell::new(GlirdirStatusPayload::default()),
            sample_library_status: Cell::new(SampleLibrarySaveStatus::Idle),
            last_midi_export: RefCell::new(None),
            midi_drag_files: RefCell::new(VecDeque::new()),
            handler: Cell::new(ptr::null_mut()),
            peer: Cell::new(ptr::null_mut()),
        }
    }

    pub(super) fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameter_index(id) else {
            return kInvalidArgument;
        };
        let mut values = self.values.get();
        values[index] = sanitize_normalized(normalized, default_parameter_values()[index]);
        self.values.set(values);

        if let Some(parameter) = parameter_by_id(id) {
            let plain = parameter.range.denormalize(values[index] as f32);
            apply_parameter_plain(&mut self.patch.borrow_mut(), id, plain);
            self.clear_midi_export_cache();
        }
        kResultOk
    }

    pub(super) fn request_arm_capture(&self) -> tresult {
        self.clear_midi_export_cache();
        self.notify_peer(GlirdirPluginMessage::arm_capture())
    }

    pub(super) fn request_clear_scratchpad(&self) -> tresult {
        self.clear_midi_export_cache();
        self.notify_peer(GlirdirPluginMessage::clear_scratchpad())
    }

    pub(super) fn request_finalize_completed_capture(&self) -> tresult {
        self.clear_midi_export_cache();
        self.notify_peer(GlirdirPluginMessage::finalize_capture_request())
    }

    pub(super) fn request_play_audition(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::play_audition())
    }

    pub(super) fn request_stop_audition(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::stop_audition())
    }

    pub(super) fn request_toggle_audition_loop(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::toggle_audition_loop())
    }

    pub(super) fn request_toggle_audition_live_edit(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::toggle_audition_live_edit())
    }

    pub(super) fn request_save_scratchpad_to_library(&self) -> tresult {
        self.sample_library_status
            .set(SampleLibrarySaveStatus::Saving);
        let result = self.notify_peer(GlirdirPluginMessage::sample_library_save_request());
        if result != kResultOk {
            self.sample_library_status
                .set(SampleLibrarySaveStatus::Error);
        }
        result
    }

    pub(super) fn request_status(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::status_request())
    }

    pub(super) fn request_telemetry(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::telemetry_request())
    }

    pub(super) fn request_midi_export(&self) -> tresult {
        self.notify_peer(GlirdirPluginMessage::midi_export_request())
    }

    pub(super) fn prepare_midi_drag_file(&self) -> GlirdirEditorMidiDrag {
        let _ = self.request_midi_export();
        let Some(export) = self.midi_export_for_drag() else {
            return GlirdirEditorMidiDrag::Requested;
        };
        match self.write_midi_drag_file(&export) {
            Ok(path) => GlirdirEditorMidiDrag::Ready { path },
            Err(_) => GlirdirEditorMidiDrag::Failed,
        }
    }

    pub(super) fn notify_parameter_edit(&self, id: u32, normalized: f64) {
        if let Some(handler) = unsafe { ComRef::from_raw(self.handler.get()) } {
            unsafe {
                handler.beginEdit(id);
                handler.performEdit(id, normalized);
                handler.endEdit(id);
            }
        }
    }

    pub(super) fn send_patch_to_processor(&self) -> tresult {
        let Ok(payload) = patch_io::to_toml_string(&self.patch.borrow()) else {
            return kResultFalse;
        };
        self.notify_peer(GlirdirPluginMessage::patch_update(payload.into_bytes()))
    }

    fn replace_patch_mirror(&self, patch: GlirdirPatch) {
        self.values.set(parameter_values_from_patch(&patch));
        self.patch.replace(patch);
        self.clear_midi_export_cache();
    }

    fn notify_parameter_values_changed(&self) {
        let Some(handler) = (unsafe { ComRef::from_raw(self.handler.get()) }) else {
            return;
        };
        unsafe {
            handler.restartComponent(RestartFlags_::kParamValuesChanged);
        }
    }

    fn notify_peer(&self, message: GlirdirPluginMessage) -> tresult {
        let Some(peer) = (unsafe { ComRef::from_raw(self.peer.get()) }) else {
            return kResultFalse;
        };
        let message = message.into_com_message();
        let Some(message) = message.to_com_ptr::<IMessage>() else {
            return kResultFalse;
        };
        unsafe { peer.notify(message.as_ptr()) }
    }

    fn midi_export_for_drag(&self) -> Option<MidiExportPayload> {
        if let Some(export) = self.last_midi_export.borrow().clone() {
            return Some(export);
        }
        (!self.status.get().has_analysis).then(|| empty_midi_export(&self.patch.borrow()))
    }

    fn write_midi_drag_file(&self, export: &MidiExportPayload) -> io::Result<PathBuf> {
        let dir = midi_drag_dir();
        fs::create_dir_all(&dir)?;
        let path = unique_midi_drag_path(dir, &export.file_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &export.bytes)?;
        self.remember_midi_drag_file(path.clone());
        Ok(path)
    }

    fn remember_midi_drag_file(&self, path: PathBuf) {
        let mut files = self.midi_drag_files.borrow_mut();
        files.push_back(path);
        while files.len() > MAX_MIDI_DRAG_FILES {
            if let Some(path) = files.pop_front() {
                remove_midi_drag_file(&path);
            }
        }
    }

    pub(super) fn cleanup_midi_drag_files(&self) {
        let files = self
            .midi_drag_files
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        for path in files {
            remove_midi_drag_file(&path);
        }
    }

    fn clear_midi_export_cache(&self) {
        self.last_midi_export.replace(None);
    }
}

impl Drop for GlirdirVst3Controller {
    fn drop(&mut self) {
        self.cleanup_midi_drag_files();
    }
}

impl IConnectionPointTrait for GlirdirVst3Controller {
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
        let message = match GlirdirPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };

        match message {
            GlirdirPluginMessage::AnalysisStatusResponse(status)
            | GlirdirPluginMessage::StatusResponse(status)
            | GlirdirPluginMessage::TelemetryResponse(status) => {
                if !status.has_analysis {
                    self.clear_midi_export_cache();
                }
                self.status.set(status);
                kResultOk
            }
            GlirdirPluginMessage::MidiExportResponse(payload) => {
                let export = MidiExportPayload::decode(&payload)
                    .unwrap_or_else(|| MidiExportPayload::new("glirdir.mid", payload));
                self.last_midi_export.replace(Some(export));
                kResultOk
            }
            GlirdirPluginMessage::SampleLibrarySaveResponse(payload) => {
                let payload = SampleLibrarySavePayload::decode(&payload)
                    .unwrap_or_else(|| SampleLibrarySavePayload::error("malformed response"));
                self.sample_library_status.set(payload.status);
                kResultOk
            }
            GlirdirPluginMessage::ArmCapture
            | GlirdirPluginMessage::ClearScratchpad
            | GlirdirPluginMessage::FinalizeCaptureRequest
            | GlirdirPluginMessage::PlayAudition
            | GlirdirPluginMessage::StopAudition
            | GlirdirPluginMessage::ToggleAuditionLoop
            | GlirdirPluginMessage::ToggleAuditionLiveEdit
            | GlirdirPluginMessage::SampleLibrarySaveRequest
            | GlirdirPluginMessage::PatchUpdate(_)
            | GlirdirPluginMessage::MidiExportRequest
            | GlirdirPluginMessage::StatusRequest
            | GlirdirPluginMessage::TelemetryRequest => kNotImplemented,
        }
    }
}

fn midi_drag_dir() -> PathBuf {
    std::env::temp_dir().join("lindelion-glirdir-midi-drag")
}

fn unique_midi_drag_path(dir: PathBuf, file_name: &str) -> PathBuf {
    let counter = MIDI_DRAG_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    dir.join(format!("{}-{timestamp}-{counter}", std::process::id()))
        .join(file_name)
}

fn remove_midi_drag_file(path: &Path) {
    let _ = fs::remove_file(path);
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
}

impl IPluginBaseTrait for GlirdirVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for GlirdirVst3Controller {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultFalse;
        };
        let Ok(patch) = patch_io::from_plugin_state(plugin_state) else {
            return kResultFalse;
        };
        self.replace_patch_mirror(patch);
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Ok(plugin_state) = patch_io::to_plugin_state(&self.patch.borrow()) else {
            return kResultFalse;
        };
        if write_plugin_state_to_stream(state, plugin_state) {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn getParameterCount(&self) -> i32 {
        VST3_PARAMETER_COUNT as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() || param_index < 0 {
            return kInvalidArgument;
        }
        let Some(parameter) = PARAMETERS.get(param_index as usize) else {
            return kInvalidArgument;
        };

        let info = &mut *info;
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
        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        copy_wstring(&format_plain_value(plain), &mut *string);
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
        let Some(parameter) = parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let len = len_wstring(string as *const TChar);
        let Ok(text) = String::from_utf16(slice::from_raw_parts(string as *const u16, len)) else {
            return kInvalidArgument;
        };
        let Ok(value) = text.trim().parse::<f32>() else {
            return kInvalidArgument;
        };
        *value_normalized = parameter.range.normalize(value) as f64;
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        parameter_by_id(id)
            .map(|parameter| parameter.range.denormalize(value_normalized as f32) as f64)
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        normalized_parameter_value(id, plain_value as f32)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        parameter_index(id)
            .map(|index| self.values.get()[index])
            .unwrap_or(0.0)
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        self.set_value(id, value)
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        self.handler.set(handler);
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        super::editor::create_editor_view(self)
    }
}

pub(super) fn default_parameter_values() -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = [0.0; VST3_PARAMETER_COUNT];
    for (index, parameter) in PARAMETERS.iter().enumerate() {
        values[index] = parameter.range.normalize(parameter.range.default) as f64;
    }
    values
}

pub(super) fn parameter_values_from_patch(patch: &GlirdirPatch) -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = default_parameter_values();
    for parameter in PARAMETERS {
        if let Some(plain) = patch_parameter_plain_value(patch, parameter.id.0)
            && let Some(index) = parameter_index(parameter.id.0)
        {
            values[index] = parameter.range.normalize(plain) as f64;
        }
    }
    values
}

pub(super) fn parameter_index(id: u32) -> Option<usize> {
    PARAMETERS.iter().position(|parameter| parameter.id.0 == id)
}

pub(super) fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    parameter_by_id(id)
        .map(|parameter| parameter.range.normalize(plain) as f64)
        .unwrap_or(0.0)
}

fn parameter_by_id(id: u32) -> Option<lindelion_plugin_shell::ParameterInfo> {
    parameter_binding(id).map(|binding| binding.info())
}

fn patch_parameter_plain_value(patch: &GlirdirPatch, id: u32) -> Option<f32> {
    Some(match id {
        crate::CAPTURE_BARS_PARAMETER_ID => patch.capture.bars.bars() as f32,
        crate::SYNC_MODE_PARAMETER_ID => sync_mode_plain(patch.capture.sync_mode),
        crate::COUNT_IN_PARAMETER_ID => f32::from(patch.capture.count_in_bars),
        crate::CONFIDENCE_PARAMETER_ID => patch.analysis.confidence_threshold,
        crate::ONSET_SENSITIVITY_PARAMETER_ID => patch.analysis.onset_sensitivity,
        crate::MIN_NOTE_PARAMETER_ID => patch.analysis.min_note_ms,
        crate::ROOT_PARAMETER_ID => patch.quantize.root.pitch_class() as f32,
        crate::SCALE_PARAMETER_ID => scale_plain(&patch.quantize.scale),
        crate::SNAP_PARAMETER_ID => snap_plain(patch.quantize.snap_mode),
        crate::GRID_PARAMETER_ID => grid_plain(patch.quantize.grid),
        crate::TIMING_STRENGTH_PARAMETER_ID => patch.quantize.timing_strength,
        crate::VELOCITY_AMOUNT_PARAMETER_ID => patch.quantize.velocity_amount,
        crate::AUDITION_VOLUME_PARAMETER_ID => patch.audition.volume,
        _ => return None,
    })
}

fn sanitize_normalized(value: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn sync_mode_plain(value: SyncMode) -> f32 {
    match value {
        SyncMode::Immediate => 0.0,
        SyncMode::PhraseBoundary => 1.0,
        SyncMode::NextDownbeat => 2.0,
    }
}

fn scale_plain(value: &Scale) -> f32 {
    match value {
        Scale::Chromatic => 0.0,
        Scale::Major => 1.0,
        Scale::NaturalMinor => 2.0,
        Scale::HarmonicMinor => 3.0,
        Scale::MelodicMinor => 4.0,
        Scale::PentatonicMajor => 5.0,
        Scale::PentatonicMinor => 6.0,
        Scale::Blues => 7.0,
        Scale::Dorian => 8.0,
        Scale::Mixolydian | Scale::Custom(_) => 9.0,
    }
}

fn snap_plain(value: SnapMode) -> f32 {
    match value {
        SnapMode::Hard => 0.0,
        SnapMode::Soft => 1.0,
        SnapMode::None => 2.0,
    }
}

fn grid_plain(value: TimingGrid) -> f32 {
    match value {
        TimingGrid::Quarter => 0.0,
        TimingGrid::Eighth => 1.0,
        TimingGrid::Sixteenth => 2.0,
        TimingGrid::ThirtySecond => 3.0,
        TimingGrid::QuarterTriplet => 4.0,
        TimingGrid::EighthTriplet => 5.0,
        TimingGrid::SixteenthTriplet => 6.0,
    }
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
