use std::ffi::c_void;

#[cfg(target_os = "macos")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use lindelion_ui::{
    WaveformPoint,
    glirdir_vizia::{
        GLIRDIR_EDITOR_HEIGHT, GLIRDIR_EDITOR_WIDTH, GlirdirEditorAnalysisStatus,
        GlirdirEditorCallbacks, GlirdirEditorCaptureState, GlirdirEditorCommand, GlirdirEditorHost,
        GlirdirEditorLibraryStatus, GlirdirEditorPianoRollPreview, GlirdirEditorPreview,
        GlirdirEditorStatus, GlirdirEditorWaveformPreview,
    },
};
use vst3::{ComWrapper, Steinberg::*};

use super::{GlirdirStatusPayload, GlirdirVst3Controller, controller::parameter_index};
use crate::{
    AnalysisStatus, CaptureState, GlirdirPatch, editor_parameter_bindings,
    sample_library::SampleLibrarySaveStatus,
};

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(GLIRDIR_EDITOR_WIDTH, GLIRDIR_EDITOR_HEIGHT);
const WAVEFORM_PREVIEW_POINTS: usize = 160;

pub(super) fn create_editor_view(controller: &GlirdirVst3Controller) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        GlirdirEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct GlirdirEditorView {
    controller: *const GlirdirVst3Controller,
    #[cfg(target_os = "macos")]
    editor: RefCell<Option<lindelion_ui::glirdir_vizia::GlirdirViziaEditor>>,
}

impl GlirdirEditorView {
    fn new(controller: &GlirdirVst3Controller) -> Self {
        Self {
            controller,
            #[cfg(target_os = "macos")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for GlirdirEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "macos")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let host = glirdir_editor_host(self.controller);
            *editor = Some(lindelion_ui::glirdir_vizia::GlirdirViziaEditor::attach(
                parent,
                host,
                lindelion_ui::glirdir_vizia::GlirdirEditorSize {
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
}

pub(super) fn glirdir_editor_host(controller: *const GlirdirVst3Controller) -> GlirdirEditorHost {
    GlirdirEditorHost::new(
        controller as usize,
        editor_parameter_bindings(),
        GlirdirEditorCallbacks {
            parameter_value,
            set_parameter,
            parameter_value_text,
            default_normalized,
            status,
            preview,
            request_status,
            handle_command,
            prepare_midi_drag,
        },
    )
    .expect("glirdir editor parameter surface should be complete")
}

unsafe fn parameter_value(context: usize, parameter_id: u32) -> f32 {
    let Some(controller) = (unsafe { controller(context) }) else {
        return unsafe { default_normalized(context, parameter_id) };
    };
    let Some(index) = parameter_index(parameter_id) else {
        return unsafe { default_normalized(context, parameter_id) };
    };
    controller.values.get()[index] as f32
}

unsafe fn set_parameter(context: usize, parameter_id: u32, normalized: f64) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    if controller.set_value(parameter_id, normalized) != kResultOk {
        return;
    }

    controller.notify_parameter_edit(parameter_id, normalized);
}

unsafe fn parameter_value_text(_context: usize, parameter_id: u32, normalized: f64) -> String {
    let Some(binding) = crate::parameter_binding(parameter_id) else {
        return String::new();
    };
    let parameter = binding.info();
    let plain = parameter
        .range
        .denormalize(normalized.clamp(0.0, 1.0) as f32);
    if parameter.units.is_empty() {
        format!("{plain:.2}")
    } else {
        format!("{plain:.2} {}", parameter.units)
    }
}

unsafe fn default_normalized(_context: usize, parameter_id: u32) -> f32 {
    crate::parameter_binding(parameter_id)
        .map(|binding| {
            let parameter = binding.info();
            parameter.range.normalize(parameter.range.default)
        })
        .unwrap_or(0.0)
}

unsafe fn status(context: usize) -> GlirdirEditorStatus {
    let Some(controller) = (unsafe { controller(context) }) else {
        return GlirdirEditorStatus::default();
    };
    editor_status(
        controller.status.get(),
        controller.sample_library_status.get(),
    )
}

unsafe fn preview(context: usize) -> GlirdirEditorPreview {
    let Some(controller) = (unsafe { controller(context) }) else {
        return GlirdirEditorPreview::default();
    };
    let patch = controller.patch.borrow();
    GlirdirEditorPreview {
        waveform: waveform_preview(&patch),
        piano_roll: GlirdirEditorPianoRollPreview {
            ppq: 960,
            bpm: patch.quantize.bpm.round().max(1.0) as u16,
            notes: Vec::new(),
        },
    }
}

unsafe fn request_status(context: usize) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    controller.request_status();
}

unsafe fn handle_command(context: usize, command: GlirdirEditorCommand) {
    let Some(controller) = (unsafe { controller(context) }) else {
        return;
    };
    match command {
        GlirdirEditorCommand::ArmCapture => {
            controller.request_arm_capture();
        }
        GlirdirEditorCommand::ClearScratchpad => {
            controller.request_clear_scratchpad();
        }
        GlirdirEditorCommand::FinalizeCapture => {
            controller.request_finalize_completed_capture();
        }
        GlirdirEditorCommand::PlayAudition => {
            controller.request_play_audition();
        }
        GlirdirEditorCommand::StopAudition => {
            controller.request_stop_audition();
        }
        GlirdirEditorCommand::ToggleLoop => {
            controller.request_toggle_audition_loop();
        }
        GlirdirEditorCommand::ToggleLiveEdit => {
            controller.request_toggle_audition_live_edit();
        }
        GlirdirEditorCommand::ExportMidi => {
            controller.request_midi_export();
        }
        GlirdirEditorCommand::SaveScratchpadToLibrary => {
            controller.request_save_scratchpad_to_library();
        }
    }
}

unsafe fn prepare_midi_drag(context: usize) -> lindelion_ui::glirdir_vizia::GlirdirEditorMidiDrag {
    let Some(controller) = (unsafe { controller(context) }) else {
        return lindelion_ui::glirdir_vizia::GlirdirEditorMidiDrag::Failed;
    };
    controller.prepare_midi_drag_file()
}

unsafe fn controller<'a>(context: usize) -> Option<&'a GlirdirVst3Controller> {
    unsafe { (context as *const GlirdirVst3Controller).as_ref() }
}

fn editor_status(
    status: GlirdirStatusPayload,
    library_status: SampleLibrarySaveStatus,
) -> GlirdirEditorStatus {
    GlirdirEditorStatus {
        capture_state: editor_capture_state(status.capture_state),
        analysis_status: editor_analysis_status(status.analysis_status),
        has_scratchpad: status.has_scratchpad,
        has_analysis: status.has_analysis,
        library_status: editor_library_status(library_status),
    }
}

fn editor_library_status(status: SampleLibrarySaveStatus) -> GlirdirEditorLibraryStatus {
    match status {
        SampleLibrarySaveStatus::Idle => GlirdirEditorLibraryStatus::Idle,
        SampleLibrarySaveStatus::Saving => GlirdirEditorLibraryStatus::Saving,
        SampleLibrarySaveStatus::Saved => GlirdirEditorLibraryStatus::Saved,
        SampleLibrarySaveStatus::EmptyScratchpad => GlirdirEditorLibraryStatus::EmptyScratchpad,
        SampleLibrarySaveStatus::Error => GlirdirEditorLibraryStatus::Error,
    }
}

fn editor_capture_state(state: CaptureState) -> GlirdirEditorCaptureState {
    match state {
        CaptureState::Idle => GlirdirEditorCaptureState::Idle,
        CaptureState::Armed => GlirdirEditorCaptureState::Armed,
        CaptureState::CountIn => GlirdirEditorCaptureState::CountIn,
        CaptureState::Capturing => GlirdirEditorCaptureState::Capturing,
        CaptureState::Captured => GlirdirEditorCaptureState::Captured,
    }
}

fn editor_analysis_status(status: AnalysisStatus) -> GlirdirEditorAnalysisStatus {
    match status {
        AnalysisStatus::Idle => GlirdirEditorAnalysisStatus::Idle,
        AnalysisStatus::Capturing => GlirdirEditorAnalysisStatus::Capturing,
        AnalysisStatus::CapturedPendingAnalysis => {
            GlirdirEditorAnalysisStatus::CapturedPendingAnalysis
        }
        AnalysisStatus::Analyzing => GlirdirEditorAnalysisStatus::Analyzing,
        AnalysisStatus::Ready => GlirdirEditorAnalysisStatus::Ready,
        AnalysisStatus::Error => GlirdirEditorAnalysisStatus::Error,
    }
}

fn waveform_preview(patch: &GlirdirPatch) -> GlirdirEditorWaveformPreview {
    let Some(scratchpad) = patch.scratchpad.as_ref() else {
        return GlirdirEditorWaveformPreview {
            sample_rate: patch.quantize.sample_rate,
            points: Vec::new(),
        };
    };

    let points = waveform_points(&scratchpad.samples, WAVEFORM_PREVIEW_POINTS);
    GlirdirEditorWaveformPreview {
        sample_rate: scratchpad.sample_rate,
        points,
    }
}

fn waveform_points(samples: &[f32], max_points: usize) -> Vec<WaveformPoint> {
    if samples.is_empty() || max_points == 0 {
        return Vec::new();
    }
    let chunk_len = samples.len().div_ceil(max_points).max(1);
    samples
        .chunks(chunk_len)
        .take(max_points)
        .map(waveform_point)
        .collect()
}

fn waveform_point(samples: &[f32]) -> WaveformPoint {
    let mut min = 0.0_f32;
    let mut max = 0.0_f32;
    let mut sum_squares = 0.0_f32;
    for sample in samples {
        let sample = if sample.is_finite() { *sample } else { 0.0 };
        min = min.min(sample);
        max = max.max(sample);
        sum_squares += sample * sample;
    }
    WaveformPoint {
        min,
        max,
        rms: (sum_squares / samples.len().max(1) as f32).sqrt(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScratchpadAudio;

    #[test]
    fn waveform_preview_is_bounded() {
        let patch = GlirdirPatch {
            scratchpad: Some(ScratchpadAudio::new(48_000, vec![0.2; 1024])),
            ..GlirdirPatch::default()
        };

        let preview = waveform_preview(&patch);

        assert_eq!(preview.sample_rate, 48_000);
        assert!(preview.points.len() <= WAVEFORM_PREVIEW_POINTS);
    }

    #[test]
    fn editor_host_builds_from_controller_surface() {
        let controller = GlirdirVst3Controller::new();

        let host = glirdir_editor_host(std::ptr::from_ref(&controller));

        assert_eq!(
            host.parameter_bindings().iter().flatten().count(),
            lindelion_ui::glirdir_vizia::GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT
        );
    }
}
