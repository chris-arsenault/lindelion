use std::ffi::c_void;

#[cfg(target_os = "macos")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize, PlugViewKeyEvent,
};
use lindelion_ui::linnod_vizia::{
    LINNOD_EDITOR_HEIGHT, LINNOD_EDITOR_WIDTH, LinnodEditorCallbacks, LinnodEditorCommand,
    LinnodEditorCommandRequest, LinnodEditorDetectionAlgorithm, LinnodEditorDetectionEdit,
    LinnodEditorDirectories, LinnodEditorHost, LinnodEditorMarkerEdit, LinnodEditorPadEdit,
    LinnodEditorPlaybackEdit, LinnodEditorSliceEdit, LinnodEditorTelemetry,
};
use vst3::{ComWrapper, Steinberg::*};

use crate::parameters::{editor_parameter_bindings, parameter_binding};

use super::{
    LinnodVst3Controller,
    controller::parameter_index,
    controller_helpers::editor_source_status,
    editor_codecs::{envelope_from_editor, playback_mode_from_editor},
    messages::{
        LinnodDetectionEditMessage, LinnodMarkerEditMessage, LinnodPadEditMessage,
        LinnodPlaybackEditMessage, LinnodSliceEditMessage,
    },
};

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(LINNOD_EDITOR_WIDTH, LINNOD_EDITOR_HEIGHT);

pub(super) fn create_editor_view(controller: &LinnodVst3Controller) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        LinnodEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct LinnodEditorView {
    controller: *const LinnodVst3Controller,
    #[cfg(target_os = "macos")]
    editor: RefCell<Option<lindelion_ui::linnod_vizia::LinnodViziaEditor>>,
}

impl LinnodEditorView {
    fn new(controller: &LinnodVst3Controller) -> Self {
        Self {
            controller,
            #[cfg(target_os = "macos")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for LinnodEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "macos")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let host = linnod_editor_host(self.controller);
            *editor = Some(lindelion_ui::linnod_vizia::LinnodViziaEditor::attach(
                parent,
                host,
                lindelion_ui::linnod_vizia::LinnodEditorSize {
                    width: size.right - size.left,
                    height: size.bottom - size.top,
                },
            ));
            kResultOk
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = parent;
            let _ = size;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        #[cfg(target_os = "macos")]
        {
            self.editor.borrow_mut().take();
        }
        kResultOk
    }

    unsafe fn key_down(&self, event: PlugViewKeyEvent) -> tresult {
        #[cfg(target_os = "macos")]
        {
            if event.is_plain_paste_shortcut() {
                let host = linnod_editor_host(self.controller);
                if unsafe { lindelion_ui::linnod_vizia::paste_source_from_clipboard(host) } {
                    return kResultTrue;
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = event;
        }
        kResultFalse
    }
}

pub(super) fn linnod_editor_host(controller: *const LinnodVst3Controller) -> LinnodEditorHost {
    LinnodEditorHost::new(
        controller as usize,
        editor_parameter_bindings(),
        LinnodEditorCallbacks {
            parameter_value,
            set_parameter,
            parameter_value_text,
            default_normalized,
            status,
            telemetry,
            summary,
            directories,
            request_status,
            request_telemetry,
            handle_command,
            edit_marker,
            edit_pad,
            edit_playback,
            edit_detection,
            edit_slice,
        },
    )
    .expect("linnod editor parameter surface should be complete")
}

unsafe fn parameter_value(context: usize, parameter_id: u32) -> f32 {
    let Some(controller) = (unsafe { controller(context) }) else {
        return unsafe { default_normalized(context, parameter_id) };
    };
    let Some(index) = parameter_index(parameter_id) else {
        return unsafe { default_normalized(context, parameter_id) };
    };
    controller.values.value(index).unwrap_or_default() as f32
}

unsafe fn set_parameter(context: usize, parameter_id: u32, normalized: f64) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    if controller.set_value(parameter_id, normalized) == kResultOk {
        controller.notify_parameter_edit(parameter_id, normalized);
    }
}

unsafe fn parameter_value_text(_context: usize, parameter_id: u32, normalized: f64) -> String {
    let Some(binding) = parameter_binding(parameter_id) else {
        return String::new();
    };
    let parameter = binding.info();
    let plain = parameter
        .range
        .denormalize(normalized.clamp(0.0, 1.0) as f32);
    if parameter.units.is_empty() {
        binding.format_plain_value(plain)
    } else {
        format!("{} {}", binding.format_plain_value(plain), parameter.units)
    }
}

unsafe fn default_normalized(_context: usize, parameter_id: u32) -> f32 {
    parameter_binding(parameter_id)
        .map(|binding| {
            let parameter = binding.info();
            parameter.range.normalize(parameter.range.default)
        })
        .unwrap_or(0.0)
}

unsafe fn status(context: usize) -> lindelion_ui::linnod_vizia::LinnodEditorStatus {
    let Some(controller) = (unsafe { controller(context) }) else {
        return Default::default();
    };
    let status = controller.status.get();
    lindelion_ui::linnod_vizia::LinnodEditorStatus {
        source_status: editor_source_status(status.source_status),
        has_source: status.has_source,
        has_analysis: status.has_analysis,
        marker_count: status.marker_count,
        selected_slice_index: status.selected_slice_index,
    }
}

unsafe fn telemetry(context: usize) -> LinnodEditorTelemetry {
    let Some(controller) = (unsafe { controller(context) }) else {
        return Default::default();
    };
    let telemetry = controller.telemetry.get();
    LinnodEditorTelemetry {
        left_peak: telemetry.left_peak,
        right_peak: telemetry.right_peak,
        active_voices: telemetry.active_voices,
    }
}

unsafe fn summary(context: usize) -> lindelion_ui::linnod_vizia::LinnodEditorPatchSummary {
    let Some(controller) = (unsafe { controller(context) }) else {
        return Default::default();
    };
    controller.summary.borrow().clone()
}

unsafe fn directories(_context: usize) -> LinnodEditorDirectories {
    let paths = lindelion_sample_library::music_library_paths("Ahara");
    LinnodEditorDirectories {
        patch_directory: paths.patches,
        sample_directory: paths.samples,
        export_directory: paths.root,
    }
}

unsafe fn request_status(context: usize) {
    if let Some(controller) = unsafe { controller(context) } {
        controller.request_status();
    }
}

unsafe fn request_telemetry(context: usize) {
    if let Some(controller) = unsafe { controller(context) } {
        controller.request_telemetry();
    }
}

unsafe fn handle_command(context: usize, request: LinnodEditorCommandRequest<'_>) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    match request.command {
        LinnodEditorCommand::SavePatch => {
            if let Some(path) = request.patch_save_path {
                let _ = controller.save_patch_to_path(path);
            }
        }
        LinnodEditorCommand::LoadPatch => {
            if let Some(path) = request.patch_load_path {
                let _ = controller.load_patch_from_path(path);
            }
        }
        LinnodEditorCommand::ExportPatchWithSamples => {
            if let Some(path) = request.patch_export_directory {
                let _ = controller.export_patch_bundle(path);
            }
        }
        LinnodEditorCommand::LoadSource => {
            if let Some(path) = request.source_path {
                controller.request_source_ingest(path);
            } else {
                controller.request_source_load();
            }
        }
        LinnodEditorCommand::RedetectSlices => {
            controller.request_redetect_slices();
        }
        LinnodEditorCommand::TuneSelectedSlice => {
            controller.request_tune_selected_slice();
        }
        LinnodEditorCommand::TuneAllSlices => {
            controller.request_tune_all_slices();
        }
        LinnodEditorCommand::SnapAllSlicesToScale => {
            controller.request_snap_all_slices_to_scale();
        }
        LinnodEditorCommand::SetTriggerMode(mode) => {
            controller.set_trigger_mode(mode);
        }
        LinnodEditorCommand::SetPitchShiftAlgorithm(algorithm) => {
            controller.set_pitch_shift_algorithm(algorithm);
        }
        LinnodEditorCommand::SelectPad(pad) => {
            controller.select_pad(pad);
        }
    }
}

unsafe fn edit_marker(context: usize, edit: LinnodEditorMarkerEdit) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    let edit = match edit {
        LinnodEditorMarkerEdit::AddUser { position_samples } => {
            LinnodMarkerEditMessage::AddUser { position_samples }
        }
        LinnodEditorMarkerEdit::RemoveAt { position_samples } => {
            LinnodMarkerEditMessage::RemoveAt { position_samples }
        }
    };
    controller.apply_marker_edit(edit);
}

unsafe fn edit_slice(context: usize, edit: LinnodEditorSliceEdit) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    if let Some(edit) = slice_edit_message(edit) {
        controller.apply_slice_edit(edit);
    }
}

unsafe fn edit_pad(context: usize, edit: LinnodEditorPadEdit) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    if let Some(edit) = pad_edit_message(edit) {
        controller.apply_pad_edit(edit);
    }
}

unsafe fn edit_playback(context: usize, edit: LinnodEditorPlaybackEdit) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    controller.apply_playback_edit(playback_edit_message(edit));
}

unsafe fn edit_detection(context: usize, edit: LinnodEditorDetectionEdit) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    controller.apply_detection_edit(detection_edit_message(edit));
}

fn playback_edit_message(edit: LinnodEditorPlaybackEdit) -> LinnodPlaybackEditMessage {
    match edit {
        LinnodEditorPlaybackEdit::Mode { mode } => {
            LinnodPlaybackEditMessage::Mode(playback_mode_from_editor(mode))
        }
        LinnodEditorPlaybackEdit::Envelope { envelope } => {
            LinnodPlaybackEditMessage::Envelope(envelope_from_editor(envelope))
        }
    }
}

unsafe fn controller<'a>(context: usize) -> Option<&'a LinnodVst3Controller> {
    unsafe { (context as *const LinnodVst3Controller).as_ref() }
}

fn detection_edit_message(edit: LinnodEditorDetectionEdit) -> LinnodDetectionEditMessage {
    match edit {
        LinnodEditorDetectionEdit::Algorithm { algorithm } => {
            LinnodDetectionEditMessage::Algorithm(detection_algorithm(algorithm))
        }
        LinnodEditorDetectionEdit::MinSliceMs { min_slice_ms } => {
            LinnodDetectionEditMessage::MinSliceMs(min_slice_ms)
        }
        LinnodEditorDetectionEdit::LookbackFrames { lookback_frames } => {
            LinnodDetectionEditMessage::LookbackFrames(lookback_frames)
        }
        LinnodEditorDetectionEdit::MaxFilterRadius { max_filter_radius } => {
            LinnodDetectionEditMessage::MaxFilterRadius(max_filter_radius)
        }
        LinnodEditorDetectionEdit::GroupDelayWeight { group_delay_weight } => {
            LinnodDetectionEditMessage::GroupDelayWeight(group_delay_weight)
        }
        LinnodEditorDetectionEdit::SpectralWindowSize { window_size } => {
            LinnodDetectionEditMessage::SpectralWindowSize(window_size)
        }
        LinnodEditorDetectionEdit::PitchStabilityThresholdCents { threshold_cents } => {
            LinnodDetectionEditMessage::PitchStabilityThresholdCents(threshold_cents)
        }
        LinnodEditorDetectionEdit::PitchStabilityDurationMs { duration_ms } => {
            LinnodDetectionEditMessage::PitchStabilityDurationMs(duration_ms)
        }
        LinnodEditorDetectionEdit::EnergyFrameSize { frame_size } => {
            LinnodDetectionEditMessage::EnergyFrameSize(frame_size)
        }
        LinnodEditorDetectionEdit::ManualGridDivisions { divisions } => {
            LinnodDetectionEditMessage::ManualGridDivisions(divisions)
        }
        LinnodEditorDetectionEdit::ManualGridOffsetMs { offset_ms } => {
            LinnodDetectionEditMessage::ManualGridOffsetMs(offset_ms)
        }
    }
}

fn detection_algorithm(
    algorithm: LinnodEditorDetectionAlgorithm,
) -> lindelion_onset_detect::DetectionAlgorithm {
    match algorithm {
        LinnodEditorDetectionAlgorithm::SuperFlux => {
            lindelion_onset_detect::DetectionAlgorithm::SuperFlux
        }
        LinnodEditorDetectionAlgorithm::ComplexFlux => {
            lindelion_onset_detect::DetectionAlgorithm::ComplexFlux
        }
        LinnodEditorDetectionAlgorithm::SpectralSparsity => {
            lindelion_onset_detect::DetectionAlgorithm::SpectralSparsity
        }
        LinnodEditorDetectionAlgorithm::PitchStability => {
            lindelion_onset_detect::DetectionAlgorithm::PitchStability
        }
        LinnodEditorDetectionAlgorithm::EnergyTransient => {
            lindelion_onset_detect::DetectionAlgorithm::EnergyTransient
        }
        LinnodEditorDetectionAlgorithm::ManualGrid => {
            lindelion_onset_detect::DetectionAlgorithm::ManualGrid
        }
    }
}

fn pad_edit_message(edit: LinnodEditorPadEdit) -> Option<LinnodPadEditMessage> {
    match edit {
        LinnodEditorPadEdit::ChokeGroup { pad, group } => {
            let group = match group {
                Some(group) => Some(crate::ChokeGroupId::new(group)?),
                None => None,
            };
            Some(LinnodPadEditMessage::ChokeGroup {
                pad: crate::PadId::new(pad.0)?,
                group,
            })
        }
    }
}

fn slice_edit_message(edit: LinnodEditorSliceEdit) -> Option<LinnodSliceEditMessage> {
    match edit {
        LinnodEditorSliceEdit::Select { slice_index } => {
            Some(LinnodSliceEditMessage::Select { slice_index })
        }
        LinnodEditorSliceEdit::Name { slice_index, name } => {
            Some(LinnodSliceEditMessage::Name { slice_index, name })
        }
        LinnodEditorSliceEdit::Offsets {
            slice_index,
            start_offset_ms,
            end_offset_ms,
        } => Some(LinnodSliceEditMessage::Offsets {
            slice_index,
            start_offset_ms,
            end_offset_ms,
        }),
        LinnodEditorSliceEdit::Pitch {
            slice_index,
            semitones,
            cents,
        } => Some(LinnodSliceEditMessage::Pitch {
            slice_index,
            semitones,
            cents,
        }),
        LinnodEditorSliceEdit::GainDb {
            slice_index,
            gain_db,
        } => Some(LinnodSliceEditMessage::GainDb {
            slice_index,
            gain_db,
        }),
        LinnodEditorSliceEdit::Pan { slice_index, pan } => {
            Some(LinnodSliceEditMessage::Pan { slice_index, pan })
        }
        LinnodEditorSliceEdit::Reverse {
            slice_index,
            reverse,
        } => Some(LinnodSliceEditMessage::Reverse {
            slice_index,
            reverse,
        }),
        LinnodEditorSliceEdit::PlaybackOverride {
            slice_index,
            enabled,
        } => Some(LinnodSliceEditMessage::PlaybackOverride {
            slice_index,
            enabled,
        }),
        LinnodEditorSliceEdit::PlaybackMode { slice_index, mode } => {
            Some(LinnodSliceEditMessage::PlaybackMode {
                slice_index,
                mode: playback_mode_from_editor(mode),
            })
        }
        LinnodEditorSliceEdit::Envelope {
            slice_index,
            envelope,
        } => Some(LinnodSliceEditMessage::Envelope {
            slice_index,
            envelope: envelope_from_editor(envelope),
        }),
        LinnodEditorSliceEdit::FilterCutoff {
            slice_index,
            cutoff_hz,
        } => Some(LinnodSliceEditMessage::FilterCutoff {
            slice_index,
            cutoff_hz,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_host_builds_from_controller_surface() {
        let controller = LinnodVst3Controller::new();

        let host = linnod_editor_host(std::ptr::from_ref(&controller));

        assert_eq!(
            host.parameter_bindings().iter().flatten().count(),
            lindelion_ui::linnod_vizia::LINNOD_EDITOR_PARAMETER_BINDING_COUNT
        );
    }
}
