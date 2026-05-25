use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use lindelion_plugin_shell::vst3::{
    Vst3ParameterInfo, Vst3ParameterMirror, Vst3PeerConnection, fill_vst3_parameter_info,
    parse_vst3_plain_value_string, read_plugin_state_from_stream, write_plugin_state_to_stream,
    write_vst3_parameter_string,
};
use lindelion_ui::glirdir_vizia::GlirdirEditorMidiDrag;
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::parameters::PARAMETER_REGISTRY;
use crate::{
    GlirdirPatch, apply_parameter_normalized, denormalized_parameter_value,
    format_parameter_plain_value,
    midi_export::{MidiExportPayload, empty_midi_export},
    normalized_parameter_value as registry_normalized_parameter_value, parameter_binding_by_index,
    parameter_binding_index, parameter_info, patch_io,
    sample_library::{SampleLibrarySavePayload, SampleLibrarySaveStatus},
};

use super::{GlirdirPluginMessage, GlirdirStatusPayload, VST3_PARAMETER_COUNT};

pub(super) const MAX_MIDI_DRAG_FILES: usize = 8;
static MIDI_DRAG_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct GlirdirVst3Controller {
    pub(super) values: Vst3ParameterMirror<VST3_PARAMETER_COUNT>,
    pub(super) patch: RefCell<GlirdirPatch>,
    pub(super) status: Cell<GlirdirStatusPayload>,
    pub(super) sample_library_status: Cell<SampleLibrarySaveStatus>,
    pub(super) last_midi_export: RefCell<Option<MidiExportPayload>>,
    pub(super) midi_drag_files: RefCell<VecDeque<PathBuf>>,
    handler: Cell<*mut IComponentHandler>,
    peer: Vst3PeerConnection,
}

impl Class for GlirdirVst3Controller {
    type Interfaces = (IEditController, IConnectionPoint);
}

impl GlirdirVst3Controller {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.controller_cid[0],
        crate::VST3_BUNDLE_METADATA.controller_cid[1],
        crate::VST3_BUNDLE_METADATA.controller_cid[2],
        crate::VST3_BUNDLE_METADATA.controller_cid[3],
    );

    pub(super) fn new() -> Self {
        Self {
            values: Vst3ParameterMirror::new(default_parameter_values()),
            patch: RefCell::new(GlirdirPatch::default()),
            status: Cell::new(GlirdirStatusPayload::default()),
            sample_library_status: Cell::new(SampleLibrarySaveStatus::Idle),
            last_midi_export: RefCell::new(None),
            midi_drag_files: RefCell::new(VecDeque::new()),
            handler: Cell::new(ptr::null_mut()),
            peer: Vst3PeerConnection::new(),
        }
    }

    pub(super) fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameter_index(id) else {
            return kInvalidArgument;
        };
        let Some(value) =
            self.values
                .set_normalized(index, normalized, default_parameter_values()[index])
        else {
            return kInvalidArgument;
        };

        if !matches!(
            apply_parameter_normalized(&mut self.patch.borrow_mut(), id, value as f32),
            crate::ParameterApplyKind::Ignored
        ) {
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

    fn replace_patch_mirror(&self, patch: GlirdirPatch) {
        self.values.replace(parameter_values_from_patch(&patch));
        self.patch.replace(patch);
        self.clear_midi_export_cache();
    }

    fn notify_peer(&self, message: GlirdirPluginMessage) -> tresult {
        self.peer.notify(message.into_com_message())
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
        self.peer.connect(other)
    }

    unsafe fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.disconnect(other)
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
        let Some(binding) = parameter_binding_by_index(param_index as usize) else {
            return kInvalidArgument;
        };
        fill_vst3_parameter_info(Vst3ParameterInfo::from_parameter(binding.info()), info)
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
        let Some(parameter) = parameter_info(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        write_vst3_parameter_string(&format_parameter_plain_value(id, plain), string)
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
        let Some(value) = parse_vst3_plain_value_string(string) else {
            return kInvalidArgument;
        };
        let Some(normalized) = registry_normalized_parameter_value(id, value) else {
            return kInvalidArgument;
        };
        *value_normalized = normalized as f64;
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        denormalized_parameter_value(id, value_normalized as f32)
            .map(f64::from)
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        normalized_parameter_value(id, plain_value as f32)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        parameter_index(id)
            .and_then(|index| self.values.value(index))
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
    PARAMETER_REGISTRY.default_normalized_values::<VST3_PARAMETER_COUNT>()
}

pub(super) fn parameter_values_from_patch(patch: &GlirdirPatch) -> [f64; VST3_PARAMETER_COUNT] {
    PARAMETER_REGISTRY.normalized_patch_values(patch, default_parameter_values())
}

pub(super) fn parameter_index(id: u32) -> Option<usize> {
    parameter_binding_index(id)
}

pub(super) fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    registry_normalized_parameter_value(id, plain)
        .map(f64::from)
        .unwrap_or(0.0)
}
