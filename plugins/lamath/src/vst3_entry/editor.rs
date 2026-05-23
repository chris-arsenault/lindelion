#![allow(non_snake_case)]
#![allow(unexpected_cfgs)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;

#[cfg(target_os = "macos")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use vst3::{ComWrapper, Steinberg::*};

use super::ResonatorVst3Controller;

const EDITOR_SIZE: FixedSizePlugViewSize = FixedSizePlugViewSize::new(960, 640);

pub(super) fn create_editor_view(controller: &ResonatorVst3Controller) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        ResonatorEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct ResonatorEditorView {
    controller: *const ResonatorVst3Controller,
    #[cfg(target_os = "macos")]
    editor: RefCell<Option<lindelion_ui::resonator_vizia::ResonatorViziaEditor>>,
}

impl ResonatorEditorView {
    fn new(controller: &ResonatorVst3Controller) -> Self {
        Self {
            controller,
            #[cfg(target_os = "macos")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for ResonatorEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "macos")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            let host = macos::resonator_editor_host(self.controller);
            *editor = Some(lindelion_ui::resonator_vizia::ResonatorViziaEditor::attach(
                parent,
                host,
                lindelion_ui::resonator_vizia::ResonatorEditorSize {
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

#[cfg(target_os = "macos")]
mod macos {
    use std::path::Path;

    use lindelion_sample_library::SampleReference;
    use lindelion_ui::{
        EditorCommandContext, EditorCommandHandler, PadId, PatchIoService, SampleSlotService,
        resonator_vizia::{
            ResonatorEditorCallbacks, ResonatorEditorCommandRequest, ResonatorEditorDirectories,
            ResonatorEditorHost, ResonatorEditorParameterBinding, ResonatorEditorPatchSummary,
            ResonatorEditorSampleSummary, ResonatorEditorSlotSummary, ResonatorEditorTelemetry,
            ResonatorEditorWaveformPoint,
        },
    };
    use vst3::{ComRef, Steinberg::Vst::IComponentHandlerTrait, Steinberg::*};

    use super::super::{
        EditorPatchSummary, EditorSampleSummary, EditorSlotSummary, EditorTelemetry,
        EditorWaveformPoint, default_library_paths, parameter_index,
    };
    use super::ResonatorVst3Controller;
    use crate::parameters::editor_parameter_bindings;

    pub(super) fn resonator_editor_host(
        controller: *const ResonatorVst3Controller,
    ) -> ResonatorEditorHost {
        ResonatorEditorHost::new(
            controller as usize,
            resonator_editor_parameter_bindings(),
            ResonatorEditorCallbacks {
                refresh_library,
                parameter_value,
                set_parameter,
                parameter_value_text,
                default_normalized,
                summary,
                telemetry,
                directories,
                request_telemetry,
                handle_command,
            },
        )
        .expect("resonator editor parameter surface should be complete")
    }

    fn resonator_editor_parameter_bindings() -> Vec<ResonatorEditorParameterBinding> {
        editor_parameter_bindings()
            .filter_map(|binding| {
                let editor = binding.editor()?;
                Some(ResonatorEditorParameterBinding::new(
                    binding.id().0,
                    editor.slot(),
                    editor.label(),
                    editor.control(),
                ))
            })
            .collect()
    }

    unsafe fn refresh_library(context: usize) {
        let Some(controller) = (unsafe { controller(context) }) else {
            return;
        };
        let _ = controller.refresh_library();
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

        let handler = controller.handler.get();
        if let Some(handler) = unsafe { ComRef::from_raw(handler) } {
            unsafe {
                handler.beginEdit(parameter_id);
                handler.performEdit(parameter_id, normalized);
                handler.endEdit(parameter_id);
            }
        }
    }

    unsafe fn parameter_value_text(_context: usize, parameter_id: u32, normalized: f64) -> String {
        let Some(binding) = crate::parameter_binding(parameter_id) else {
            return String::new();
        };
        let parameter = binding.info();
        let value = parameter
            .range
            .denormalize(normalized.clamp(0.0, 1.0) as f32);
        if parameter.units.is_empty() {
            binding.format_plain_value(value)
        } else {
            format!("{} {}", binding.format_plain_value(value), parameter.units)
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

    unsafe fn summary(context: usize) -> ResonatorEditorPatchSummary {
        let Some(controller) = (unsafe { controller(context) }) else {
            return editor_patch_summary(EditorPatchSummary::from_patch(
                &crate::ResonatorSynthPatch::default(),
            ));
        };

        editor_patch_summary(controller.editor_summary())
    }

    unsafe fn telemetry(context: usize) -> ResonatorEditorTelemetry {
        let Some(controller) = (unsafe { controller(context) }) else {
            return ResonatorEditorTelemetry::default();
        };

        editor_telemetry(controller.telemetry())
    }

    unsafe fn directories(_context: usize) -> ResonatorEditorDirectories {
        let paths = default_library_paths();
        ResonatorEditorDirectories {
            patch_directory: paths.patches,
            sample_directory: paths.samples,
            export_directory: paths.root,
        }
    }

    unsafe fn request_telemetry(context: usize) {
        let Some(controller) = (unsafe { controller(context) }) else {
            return;
        };
        controller.request_telemetry();
    }

    unsafe fn handle_command(context: usize, request: ResonatorEditorCommandRequest<'_>) {
        let Some(controller) = (unsafe { controller(context) }) else {
            return;
        };

        let context = EditorCommandContext {
            patch_save_path: request.patch_save_path,
            patch_load_path: request.patch_load_path,
            patch_export_directory: request.patch_export_directory,
            sample_path: request.sample_path,
            selected_library_sample: request.selected_library_sample,
        };
        let mut patch_io = ControllerPatchIoService { controller };
        let mut sample_slots = ControllerSampleSlotService { controller };
        let _ = EditorCommandHandler::handle(
            request.command,
            context,
            &mut patch_io,
            &mut sample_slots,
        );
    }

    unsafe fn controller<'a>(context: usize) -> Option<&'a ResonatorVst3Controller> {
        unsafe { (context as *const ResonatorVst3Controller).as_ref() }
    }

    fn editor_patch_summary(summary: EditorPatchSummary) -> ResonatorEditorPatchSummary {
        ResonatorEditorPatchSummary {
            patch_name: summary.patch_name,
            slots: summary.slots.map(editor_slot_summary),
            library_samples: summary
                .library_samples
                .into_iter()
                .map(editor_sample_summary)
                .collect(),
        }
    }

    fn editor_sample_summary(summary: EditorSampleSummary) -> ResonatorEditorSampleSummary {
        ResonatorEditorSampleSummary {
            label: summary.label,
            detail: summary.detail,
            preview: summary
                .preview
                .into_iter()
                .map(editor_waveform_point)
                .collect(),
        }
    }

    fn editor_waveform_point(point: EditorWaveformPoint) -> ResonatorEditorWaveformPoint {
        ResonatorEditorWaveformPoint {
            min: point.min,
            max: point.max,
            rms: point.rms,
        }
    }

    fn editor_slot_summary(summary: EditorSlotSummary) -> ResonatorEditorSlotSummary {
        ResonatorEditorSlotSummary {
            label: summary.label,
            detail: summary.detail,
            sample_backed: summary.sample_backed,
            pitch_track: summary.pitch_track,
            looping: summary.looping,
        }
    }

    fn editor_telemetry(telemetry: EditorTelemetry) -> ResonatorEditorTelemetry {
        ResonatorEditorTelemetry {
            left_peak: telemetry.left_peak,
            right_peak: telemetry.right_peak,
            left_rms: telemetry.left_rms,
            right_rms: telemetry.right_rms,
            active_voices: telemetry.active_voices,
        }
    }

    struct ControllerPatchIoService<'a> {
        controller: &'a ResonatorVst3Controller,
    }

    impl PatchIoService for ControllerPatchIoService<'_> {
        type Error = ControllerPatchIoError;

        fn save_patch(&mut self, path: &Path) -> Result<(), Self::Error> {
            self.controller
                .save_patch_to_path(path)
                .map_err(ControllerPatchIoError::Patch)
        }

        fn load_patch(&mut self, path: &Path) -> Result<(), Self::Error> {
            self.controller
                .load_patch_from_path(path)
                .map(|_| ())
                .map_err(ControllerPatchIoError::Patch)
        }

        fn export_patch_with_samples(&mut self, directory: &Path) -> Result<(), Self::Error> {
            self.controller
                .export_patch_bundle(directory)
                .map(|_| ())
                .map_err(ControllerPatchIoError::Io)
        }
    }

    #[allow(dead_code)]
    enum ControllerPatchIoError {
        Patch(crate::patch_io::PatchIoError),
        Io(std::io::Error),
    }

    struct ControllerSampleSlotService<'a> {
        controller: &'a ResonatorVst3Controller,
    }

    impl SampleSlotService for ControllerSampleSlotService<'_> {
        type SampleReference = SampleReference;
        type Error = ControllerSampleSlotError;

        fn refresh_library(&mut self) -> Result<(), Self::Error> {
            self.controller
                .refresh_library()
                .map_err(ControllerSampleSlotError::Io)
        }

        fn ingest_sample(&mut self, path: &Path) -> Result<Self::SampleReference, Self::Error> {
            self.controller
                .ingest_sample(path.to_path_buf())
                .map_err(ControllerSampleSlotError::Io)
        }

        fn assign_library_sample_to_slot(
            &mut self,
            sample_index: usize,
            slot: PadId,
        ) -> Result<(), Self::Error> {
            tresult_to_result(
                self.controller
                    .assign_library_sample_to_slot(sample_index, slot_index(slot)),
            )
        }

        fn assign_sample_to_slot(
            &mut self,
            reference: Self::SampleReference,
            slot: PadId,
        ) -> Result<(), Self::Error> {
            tresult_to_result(
                self.controller
                    .assign_sample_reference_to_slot(reference, slot_index(slot)),
            )
        }

        fn clear_slot(&mut self, slot: PadId) -> Result<(), Self::Error> {
            tresult_to_result(self.controller.clear_slot(slot_index(slot)))
        }
    }

    #[allow(dead_code)]
    enum ControllerSampleSlotError {
        Io(std::io::Error),
        Host(tresult),
    }

    fn tresult_to_result(result: tresult) -> Result<(), ControllerSampleSlotError> {
        if result == kResultOk {
            Ok(())
        } else {
            Err(ControllerSampleSlotError::Host(result))
        }
    }

    fn slot_index(slot: PadId) -> usize {
        usize::from(slot.0.saturating_sub(1)).min(3)
    }
}
