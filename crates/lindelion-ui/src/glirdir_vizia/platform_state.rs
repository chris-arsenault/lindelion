const STYLE: &str = r#"
    :root {
        background-color: #111413;
        color: #dce4df;
        font-size: 12px;
    }

    label {
        color: #ced7d1;
    }

    .root {
        background-color: #111413;
    }

    .topbar, .panel {
        background-color: #171c1b;
        border-width: 1px;
        border-color: #2c3633;
        border-radius: 8px;
        padding: 12px;
    }

    .preview {
        background-color: #0f1314;
        border-width: 1px;
        border-color: #293431;
        border-radius: 7px;
    }

    .title {
        font-size: 18px;
        color: #eef6f0;
    }

    .section-title {
        font-size: 12px;
        color: #eef6f0;
    }

    .muted, .meter-label {
        color: #87948e;
        font-size: 10px;
    }

    .value-label {
        color: #e7eee9;
        font-size: 11px;
    }

    .status-chip {
        background-color: #202827;
        border-width: 1px;
        border-color: #394641;
        border-radius: 6px;
        color: #c7d2cc;
        padding-left: 8px;
        padding-right: 8px;
    }

    .segmented {
        background-color: #101515;
        border-width: 1px;
        border-color: #303b39;
        border-radius: 6px;
        padding: 2px;
    }

    button.seg-button {
        background-color: transparent;
        border-width: 0px;
        border-radius: 4px;
        color: #909c97;
        font-size: 10px;
    }

    button.seg-button:hover {
        background-color: #252d2b;
        color: #e0e8e3;
    }

    button.seg-active {
        background-color: #315040;
        color: #f0f8f2;
    }

    button.toolbar-button {
        background-color: #202827;
        border-width: 1px;
        border-color: #394641;
        border-radius: 6px;
        color: #dce5df;
    }

    button.toolbar-button:hover {
        background-color: #273231;
        border-color: #78a891;
    }

    .toolbar-icon {
        color: #dce5df;
        width: 17px;
        height: 17px;
    }

    slider {
        height: 22px;
    }

    slider .track {
        background-color: #26312f;
        border-radius: 4px;
    }

    slider .active {
        background-color: #82bc98;
        border-radius: 4px;
    }

    slider .thumb {
        background-color: #eef6f0;
        border-color: #0e1112;
        border-width: 1px;
        border-radius: 6px;
        width: 13px;
        height: 18px;
    }

    .tooltip {
        background-color: #202827;
        border-width: 1px;
        border-color: #48544f;
        border-radius: 5px;
    }
"#;

pub struct GlirdirViziaEditor {
    window: WindowHandle,
}

impl GlirdirViziaEditor {
    pub unsafe fn attach(
        parent: *mut c_void,
        host: GlirdirEditorHost,
        size: GlirdirEditorSize,
    ) -> Self {
        unsafe { host.request_status() };
        let parent_view = parent as usize;
        let parent = ParentWindow(parent);
        let window =
            unsafe { build_glirdir_application_with_parent(host, size, parent_view) }
                .open_parented(&parent);
        Self { window }
    }
}

impl Drop for GlirdirViziaEditor {
    fn drop(&mut self) {
        if self.window.is_open() {
            self.window.close();
        }
    }
}

#[derive(Clone)]
struct EditorValues {
    parameters: EditorParameterValues,
    status: GlirdirEditorStatus,
    preview: GlirdirEditorPreview,
    command_status: Option<GlirdirEditorCommand>,
}

impl EditorValues {
    unsafe fn from_host(host: GlirdirEditorHost) -> Self {
        Self {
            parameters: unsafe { EditorParameterValues::from_host(host) },
            status: unsafe { host.status() },
            preview: unsafe { host.preview() },
            command_status: None,
        }
    }
}

#[derive(Clone, Copy)]
struct EditorParameterValue {
    id: u32,
    editor: GlirdirEditorParameterBinding,
    normalized: f32,
}

#[derive(Clone, Copy)]
struct EditorParameterValues {
    entries: [Option<EditorParameterValue>; GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT],
}

impl EditorParameterValues {
    unsafe fn from_host(host: GlirdirEditorHost) -> Self {
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
    editor: GlirdirEditorParameterBinding,
    signal: Signal<f32>,
}

#[derive(Clone, Copy)]
struct EditorParameterSignals {
    entries: [Option<EditorParameterSignal>; GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT],
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

    fn control(self, slot: GlirdirEditorSurfaceSlot, host: GlirdirEditorHost) -> EditorParameterControl {
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
}

#[derive(Clone, Copy)]
struct EditorParameterControl {
    id: u32,
    editor: GlirdirEditorParameterBinding,
    signal: Signal<f32>,
    host: GlirdirEditorHost,
}

impl EditorParameterControl {
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

#[derive(Clone, Copy)]
struct EditorSignals {
    host: GlirdirEditorHost,
    parent_view: usize,
    parameters: EditorParameterSignals,
    status: Signal<GlirdirEditorStatus>,
    preview: Signal<GlirdirEditorPreview>,
    command_status: Signal<Option<GlirdirEditorCommand>>,
}

impl EditorSignals {
    fn parameter(self, slot: GlirdirEditorSurfaceSlot) -> EditorParameterControl {
        self.parameters.control(slot, self.host)
    }
}

struct EditorModel {
    host: GlirdirEditorHost,
    signals: EditorSignals,
    pending_export: Option<PendingMidiExport>,
}

struct PendingMidiExport {
    source: PathBuf,
    dialog: crate::vizia_file_dialogs::PendingFileDialog,
}

enum EditorEvent {
    SetParameter { id: u32, normalized: f32 },
    Command(GlirdirEditorCommand),
    ExportMidiFile,
    SyncFromController,
}

impl Model for EditorModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|editor_event, _| match editor_event {
            EditorEvent::SetParameter { id, normalized } => {
                if let Some(entry) = parameter_signal_by_id(self.signals, *id) {
                    entry.signal.set(*normalized);
                }
                unsafe { self.host.set_parameter(*id, f64::from(*normalized)) };
            }
            EditorEvent::Command(command) => unsafe {
                self.host.handle_command(*command);
                self.host.request_status();
                sync_from_host(self.host, self.signals);
                self.signals.command_status.set(Some(*command));
            },
            EditorEvent::ExportMidiFile => unsafe {
                self.pending_export = start_midi_export(self.host, self.signals.parent_view);
                if self.pending_export.is_none() {
                    self.host.request_status();
                    sync_from_host(self.host, self.signals);
                }
                self.signals.command_status.set(Some(GlirdirEditorCommand::ExportMidi));
            },
            EditorEvent::SyncFromController => unsafe {
                self.complete_pending_export();
                self.host.request_status();
                sync_from_host(self.host, self.signals);
            },
        });
    }
}

impl EditorModel {
    fn complete_pending_export(&mut self) {
        let Some(pending) = self.pending_export.as_mut() else {
            return;
        };
        let std::task::Poll::Ready(selection) = pending.dialog.poll_path() else {
            return;
        };
        let pending = self.pending_export.take().expect("pending export");
        if let Some(target) = selection {
            let _ = std::fs::copy(pending.source, target);
            unsafe {
                self.host.request_status();
                sync_from_host(self.host, self.signals);
            }
        }
    }
}

pub unsafe fn build_glirdir_application(
    host: GlirdirEditorHost,
    size: GlirdirEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    unsafe { build_glirdir_application_with_parent(host, size, 0) }
}

unsafe fn build_glirdir_application_with_parent(
    host: GlirdirEditorHost,
    size: GlirdirEditorSize,
    parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let values = unsafe { EditorValues::from_host(host) };
    build_application(host, values, size, parent_view)
}

fn parameter_signal_by_id(signals: EditorSignals, id: u32) -> Option<EditorParameterSignal> {
    signals
        .parameters
        .entries
        .iter()
        .flatten()
        .find(|entry| entry.id == id)
        .copied()
}

unsafe fn sync_from_host(host: GlirdirEditorHost, signals: EditorSignals) {
    signals.status.set(unsafe { host.status() });
    signals.preview.set(unsafe { host.preview() });
    for entry in signals.parameters.entries.iter().flatten() {
        entry.signal.set(unsafe { host.parameter_value(entry.id) });
    }
}

unsafe fn start_midi_export(
    host: GlirdirEditorHost,
    parent_view: usize,
) -> Option<PendingMidiExport> {
    let GlirdirEditorMidiDrag::Ready { path } = (unsafe { host.prepare_midi_drag() }) else {
        return None;
    };
    let parent = crate::vizia_file_dialogs::DialogParent::from_ns_view(parent_view);
    let dialog = crate::vizia_file_dialogs::midi_save_file_dialog("glirdir-scratch.mid", parent);
    Some(PendingMidiExport {
        source: path,
        dialog: crate::vizia_file_dialogs::PendingFileDialog::save_file(dialog),
    })
}
