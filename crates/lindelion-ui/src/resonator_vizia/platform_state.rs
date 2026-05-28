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

pub struct ResonatorViziaEditor {
    window: WindowHandle,
    drop_targets: Option<platform_drop::NativeLayerDropTargets>,
}

impl ResonatorViziaEditor {
    pub unsafe fn attach(
        parent: *mut c_void,
        host: ResonatorEditorHost,
        size: ResonatorEditorSize,
    ) -> Self {
        unsafe { host.refresh_library() };
        let parent_view = parent as usize;
        let parent = ParentWindow(parent);
        let window =
            unsafe { build_resonator_application_with_parent(host, size, parent_view) }
                .open_parented(&parent);
        let drop_targets = platform_drop::NativeLayerDropTargets::install(&window, host);
        Self {
            window,
            drop_targets,
        }
    }
}

impl Drop for ResonatorViziaEditor {
    fn drop(&mut self) {
        self.drop_targets.take();
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
            selected_sample: -1.0,
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
    dialog_parent: Option<crate::vizia_file_dialogs::DialogParent>,
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
    library_page_start: Signal<usize>,
    library_location: Signal<String>,
    settings_open: Signal<bool>,
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
    pending_dialog: Option<PendingEditorDialog>,
}

struct PendingEditorDialog {
    command: UiCommand,
    selected_sample: Option<usize>,
    dialog: crate::vizia_file_dialogs::PendingFileDialog,
}

impl PendingEditorDialog {
    fn pick_file(
        command: UiCommand,
        selected_sample: Option<usize>,
        dialog: FileDialog,
    ) -> Self {
        Self {
            command,
            selected_sample,
            dialog: crate::vizia_file_dialogs::PendingFileDialog::pick_file(dialog),
        }
    }

    fn pick_folder(
        command: UiCommand,
        selected_sample: Option<usize>,
        dialog: FileDialog,
    ) -> Self {
        Self {
            command,
            selected_sample,
            dialog: crate::vizia_file_dialogs::PendingFileDialog::pick_folder(dialog),
        }
    }

    fn save_file(
        command: UiCommand,
        selected_sample: Option<usize>,
        dialog: FileDialog,
    ) -> Self {
        Self {
            command,
            selected_sample,
            dialog: crate::vizia_file_dialogs::PendingFileDialog::save_file(dialog),
        }
    }
}

enum EditorEvent {
    SetParameter { id: u32, normalized: f32 },
    Command(UiCommand),
    UseLibrarySample(usize),
    ChooseSampleFile,
    AddLibrarySample,
    LibraryPagePrevious,
    LibraryPageNext,
    ToggleSettings,
    CloseSettings,
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
                if let Some(pending) = start_editor_dialog(
                    self.host,
                    self.signals.dialog_parent,
                    dispatch.command,
                    self.selected_library_sample,
                ) {
                    self.pending_dialog = Some(pending);
                } else {
                    handle_editor_command(
                        self.host,
                        Some(dispatch.command),
                        self.selected_library_sample,
                        None,
                    );
                    unsafe {
                        sync_summary_from_controller(self.host, self.signals);
                    }
                }
                self.signals.command_status.set(Some(dispatch.command));
            }
            EditorEvent::UseLibrarySample(index) => {
                if self.signals.library_samples.get().get(*index).is_none() {
                    return;
                }
                self.selected_library_sample = Some(*index);
                self.signals.selected_sample.set(*index as f32);
                let dispatch = self.command_bus.dispatch(UiCommand::LoadSelectedExcitationSlot);
                if let Some(slot) = dispatch.selected_slot {
                    self.signals.selected_slot.set(f32::from(slot.0 - 1));
                }
                handle_editor_command(
                    self.host,
                    Some(dispatch.command),
                    self.selected_library_sample,
                    None,
                );
                unsafe {
                    sync_summary_from_controller(self.host, self.signals);
                }
                self.signals.command_status.set(Some(dispatch.command));
            }
            EditorEvent::ChooseSampleFile => {
                self.selected_library_sample = None;
                self.signals.selected_sample.set(-1.0);
                let dispatch = self.command_bus.dispatch(UiCommand::LoadSelectedExcitationSlot);
                if let Some(slot) = dispatch.selected_slot {
                    self.signals.selected_slot.set(f32::from(slot.0 - 1));
                }
                if let Some(pending) = start_editor_dialog(
                    self.host,
                    self.signals.dialog_parent,
                    dispatch.command,
                    None,
                ) {
                    self.pending_dialog = Some(pending);
                }
                self.signals.command_status.set(Some(dispatch.command));
            }
            EditorEvent::AddLibrarySample => {
                self.selected_library_sample = None;
                self.signals.selected_sample.set(-1.0);
                let command = UiCommand::OpenLibrary;
                let directories = unsafe { self.host.directories() };
                self.pending_dialog = Some(PendingEditorDialog::pick_file(
                    command,
                    None,
                    wav_audio_dialog(&directories.sample_directory, self.signals.dialog_parent),
                ));
                self.signals.command_status.set(Some(command));
            }
            EditorEvent::LibraryPagePrevious => {
                let current = self.signals.library_page_start.get();
                self.signals
                    .library_page_start
                    .set(current.saturating_sub(LIBRARY_BROWSER_ROWS));
            }
            EditorEvent::LibraryPageNext => {
                let sample_count = self.signals.library_samples.get().len();
                let current = self.signals.library_page_start.get();
                self.signals.library_page_start.set(clamped_library_page_start(
                    current.saturating_add(LIBRARY_BROWSER_ROWS),
                    sample_count,
                ));
            }
            EditorEvent::ToggleSettings => {
                self.signals.settings_open.set(!self.signals.settings_open.get());
            }
            EditorEvent::CloseSettings => {
                self.signals.settings_open.set(false);
            }
            EditorEvent::SyncFromController => unsafe {
                self.complete_pending_dialog();
                request_telemetry_from_controller(self.host);
                sync_signals_from_controller(self.host, self.signals);
                sync_telemetry_from_controller(self.host, self.signals);
            },
        });
    }
}

const LIBRARY_BROWSER_ROWS: usize = 3;

fn clamped_library_page_start(start: usize, sample_count: usize) -> usize {
    if sample_count <= LIBRARY_BROWSER_ROWS {
        0
    } else {
        start.min(sample_count - LIBRARY_BROWSER_ROWS)
    }
}

fn clamp_library_page_signal(signals: EditorSignals, sample_count: usize) {
    let current = signals.library_page_start.get();
    signals
        .library_page_start
        .set(clamped_library_page_start(current, sample_count));
}

fn is_resonator_supported_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("wav") || extension.eq_ignore_ascii_case("wave")
        })
}

fn transient_exciter_audio_path() -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "lamath-exciter-audio-{}-{timestamp}.wav",
        process::id()
    ))
}

impl EditorModel {
    fn complete_pending_dialog(&mut self) {
        let Some(pending) = self.pending_dialog.as_mut() else {
            return;
        };
        let std::task::Poll::Ready(selection) = pending.dialog.poll_path() else {
            return;
        };
        let pending = self.pending_dialog.take().expect("pending dialog");
        if let Some(path) = selection {
            handle_editor_command(
                self.host,
                Some(pending.command),
                pending.selected_sample,
                Some(&path),
            );
            unsafe {
                sync_summary_from_controller(self.host, self.signals);
            }
        }
    }
}

pub unsafe fn build_resonator_application(
    host: ResonatorEditorHost,
    size: ResonatorEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    unsafe { build_resonator_application_with_parent(host, size, 0) }
}

unsafe fn build_resonator_application_with_parent(
    host: ResonatorEditorHost,
    size: ResonatorEditorSize,
    parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let values = unsafe { EditorValues::from_host(host) };
    build_application(host, values, size, parent_view)
}
