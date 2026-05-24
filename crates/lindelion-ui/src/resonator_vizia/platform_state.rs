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
    sidechain_required: Signal<bool>,
    sidechain_input_detected: Signal<bool>,
    sidechain_signal_active: Signal<bool>,
    audio_note_detected: Signal<bool>,
    audio_note_pitch_confidence: Signal<f32>,
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
