use std::path::{Path, PathBuf};

pub const RESONATOR_EDITOR_WIDTH: i32 = 960;
pub const RESONATOR_EDITOR_HEIGHT: i32 = 640;
pub const RESONATOR_EDITOR_PARAMETER_BINDING_COUNT: usize = ResonatorEditorSurfaceSlot::ALL.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResonatorEditorSurfaceSlot {
    Master,
    Cutoff,
    Saturation,
    Pan,
    Resonance,
    FilterMode,
    Routing,
    RetriggerResonators,
    ResonatorAModel,
    ResonatorAPreset,
    ResonatorABrightness,
    ResonatorADecay,
    ResonatorAWaveguideStyle,
    ResonatorABoundaryReflection,
    ResonatorBModel,
    ResonatorBLoopFilter,
    ResonatorBLoopGain,
    ResonatorBNonlinearity,
    ResonatorBWaveguideStyle,
    ResonatorBBoundaryReflection,
    AmpAttack,
    AmpRelease,
    LfoRate,
    LfoShape,
    Mod1Enabled,
    Mod1Source,
    Mod1Destination,
    Mod1Amount,
}

impl ResonatorEditorSurfaceSlot {
    pub const ALL: [Self; 28] = [
        Self::Master,
        Self::Cutoff,
        Self::Saturation,
        Self::Pan,
        Self::Resonance,
        Self::FilterMode,
        Self::Routing,
        Self::RetriggerResonators,
        Self::ResonatorAModel,
        Self::ResonatorAPreset,
        Self::ResonatorABrightness,
        Self::ResonatorADecay,
        Self::ResonatorAWaveguideStyle,
        Self::ResonatorABoundaryReflection,
        Self::ResonatorBModel,
        Self::ResonatorBLoopFilter,
        Self::ResonatorBLoopGain,
        Self::ResonatorBNonlinearity,
        Self::ResonatorBWaveguideStyle,
        Self::ResonatorBBoundaryReflection,
        Self::AmpAttack,
        Self::AmpRelease,
        Self::LfoRate,
        Self::LfoShape,
        Self::Mod1Enabled,
        Self::Mod1Source,
        Self::Mod1Destination,
        Self::Mod1Amount,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Master => 0,
            Self::Cutoff => 1,
            Self::Saturation => 2,
            Self::Pan => 3,
            Self::Resonance => 4,
            Self::FilterMode => 5,
            Self::Routing => 6,
            Self::RetriggerResonators => 7,
            Self::ResonatorAModel => 8,
            Self::ResonatorAPreset => 9,
            Self::ResonatorABrightness => 10,
            Self::ResonatorADecay => 11,
            Self::ResonatorAWaveguideStyle => 12,
            Self::ResonatorABoundaryReflection => 13,
            Self::ResonatorBModel => 14,
            Self::ResonatorBLoopFilter => 15,
            Self::ResonatorBLoopGain => 16,
            Self::ResonatorBNonlinearity => 17,
            Self::ResonatorBWaveguideStyle => 18,
            Self::ResonatorBBoundaryReflection => 19,
            Self::AmpAttack => 20,
            Self::AmpRelease => 21,
            Self::LfoRate => 22,
            Self::LfoShape => 23,
            Self::Mod1Enabled => 24,
            Self::Mod1Source => 25,
            Self::Mod1Destination => 26,
            Self::Mod1Amount => 27,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResonatorEditorControlKind {
    Knob,
    Slider,
    Binary {
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResonatorEditorParameterBinding {
    id: u32,
    slot: ResonatorEditorSurfaceSlot,
    label: &'static str,
    control: ResonatorEditorControlKind,
}

impl ResonatorEditorParameterBinding {
    pub const fn new(
        id: u32,
        slot: ResonatorEditorSurfaceSlot,
        label: &'static str,
        control: ResonatorEditorControlKind,
    ) -> Self {
        Self {
            id,
            slot,
            label,
            control,
        }
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub const fn slot(self) -> ResonatorEditorSurfaceSlot {
        self.slot
    }

    pub const fn label(self) -> &'static str {
        self.label
    }

    pub const fn control(self) -> ResonatorEditorControlKind {
        self.control
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResonatorEditorTelemetry {
    pub left_peak: f32,
    pub right_peak: f32,
    pub left_rms: f32,
    pub right_rms: f32,
    pub active_voices: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonatorEditorPatchSummary {
    pub patch_name: String,
    pub slots: [ResonatorEditorSlotSummary; 4],
    pub library_samples: Vec<ResonatorEditorSampleSummary>,
}

impl Default for ResonatorEditorPatchSummary {
    fn default() -> Self {
        Self {
            patch_name: "Default".to_string(),
            slots: std::array::from_fn(ResonatorEditorSlotSummary::empty),
            library_samples: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonatorEditorSampleSummary {
    pub label: String,
    pub detail: String,
    pub preview: Vec<ResonatorEditorWaveformPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResonatorEditorWaveformPoint {
    pub min: f32,
    pub max: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResonatorEditorSlotSummary {
    pub label: String,
    pub detail: String,
    pub sample_backed: bool,
    pub pitch_track: bool,
    pub looping: bool,
}

impl ResonatorEditorSlotSummary {
    pub fn empty(index: usize) -> Self {
        Self {
            label: format!("Slot {}", index + 1),
            detail: "Empty layer".to_string(),
            sample_backed: false,
            pitch_track: false,
            looping: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonatorEditorDirectories {
    pub patch_directory: PathBuf,
    pub sample_directory: PathBuf,
    pub export_directory: PathBuf,
}

impl Default for ResonatorEditorDirectories {
    fn default() -> Self {
        Self {
            patch_directory: PathBuf::from("."),
            sample_directory: PathBuf::from("."),
            export_directory: PathBuf::from("."),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorCallbacks {
    pub refresh_library: unsafe fn(usize),
    pub parameter_value: unsafe fn(usize, u32) -> f32,
    pub set_parameter: unsafe fn(usize, u32, f64),
    pub parameter_value_text: unsafe fn(usize, u32, f64) -> String,
    pub default_normalized: unsafe fn(usize, u32) -> f32,
    pub summary: unsafe fn(usize) -> ResonatorEditorPatchSummary,
    pub telemetry: unsafe fn(usize) -> ResonatorEditorTelemetry,
    pub directories: unsafe fn(usize) -> ResonatorEditorDirectories,
    pub request_telemetry: unsafe fn(usize),
    pub handle_command: for<'a> unsafe fn(usize, ResonatorEditorCommandRequest<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorCommandRequest<'a> {
    pub command: crate::UiCommand,
    pub patch_save_path: Option<&'a Path>,
    pub patch_load_path: Option<&'a Path>,
    pub patch_export_directory: Option<&'a Path>,
    pub sample_path: Option<&'a Path>,
    pub selected_library_sample: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResonatorEditorHostError {
    DuplicateSlot(ResonatorEditorSurfaceSlot),
    MissingSlot(ResonatorEditorSurfaceSlot),
}

#[derive(Debug, Clone, Copy)]
pub struct ResonatorEditorHost {
    context: usize,
    parameter_bindings:
        [Option<ResonatorEditorParameterBinding>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT],
    callbacks: ResonatorEditorCallbacks,
}

impl ResonatorEditorHost {
    pub fn new(
        context: usize,
        bindings: impl IntoIterator<Item = ResonatorEditorParameterBinding>,
        callbacks: ResonatorEditorCallbacks,
    ) -> Result<Self, ResonatorEditorHostError> {
        let mut parameter_bindings = [None; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT];
        for binding in bindings {
            let index = binding.slot().index();
            if parameter_bindings[index].is_some() {
                return Err(ResonatorEditorHostError::DuplicateSlot(binding.slot()));
            }
            parameter_bindings[index] = Some(binding);
        }

        for slot in ResonatorEditorSurfaceSlot::ALL {
            if parameter_bindings[slot.index()].is_none() {
                return Err(ResonatorEditorHostError::MissingSlot(slot));
            }
        }

        Ok(Self {
            context,
            parameter_bindings,
            callbacks,
        })
    }

    pub const fn parameter_bindings(
        self,
    ) -> [Option<ResonatorEditorParameterBinding>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT] {
        self.parameter_bindings
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn refresh_library(self) {
        unsafe { (self.callbacks.refresh_library)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn parameter_value(self, id: u32) -> f32 {
        unsafe { (self.callbacks.parameter_value)(self.context, id) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn set_parameter(self, id: u32, normalized: f64) {
        unsafe { (self.callbacks.set_parameter)(self.context, id, normalized) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn parameter_value_text(self, id: u32, normalized: f64) -> String {
        unsafe { (self.callbacks.parameter_value_text)(self.context, id, normalized) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn default_normalized(self, id: u32) -> f32 {
        unsafe { (self.callbacks.default_normalized)(self.context, id) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn summary(self) -> ResonatorEditorPatchSummary {
        unsafe { (self.callbacks.summary)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn telemetry(self) -> ResonatorEditorTelemetry {
        unsafe { (self.callbacks.telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn directories(self) -> ResonatorEditorDirectories {
        unsafe { (self.callbacks.directories)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn request_telemetry(self) {
        unsafe { (self.callbacks.request_telemetry)(self.context) }
    }

    /// # Safety
    /// The callback context must still reference a live host object owned by the plugin editor.
    pub unsafe fn handle_command(self, request: ResonatorEditorCommandRequest<'_>) {
        unsafe { (self.callbacks.handle_command)(self.context, request) }
    }
}

#[cfg(target_os = "macos")]
pub use platform::{ResonatorEditorSize, ResonatorViziaEditor, build_resonator_application};

#[cfg(target_os = "macos")]
mod platform {
    use std::{ffi::c_void, time::Duration};

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

    use super::{
        RESONATOR_EDITOR_HEIGHT, RESONATOR_EDITOR_PARAMETER_BINDING_COUNT, RESONATOR_EDITOR_WIDTH,
        ResonatorEditorCommandRequest, ResonatorEditorControlKind, ResonatorEditorDirectories,
        ResonatorEditorHost, ResonatorEditorParameterBinding, ResonatorEditorPatchSummary,
        ResonatorEditorSampleSummary, ResonatorEditorSlotSummary, ResonatorEditorSurfaceSlot,
        ResonatorEditorTelemetry, ResonatorEditorWaveformPoint,
    };
    use crate::{EditorCommandBus, PadId, UiCommand, command_label};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ResonatorEditorSize {
        pub width: i32,
        pub height: i32,
    }

    impl Default for ResonatorEditorSize {
        fn default() -> Self {
            Self {
                width: RESONATOR_EDITOR_WIDTH,
                height: RESONATOR_EDITOR_HEIGHT,
            }
        }
    }

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

    pub struct ResonatorViziaEditor {
        window: WindowHandle,
    }

    impl ResonatorViziaEditor {
        pub unsafe fn attach(
            parent: *mut c_void,
            host: ResonatorEditorHost,
            size: ResonatorEditorSize,
        ) -> Self {
            unsafe { host.refresh_library() };
            let parent = ParentWindow(parent);
            let window = unsafe { build_resonator_application(host, size) }.open_parented(&parent);
            Self { window }
        }
    }

    impl Drop for ResonatorViziaEditor {
        fn drop(&mut self) {
            if self.window.is_open() {
                self.window.close();
            }
        }
    }

    #[derive(Clone)]
    struct EditorValues {
        parameters: EditorParameterValues,
        selected_slot: f32,
        selected_sample: f32,
        command_status: Option<UiCommand>,
        telemetry: ResonatorEditorTelemetry,
        summary: ResonatorEditorPatchSummary,
    }

    impl EditorValues {
        unsafe fn from_host(host: ResonatorEditorHost) -> Self {
            Self {
                parameters: unsafe { EditorParameterValues::from_host(host) },
                selected_slot: 0.0,
                selected_sample: 0.0,
                command_status: None,
                telemetry: unsafe { host.telemetry() },
                summary: unsafe { host.summary() },
            }
        }
    }

    #[derive(Clone, Copy)]
    struct EditorParameterValue {
        id: u32,
        editor: ResonatorEditorParameterBinding,
        normalized: f32,
    }

    #[derive(Clone, Copy)]
    struct EditorParameterValues {
        entries: [Option<EditorParameterValue>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT],
    }

    impl EditorParameterValues {
        unsafe fn from_host(host: ResonatorEditorHost) -> Self {
            Self {
                entries: host.parameter_bindings().map(|binding| {
                    binding.map(|editor| EditorParameterValue {
                        id: editor.id(),
                        editor,
                        normalized: unsafe { host.parameter_value(editor.id()) },
                    })
                }),
            }
        }
    }

    #[derive(Clone, Copy)]
    struct EditorParameterSignal {
        id: u32,
        editor: ResonatorEditorParameterBinding,
        signal: Signal<f32>,
    }

    #[derive(Clone, Copy)]
    struct EditorParameterSignals {
        entries: [Option<EditorParameterSignal>; RESONATOR_EDITOR_PARAMETER_BINDING_COUNT],
    }

    impl EditorParameterSignals {
        fn new(values: EditorParameterValues) -> Self {
            Self {
                entries: values.entries.map(|entry| {
                    entry.map(|entry| EditorParameterSignal {
                        id: entry.id,
                        editor: entry.editor,
                        signal: Signal::new(entry.normalized),
                    })
                }),
            }
        }

        fn control(
            self,
            slot: ResonatorEditorSurfaceSlot,
            host: ResonatorEditorHost,
        ) -> EditorParameterControl {
            self.entries
                .iter()
                .flatten()
                .find(|entry| entry.editor.slot() == slot)
                .map(|entry| EditorParameterControl {
                    id: entry.id,
                    editor: entry.editor,
                    signal: entry.signal,
                    host,
                })
                .unwrap_or_else(|| panic!("missing editor parameter signal for {slot:?}"))
        }

        fn set_by_id(self, id: u32, normalized: f32) -> bool {
            let Some(entry) = self.entries.iter().flatten().find(|entry| entry.id == id) else {
                return false;
            };
            entry.signal.set(normalized);
            true
        }
    }

    #[derive(Clone, Copy)]
    struct EditorParameterControl {
        id: u32,
        editor: ResonatorEditorParameterBinding,
        signal: Signal<f32>,
        host: ResonatorEditorHost,
    }

    impl EditorParameterControl {
        fn label(self) -> &'static str {
            self.editor.label()
        }

        fn value_text(self) -> Memo<String> {
            let host = self.host;
            let id = self.id;
            let signal = self.signal;
            Memo::new(move |_| unsafe { host.parameter_value_text(id, f64::from(signal.get())) })
        }

        fn binary_labels(self) -> Option<(&'static str, &'static str, f32)> {
            match self.editor.control() {
                ResonatorEditorControlKind::Binary {
                    left_label,
                    right_label,
                    width,
                } => Some((left_label, right_label, width)),
                ResonatorEditorControlKind::Knob | ResonatorEditorControlKind::Slider => None,
            }
        }
    }

    #[derive(Clone, Copy)]
    struct EditorSignals {
        host: ResonatorEditorHost,
        parameters: EditorParameterSignals,
        selected_slot: Signal<f32>,
        selected_sample: Signal<f32>,
        command_status: Signal<Option<UiCommand>>,
        left_peak: Signal<f32>,
        right_peak: Signal<f32>,
        left_rms: Signal<f32>,
        right_rms: Signal<f32>,
        active_voices: Signal<f32>,
        patch_name: Signal<String>,
        slot_summaries: Signal<[ResonatorEditorSlotSummary; 4]>,
        library_samples: Signal<Vec<ResonatorEditorSampleSummary>>,
    }

    impl EditorSignals {
        fn parameter(self, slot: ResonatorEditorSurfaceSlot) -> EditorParameterControl {
            self.parameters.control(slot, self.host)
        }
    }

    struct EditorModel {
        host: ResonatorEditorHost,
        signals: EditorSignals,
        command_bus: EditorCommandBus,
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
                        self.host
                            .set_parameter(*id, f64::from(normalized.clamp(0.0, 1.0)));
                    }
                }
                EditorEvent::Command(command) => {
                    let dispatch = self.command_bus.dispatch(*command);
                    if let Some(slot) = dispatch.selected_slot {
                        self.signals.selected_slot.set(f32::from(slot.0 - 1));
                    }
                    handle_editor_command(
                        self.host,
                        Some(dispatch.command),
                        self.selected_library_sample,
                    );
                    unsafe {
                        sync_summary_from_controller(self.host, self.signals);
                    }
                    self.signals.command_status.set(Some(dispatch.command));
                }
                EditorEvent::SelectLibrarySample(index) => {
                    self.selected_library_sample = Some(*index);
                    self.signals.selected_sample.set(*index as f32);
                }
                EditorEvent::SyncFromController => unsafe {
                    request_telemetry_from_controller(self.host);
                    sync_signals_from_controller(self.host, self.signals);
                    sync_telemetry_from_controller(self.host, self.signals);
                },
            });
        }
    }

    pub unsafe fn build_resonator_application(
        host: ResonatorEditorHost,
        size: ResonatorEditorSize,
    ) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
        let values = unsafe { EditorValues::from_host(host) };
        build_application(host, values, size)
    }

    fn build_application(
        host: ResonatorEditorHost,
        values: EditorValues,
        size: ResonatorEditorSize,
    ) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
        let width = size.width.max(RESONATOR_EDITOR_WIDTH) as u32;
        let height = size.height.max(RESONATOR_EDITOR_HEIGHT) as u32;

        vizia::Application::new(move |cx| {
            cx.add_stylesheet(STYLE)
                .expect("failed to add editor style");

            let signals = EditorSignals {
                host,
                parameters: EditorParameterSignals::new(values.parameters),
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
                host,
                signals,
                command_bus: EditorCommandBus::default(),
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
        .title("Lamath")
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
                Label::new(cx, "Lamath").class("title");
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
                binary_switch(cx, signals.parameter(ResonatorEditorSurfaceSlot::Routing));
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
            binary_switch(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::RetriggerResonators),
            );
        })
        .height(Pixels(28.0))
        .alignment(Alignment::Center);
    }

    fn resonator_a_panel(cx: &mut Context, signals: EditorSignals) {
        resonator_panel(
            cx,
            "A",
            "Resonator A",
            signals.parameter(ResonatorEditorSurfaceSlot::ResonatorAModel),
            signals.parameter(ResonatorEditorSurfaceSlot::ResonatorAWaveguideStyle),
            [
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorAPreset),
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorABrightness),
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorADecay),
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorABoundaryReflection),
            ],
            0.72,
        );
    }

    fn resonator_b_panel(cx: &mut Context, signals: EditorSignals) {
        resonator_panel(
            cx,
            "B",
            "Resonator B",
            signals.parameter(ResonatorEditorSurfaceSlot::ResonatorBModel),
            signals.parameter(ResonatorEditorSurfaceSlot::ResonatorBWaveguideStyle),
            [
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorBLoopFilter),
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorBLoopGain),
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorBNonlinearity),
                signals.parameter(ResonatorEditorSurfaceSlot::ResonatorBBoundaryReflection),
            ],
            0.56,
        );
    }

    fn resonator_panel(
        cx: &mut Context,
        slot: &'static str,
        title: &'static str,
        model: EditorParameterControl,
        style: EditorParameterControl,
        controls: [EditorParameterControl; 4],
        energy: f32,
    ) {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Label::new(cx, slot).class("title");
                Label::new(cx, title).class("value-label");
                Spacer::new(cx);
                ResonatorBadge::new(cx, model.signal)
                    .height(Pixels(20.0))
                    .width(Pixels(50.0));
            })
            .height(Pixels(34.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(10.0));

            HStack::new(cx, |cx| {
                compact_binary_switch(cx, model);
                compact_binary_switch(cx, style);
            })
            .height(Pixels(26.0))
            .horizontal_gap(Pixels(8.0));

            MeterTrack::new(cx, energy, Color::rgb(124, 188, 148))
                .height(Pixels(8.0))
                .width(Stretch(1.0));
            MeterTrack::new(cx, 1.0 - energy * 0.5, Color::rgb(121, 156, 204))
                .height(Pixels(8.0))
                .width(Stretch(1.0));

            for control in controls {
                parameter_slider(cx, control);
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
                parameter_knob(cx, signals.parameter(ResonatorEditorSurfaceSlot::Master));
                parameter_knob(cx, signals.parameter(ResonatorEditorSurfaceSlot::Pan));
                parameter_knob(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::Saturation),
                );
            })
            .height(Pixels(96.0))
            .horizontal_gap(Pixels(0.0));

            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    Svg::new(cx, ICON_FILTER).class("toolbar-icon");
                    Label::new(cx, "Filter").class("value-label");
                    Spacer::new(cx);
                    let cutoff = signals.parameter(ResonatorEditorSurfaceSlot::Cutoff);
                    Label::new(cx, value_text(cutoff)).class("value-label");
                })
                .height(Pixels(22.0))
                .alignment(Alignment::Center)
                .horizontal_gap(Pixels(8.0));

                parameter_slider(cx, signals.parameter(ResonatorEditorSurfaceSlot::Cutoff));
                parameter_slider(cx, signals.parameter(ResonatorEditorSurfaceSlot::Resonance));
                parameter_slider(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::FilterMode),
                );
            })
            .class("strip")
            .height(Pixels(102.0))
            .padding(Pixels(10.0))
            .vertical_gap(Pixels(7.0));

            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    Label::new(cx, "Envelope").class("value-label");
                    Spacer::new(cx);
                    let release = signals.parameter(ResonatorEditorSurfaceSlot::AmpRelease);
                    Label::new(cx, value_text(release)).class("value-label");
                })
                .height(Pixels(20.0))
                .alignment(Alignment::Center);
                parameter_slider(cx, signals.parameter(ResonatorEditorSurfaceSlot::AmpAttack));
                parameter_slider(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::AmpRelease),
                );
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
                parameter_slider(cx, signals.parameter(ResonatorEditorSurfaceSlot::LfoRate));
                parameter_slider(cx, signals.parameter(ResonatorEditorSurfaceSlot::LfoShape));
                parameter_slider(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::Mod1Enabled),
                );
                parameter_slider(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::Mod1Source),
                );
                parameter_slider(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::Mod1Destination),
                );
                parameter_slider(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::Mod1Amount),
                );
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

    fn binary_switch(cx: &mut Context, parameter: EditorParameterControl) {
        segmented_switch(cx, parameter);
    }

    fn compact_binary_switch(cx: &mut Context, parameter: EditorParameterControl) {
        segmented_switch(cx, parameter);
    }

    fn segmented_switch(cx: &mut Context, parameter: EditorParameterControl) {
        let id = parameter.id;
        let signal = parameter.signal;
        let (left_label, right_label, width) = parameter
            .binary_labels()
            .expect("segmented switch requires binary editor metadata");
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

    fn parameter_knob(cx: &mut Context, parameter: EditorParameterControl) {
        let id = parameter.id;
        let signal = parameter.signal;
        let label = parameter.label();
        VStack::new(cx, move |cx| {
            Knob::new(cx, default_normalized(parameter.host, id), signal, false).on_change(
                move |cx, normalized| {
                    cx.emit(EditorEvent::SetParameter { id, normalized });
                },
            );
            Label::new(cx, label)
                .class("value-label")
                .alignment(Alignment::Center)
                .width(Pixels(84.0));
            Label::new(cx, value_text(parameter))
                .class("muted")
                .alignment(Alignment::Center)
                .width(Pixels(84.0));
        })
        .width(Pixels(92.0))
        .height(Pixels(88.0))
        .alignment(Alignment::Center)
        .vertical_gap(Pixels(3.0));
    }

    fn parameter_slider(cx: &mut Context, parameter: EditorParameterControl) {
        let id = parameter.id;
        let signal = parameter.signal;
        let label = parameter.label();
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
            Label::new(cx, value_text(parameter))
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
        item: impl SignalGet<ResonatorEditorSampleSummary> + Copy + 'static,
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

    fn value_text(parameter: EditorParameterControl) -> Memo<String> {
        parameter.value_text()
    }

    fn command_status_text(signal: Signal<Option<UiCommand>>) -> Memo<String> {
        Memo::new(move |_| command_label(signal.get()).to_string())
    }

    fn handle_editor_command(
        host: ResonatorEditorHost,
        command: Option<UiCommand>,
        selected_sample: Option<usize>,
    ) {
        let directories = unsafe { host.directories() };
        let mut patch_save_path = None;
        let mut patch_load_path = None;
        let mut patch_export_directory = None;
        let mut sample_path = None;

        match command {
            Some(UiCommand::SavePatch) => {
                patch_save_path = FileDialog::new()
                    .add_filter("Lamath Patch", &["toml"])
                    .set_directory(&directories.patch_directory)
                    .set_file_name("Lamath Patch.toml")
                    .save_file();
            }
            Some(UiCommand::LoadPatch) => {
                patch_load_path = FileDialog::new()
                    .add_filter("Lamath Patch", &["toml"])
                    .set_directory(&directories.patch_directory)
                    .pick_file();
            }
            Some(UiCommand::ExportPatchWithSamples) => {
                patch_export_directory = FileDialog::new()
                    .set_directory(&directories.export_directory)
                    .pick_folder();
            }
            Some(UiCommand::OpenLibrary) => {
                sample_path = sample_dialog(&directories).pick_file();
            }
            Some(UiCommand::LoadExcitationSlot(_)) if selected_sample.is_none() => {
                sample_path = sample_dialog(&directories).pick_file();
            }
            Some(UiCommand::LoadExcitationSlot(_))
            | Some(UiCommand::ClearExcitationSlot(_))
            | Some(UiCommand::SelectExcitationSlot(_))
            | Some(UiCommand::LoadSelectedExcitationSlot)
            | Some(UiCommand::ClearSelectedExcitationSlot)
            | Some(UiCommand::RedetectSlices)
            | Some(UiCommand::TuneSelectedSlice)
            | Some(UiCommand::TuneAllSlices)
            | Some(UiCommand::SnapAllSlicesToScale)
            | None => {}
        }

        let Some(command) = command else {
            return;
        };

        unsafe {
            host.handle_command(ResonatorEditorCommandRequest {
                command,
                patch_save_path: patch_save_path.as_deref(),
                patch_load_path: patch_load_path.as_deref(),
                patch_export_directory: patch_export_directory.as_deref(),
                sample_path: sample_path.as_deref(),
                selected_library_sample: selected_sample,
            });
        }
    }

    fn sample_dialog(directories: &ResonatorEditorDirectories) -> FileDialog {
        FileDialog::new()
            .add_filter("WAV audio", &["wav", "wave"])
            .set_directory(&directories.sample_directory)
    }

    unsafe fn request_telemetry_from_controller(host: ResonatorEditorHost) {
        unsafe { host.request_telemetry() };
    }

    unsafe fn sync_summary_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
        let summary = unsafe { host.summary() };
        signals.patch_name.set(summary.patch_name);
        signals.slot_summaries.set(summary.slots);
        signals.library_samples.set(summary.library_samples);
    }

    fn update_signal(id: u32, normalized: f32, signals: EditorSignals) {
        let normalized = normalized.clamp(0.0, 1.0);
        signals.parameters.set_by_id(id, normalized);
    }

    unsafe fn sync_signals_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
        for binding in host.parameter_bindings().iter().flatten() {
            let parameter_id = binding.id();
            update_signal(
                parameter_id,
                unsafe { host.parameter_value(parameter_id) },
                signals,
            );
        }
    }

    unsafe fn sync_telemetry_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
        let telemetry = unsafe { host.telemetry() };
        signals.left_peak.set(telemetry.left_peak);
        signals.right_peak.set(telemetry.right_peak);
        signals.left_rms.set(telemetry.left_rms);
        signals.right_rms.set(telemetry.right_rms);
        signals.active_voices.set(telemetry.active_voices);
    }

    fn default_normalized(host: ResonatorEditorHost, parameter_id: u32) -> f32 {
        unsafe { host.default_normalized(parameter_id) }
    }

    fn slot_label(slots: Signal<[ResonatorEditorSlotSummary; 4]>, index: usize) -> Memo<String> {
        Memo::new(move |_| slots.get()[index].label.clone())
    }

    fn slot_detail(slots: Signal<[ResonatorEditorSlotSummary; 4]>, index: usize) -> Memo<String> {
        Memo::new(move |_| slots.get()[index].detail.clone())
    }

    fn slot_pitch_track(
        slots: Signal<[ResonatorEditorSlotSummary; 4]>,
        index: usize,
    ) -> Memo<bool> {
        Memo::new(move |_| slots.get()[index].pitch_track)
    }

    fn slot_looping(slots: Signal<[ResonatorEditorSlotSummary; 4]>, index: usize) -> Memo<bool> {
        Memo::new(move |_| slots.get()[index].looping)
    }

    fn slot_waveform_phase(
        slots: Signal<[ResonatorEditorSlotSummary; 4]>,
        index: usize,
    ) -> Memo<f32> {
        Memo::new(move |_| {
            if slots.get()[index].sample_backed {
                index as f32 * 0.17 + 0.42
            } else {
                index as f32 * 0.21 + 0.2
            }
        })
    }

    fn library_count_text(samples: Signal<Vec<ResonatorEditorSampleSummary>>) -> Memo<String> {
        Memo::new(move |_| {
            let count = samples.get().len();
            match count {
                0 => "No samples".to_string(),
                1 => "1 sample".to_string(),
                count => format!("{count} samples"),
            }
        })
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
        samples: Signal<Vec<ResonatorEditorSampleSummary>>,
        index: usize,
    }

    impl LibraryWaveform {
        fn new(
            cx: &mut Context,
            samples: Signal<Vec<ResonatorEditorSampleSummary>>,
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
        points: &[ResonatorEditorWaveformPoint],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_requires_complete_parameter_surface() {
        let mut bindings = all_parameter_bindings();
        bindings.pop();

        assert!(matches!(
            ResonatorEditorHost::new(0, bindings, mock_callbacks()),
            Err(ResonatorEditorHostError::MissingSlot(
                ResonatorEditorSurfaceSlot::Mod1Amount
            ))
        ));
    }

    #[test]
    fn host_rejects_duplicate_surface_slot() {
        let mut bindings = all_parameter_bindings();
        bindings.push(ResonatorEditorParameterBinding::new(
            999,
            ResonatorEditorSurfaceSlot::Master,
            "Duplicate",
            ResonatorEditorControlKind::Knob,
        ));

        assert!(matches!(
            ResonatorEditorHost::new(0, bindings, mock_callbacks()),
            Err(ResonatorEditorHostError::DuplicateSlot(
                ResonatorEditorSurfaceSlot::Master
            ))
        ));
    }

    #[test]
    fn host_callbacks_project_mock_editor_state() {
        let host = ResonatorEditorHost::new(0, all_parameter_bindings(), mock_callbacks()).unwrap();

        assert_eq!(unsafe { host.parameter_value(12) }, 0.25);
        assert_eq!(unsafe { host.parameter_value_text(12, 0.25) }, "12=0.25");
        assert_eq!(unsafe { host.summary() }.patch_name, "Mock");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn constructs_vizia_application_from_mock_binding() {
        let host = ResonatorEditorHost::new(0, all_parameter_bindings(), mock_callbacks()).unwrap();
        let _application =
            unsafe { build_resonator_application(host, platform::ResonatorEditorSize::default()) };
    }

    fn all_parameter_bindings() -> Vec<ResonatorEditorParameterBinding> {
        ResonatorEditorSurfaceSlot::ALL
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                ResonatorEditorParameterBinding::new(
                    index as u32 + 1,
                    *slot,
                    "Parameter",
                    ResonatorEditorControlKind::Knob,
                )
            })
            .collect()
    }

    fn mock_callbacks() -> ResonatorEditorCallbacks {
        ResonatorEditorCallbacks {
            refresh_library: |_| {},
            parameter_value: |_, _| 0.25,
            set_parameter: |_, _, _| {},
            parameter_value_text: |_, id, normalized| format!("{id}={normalized:.2}"),
            default_normalized: |_, _| 0.5,
            summary: |_| ResonatorEditorPatchSummary {
                patch_name: "Mock".to_string(),
                ..ResonatorEditorPatchSummary::default()
            },
            telemetry: |_| ResonatorEditorTelemetry::default(),
            directories: |_| ResonatorEditorDirectories::default(),
            request_telemetry: |_| {},
            handle_command: |_, _| {},
        }
    }
}
