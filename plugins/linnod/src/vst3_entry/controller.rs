use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    fs, io,
    path::{Path, PathBuf},
    ptr,
};

use lindelion_plugin_shell::vst3::{
    Vst3ParameterInfo, Vst3ParameterMirror, Vst3PeerConnection, fill_vst3_parameter_info,
    notify_vst3_patch_update, parse_vst3_plain_value_string, read_plugin_state_from_stream,
    restart_vst3_parameter_values_changed, write_plugin_state_to_stream,
    write_vst3_parameter_string,
};
use lindelion_ui::{
    PadId as UiPadId,
    linnod_vizia::{
        LinnodEditorPatchSummary, LinnodEditorPitchShiftAlgorithm, LinnodEditorTriggerMode,
    },
};
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{
    LinnodPatch,
    parameters::{
        PARAMETER_REGISTRY, ParameterApplyKind, denormalized_parameter_value,
        format_parameter_plain_value,
        normalized_parameter_value as registry_normalized_parameter_value,
        parameter_binding_by_index, parameter_binding_index, parameter_info,
    },
    patch::{EngineEdit, PitchShiftAlgorithm, TriggerMode},
    patch_io,
};

use super::{
    LinnodPluginMessage, LinnodStatusPayload, VST3_PARAMETER_COUNT,
    controller_helpers::{
        apply_source_summary_payload, editor_summary_from_patch, sanitize_patch_filename,
    },
    editor,
    messages::{
        LinnodDetectionEditMessage, LinnodMarkerEditMessage, LinnodPadEditMessage,
        LinnodPlaybackEditMessage, LinnodSliceEditMessage, LinnodSourceSummaryPayload,
        LinnodTelemetryPayload,
    },
    patch_edits::{
        apply_detection_edit_message, apply_marker_edit_message, apply_pad_edit_message,
        apply_playback_edit_message, apply_slice_edit_message,
    },
};

pub(super) struct LinnodVst3Controller {
    pub(super) values: Vst3ParameterMirror<VST3_PARAMETER_COUNT>,
    pub(super) patch: RefCell<LinnodPatch>,
    pub(super) summary: RefCell<LinnodEditorPatchSummary>,
    pub(super) status: Cell<LinnodStatusPayload>,
    pub(super) telemetry: Cell<LinnodTelemetryPayload>,
    source_summary: RefCell<Option<LinnodSourceSummaryPayload>>,
    pub(super) handler: Cell<*mut IComponentHandler>,
    peer: Vst3PeerConnection,
}

impl Class for LinnodVst3Controller {
    type Interfaces = (IEditController, IConnectionPoint);
}

impl LinnodVst3Controller {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.controller_cid[0],
        crate::VST3_BUNDLE_METADATA.controller_cid[1],
        crate::VST3_BUNDLE_METADATA.controller_cid[2],
        crate::VST3_BUNDLE_METADATA.controller_cid[3],
    );

    pub(super) fn new() -> Self {
        let patch = LinnodPatch::default();
        Self {
            values: Vst3ParameterMirror::new(default_parameter_values()),
            summary: RefCell::new(editor_summary_from_patch(&patch)),
            patch: RefCell::new(patch),
            status: Cell::new(LinnodStatusPayload::default()),
            telemetry: Cell::new(LinnodTelemetryPayload::default()),
            source_summary: RefCell::new(None),
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

        let apply = {
            let mut patch = self.patch.borrow_mut();
            crate::parameters::apply_parameter_normalized(&mut patch, id, value as f32)
        };
        if let Some(apply) = apply {
            if matches!(apply, ParameterApplyKind::Analysis) {
                self.source_summary.replace(None);
            }
            self.refresh_summary();
        }
        kResultOk
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

    pub(super) fn request_status(&self) -> tresult {
        let result = self
            .peer
            .notify(LinnodPluginMessage::status_request().into_com_message());
        if result == kResultOk {
            self.request_source_summary_if_missing()
        } else {
            result
        }
    }

    pub(super) fn request_telemetry(&self) -> tresult {
        self.peer
            .notify(LinnodPluginMessage::telemetry_request().into_com_message())
    }

    pub(super) fn request_source_load(&self) -> tresult {
        self.clear_source_summary();
        self.peer
            .notify(LinnodPluginMessage::SourceLoadRequest.into_com_message())
    }

    pub(super) fn request_source_ingest(&self, path: &Path) -> tresult {
        self.clear_source_summary();
        self.peer.notify(
            LinnodPluginMessage::source_ingest_request(
                path.to_string_lossy().into_owned().into_bytes(),
            )
            .into_com_message(),
        )
    }

    pub(super) fn request_redetect_slices(&self) -> tresult {
        self.clear_source_summary();
        self.peer
            .notify(LinnodPluginMessage::RedetectSlices.into_com_message())
    }

    pub(super) fn request_tune_selected_slice(&self) -> tresult {
        self.peer
            .notify(LinnodPluginMessage::TuneSelectedSlice.into_com_message())
    }

    pub(super) fn request_tune_all_slices(&self) -> tresult {
        self.peer
            .notify(LinnodPluginMessage::TuneAllSlices.into_com_message())
    }

    pub(super) fn request_snap_all_slices_to_scale(&self) -> tresult {
        self.peer
            .notify(LinnodPluginMessage::SnapAllSlicesToScale.into_com_message())
    }

    pub(super) fn set_trigger_mode(&self, mode: LinnodEditorTriggerMode) -> tresult {
        self.patch.borrow_mut().trigger_mode = match mode {
            LinnodEditorTriggerMode::Pad => TriggerMode::Pad,
            LinnodEditorTriggerMode::Chromatic => TriggerMode::Chromatic,
        };
        self.refresh_summary();
        self.send_patch_to_processor()
    }

    pub(super) fn set_pitch_shift_algorithm(
        &self,
        algorithm: LinnodEditorPitchShiftAlgorithm,
    ) -> tresult {
        self.patch
            .borrow_mut()
            .apply_engine_edit(EngineEdit::PitchShiftAlgorithm(match algorithm {
                LinnodEditorPitchShiftAlgorithm::SpectralPeak => PitchShiftAlgorithm::SpectralPeak,
                LinnodEditorPitchShiftAlgorithm::Varispeed => PitchShiftAlgorithm::Varispeed,
                LinnodEditorPitchShiftAlgorithm::TimeStretch => PitchShiftAlgorithm::TimeStretch,
                LinnodEditorPitchShiftAlgorithm::ResampleStretch => {
                    PitchShiftAlgorithm::ResampleStretch
                }
            }));
        self.refresh_summary();
        self.send_patch_to_processor()
    }

    pub(super) fn select_pad(&self, pad: UiPadId) -> tresult {
        self.patch.borrow_mut().active_chromatic_pad = crate::PadId(pad.0).sanitized();
        self.refresh_summary();
        self.send_patch_to_processor()
    }

    pub(super) fn apply_marker_edit(&self, edit: LinnodMarkerEditMessage) -> tresult {
        let mut patch = self.patch.borrow().clone();
        apply_marker_edit_message(&mut patch, edit, usize::MAX);
        self.replace_patch_mirror(patch);
        self.peer
            .notify(LinnodPluginMessage::MarkerEdit(edit.encode()).into_com_message())
    }

    pub(super) fn apply_slice_edit(&self, edit: LinnodSliceEditMessage) -> tresult {
        let payload = edit.encode();
        let mut patch = self.patch.borrow().clone();
        let applied = apply_slice_edit_message(&mut patch, edit);
        if !applied {
            return kInvalidArgument;
        }
        self.replace_patch_mirror(patch);
        self.peer
            .notify(LinnodPluginMessage::SliceEdit(payload).into_com_message())
    }

    pub(super) fn apply_pad_edit(&self, edit: LinnodPadEditMessage) -> tresult {
        let payload = edit.encode();
        let mut patch = self.patch.borrow().clone();
        if !apply_pad_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        self.replace_patch_mirror(patch);
        self.peer
            .notify(LinnodPluginMessage::PadEdit(payload).into_com_message())
    }

    pub(super) fn apply_playback_edit(&self, edit: LinnodPlaybackEditMessage) -> tresult {
        let payload = edit.encode();
        let mut patch = self.patch.borrow().clone();
        if !apply_playback_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        self.replace_patch_mirror(patch);
        self.peer
            .notify(LinnodPluginMessage::PlaybackEdit(payload).into_com_message())
    }

    pub(super) fn apply_detection_edit(&self, edit: LinnodDetectionEditMessage) -> tresult {
        let payload = edit.encode();
        let mut patch = self.patch.borrow().clone();
        if !apply_detection_edit_message(&mut patch, edit) {
            return kInvalidArgument;
        }
        self.replace_patch_mirror(patch);
        self.peer
            .notify(LinnodPluginMessage::DetectionEdit(payload).into_com_message())
    }

    pub(super) fn save_patch_to_path(&self, path: &Path) -> Result<(), patch_io::PatchIoError> {
        patch_io::save_patch(path, &self.patch.borrow())
    }

    pub(super) fn load_patch_from_path(
        &self,
        path: &Path,
    ) -> Result<tresult, patch_io::PatchIoError> {
        let patch = patch_io::load_patch(path)?;
        self.replace_patch_mirror(patch);
        self.notify_parameter_values_changed();
        Ok(self.send_patch_to_processor())
    }

    pub(super) fn export_patch_bundle(&self, directory: &Path) -> io::Result<PathBuf> {
        fs::create_dir_all(directory)?;
        let patch = self.patch.borrow();
        let path = directory.join(format!("{}.toml", sanitize_patch_filename(&patch.name)));
        patch_io::save_patch(&path, &patch)
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        Ok(path)
    }

    fn send_patch_to_processor(&self) -> tresult {
        notify_vst3_patch_update(
            &self.peer,
            &*self.patch.borrow(),
            patch_io::to_toml_string,
            |payload| LinnodPluginMessage::patch_update(payload).into_com_message(),
        )
    }

    fn replace_patch_mirror(&self, patch: LinnodPatch) {
        let source_summary_must_clear = {
            let current = self.patch.borrow();
            source_summary_cache_must_clear(&current, &patch)
        };
        if source_summary_must_clear {
            self.source_summary.replace(None);
        }
        self.values.replace(parameter_values_from_patch(&patch));
        self.summary.replace(self.editor_summary_for_patch(&patch));
        self.patch.replace(patch);
    }

    fn refresh_summary(&self) {
        let patch = self.patch.borrow();
        self.summary.replace(self.editor_summary_for_patch(&patch));
    }

    fn editor_summary_for_patch(&self, patch: &LinnodPatch) -> LinnodEditorPatchSummary {
        let mut summary = editor_summary_from_patch(patch);
        if let Some(source_summary) = self.source_summary.borrow().as_ref() {
            apply_source_summary_payload(&mut summary, source_summary);
        }
        summary
    }

    fn clear_source_summary(&self) {
        self.source_summary.replace(None);
        self.refresh_summary();
    }

    fn request_source_summary_if_missing(&self) -> tresult {
        if self.status.get().has_analysis && self.source_summary.borrow().is_none() {
            self.peer
                .notify(LinnodPluginMessage::source_summary_request().into_com_message())
        } else {
            kResultOk
        }
    }

    fn notify_parameter_values_changed(&self) {
        unsafe {
            restart_vst3_parameter_values_changed(self.handler.get());
        }
    }
}

impl IConnectionPointTrait for LinnodVst3Controller {
    unsafe fn connect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.connect(other)
    }

    unsafe fn disconnect(&self, other: *mut IConnectionPoint) -> tresult {
        self.peer.disconnect(other)
    }

    unsafe fn notify(&self, message: *mut IMessage) -> tresult {
        let message = match LinnodPluginMessage::decode(message) {
            Ok(Some(message)) => message,
            Ok(None) => return kNotImplemented,
            Err(_) => return kResultFalse,
        };
        match message {
            LinnodPluginMessage::PatchUpdate(payload) => {
                let Ok(text) = std::str::from_utf8(&payload) else {
                    return kResultFalse;
                };
                let Ok(patch) = patch_io::from_toml_str(text) else {
                    return kResultFalse;
                };
                self.replace_patch_mirror(patch);
                self.notify_parameter_values_changed();
                kResultOk
            }
            LinnodPluginMessage::AnalysisStatusResponse(status)
            | LinnodPluginMessage::StatusResponse(status) => {
                self.status.set(status);
                kResultOk
            }
            LinnodPluginMessage::SourceSummaryResponse(payload) => {
                let Some(source_summary) = LinnodSourceSummaryPayload::decode(&payload) else {
                    return kResultFalse;
                };
                self.source_summary.replace(Some(source_summary));
                self.refresh_summary();
                kResultOk
            }
            LinnodPluginMessage::TelemetryResponse(payload) => {
                let Some(telemetry) = LinnodTelemetryPayload::decode(&payload) else {
                    return kResultFalse;
                };
                self.telemetry.set(telemetry);
                kResultOk
            }
            LinnodPluginMessage::SourceLoadRequest
            | LinnodPluginMessage::SourceIngestRequest(_)
            | LinnodPluginMessage::RedetectSlices
            | LinnodPluginMessage::TuneSelectedSlice
            | LinnodPluginMessage::TuneAllSlices
            | LinnodPluginMessage::SnapAllSlicesToScale
            | LinnodPluginMessage::MarkerEdit(_)
            | LinnodPluginMessage::PadEdit(_)
            | LinnodPluginMessage::PlaybackEdit(_)
            | LinnodPluginMessage::DetectionEdit(_)
            | LinnodPluginMessage::SliceEdit(_)
            | LinnodPluginMessage::StatusRequest
            | LinnodPluginMessage::SourceSummaryRequest
            | LinnodPluginMessage::TelemetryRequest => kNotImplemented,
        }
    }
}

impl IPluginBaseTrait for LinnodVst3Controller {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for LinnodVst3Controller {
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
        editor::create_editor_view(self)
    }
}

fn source_summary_cache_must_clear(current: &LinnodPatch, next: &LinnodPatch) -> bool {
    current.source_sample != next.source_sample || current.detection != next.detection
}

pub(super) fn default_parameter_values() -> [f64; VST3_PARAMETER_COUNT] {
    PARAMETER_REGISTRY.default_normalized_values::<VST3_PARAMETER_COUNT>()
}

pub(super) fn parameter_values_from_patch(patch: &LinnodPatch) -> [f64; VST3_PARAMETER_COUNT] {
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
