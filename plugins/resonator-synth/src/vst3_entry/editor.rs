#![allow(non_snake_case)]
#![allow(unexpected_cfgs)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::{
    cell::Cell,
    ffi::{CStr, c_char, c_void},
    ptr,
};

#[cfg(target_os = "macos")]
use std::cell::RefCell;

#[cfg(target_os = "macos")]
use vst3::ComRef;
#[cfg(target_os = "macos")]
use vst3::Steinberg::Vst::IComponentHandlerTrait;
use vst3::{Class, ComWrapper, Steinberg::*};

use super::ResonatorVst3Controller;

const EDITOR_WIDTH: i32 = 960;
const EDITOR_HEIGHT: i32 = 640;

pub(super) fn create_editor_view(controller: &ResonatorVst3Controller) -> *mut IPlugView {
    ComWrapper::new(ResonatorEditorView::new(controller))
        .to_com_ptr::<IPlugView>()
        .unwrap()
        .into_raw()
}

struct ResonatorEditorView {
    controller: *const ResonatorVst3Controller,
    frame: Cell<*mut IPlugFrame>,
    size: Cell<ViewRect>,
    #[cfg(target_os = "macos")]
    editor: RefCell<Option<macos::ViziaEditor>>,
}

impl ResonatorEditorView {
    fn new(controller: &ResonatorVst3Controller) -> Self {
        Self {
            controller,
            frame: Cell::new(ptr::null_mut()),
            size: Cell::new(default_view_rect()),
            #[cfg(target_os = "macos")]
            editor: RefCell::new(None),
        }
    }
}

impl Class for ResonatorEditorView {
    type Interfaces = (IPlugView,);
}

impl IPlugViewTrait for ResonatorEditorView {
    unsafe fn isPlatformTypeSupported(&self, r#type: FIDString) -> tresult {
        #[cfg(target_os = "macos")]
        {
            if is_ns_view_platform(r#type) {
                kResultTrue
            } else {
                kResultFalse
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = r#type;
            kResultFalse
        }
    }

    unsafe fn attached(&self, parent: *mut c_void, r#type: FIDString) -> tresult {
        if parent.is_null() {
            return kInvalidArgument;
        }
        if !is_ns_view_platform(r#type) {
            return kResultFalse;
        }

        #[cfg(target_os = "macos")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            *editor = Some(macos::ViziaEditor::attach(
                parent,
                self.controller,
                self.size.get(),
            ));
            kResultOk
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = parent;
            let _ = self.controller;
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

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyDown(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyUp(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        *size = self.size.get();
        kResultOk
    }

    unsafe fn onSize(&self, newSize: *mut ViewRect) -> tresult {
        if newSize.is_null() {
            return kInvalidArgument;
        }
        let size = clamped_view_rect(*newSize);
        self.size.set(size);
        kResultOk
    }

    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setFrame(&self, frame: *mut IPlugFrame) -> tresult {
        self.frame.set(frame);
        kResultOk
    }

    unsafe fn canResize(&self) -> tresult {
        kResultFalse
    }

    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        if rect.is_null() {
            return kInvalidArgument;
        }
        *rect = default_view_rect();
        kResultOk
    }
}

fn default_view_rect() -> ViewRect {
    ViewRect {
        left: 0,
        top: 0,
        right: EDITOR_WIDTH,
        bottom: EDITOR_HEIGHT,
    }
}

fn clamped_view_rect(rect: ViewRect) -> ViewRect {
    ViewRect {
        left: rect.left,
        top: rect.top,
        right: rect.left + EDITOR_WIDTH,
        bottom: rect.top + EDITOR_HEIGHT,
    }
}

fn is_ns_view_platform(platform: FIDString) -> bool {
    if platform.is_null() {
        return false;
    }
    unsafe { CStr::from_ptr(platform as *const c_char).to_bytes() == b"NSView" }
}

#[cfg(target_os = "macos")]
unsafe fn set_parameter_from_editor(
    controller: *const ResonatorVst3Controller,
    parameter_id: u32,
    normalized: f64,
) {
    if controller.is_null() {
        return;
    }

    let controller = &*controller;
    if controller.set_value(parameter_id, normalized) != kResultOk {
        return;
    }

    let handler = controller.handler.get();
    if let Some(handler) = ComRef::from_raw(handler) {
        handler.beginEdit(parameter_id);
        handler.performEdit(parameter_id, normalized);
        handler.endEdit(parameter_id);
    }
}

#[cfg(target_os = "macos")]
fn parameter_value_text(parameter_id: u32, normalized: f64) -> String {
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

#[cfg(target_os = "macos")]
mod macos {
    use std::{ffi::c_void, path::PathBuf, time::Duration};

    use ahara_ui::{PadId, UiCommand, UiCommandState, command_label};
    use rfd::FileDialog;
    use vizia::{
        ParentWindow, WindowHandle, WindowScalePolicy,
        icons::{
            ICON_ACTIVITY, ICON_ADJUSTMENTS_HORIZONTAL, ICON_DOWNLOAD, ICON_FILTER,
            ICON_FOLDER_OPEN, ICON_LIBRARY, ICON_ROUTE, ICON_TRASH, ICON_VOLUME_2, ICON_WAVE_SINE,
        },
        prelude::*,
        vg,
    };
    use vst3::Steinberg::{ViewRect, kResultOk};

    use super::super::{
        EditorPatchSummary, EditorSampleSummary, EditorSlotSummary, EditorTelemetry,
        EditorWaveformPoint, default_library_paths, parameter_index,
    };
    use super::{
        EDITOR_HEIGHT, EDITOR_WIDTH, ResonatorVst3Controller, parameter_value_text,
        set_parameter_from_editor,
    };
    use crate::parameters::{EditorSignalId, editor_parameter_binding, editor_parameter_bindings};

    const STYLE: &str = r#"
        :root {
            background-color: #101315;
            color: #d9e1dd;
            font-size: 12px;
        }

        label {
            color: #cbd4cf;
        }

        .muted {
            color: #7e8a86;
        }

        .title {
            font-size: 17px;
            color: #edf5ef;
        }

        .section-title {
            font-size: 12px;
            color: #edf5ef;
        }

        .root {
            background-color: #101315;
        }

        .topbar {
            background-color: #171b1d;
            border-width: 1px;
            border-color: #283036;
            border-radius: 8px;
            padding: 14px;
        }

        .panel {
            background-color: #151a1d;
            border-width: 1px;
            border-color: #283139;
            border-radius: 8px;
            padding: 14px;
        }

        .strip {
            background-color: #111619;
            border-width: 1px;
            border-color: #263239;
            border-radius: 6px;
        }

        .slot-row {
            background-color: #1b2124;
            border-width: 1px;
            border-color: #2f3a40;
            border-radius: 6px;
            padding: 9px;
        }

        .slot-active {
            border-color: #6da684;
        }

        .sample-row {
            background-color: #1b2124;
            border-width: 1px;
            border-color: #2f3a40;
            border-radius: 6px;
            padding: 6px;
        }

        .sample-selected {
            border-color: #7fc49c;
            background-color: #202a25;
        }

        .chip {
            background-color: #20282d;
            border-width: 1px;
            border-color: #37434a;
            border-radius: 6px;
            color: #b9c7c0;
            font-size: 10px;
            padding-left: 8px;
            padding-right: 8px;
        }

        .chip-on {
            background-color: #26392f;
            border-color: #6da684;
            color: #d8efe0;
        }

        .chip-warm {
            background-color: #3a3124;
            border-color: #b2844c;
            color: #efd8b7;
        }

        button.toolbar-button {
            background-color: #20272b;
            border-width: 1px;
            border-color: #39454d;
            border-radius: 6px;
            color: #dce6e0;
        }

        button.toolbar-button:hover {
            background-color: #263139;
            border-color: #6d91a6;
        }

        .segmented {
            background-color: #0f1417;
            border-width: 1px;
            border-color: #2c373e;
            border-radius: 6px;
            padding: 2px;
        }

        button.seg-button {
            background-color: transparent;
            border-width: 0px;
            border-radius: 4px;
            color: #8f9c97;
            font-size: 10px;
        }

        button.seg-button:hover {
            background-color: #20282d;
            color: #d9e1dd;
        }

        button.seg-active {
            background-color: #2b4436;
            color: #e5f5e9;
        }

        .toolbar-icon {
            color: #dce6e0;
            width: 17px;
            height: 17px;
        }

        .meter-label {
            color: #8f9c97;
            font-size: 10px;
        }

        .value-label {
            color: #e8f0ea;
            font-size: 11px;
        }

        knob {
            width: 54px;
            height: 54px;
        }

        .knob-track {
            color: #7fc49c;
            background-color: #263036;
        }

        .knob-head {
            color: #eef6f0;
        }

        .knob-tick {
            background-color: #eef6f0;
            width: 3px;
            height: 16px;
            border-radius: 2px;
        }

        slider {
            height: 22px;
        }

        slider .track {
            background-color: #253038;
            border-radius: 4px;
        }

        slider .active {
            background-color: #82bc98;
            border-radius: 4px;
        }

        slider .thumb {
            background-color: #e8f0ea;
            border-color: #0f1214;
            border-width: 1px;
            border-radius: 6px;
            width: 13px;
            height: 18px;
        }

        .tooltip {
            background-color: #20272b;
            border-width: 1px;
            border-color: #48545c;
            border-radius: 5px;
        }
    "#;

    pub(super) struct ViziaEditor {
        window: WindowHandle,
    }

    impl ViziaEditor {
        pub(super) unsafe fn attach(
            parent: *mut c_void,
            controller: *const ResonatorVst3Controller,
            size: ViewRect,
        ) -> Self {
            let controller = ControllerHandle(controller as usize);
            if !controller.as_ptr().is_null() {
                let _ = (*controller.as_ptr()).refresh_library();
            }
            let values = EditorValues::from_controller(controller);
            let parent = ParentWindow(parent);
            let window = build_application(controller, values, size).open_parented(&parent);
            Self { window }
        }
    }

    impl Drop for ViziaEditor {
        fn drop(&mut self) {
            if self.window.is_open() {
                self.window.close();
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ControllerHandle(usize);

    impl ControllerHandle {
        const fn as_ptr(self) -> *const ResonatorVst3Controller {
            self.0 as *const ResonatorVst3Controller
        }
    }

    #[derive(Clone)]
    struct EditorValues {
        master: f32,
        cutoff: f32,
        saturation: f32,
        pan: f32,
        resonance: f32,
        filter_mode: f32,
        routing: f32,
        retrigger_resonators: f32,
        resonator_a_model: f32,
        resonator_a_preset: f32,
        resonator_a_brightness: f32,
        resonator_a_decay: f32,
        resonator_a_waveguide_style: f32,
        resonator_a_boundary_reflection: f32,
        resonator_b_model: f32,
        resonator_b_loop_filter: f32,
        resonator_b_loop_gain: f32,
        resonator_b_nonlinearity: f32,
        resonator_b_waveguide_style: f32,
        resonator_b_boundary_reflection: f32,
        amp_attack: f32,
        amp_release: f32,
        lfo_rate: f32,
        lfo_shape: f32,
        mod1_enabled: f32,
        mod1_source: f32,
        mod1_destination: f32,
        mod1_amount: f32,
        selected_slot: f32,
        selected_sample: f32,
        command_status: f32,
        telemetry: EditorTelemetry,
        summary: EditorPatchSummary,
    }

    impl EditorValues {
        unsafe fn from_controller(controller: ControllerHandle) -> Self {
            Self {
                master: editor_value(controller, EditorSignalId::Master),
                cutoff: editor_value(controller, EditorSignalId::Cutoff),
                saturation: editor_value(controller, EditorSignalId::Saturation),
                pan: editor_value(controller, EditorSignalId::Pan),
                resonance: editor_value(controller, EditorSignalId::Resonance),
                filter_mode: editor_value(controller, EditorSignalId::FilterMode),
                routing: editor_value(controller, EditorSignalId::Routing),
                retrigger_resonators: editor_value(controller, EditorSignalId::RetriggerResonators),
                resonator_a_model: editor_value(controller, EditorSignalId::ResonatorAModel),
                resonator_a_preset: editor_value(controller, EditorSignalId::ResonatorAPreset),
                resonator_a_brightness: editor_value(
                    controller,
                    EditorSignalId::ResonatorABrightness,
                ),
                resonator_a_decay: editor_value(controller, EditorSignalId::ResonatorADecay),
                resonator_a_waveguide_style: editor_value(
                    controller,
                    EditorSignalId::ResonatorAWaveguideStyle,
                ),
                resonator_a_boundary_reflection: editor_value(
                    controller,
                    EditorSignalId::ResonatorABoundaryReflection,
                ),
                resonator_b_model: editor_value(controller, EditorSignalId::ResonatorBModel),
                resonator_b_loop_filter: editor_value(
                    controller,
                    EditorSignalId::ResonatorBLoopFilter,
                ),
                resonator_b_loop_gain: editor_value(controller, EditorSignalId::ResonatorBLoopGain),
                resonator_b_nonlinearity: editor_value(
                    controller,
                    EditorSignalId::ResonatorBNonlinearity,
                ),
                resonator_b_waveguide_style: editor_value(
                    controller,
                    EditorSignalId::ResonatorBWaveguideStyle,
                ),
                resonator_b_boundary_reflection: editor_value(
                    controller,
                    EditorSignalId::ResonatorBBoundaryReflection,
                ),
                amp_attack: editor_value(controller, EditorSignalId::AmpAttack),
                amp_release: editor_value(controller, EditorSignalId::AmpRelease),
                lfo_rate: editor_value(controller, EditorSignalId::LfoRate),
                lfo_shape: editor_value(controller, EditorSignalId::LfoShape),
                mod1_enabled: editor_value(controller, EditorSignalId::Mod1Enabled),
                mod1_source: editor_value(controller, EditorSignalId::Mod1Source),
                mod1_destination: editor_value(controller, EditorSignalId::Mod1Destination),
                mod1_amount: editor_value(controller, EditorSignalId::Mod1Amount),
                selected_slot: 0.0,
                selected_sample: 0.0,
                command_status: 0.0,
                telemetry: controller_telemetry(controller),
                summary: controller_summary(controller),
            }
        }
    }

    unsafe fn editor_value(controller: ControllerHandle, signal: EditorSignalId) -> f32 {
        editor_parameter_binding(signal)
            .map(|binding| controller_value(controller, binding.id().0))
            .unwrap_or_default()
    }

    #[derive(Clone, Copy)]
    struct EditorSignals {
        master: Signal<f32>,
        cutoff: Signal<f32>,
        saturation: Signal<f32>,
        pan: Signal<f32>,
        resonance: Signal<f32>,
        filter_mode: Signal<f32>,
        routing: Signal<f32>,
        retrigger_resonators: Signal<f32>,
        resonator_a_model: Signal<f32>,
        resonator_a_preset: Signal<f32>,
        resonator_a_brightness: Signal<f32>,
        resonator_a_decay: Signal<f32>,
        resonator_a_waveguide_style: Signal<f32>,
        resonator_a_boundary_reflection: Signal<f32>,
        resonator_b_model: Signal<f32>,
        resonator_b_loop_filter: Signal<f32>,
        resonator_b_loop_gain: Signal<f32>,
        resonator_b_nonlinearity: Signal<f32>,
        resonator_b_waveguide_style: Signal<f32>,
        resonator_b_boundary_reflection: Signal<f32>,
        amp_attack: Signal<f32>,
        amp_release: Signal<f32>,
        lfo_rate: Signal<f32>,
        lfo_shape: Signal<f32>,
        mod1_enabled: Signal<f32>,
        mod1_source: Signal<f32>,
        mod1_destination: Signal<f32>,
        mod1_amount: Signal<f32>,
        selected_slot: Signal<f32>,
        selected_sample: Signal<f32>,
        command_status: Signal<f32>,
        left_peak: Signal<f32>,
        right_peak: Signal<f32>,
        left_rms: Signal<f32>,
        right_rms: Signal<f32>,
        active_voices: Signal<f32>,
        patch_name: Signal<String>,
        slot_summaries: Signal<[EditorSlotSummary; 4]>,
        library_samples: Signal<Vec<EditorSampleSummary>>,
    }

    struct EditorModel {
        controller: ControllerHandle,
        signals: EditorSignals,
        ui_state: UiCommandState,
        selected_library_sample: Option<usize>,
    }

    enum EditorEvent {
        SetParameter { id: u32, normalized: f32 },
        Command(UiCommand),
        SelectLibrarySample(usize),
        SyncFromController,
    }

    impl Model for EditorModel {
        fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
            event.map(|editor_event, _| match editor_event {
                EditorEvent::SetParameter { id, normalized } => {
                    update_signal(*id, *normalized, self.signals);
                    unsafe {
                        set_parameter_from_editor(
                            self.controller.as_ptr(),
                            *id,
                            f64::from(normalized.clamp(0.0, 1.0)),
                        );
                    }
                }
                EditorEvent::Command(command) => {
                    self.ui_state.dispatch(*command);
                    if let Some(slot) = self.ui_state.selected_slot {
                        self.signals.selected_slot.set(f32::from(slot.0 - 1));
                    }
                    unsafe {
                        handle_editor_command(
                            self.controller,
                            self.ui_state.last_command,
                            self.selected_library_sample,
                        );
                        sync_summary_from_controller(self.controller, self.signals);
                    }
                    self.signals
                        .command_status
                        .set(command_code(self.ui_state.last_command));
                }
                EditorEvent::SelectLibrarySample(index) => {
                    self.selected_library_sample = Some(*index);
                    self.signals.selected_sample.set(*index as f32);
                }
                EditorEvent::SyncFromController => unsafe {
                    request_telemetry_from_controller(self.controller);
                    sync_signals_from_controller(self.controller, self.signals);
                    sync_telemetry_from_controller(self.controller, self.signals);
                },
            });
        }
    }

    fn build_application(
        controller: ControllerHandle,
        values: EditorValues,
        size: ViewRect,
    ) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
        let width = (size.right - size.left).max(EDITOR_WIDTH) as u32;
        let height = (size.bottom - size.top).max(EDITOR_HEIGHT) as u32;

        vizia::Application::new(move |cx| {
            cx.add_stylesheet(STYLE)
                .expect("failed to add editor style");

            let signals = EditorSignals {
                master: Signal::new(values.master),
                cutoff: Signal::new(values.cutoff),
                saturation: Signal::new(values.saturation),
                pan: Signal::new(values.pan),
                resonance: Signal::new(values.resonance),
                filter_mode: Signal::new(values.filter_mode),
                routing: Signal::new(values.routing),
                retrigger_resonators: Signal::new(values.retrigger_resonators),
                resonator_a_model: Signal::new(values.resonator_a_model),
                resonator_a_preset: Signal::new(values.resonator_a_preset),
                resonator_a_brightness: Signal::new(values.resonator_a_brightness),
                resonator_a_decay: Signal::new(values.resonator_a_decay),
                resonator_a_waveguide_style: Signal::new(values.resonator_a_waveguide_style),
                resonator_a_boundary_reflection: Signal::new(
                    values.resonator_a_boundary_reflection,
                ),
                resonator_b_model: Signal::new(values.resonator_b_model),
                resonator_b_loop_filter: Signal::new(values.resonator_b_loop_filter),
                resonator_b_loop_gain: Signal::new(values.resonator_b_loop_gain),
                resonator_b_nonlinearity: Signal::new(values.resonator_b_nonlinearity),
                resonator_b_waveguide_style: Signal::new(values.resonator_b_waveguide_style),
                resonator_b_boundary_reflection: Signal::new(
                    values.resonator_b_boundary_reflection,
                ),
                amp_attack: Signal::new(values.amp_attack),
                amp_release: Signal::new(values.amp_release),
                lfo_rate: Signal::new(values.lfo_rate),
                lfo_shape: Signal::new(values.lfo_shape),
                mod1_enabled: Signal::new(values.mod1_enabled),
                mod1_source: Signal::new(values.mod1_source),
                mod1_destination: Signal::new(values.mod1_destination),
                mod1_amount: Signal::new(values.mod1_amount),
                selected_slot: Signal::new(values.selected_slot),
                selected_sample: Signal::new(values.selected_sample),
                command_status: Signal::new(values.command_status),
                left_peak: Signal::new(values.telemetry.left_peak),
                right_peak: Signal::new(values.telemetry.right_peak),
                left_rms: Signal::new(values.telemetry.left_rms),
                right_rms: Signal::new(values.telemetry.right_rms),
                active_voices: Signal::new(values.telemetry.active_voices),
                patch_name: Signal::new(values.summary.patch_name.clone()),
                slot_summaries: Signal::new(values.summary.slots.clone()),
                library_samples: Signal::new(values.summary.library_samples.clone()),
            };
            EditorModel {
                controller,
                signals,
                ui_state: UiCommandState::default(),
                selected_library_sample: None,
            }
            .build(cx);

            let sync_timer = cx.add_timer(Duration::from_millis(33), None, |cx, action| {
                if matches!(action, TimerAction::Tick(_)) {
                    cx.emit(EditorEvent::SyncFromController);
                }
            });
            cx.start_timer(sync_timer);

            build_editor(cx, signals);
        })
        .ignore_default_theme()
        .title("Ahara Resonator Synth")
        .inner_size((width, height))
        // Hosts provide the parent NSView in plugin view coordinates. Letting baseview apply
        // Retina/system scaling here makes Vizia render and hit-test in different spaces.
        .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
    }

    fn build_editor(cx: &mut Context, signals: EditorSignals) {
        VStack::new(cx, |cx| {
            top_bar(cx, signals);

            HStack::new(cx, |cx| {
                excitation_column(cx, signals);
                resonator_column(cx, signals);
                output_column(cx, signals);
            })
            .height(Pixels(448.0))
            .horizontal_gap(Pixels(12.0));

            sample_drawer(cx, signals);
        })
        .class("root")
        .size(Stretch(1.0))
        .padding(Pixels(14.0))
        .vertical_gap(Pixels(10.0));
    }

    fn top_bar(cx: &mut Context, signals: EditorSignals) {
        HStack::new(cx, |cx| {
            VStack::new(cx, |cx| {
                Label::new(cx, "Ahara Resonator Synth").class("title");
                Label::new(cx, signals.patch_name).class("muted");
                Label::new(cx, command_status_text(signals.command_status)).class("meter-label");
            })
            .width(Pixels(250.0))
            .height(Stretch(1.0))
            .vertical_gap(Pixels(2.0));

            HStack::new(cx, |cx| {
                icon_button(cx, ICON_FOLDER_OPEN, "Browse patches", UiCommand::LoadPatch);
                icon_button(
                    cx,
                    ICON_DOWNLOAD,
                    "Export patch",
                    UiCommand::ExportPatchWithSamples,
                );
                icon_button(cx, ICON_LIBRARY, "Sample library", UiCommand::OpenLibrary);
                icon_button(
                    cx,
                    ICON_ADJUSTMENTS_HORIZONTAL,
                    "Save patch",
                    UiCommand::SavePatch,
                );
            })
            .width(Pixels(176.0))
            .height(Pixels(32.0))
            .horizontal_gap(Pixels(8.0));

            Spacer::new(cx);

            HStack::new(cx, |cx| {
                Svg::new(cx, ICON_ACTIVITY).class("toolbar-icon");
                Label::new(cx, "MIDI").class("value-label");
                Element::new(cx)
                    .class("chip-on")
                    .width(Pixels(54.0))
                    .height(Pixels(20.0))
                    .text("Live");
            })
            .alignment(Alignment::Center)
            .width(Pixels(132.0))
            .horizontal_gap(Pixels(8.0));

            LevelMeter::new(cx, signals.left_peak, signals.right_peak)
                .width(Pixels(170.0))
                .height(Pixels(30.0));
        })
        .class("topbar")
        .height(Pixels(58.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(18.0));
    }

    fn excitation_column(cx: &mut Context, signals: EditorSignals) {
        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Svg::new(cx, ICON_WAVE_SINE).class("toolbar-icon");
                Label::new(cx, "Excitation").class("section-title");
                Spacer::new(cx);
                Label::new(cx, "4 slots").class("muted");
            })
            .height(Pixels(22.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(8.0));

            WaveformStrip::new(cx, 0.82)
                .class("strip")
                .height(Pixels(92.0))
                .width(Stretch(1.0));

            for slot in 0..4 {
                excitation_slot(cx, slot, signals);
            }
        })
        .class("panel")
        .width(Pixels(244.0))
        .height(Stretch(1.0))
        .vertical_gap(Pixels(12.0));
    }

    fn excitation_slot(cx: &mut Context, slot: usize, signals: EditorSignals) {
        let slot_id = PadId::new(slot as u8 + 1).unwrap();
        Button::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                MiniWaveform::new(cx, slot_waveform_phase(signals.slot_summaries, slot))
                    .width(Pixels(68.0))
                    .height(Pixels(36.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, slot_label(signals.slot_summaries, slot)).class("value-label");
                    Label::new(cx, slot_detail(signals.slot_summaries, slot)).class("muted");
                })
                .width(Pixels(82.0))
                .vertical_gap(Pixels(1.0));

                VStack::new(cx, |cx| {
                    Element::new(cx)
                        .class("chip")
                        .toggle_class("chip-on", slot_pitch_track(signals.slot_summaries, slot))
                        .width(Pixels(34.0))
                        .height(Pixels(20.0))
                        .text("PT");
                    Element::new(cx)
                        .class("chip")
                        .toggle_class("chip-warm", slot_looping(signals.slot_summaries, slot))
                        .width(Pixels(34.0))
                        .height(Pixels(20.0))
                        .text("M");
                })
                .width(Pixels(38.0))
                .vertical_gap(Pixels(4.0));
            })
        })
        .on_press(move |cx| {
            cx.emit(EditorEvent::Command(UiCommand::SelectExcitationSlot(
                slot_id,
            )));
        })
        .class("slot-row")
        .toggle_class(
            "slot-active",
            signals
                .selected_slot
                .map(move |selected| selected.round() as usize == slot),
        )
        .height(Pixels(58.0))
        .width(Stretch(1.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(10.0));
    }

    fn resonator_column(cx: &mut Context, signals: EditorSignals) {
        VStack::new(cx, |cx| {
            resonator_header(cx, signals);

            ResonatorScope::new(
                cx,
                signals.left_rms,
                signals.right_rms,
                signals.active_voices,
            )
            .class("strip")
            .height(Pixels(122.0))
            .width(Stretch(1.0));

            HStack::new(cx, |cx| {
                resonator_a_panel(cx, signals);
                resonator_b_panel(cx, signals);
            })
            .height(Pixels(206.0))
            .horizontal_gap(Pixels(12.0));

            HStack::new(cx, |cx| {
                Svg::new(cx, ICON_ROUTE).class("toolbar-icon");
                Label::new(cx, "Routing").class("value-label");
                Spacer::new(cx);
                binary_switch(cx, 10, signals.routing, "Parallel", "Series");
            })
            .height(Pixels(28.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(8.0));
        })
        .class("panel")
        .width(Pixels(384.0))
        .height(Stretch(1.0))
        .vertical_gap(Pixels(14.0));
    }

    fn resonator_header(cx: &mut Context, signals: EditorSignals) {
        HStack::new(cx, |cx| {
            Label::new(cx, "Resonators").class("section-title");
            Spacer::new(cx);
            binary_switch(cx, 13, signals.retrigger_resonators, "Carry", "Retrig");
        })
        .height(Pixels(28.0))
        .alignment(Alignment::Center);
    }

    fn resonator_a_panel(cx: &mut Context, signals: EditorSignals) {
        resonator_panel(
            cx,
            "A",
            "Resonator A",
            20,
            signals.resonator_a_model,
            35,
            signals.resonator_a_waveguide_style,
            [
                (21, "Preset", signals.resonator_a_preset),
                (26, "Bright", signals.resonator_a_brightness),
                (27, "Decay", signals.resonator_a_decay),
                (36, "Reflect", signals.resonator_a_boundary_reflection),
            ],
            0.72,
        );
    }

    fn resonator_b_panel(cx: &mut Context, signals: EditorSignals) {
        resonator_panel(
            cx,
            "B",
            "Resonator B",
            40,
            signals.resonator_b_model,
            55,
            signals.resonator_b_waveguide_style,
            [
                (50, "Filter", signals.resonator_b_loop_filter),
                (52, "Loop", signals.resonator_b_loop_gain),
                (53, "Drive", signals.resonator_b_nonlinearity),
                (56, "Reflect", signals.resonator_b_boundary_reflection),
            ],
            0.56,
        );
    }

    fn resonator_panel(
        cx: &mut Context,
        slot: &'static str,
        title: &'static str,
        model_id: u32,
        model: Signal<f32>,
        style_id: u32,
        style: Signal<f32>,
        controls: [(u32, &'static str, Signal<f32>); 4],
        energy: f32,
    ) {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, slot).class("title");
                Label::new(cx, title).class("value-label");
                Spacer::new(cx);
                ResonatorBadge::new(cx, model)
                    .height(Pixels(20.0))
                    .width(Pixels(50.0));
            })
            .height(Pixels(34.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(10.0));

            HStack::new(cx, |cx| {
                compact_binary_switch(cx, model_id, model, "Modal", "Wave");
                compact_binary_switch(cx, style_id, style, "String", "Tube");
            })
            .height(Pixels(26.0))
            .horizontal_gap(Pixels(8.0));

            MeterTrack::new(cx, energy, Color::rgb(124, 188, 148))
                .height(Pixels(8.0))
                .width(Stretch(1.0));
            MeterTrack::new(cx, 1.0 - energy * 0.5, Color::rgb(121, 156, 204))
                .height(Pixels(8.0))
                .width(Stretch(1.0));

            for (id, label, signal) in controls {
                parameter_slider(cx, id, label, signal);
            }
        })
        .class("strip")
        .height(Stretch(1.0))
        .width(Stretch(1.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(7.0));
    }

    fn output_column(cx: &mut Context, signals: EditorSignals) {
        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Svg::new(cx, ICON_VOLUME_2).class("toolbar-icon");
                Label::new(cx, "Output").class("section-title");
                Spacer::new(cx);
                Label::new(cx, "Smoothed").class("muted");
            })
            .height(Pixels(22.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(8.0));

            HStack::new(cx, |cx| {
                parameter_knob(cx, 1, "Master", signals.master);
                parameter_knob(cx, 5, "Pan", signals.pan);
                parameter_knob(cx, 4, "Saturate", signals.saturation);
            })
            .height(Pixels(96.0))
            .horizontal_gap(Pixels(0.0));

            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    Svg::new(cx, ICON_FILTER).class("toolbar-icon");
                    Label::new(cx, "Filter").class("value-label");
                    Spacer::new(cx);
                    Label::new(cx, value_text(3, signals.cutoff)).class("value-label");
                })
                .height(Pixels(22.0))
                .alignment(Alignment::Center)
                .horizontal_gap(Pixels(8.0));

                parameter_slider(cx, 3, "Cutoff", signals.cutoff);
                parameter_slider(cx, 6, "Res", signals.resonance);
                parameter_slider(cx, 7, "Mode", signals.filter_mode);
            })
            .class("strip")
            .height(Pixels(102.0))
            .padding(Pixels(10.0))
            .vertical_gap(Pixels(7.0));

            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    Label::new(cx, "Envelope").class("value-label");
                    Spacer::new(cx);
                    Label::new(cx, value_text(63, signals.amp_release)).class("value-label");
                })
                .height(Pixels(20.0))
                .alignment(Alignment::Center);
                parameter_slider(cx, 60, "Attack", signals.amp_attack);
                parameter_slider(cx, 63, "Release", signals.amp_release);
                ActivationBars::new(
                    cx,
                    signals.active_voices,
                    signals.left_rms,
                    signals.right_rms,
                )
                .height(Pixels(18.0))
                .width(Stretch(1.0));
            })
            .class("strip")
            .height(Pixels(98.0))
            .padding(Pixels(10.0))
            .vertical_gap(Pixels(5.0));

            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    Label::new(cx, "Modulation").class("value-label");
                    Spacer::new(cx);
                    Label::new(cx, "4 slots").class("muted");
                })
                .height(Pixels(18.0))
                .alignment(Alignment::Center);
                parameter_slider(cx, 68, "LFO", signals.lfo_rate);
                parameter_slider(cx, 69, "Shape", signals.lfo_shape);
                parameter_slider(cx, 80, "Enable", signals.mod1_enabled);
                parameter_slider(cx, 81, "Source", signals.mod1_source);
                parameter_slider(cx, 82, "Target", signals.mod1_destination);
                parameter_slider(cx, 83, "Amount", signals.mod1_amount);
            })
            .class("strip")
            .height(Pixels(130.0))
            .padding(Pixels(10.0))
            .vertical_gap(Pixels(6.0));
        })
        .class("panel")
        .width(Pixels(284.0))
        .height(Stretch(1.0))
        .vertical_gap(Pixels(8.0));
    }

    fn binary_switch(
        cx: &mut Context,
        id: u32,
        signal: Signal<f32>,
        left_label: &'static str,
        right_label: &'static str,
    ) {
        segmented_switch(cx, id, signal, left_label, right_label, 144.0);
    }

    fn compact_binary_switch(
        cx: &mut Context,
        id: u32,
        signal: Signal<f32>,
        left_label: &'static str,
        right_label: &'static str,
    ) {
        segmented_switch(cx, id, signal, left_label, right_label, 118.0);
    }

    fn segmented_switch(
        cx: &mut Context,
        id: u32,
        signal: Signal<f32>,
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    ) {
        HStack::new(cx, move |cx| {
            binary_switch_button(cx, id, signal, 0.0, left_label);
            binary_switch_button(cx, id, signal, 1.0, right_label);
        })
        .class("segmented")
        .height(Pixels(26.0))
        .width(Pixels(width))
        .horizontal_gap(Pixels(2.0));
    }

    fn binary_switch_button(
        cx: &mut Context,
        id: u32,
        signal: Signal<f32>,
        normalized: f32,
        label: &'static str,
    ) {
        Button::new(cx, move |cx| {
            Label::new(cx, label).alignment(Alignment::Center)
        })
        .on_press(move |cx| {
            cx.emit(EditorEvent::SetParameter { id, normalized });
        })
        .class("seg-button")
        .toggle_class(
            "seg-active",
            signal.map(move |value| (value - normalized).abs() < 0.25),
        )
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    }

    fn parameter_knob(cx: &mut Context, id: u32, label: &'static str, signal: Signal<f32>) {
        VStack::new(cx, move |cx| {
            Knob::new(cx, default_normalized(id), signal, false).on_change(
                move |cx, normalized| {
                    cx.emit(EditorEvent::SetParameter { id, normalized });
                },
            );
            Label::new(cx, label)
                .class("value-label")
                .alignment(Alignment::Center)
                .width(Pixels(84.0));
            Label::new(cx, value_text(id, signal))
                .class("muted")
                .alignment(Alignment::Center)
                .width(Pixels(84.0));
        })
        .width(Pixels(92.0))
        .height(Pixels(88.0))
        .alignment(Alignment::Center)
        .vertical_gap(Pixels(3.0));
    }

    fn parameter_slider(cx: &mut Context, id: u32, label: &'static str, signal: Signal<f32>) {
        HStack::new(cx, move |cx| {
            Label::new(cx, label)
                .class("meter-label")
                .width(Pixels(46.0));
            Slider::new(cx, signal)
                .range(0.0..1.0)
                .on_change(move |cx, normalized| {
                    cx.emit(EditorEvent::SetParameter { id, normalized });
                })
                .width(Stretch(1.0));
            Label::new(cx, value_text(id, signal))
                .class("value-label")
                .width(Pixels(78.0));
        })
        .height(Pixels(18.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
    }

    fn sample_drawer(cx: &mut Context, signals: EditorSignals) {
        HStack::new(cx, |cx| {
            VStack::new(cx, |cx| {
                Label::new(cx, "Sample Library").class("section-title");
                Label::new(cx, library_count_text(signals.library_samples)).class("muted");
            })
            .width(Pixels(112.0))
            .vertical_gap(Pixels(2.0));

            List::new(cx, signals.library_samples, move |cx, index, item| {
                library_sample_row(cx, index, item, signals);
            })
            .class("strip")
            .width(Pixels(558.0))
            .height(Pixels(54.0));

            icon_button(cx, ICON_LIBRARY, "Open library", UiCommand::OpenLibrary);
            icon_button(
                cx,
                ICON_DOWNLOAD,
                "Load selected slot",
                UiCommand::LoadSelectedExcitationSlot,
            );
            icon_button(
                cx,
                ICON_TRASH,
                "Clear selected slot",
                UiCommand::ClearSelectedExcitationSlot,
            );
        })
        .class("panel")
        .height(Pixels(72.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(14.0));
    }

    fn library_sample_row(
        cx: &mut Context,
        index: usize,
        item: impl SignalGet<EditorSampleSummary> + Copy + 'static,
        signals: EditorSignals,
    ) {
        Button::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                LibraryWaveform::new(cx, signals.library_samples, index)
                    .width(Pixels(84.0))
                    .height(Pixels(32.0));
                VStack::new(cx, move |cx| {
                    Label::new(cx, Memo::new(move |_| item.get().label)).class("value-label");
                    Label::new(cx, Memo::new(move |_| item.get().detail)).class("muted");
                })
                .vertical_gap(Pixels(1.0));
            })
            .horizontal_gap(Pixels(8.0))
            .alignment(Alignment::Center)
        })
        .on_press(move |cx| {
            cx.emit(EditorEvent::SelectLibrarySample(index));
        })
        .class("sample-row")
        .toggle_class(
            "sample-selected",
            signals
                .selected_sample
                .map(move |selected| selected.round() as usize == index),
        )
        .height(Pixels(46.0))
        .width(Stretch(1.0));
    }

    fn icon_button(
        cx: &mut Context,
        icon: &'static str,
        tooltip: &'static str,
        command: UiCommand,
    ) {
        Button::new(cx, move |cx| Svg::new(cx, icon).class("toolbar-icon"))
            .on_press(move |cx| {
                cx.emit(EditorEvent::Command(command));
            })
            .class("toolbar-button")
            .width(Pixels(34.0))
            .height(Pixels(30.0))
            .tooltip(move |cx| {
                Tooltip::new(cx, move |cx| {
                    Label::new(cx, tooltip).padding(Pixels(5.0));
                })
                .class("tooltip")
                .padding(Pixels(3.0))
                .size(Auto)
                .placement(Placement::Bottom)
            });
    }

    fn value_text(id: u32, signal: Signal<f32>) -> Memo<String> {
        Memo::new(move |_| parameter_value_text(id, f64::from(signal.get())))
    }

    fn command_status_text(signal: Signal<f32>) -> Memo<String> {
        Memo::new(move |_| command_label(command_from_code(signal.get())).to_string())
    }

    fn command_code(command: Option<UiCommand>) -> f32 {
        match command {
            Some(UiCommand::SavePatch) => 1.0,
            Some(UiCommand::LoadPatch) => 2.0,
            Some(UiCommand::ExportPatchWithSamples) => 3.0,
            Some(UiCommand::OpenLibrary) => 4.0,
            Some(UiCommand::LoadSelectedExcitationSlot) => 5.0,
            Some(UiCommand::ClearSelectedExcitationSlot) => 6.0,
            Some(UiCommand::SelectExcitationSlot(slot)) => 10.0 + f32::from(slot.0),
            Some(UiCommand::LoadExcitationSlot(slot)) => 20.0 + f32::from(slot.0),
            Some(UiCommand::ClearExcitationSlot(slot)) => 30.0 + f32::from(slot.0),
            Some(UiCommand::RedetectSlices) => 40.0,
            Some(UiCommand::TuneSelectedSlice) => 41.0,
            Some(UiCommand::TuneAllSlices) => 42.0,
            Some(UiCommand::SnapAllSlicesToScale) => 43.0,
            None => 0.0,
        }
    }

    fn command_from_code(code: f32) -> Option<UiCommand> {
        match code.round() as i32 {
            1 => Some(UiCommand::SavePatch),
            2 => Some(UiCommand::LoadPatch),
            3 => Some(UiCommand::ExportPatchWithSamples),
            4 => Some(UiCommand::OpenLibrary),
            5 => Some(UiCommand::LoadSelectedExcitationSlot),
            6 => Some(UiCommand::ClearSelectedExcitationSlot),
            11..=14 => {
                PadId::new((code.round() as i32 - 10) as u8).map(UiCommand::SelectExcitationSlot)
            }
            21..=24 => {
                PadId::new((code.round() as i32 - 20) as u8).map(UiCommand::LoadExcitationSlot)
            }
            31..=34 => {
                PadId::new((code.round() as i32 - 30) as u8).map(UiCommand::ClearExcitationSlot)
            }
            40 => Some(UiCommand::RedetectSlices),
            41 => Some(UiCommand::TuneSelectedSlice),
            42 => Some(UiCommand::TuneAllSlices),
            43 => Some(UiCommand::SnapAllSlicesToScale),
            _ => None,
        }
    }

    unsafe fn handle_editor_command(
        controller: ControllerHandle,
        command: Option<UiCommand>,
        selected_sample: Option<usize>,
    ) {
        if controller.as_ptr().is_null() {
            return;
        }
        let controller = &*controller.as_ptr();
        match command {
            Some(UiCommand::SavePatch) => {
                if let Some(path) = FileDialog::new()
                    .add_filter("Ahara Patch", &["toml"])
                    .set_directory(default_patch_dir())
                    .set_file_name("Ahara Resonator Patch.toml")
                    .save_file()
                {
                    let _ = controller.save_patch_to_path(&path);
                }
            }
            Some(UiCommand::LoadPatch) => {
                if let Some(path) = FileDialog::new()
                    .add_filter("Ahara Patch", &["toml"])
                    .set_directory(default_patch_dir())
                    .pick_file()
                {
                    let _ = controller.load_patch_from_path(&path);
                }
            }
            Some(UiCommand::ExportPatchWithSamples) => {
                if let Some(directory) = FileDialog::new()
                    .set_directory(default_export_dir())
                    .pick_folder()
                {
                    let _ = controller.export_patch_bundle(&directory);
                }
            }
            Some(UiCommand::OpenLibrary) => {
                if let Some(path) = sample_dialog().pick_file() {
                    let _ = controller.ingest_sample(path);
                } else {
                    let _ = controller.refresh_library();
                }
            }
            Some(UiCommand::LoadExcitationSlot(slot)) => {
                assign_sample_to_slot(controller, selected_sample, slot_index(slot));
            }
            Some(UiCommand::ClearExcitationSlot(slot)) => {
                let _ = controller.clear_slot(slot_index(slot));
            }
            _ => {}
        }
    }

    unsafe fn assign_sample_to_slot(
        controller: &ResonatorVst3Controller,
        selected_sample: Option<usize>,
        slot_index: usize,
    ) {
        if let Some(selected_sample) = selected_sample {
            if controller.assign_library_sample_to_slot(selected_sample, slot_index) == kResultOk {
                return;
            }
        }

        if let Some(path) = sample_dialog().pick_file()
            && let Ok(reference) = controller.ingest_sample(path)
        {
            let _ = controller.assign_sample_reference_to_slot(reference, slot_index);
        }
    }

    fn sample_dialog() -> FileDialog {
        FileDialog::new()
            .add_filter("WAV audio", &["wav", "wave"])
            .set_directory(default_sample_dir())
    }

    fn default_patch_dir() -> PathBuf {
        default_library_paths().patches
    }

    fn default_sample_dir() -> PathBuf {
        default_library_paths().samples
    }

    fn default_export_dir() -> PathBuf {
        default_library_paths().root
    }

    fn slot_index(slot: PadId) -> usize {
        usize::from(slot.0.saturating_sub(1)).min(3)
    }

    unsafe fn request_telemetry_from_controller(controller: ControllerHandle) {
        if !controller.as_ptr().is_null() {
            (*controller.as_ptr()).request_telemetry();
        }
    }

    unsafe fn sync_summary_from_controller(controller: ControllerHandle, signals: EditorSignals) {
        let summary = controller_summary(controller);
        signals.patch_name.set(summary.patch_name);
        signals.slot_summaries.set(summary.slots);
        signals.library_samples.set(summary.library_samples);
    }

    impl EditorSignals {
        fn set_parameter(self, signal: EditorSignalId, normalized: f32) {
            match signal {
                EditorSignalId::Master => self.master.set(normalized),
                EditorSignalId::Cutoff => self.cutoff.set(normalized),
                EditorSignalId::Saturation => self.saturation.set(normalized),
                EditorSignalId::Pan => self.pan.set(normalized),
                EditorSignalId::Resonance => self.resonance.set(normalized),
                EditorSignalId::FilterMode => self.filter_mode.set(normalized),
                EditorSignalId::Routing => self.routing.set(normalized),
                EditorSignalId::RetriggerResonators => self.retrigger_resonators.set(normalized),
                EditorSignalId::ResonatorAModel => self.resonator_a_model.set(normalized),
                EditorSignalId::ResonatorAPreset => self.resonator_a_preset.set(normalized),
                EditorSignalId::ResonatorABrightness => {
                    self.resonator_a_brightness.set(normalized);
                }
                EditorSignalId::ResonatorADecay => self.resonator_a_decay.set(normalized),
                EditorSignalId::ResonatorAWaveguideStyle => {
                    self.resonator_a_waveguide_style.set(normalized);
                }
                EditorSignalId::ResonatorABoundaryReflection => {
                    self.resonator_a_boundary_reflection.set(normalized);
                }
                EditorSignalId::ResonatorBModel => self.resonator_b_model.set(normalized),
                EditorSignalId::ResonatorBLoopFilter => {
                    self.resonator_b_loop_filter.set(normalized);
                }
                EditorSignalId::ResonatorBLoopGain => {
                    self.resonator_b_loop_gain.set(normalized);
                }
                EditorSignalId::ResonatorBNonlinearity => {
                    self.resonator_b_nonlinearity.set(normalized);
                }
                EditorSignalId::ResonatorBWaveguideStyle => {
                    self.resonator_b_waveguide_style.set(normalized);
                }
                EditorSignalId::ResonatorBBoundaryReflection => {
                    self.resonator_b_boundary_reflection.set(normalized);
                }
                EditorSignalId::AmpAttack => self.amp_attack.set(normalized),
                EditorSignalId::AmpRelease => self.amp_release.set(normalized),
                EditorSignalId::LfoRate => self.lfo_rate.set(normalized),
                EditorSignalId::LfoShape => self.lfo_shape.set(normalized),
                EditorSignalId::Mod1Enabled => self.mod1_enabled.set(normalized),
                EditorSignalId::Mod1Source => self.mod1_source.set(normalized),
                EditorSignalId::Mod1Destination => self.mod1_destination.set(normalized),
                EditorSignalId::Mod1Amount => self.mod1_amount.set(normalized),
            }
        }
    }

    fn update_signal(id: u32, normalized: f32, signals: EditorSignals) {
        let normalized = normalized.clamp(0.0, 1.0);
        if let Some(binding) = crate::parameter_binding(id)
            && let Some(editor) = binding.editor()
        {
            signals.set_parameter(editor.signal(), normalized);
        }
    }

    unsafe fn sync_signals_from_controller(controller: ControllerHandle, signals: EditorSignals) {
        for binding in editor_parameter_bindings() {
            let parameter_id = binding.id().0;
            update_signal(
                parameter_id,
                controller_value(controller, parameter_id),
                signals,
            );
        }
    }

    unsafe fn sync_telemetry_from_controller(controller: ControllerHandle, signals: EditorSignals) {
        let telemetry = controller_telemetry(controller);
        signals.left_peak.set(telemetry.left_peak);
        signals.right_peak.set(telemetry.right_peak);
        signals.left_rms.set(telemetry.left_rms);
        signals.right_rms.set(telemetry.right_rms);
        signals.active_voices.set(telemetry.active_voices);
    }

    unsafe fn controller_value(controller: ControllerHandle, parameter_id: u32) -> f32 {
        if controller.as_ptr().is_null() {
            return default_normalized(parameter_id);
        }

        let Some(index) = parameter_index(parameter_id) else {
            return default_normalized(parameter_id);
        };
        (*controller.as_ptr()).values.get()[index] as f32
    }

    unsafe fn controller_summary(controller: ControllerHandle) -> EditorPatchSummary {
        if controller.as_ptr().is_null() {
            return EditorPatchSummary::from_patch(&crate::ResonatorSynthPatch::default());
        }

        (*controller.as_ptr()).editor_summary()
    }

    unsafe fn controller_telemetry(controller: ControllerHandle) -> EditorTelemetry {
        if controller.as_ptr().is_null() {
            return EditorTelemetry::default();
        }

        (*controller.as_ptr()).telemetry()
    }

    fn slot_label(slots: Signal<[EditorSlotSummary; 4]>, index: usize) -> Memo<String> {
        Memo::new(move |_| slots.get()[index].label.clone())
    }

    fn slot_detail(slots: Signal<[EditorSlotSummary; 4]>, index: usize) -> Memo<String> {
        Memo::new(move |_| slots.get()[index].detail.clone())
    }

    fn slot_pitch_track(slots: Signal<[EditorSlotSummary; 4]>, index: usize) -> Memo<bool> {
        Memo::new(move |_| slots.get()[index].pitch_track)
    }

    fn slot_looping(slots: Signal<[EditorSlotSummary; 4]>, index: usize) -> Memo<bool> {
        Memo::new(move |_| slots.get()[index].looping)
    }

    fn slot_waveform_phase(slots: Signal<[EditorSlotSummary; 4]>, index: usize) -> Memo<f32> {
        Memo::new(move |_| {
            if slots.get()[index].sample_backed {
                index as f32 * 0.17 + 0.42
            } else {
                index as f32 * 0.21 + 0.2
            }
        })
    }

    fn library_count_text(samples: Signal<Vec<EditorSampleSummary>>) -> Memo<String> {
        Memo::new(move |_| {
            let count = samples.get().len();
            match count {
                0 => "No samples".to_string(),
                1 => "1 sample".to_string(),
                count => format!("{count} samples"),
            }
        })
    }

    fn default_normalized(parameter_id: u32) -> f32 {
        crate::parameter_binding(parameter_id)
            .map(|binding| {
                let parameter = binding.info();
                parameter.range.normalize(parameter.range.default)
            })
            .unwrap_or(0.0)
    }

    struct WaveformStrip {
        emphasis: f32,
    }

    impl WaveformStrip {
        fn new(cx: &mut Context, emphasis: f32) -> Handle<'_, Self> {
            Self {
                emphasis: emphasis.clamp(0.0, 1.0),
            }
            .build(cx, |_| {})
        }
    }

    impl View for WaveformStrip {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            draw_panel_background(bounds, canvas);

            let center_y = bounds.y + bounds.h * 0.5;
            let mut baseline = vg::Paint::default();
            baseline.set_color(Color::rgba(93, 111, 117, 150));
            baseline.set_stroke_width(1.0);
            baseline.set_style(vg::PaintStyle::Stroke);
            let mut baseline_path = vg::PathBuilder::new();
            baseline_path.move_to((bounds.x + 8.0, center_y));
            baseline_path.line_to((bounds.x + bounds.w - 8.0, center_y));
            canvas.draw_path(&baseline_path.detach(), &baseline);

            let mut path = vg::PathBuilder::new();
            let steps = 72;
            for index in 0..steps {
                let t = index as f32 / (steps - 1) as f32;
                let x = bounds.x + 10.0 + t * (bounds.w - 20.0);
                let envelope = (1.0 - t).powf(1.7);
                let wave = (t * 36.0).sin() * 0.62 + (t * 91.0).sin() * 0.25;
                let y = center_y - wave * envelope * bounds.h * (0.20 + self.emphasis * 0.20);
                if index == 0 {
                    path.move_to((x, y));
                } else {
                    path.line_to((x, y));
                }
            }

            let mut paint = vg::Paint::default();
            paint.set_color(Color::rgb(128, 196, 158));
            paint.set_stroke_width(2.0);
            paint.set_stroke_cap(vg::PaintCap::Round);
            paint.set_style(vg::PaintStyle::Stroke);
            canvas.draw_path(&path.detach(), &paint);
        }
    }

    struct MiniWaveform {
        phase: Memo<f32>,
    }

    impl MiniWaveform {
        fn new(cx: &mut Context, phase: Memo<f32>) -> Handle<'_, Self> {
            Self { phase }
                .build(cx, |_| {})
                .bind(phase, |mut view| view.needs_redraw())
        }
    }

    impl View for MiniWaveform {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            draw_panel_background(bounds, canvas);

            let mut path = vg::PathBuilder::new();
            let phase = self.phase.get();
            for index in 0..28 {
                let t = index as f32 / 27.0;
                let x = bounds.x + 5.0 + t * (bounds.w - 10.0);
                let y = bounds.y
                    + bounds.h * 0.5
                    + ((t + phase) * 22.0).sin() * (1.0 - t * 0.55) * bounds.h * 0.28;
                if index == 0 {
                    path.move_to((x, y));
                } else {
                    path.line_to((x, y));
                }
            }

            let mut paint = vg::Paint::default();
            paint.set_color(Color::rgb(121, 156, 204));
            paint.set_stroke_width(1.6);
            paint.set_stroke_cap(vg::PaintCap::Round);
            paint.set_style(vg::PaintStyle::Stroke);
            canvas.draw_path(&path.detach(), &paint);
        }
    }

    struct LibraryWaveform {
        samples: Signal<Vec<EditorSampleSummary>>,
        index: usize,
    }

    impl LibraryWaveform {
        fn new(
            cx: &mut Context,
            samples: Signal<Vec<EditorSampleSummary>>,
            index: usize,
        ) -> Handle<'_, Self> {
            Self { samples, index }
                .build(cx, |_| {})
                .bind(samples, |mut view| view.needs_redraw())
        }
    }

    impl View for LibraryWaveform {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            draw_panel_background(bounds, canvas);

            let samples = self.samples.get();
            let Some(sample) = samples.get(self.index) else {
                return;
            };
            draw_waveform_preview(bounds, canvas, &sample.preview, Color::rgb(121, 156, 204));
        }
    }

    struct ResonatorScope {
        left_rms: Signal<f32>,
        right_rms: Signal<f32>,
        active_voices: Signal<f32>,
    }

    impl ResonatorScope {
        fn new(
            cx: &mut Context,
            left_rms: Signal<f32>,
            right_rms: Signal<f32>,
            active_voices: Signal<f32>,
        ) -> Handle<'_, Self> {
            Self {
                left_rms,
                right_rms,
                active_voices,
            }
            .build(cx, |_| {})
            .bind(left_rms, |mut view| view.needs_redraw())
            .bind(right_rms, |mut view| view.needs_redraw())
            .bind(active_voices, |mut view| view.needs_redraw())
        }
    }

    impl View for ResonatorScope {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            draw_panel_background(bounds, canvas);

            let left_amount = meter_amount(self.left_rms.get());
            let right_amount = meter_amount(self.right_rms.get());
            let voice_amount = (self.active_voices.get() / 8.0).clamp(0.0, 1.0);
            let left = (bounds.x + bounds.w * 0.32, bounds.y + bounds.h * 0.52);
            let right = (bounds.x + bounds.w * 0.68, bounds.y + bounds.h * 0.52);

            draw_connection(canvas, left, right, voice_amount);
            draw_resonator(canvas, left, 38.0, left_amount, Color::rgb(124, 188, 148));
            draw_resonator(canvas, right, 34.0, right_amount, Color::rgb(196, 151, 81));
        }
    }

    struct ResonatorBadge {
        model: Signal<f32>,
    }

    impl ResonatorBadge {
        fn new(cx: &mut Context, model: Signal<f32>) -> Handle<'_, Self> {
            Self { model }
                .build(cx, |_| {})
                .bind(model, |mut view| view.needs_redraw())
        }
    }

    impl View for ResonatorBadge {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            let model = self.model.get().clamp(0.0, 1.0);
            let color = if model < 0.5 {
                Color::rgb(124, 188, 148)
            } else {
                Color::rgb(121, 156, 204)
            };
            draw_meter_track(bounds, canvas, 0.35 + model * 0.55, color);
        }
    }

    struct MeterTrack {
        amount: f32,
        color: Color,
    }

    impl MeterTrack {
        fn new(cx: &mut Context, amount: f32, color: Color) -> Handle<'_, Self> {
            Self {
                amount: amount.clamp(0.0, 1.0),
                color,
            }
            .build(cx, |_| {})
        }
    }

    impl View for MeterTrack {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            draw_meter_track(bounds, canvas, self.amount, self.color);
        }
    }

    struct LevelMeter {
        left_peak: Signal<f32>,
        right_peak: Signal<f32>,
    }

    impl LevelMeter {
        fn new(
            cx: &mut Context,
            left_peak: Signal<f32>,
            right_peak: Signal<f32>,
        ) -> Handle<'_, Self> {
            Self {
                left_peak,
                right_peak,
            }
            .build(cx, |_| {})
            .bind(left_peak, |mut view| view.needs_redraw())
            .bind(right_peak, |mut view| view.needs_redraw())
        }
    }

    impl View for LevelMeter {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            draw_panel_background(bounds, canvas);

            let level = meter_amount(self.left_peak.get().max(self.right_peak.get())).max(0.02);
            for index in 0..18 {
                let t = index as f32 / 17.0;
                let x = bounds.x + 8.0 + index as f32 * ((bounds.w - 16.0) / 18.0);
                let h = 5.0 + (t * std::f32::consts::PI).sin().abs() * 14.0;
                let y = bounds.y + bounds.h - h - 6.0;
                let active = t <= level;
                let color = if !active {
                    Color::rgba(70, 82, 88, 150)
                } else if t > 0.84 {
                    Color::rgb(211, 133, 92)
                } else if t > 0.66 {
                    Color::rgb(196, 151, 81)
                } else {
                    Color::rgb(124, 188, 148)
                };
                draw_rect(
                    canvas,
                    vg::Rect::new(x, y, x + 5.0, bounds.y + bounds.h - 6.0),
                    color,
                );
            }
        }
    }

    struct ActivationBars {
        active_voices: Signal<f32>,
        left_rms: Signal<f32>,
        right_rms: Signal<f32>,
    }

    impl ActivationBars {
        fn new(
            cx: &mut Context,
            active_voices: Signal<f32>,
            left_rms: Signal<f32>,
            right_rms: Signal<f32>,
        ) -> Handle<'_, Self> {
            Self {
                active_voices,
                left_rms,
                right_rms,
            }
            .build(cx, |_| {})
            .bind(active_voices, |mut view| view.needs_redraw())
            .bind(left_rms, |mut view| view.needs_redraw())
            .bind(right_rms, |mut view| view.needs_redraw())
        }
    }

    impl View for ActivationBars {
        fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
            let bounds = cx.bounds();
            let voice_amount = (self.active_voices.get() / 8.0).clamp(0.0, 1.0);
            let rms_amount = meter_amount((self.left_rms.get() + self.right_rms.get()) * 0.5);
            for index in 0..12 {
                let t = index as f32 / 11.0;
                let amount = (rms_amount * 0.7 + voice_amount * 0.3).clamp(0.0, 1.0);
                let h = bounds.h * (0.18 + (t * 5.4).sin().abs() * 0.72 * amount);
                let x = bounds.x + index as f32 * (bounds.w / 12.0) + 2.0;
                draw_rect(
                    canvas,
                    vg::Rect::new(
                        x,
                        bounds.y + bounds.h - h,
                        x + bounds.w / 18.0,
                        bounds.y + bounds.h,
                    ),
                    if t < amount {
                        Color::rgb(124, 188, 148)
                    } else {
                        Color::rgba(71, 84, 90, 130)
                    },
                );
            }
        }
    }

    fn draw_panel_background(bounds: BoundingBox, canvas: &Canvas) {
        draw_rect(
            canvas,
            vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
            Color::rgb(17, 22, 25),
        );
    }

    fn draw_meter_track(bounds: BoundingBox, canvas: &Canvas, amount: f32, color: Color) {
        draw_rect(
            canvas,
            vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
            Color::rgb(35, 44, 50),
        );
        draw_rect(
            canvas,
            vg::Rect::new(
                bounds.x,
                bounds.y,
                bounds.x + bounds.w * amount.clamp(0.0, 1.0),
                bounds.y + bounds.h,
            ),
            color,
        );
    }

    fn draw_waveform_preview(
        bounds: BoundingBox,
        canvas: &Canvas,
        points: &[EditorWaveformPoint],
        color: Color,
    ) {
        let center_y = bounds.y + bounds.h * 0.5;
        let mut baseline = vg::Paint::default();
        baseline.set_color(Color::rgba(81, 96, 102, 120));
        baseline.set_stroke_width(1.0);
        baseline.set_style(vg::PaintStyle::Stroke);
        let mut baseline_path = vg::PathBuilder::new();
        baseline_path.move_to((bounds.x + 5.0, center_y));
        baseline_path.line_to((bounds.x + bounds.w - 5.0, center_y));
        canvas.draw_path(&baseline_path.detach(), &baseline);

        if points.is_empty() {
            return;
        }

        let mut path = vg::PathBuilder::new();
        for (index, point) in points.iter().enumerate() {
            let t = if points.len() <= 1 {
                0.0
            } else {
                index as f32 / (points.len() - 1) as f32
            };
            let x = bounds.x + 5.0 + t * (bounds.w - 10.0);
            let extent = point
                .max
                .abs()
                .max(point.min.abs())
                .max(point.rms)
                .clamp(0.0, 1.0);
            let y = center_y - extent * bounds.h * 0.38;
            if index == 0 {
                path.move_to((x, y));
            } else {
                path.line_to((x, y));
            }
        }

        let mut mirror = vg::PathBuilder::new();
        for (index, point) in points.iter().enumerate() {
            let t = if points.len() <= 1 {
                0.0
            } else {
                index as f32 / (points.len() - 1) as f32
            };
            let x = bounds.x + 5.0 + t * (bounds.w - 10.0);
            let extent = point
                .max
                .abs()
                .max(point.min.abs())
                .max(point.rms)
                .clamp(0.0, 1.0);
            let y = center_y + extent * bounds.h * 0.38;
            if index == 0 {
                mirror.move_to((x, y));
            } else {
                mirror.line_to((x, y));
            }
        }

        let mut paint = vg::Paint::default();
        paint.set_color(color);
        paint.set_stroke_width(1.4);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_style(vg::PaintStyle::Stroke);
        canvas.draw_path(&path.detach(), &paint);
        canvas.draw_path(&mirror.detach(), &paint);
    }

    fn meter_amount(value: f32) -> f32 {
        if value.is_finite() {
            value.abs().sqrt().clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn draw_connection(canvas: &Canvas, left: (f32, f32), right: (f32, f32), amount: f32) {
        let mut path = vg::PathBuilder::new();
        path.move_to((left.0 + 42.0, left.1));
        path.cubic_to(
            (left.0 + 74.0, left.1 - 40.0),
            (right.0 - 74.0, right.1 + 40.0),
            (right.0 - 42.0, right.1),
        );
        let mut paint = vg::Paint::default();
        paint.set_color(Color::rgba(112, 144, 170, (95.0 + amount * 115.0) as u8));
        paint.set_stroke_width(3.0);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_style(vg::PaintStyle::Stroke);
        canvas.draw_path(&path.detach(), &paint);
    }

    fn draw_resonator(canvas: &Canvas, center: (f32, f32), radius: f32, amount: f32, color: Color) {
        let amount = amount.clamp(0.0, 1.0);
        let rings = 4;
        for ring in 0..rings {
            let r = radius + ring as f32 * 10.0;
            let alpha = (70.0 + amount * 110.0 - ring as f32 * 17.0).clamp(20.0, 190.0) as u8;
            let mut paint = vg::Paint::default();
            paint.set_color(with_alpha(color, alpha));
            paint.set_stroke_width(2.0);
            paint.set_style(vg::PaintStyle::Stroke);
            paint.set_anti_alias(true);
            canvas.draw_arc(
                vg::Rect::new(center.0 - r, center.1 - r, center.0 + r, center.1 + r),
                -140.0 + ring as f32 * 14.0,
                220.0 + amount * 100.0,
                false,
                &paint,
            );
        }

        draw_rect(
            canvas,
            vg::Rect::new(
                center.0 - 4.0,
                center.1 - 4.0,
                center.0 + 4.0,
                center.1 + 4.0,
            ),
            Color::rgb(235, 242, 237),
        );
    }

    fn draw_rect(canvas: &Canvas, rect: vg::Rect, color: Color) {
        let mut paint = vg::Paint::default();
        paint.set_color(color);
        paint.set_anti_alias(true);
        canvas.draw_rect(rect, &paint);
    }

    fn with_alpha(color: Color, alpha: u8) -> Color {
        Color::rgba(color.r(), color.g(), color.b(), alpha)
    }
}
