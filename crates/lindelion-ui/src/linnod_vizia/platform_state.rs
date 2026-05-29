#[derive(Clone)]
struct EditorValues {
    parameters: EditorParameterValues,
    status: LinnodEditorStatus,
    telemetry: LinnodEditorTelemetry,
    summary: LinnodEditorPatchSummary,
}

impl EditorValues {
    unsafe fn from_host(host: LinnodEditorHost) -> Self {
        Self {
            parameters: unsafe { EditorParameterValues::from_host(host) },
            status: unsafe { host.status() },
            telemetry: unsafe { host.telemetry() },
            summary: unsafe { host.summary() },
        }
    }
}

#[derive(Clone, Copy)]
struct EditorParameterValue {
    id: u32,
    editor: LinnodEditorParameterBinding,
    normalized: f32,
}

#[derive(Clone, Copy)]
struct EditorParameterValues {
    entries: [Option<EditorParameterValue>; LINNOD_EDITOR_PARAMETER_BINDING_COUNT],
}

impl EditorParameterValues {
    unsafe fn from_host(host: LinnodEditorHost) -> Self {
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
    editor: LinnodEditorParameterBinding,
    signal: Signal<f32>,
}

#[derive(Clone, Copy)]
struct EditorParameterSignals {
    entries: [Option<EditorParameterSignal>; LINNOD_EDITOR_PARAMETER_BINDING_COUNT],
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

    fn control(self, slot: LinnodEditorSurfaceSlot, host: LinnodEditorHost) -> EditorParameterControl {
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

    fn set_by_id(self, id: u32, normalized: f32) {
        if let Some(entry) = self.entries.iter().flatten().find(|entry| entry.id == id) {
            entry.signal.set(normalized.clamp(0.0, 1.0));
        }
    }
}

#[derive(Clone, Copy)]
struct EditorParameterControl {
    id: u32,
    editor: LinnodEditorParameterBinding,
    signal: Signal<f32>,
    host: LinnodEditorHost,
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

    fn default_normalized(self) -> f32 {
        unsafe { self.host.default_normalized(self.id) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlScope {
    Global,
    Selected,
}

#[derive(Clone, Copy)]
struct EditorSignals {
    host: LinnodEditorHost,
    dialog_parent: Option<crate::vizia_file_dialogs::DialogParent>,
    parameters: EditorParameterSignals,
    status: Signal<LinnodEditorStatus>,
    telemetry: Signal<LinnodEditorTelemetry>,
    summary: Signal<LinnodEditorPatchSummary>,
    command_status: Signal<Option<LinnodEditorCommand>>,
    drop_active: Signal<bool>,
    control_scope: Signal<ControlScope>,
    settings_open: Signal<bool>,
}

impl EditorSignals {
    fn parameter(self, slot: LinnodEditorSurfaceSlot) -> EditorParameterControl {
        self.parameters.control(slot, self.host)
    }
}

struct EditorModel {
    host: LinnodEditorHost,
    signals: EditorSignals,
    pending_dialog: Option<PendingEditorDialog>,
}

struct PendingEditorDialog {
    command: LinnodEditorCommand,
    dialog: crate::vizia_file_dialogs::PendingFileDialog,
}

impl PendingEditorDialog {
    fn pick_file(command: LinnodEditorCommand, dialog: FileDialog) -> Self {
        Self {
            command,
            dialog: crate::vizia_file_dialogs::PendingFileDialog::pick_file(dialog),
        }
    }

    fn pick_folder(command: LinnodEditorCommand, dialog: FileDialog) -> Self {
        Self {
            command,
            dialog: crate::vizia_file_dialogs::PendingFileDialog::pick_folder(dialog),
        }
    }

    fn save_file(command: LinnodEditorCommand, dialog: FileDialog) -> Self {
        Self {
            command,
            dialog: crate::vizia_file_dialogs::PendingFileDialog::save_file(dialog),
        }
    }
}

#[derive(Clone)]
enum EditorEvent {
    SetParameter { id: u32, normalized: f32 },
    Command(LinnodEditorCommand),
    LoadSourceDialog,
    SavePatchDialog,
    LoadPatchDialog,
    ExportPatchDialog,
    MarkerEdit(LinnodEditorMarkerEdit),
    DetectionEdit(LinnodEditorDetectionEdit),
    SliceEdit(LinnodEditorSliceEdit),
    PadEdit(LinnodEditorPadEdit),
    PlaybackEdit(LinnodEditorPlaybackEdit),
    AutoTuneEdit(LinnodEditorAutoTuneEdit),
    SetControlScope(ControlScope),
    ToggleSettings,
    CloseSettings,
    SyncFromController,
}

impl Model for EditorModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|editor_event, _| match editor_event {
            EditorEvent::SetParameter { id, normalized } => {
                self.signals.parameters.set_by_id(*id, *normalized);
                unsafe { self.host.set_parameter(*id, f64::from(normalized.clamp(0.0, 1.0))) };
            }
            EditorEvent::Command(command) => {
                send_command(self.host, *command, None, None, None, None);
                sync_after_edit(self.host, self.signals, Some(*command));
            }
            EditorEvent::LoadSourceDialog => {
                self.pending_dialog = Some(PendingEditorDialog::pick_file(
                    LinnodEditorCommand::LoadSource,
                    source_dialog(self.host, self.signals.dialog_parent),
                ));
            }
            EditorEvent::SavePatchDialog => {
                self.pending_dialog = Some(PendingEditorDialog::save_file(
                    LinnodEditorCommand::SavePatch,
                    patch_save_dialog(self.host, self.signals.summary, self.signals.dialog_parent),
                ));
            }
            EditorEvent::LoadPatchDialog => {
                self.pending_dialog = Some(PendingEditorDialog::pick_file(
                    LinnodEditorCommand::LoadPatch,
                    patch_load_dialog(self.host, self.signals.dialog_parent),
                ));
            }
            EditorEvent::ExportPatchDialog => {
                self.pending_dialog = Some(PendingEditorDialog::pick_folder(
                    LinnodEditorCommand::ExportPatchWithSamples,
                    patch_export_dialog(self.host, self.signals.dialog_parent),
                ));
            }
            EditorEvent::DetectionEdit(edit) => unsafe {
                self.host.edit_detection(*edit);
                sync_after_edit(self.host, self.signals, None);
            },
            EditorEvent::MarkerEdit(edit) => unsafe {
                self.host.edit_marker(*edit);
                sync_after_edit(self.host, self.signals, None);
            },
            EditorEvent::SliceEdit(edit) => unsafe {
                self.host.edit_slice(edit.clone());
                sync_after_edit(self.host, self.signals, None);
            },
            EditorEvent::PadEdit(edit) => unsafe {
                self.host.edit_pad(*edit);
                sync_after_edit(self.host, self.signals, None);
            },
            EditorEvent::PlaybackEdit(edit) => unsafe {
                self.host.edit_playback(*edit);
                sync_after_edit(self.host, self.signals, None);
            },
            EditorEvent::AutoTuneEdit(edit) => unsafe {
                self.host.edit_auto_tune(*edit);
                sync_after_edit(self.host, self.signals, None);
            },
            EditorEvent::SetControlScope(scope) => {
                self.signals.control_scope.set(*scope);
            }
            EditorEvent::ToggleSettings => {
                self.signals.settings_open.set(!self.signals.settings_open.get());
            }
            EditorEvent::CloseSettings => {
                self.signals.settings_open.set(false);
            }
            EditorEvent::SyncFromController => unsafe {
                self.complete_pending_dialog();
                self.host.request_status();
                self.host.request_telemetry();
                sync_from_host(self.host, self.signals);
            },
        });
        event.map(|window_event, meta| {
            if let WindowEvent::KeyDown(code, _) = window_event {
                if is_linnod_paste_shortcut(cx, *code)
                    && paste_source_from_clipboard_inner(self.host)
                {
                    sync_after_edit(self.host, self.signals, Some(LinnodEditorCommand::LoadSource));
                    meta.consume();
                }
            }
        });
    }
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
            complete_dialog_command(self.host, self.signals, pending.command, &path);
        }
    }
}

pub unsafe fn build_linnod_application(
    host: LinnodEditorHost,
    size: LinnodEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    unsafe { build_linnod_application_with_parent(host, size, 0) }
}

unsafe fn build_linnod_application_with_parent(
    host: LinnodEditorHost,
    size: LinnodEditorSize,
    parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let values = unsafe { EditorValues::from_host(host) };
    build_application(host, values, size, parent_view)
}

fn build_application(
    host: LinnodEditorHost,
    values: EditorValues,
    size: LinnodEditorSize,
    parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LINNOD_EDITOR_WIDTH) as u32;
    let height = size.height.max(LINNOD_EDITOR_HEIGHT) as u32;

    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add linnod editor style");
        cx.add_stylesheet(crate::vizia_controls::COMMON_CONTROL_STYLE)
            .expect("failed to add common control style");
        let signals = EditorSignals {
            host,
            dialog_parent: crate::vizia_file_dialogs::DialogParent::from_ns_view(parent_view),
            parameters: EditorParameterSignals::new(values.parameters),
            status: Signal::new(values.status),
            telemetry: Signal::new(values.telemetry),
            summary: Signal::new(values.summary.clone()),
            command_status: Signal::new(None),
            drop_active: Signal::new(false),
            control_scope: Signal::new(ControlScope::Global),
            settings_open: Signal::new(false),
        };
        EditorModel {
            host,
            signals,
            pending_dialog: None,
        }
        .build(cx);
        let sync_timer = cx.add_timer(Duration::from_millis(66), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(EditorEvent::SyncFromController);
            }
        });
        cx.start_timer(sync_timer);
        build_editor(cx, signals);
    })
    .ignore_default_theme()
    .title("Linnod")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

fn send_command(
    host: LinnodEditorHost,
    command: LinnodEditorCommand,
    source_path: Option<&Path>,
    patch_save_path: Option<&Path>,
    patch_load_path: Option<&Path>,
    patch_export_directory: Option<&Path>,
) {
    unsafe {
        host.handle_command(LinnodEditorCommandRequest {
            command,
            source_path,
            patch_save_path,
            patch_load_path,
            patch_export_directory,
        });
    }
}

fn sync_after_edit(
    host: LinnodEditorHost,
    signals: EditorSignals,
    command: Option<LinnodEditorCommand>,
) {
    unsafe {
        host.request_status();
        sync_from_host(host, signals);
    }
    signals.command_status.set(command);
}

fn complete_dialog_command(
    host: LinnodEditorHost,
    signals: EditorSignals,
    command: LinnodEditorCommand,
    path: &Path,
) {
    match command {
        LinnodEditorCommand::LoadSource => {
            send_command(host, command, Some(path), None, None, None);
        }
        LinnodEditorCommand::SavePatch => {
            send_command(host, command, None, Some(path), None, None);
        }
        LinnodEditorCommand::LoadPatch => {
            send_command(host, command, None, None, Some(path), None);
        }
        LinnodEditorCommand::ExportPatchWithSamples => {
            send_command(host, command, None, None, None, Some(path));
        }
        _ => {}
    }
    sync_after_edit(host, signals, Some(command));
}

/// # Safety
/// The callback context in `host` must still reference a live Linnod editor/controller.
pub unsafe fn paste_source_from_clipboard(host: LinnodEditorHost) -> bool {
    if paste_source_from_clipboard_inner(host) {
        unsafe {
            host.request_status();
            host.request_telemetry();
        }
        true
    } else {
        false
    }
}

fn paste_source_from_clipboard_inner(host: LinnodEditorHost) -> bool {
    let Some(path) = clipboard_source_path() else {
        return false;
    };
    send_command(
        host,
        LinnodEditorCommand::LoadSource,
        Some(&path),
        None,
        None,
        None,
    );
    true
}

fn clipboard_source_path() -> Option<PathBuf> {
    crate::vizia_clipboard::file_path_from_general_pasteboard()
        .filter(|path| is_linnod_supported_audio_path(path))
        .or_else(|| {
            let path = transient_source_audio_path();
            crate::vizia_clipboard::read_file_contents_from_general_pasteboard(&path)
                .filter(|path| is_linnod_supported_audio_path(path))
        })
}

fn is_linnod_paste_shortcut(cx: &EventContext, code: Code) -> bool {
    let modifiers = cx.modifiers();
    code == Code::KeyV
        && (modifiers.logo() || modifiers.ctrl())
        && !modifiers.alt()
        && !modifiers.shift()
}

fn is_linnod_supported_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wav") || extension.eq_ignore_ascii_case("wave"))
}

fn transient_source_audio_path() -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    env::temp_dir().join(format!(
        "linnod-source-audio-{}-{timestamp}.wav",
        process::id()
    ))
}

unsafe fn sync_from_host(host: LinnodEditorHost, signals: EditorSignals) {
    signals.status.set(unsafe { host.status() });
    signals.telemetry.set(unsafe { host.telemetry() });
    signals.summary.set(unsafe { host.summary() });
    for entry in signals.parameters.entries.iter().flatten() {
        entry.signal.set(unsafe { host.parameter_value(entry.id) });
    }
}

fn source_dialog(
    host: LinnodEditorHost,
    parent: Option<crate::vizia_file_dialogs::DialogParent>,
) -> FileDialog {
    let directories = unsafe { host.directories() };
    crate::vizia_file_dialogs::wav_audio_dialog(&directories.sample_directory, parent)
}

fn patch_save_dialog(
    host: LinnodEditorHost,
    summary: Signal<LinnodEditorPatchSummary>,
    parent: Option<crate::vizia_file_dialogs::DialogParent>,
) -> FileDialog {
    let directories = unsafe { host.directories() };
    crate::vizia_file_dialogs::patch_save_file_dialog(
        "Linnod",
        &directories.patch_directory,
        format!("{}.toml", summary.get().patch_name),
        parent,
    )
}

fn patch_load_dialog(
    host: LinnodEditorHost,
    parent: Option<crate::vizia_file_dialogs::DialogParent>,
) -> FileDialog {
    let directories = unsafe { host.directories() };
    crate::vizia_file_dialogs::patch_load_file_dialog(
        "Linnod",
        &directories.patch_directory,
        parent,
    )
}

fn patch_export_dialog(
    host: LinnodEditorHost,
    parent: Option<crate::vizia_file_dialogs::DialogParent>,
) -> FileDialog {
    let directories = unsafe { host.directories() };
    crate::vizia_file_dialogs::patch_export_directory_dialog(&directories.export_directory, parent)
}
