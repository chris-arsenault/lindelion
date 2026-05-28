fn build_application(
    host: ResonatorEditorHost,
    values: EditorValues,
    size: ResonatorEditorSize,
    parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(RESONATOR_EDITOR_WIDTH) as u32;
    let height = size.height.max(RESONATOR_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add resonator editor style");
        cx.add_stylesheet(crate::vizia_controls::COMMON_CONTROL_STYLE)
            .expect("failed to add common control style");
        let signals = resonator_signals(host, &values, parent_view);
        EditorModel {
            host,
            signals,
            command_bus: EditorCommandBus::default(),
            selected_library_sample: None,
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
    .title("Lamath")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

fn resonator_signals(
    host: ResonatorEditorHost,
    values: &EditorValues,
    parent_view: usize,
) -> EditorSignals {
    let directories = unsafe { host.directories() };
    EditorSignals {
        host,
        dialog_parent: crate::vizia_file_dialogs::DialogParent::from_ns_view(parent_view),
        parameters: EditorParameterSignals::new(values.parameters),
        selected_slot: Signal::new(values.selected_slot),
        selected_sample: Signal::new(values.selected_sample),
        command_status: Signal::new(values.command_status),
        left_peak: Signal::new(values.telemetry.left_peak),
        right_peak: Signal::new(values.telemetry.right_peak),
        left_rms: Signal::new(values.telemetry.left_rms),
        right_rms: Signal::new(values.telemetry.right_rms),
        active_voices: Signal::new(values.telemetry.active_voices),
        sidechain_required: Signal::new(values.telemetry.sidechain_required),
        sidechain_input_detected: Signal::new(values.telemetry.sidechain_input_detected),
        sidechain_signal_active: Signal::new(values.telemetry.sidechain_signal_active),
        audio_note_detected: Signal::new(values.telemetry.audio_note_detected),
        audio_note_pitch_confidence: Signal::new(values.telemetry.audio_note_pitch_confidence),
        patch_name: Signal::new(values.summary.patch_name.clone()),
        slot_summaries: Signal::new(values.summary.slots.clone()),
        library_samples: Signal::new(values.summary.library_samples.clone()),
        library_page_start: Signal::new(0),
        library_location: Signal::new(directories.sample_directory.display().to_string()),
        settings_open: Signal::new(false),
    }
}

fn update_signal(id: u32, normalized: f32, signals: EditorSignals) {
    signals.parameters.set_by_id(id, normalized.clamp(0.0, 1.0));
}

fn start_editor_dialog(
    host: ResonatorEditorHost,
    parent: Option<crate::vizia_file_dialogs::DialogParent>,
    command: UiCommand,
    selected_sample: Option<usize>,
) -> Option<PendingEditorDialog> {
    let directories = unsafe { host.directories() };
    match command {
        UiCommand::SavePatch => Some(PendingEditorDialog::save_file(
            command,
            selected_sample,
            patch_save_file_dialog("Lamath", &directories.patch_directory, "lamath.toml", parent),
        )),
        UiCommand::LoadPatch => Some(PendingEditorDialog::pick_file(
            command,
            selected_sample,
            patch_load_file_dialog("Lamath", &directories.patch_directory, parent),
        )),
        UiCommand::ExportPatchWithSamples => Some(PendingEditorDialog::pick_folder(
            command,
            selected_sample,
            patch_export_directory_dialog(&directories.export_directory, parent),
        )),
        UiCommand::LoadExcitationSlot(_) if selected_sample.is_none() => Some(
            PendingEditorDialog::pick_file(
                command,
                selected_sample,
                wav_audio_dialog(&directories.sample_directory, parent),
            ),
        ),
        _ => None,
    }
}

fn handle_editor_command(
    host: ResonatorEditorHost,
    command: Option<UiCommand>,
    selected_sample: Option<usize>,
    path: Option<&Path>,
) {
    let Some(command) = command else {
        return;
    };
    let request = ResonatorEditorCommandRequest {
        command,
        patch_save_path: matches!(command, UiCommand::SavePatch).then_some(path).flatten(),
        patch_load_path: matches!(command, UiCommand::LoadPatch).then_some(path).flatten(),
        patch_export_directory: matches!(command, UiCommand::ExportPatchWithSamples)
            .then_some(path)
            .flatten(),
        sample_path: matches!(command, UiCommand::LoadExcitationSlot(_) | UiCommand::OpenLibrary)
            .then_some(path)
            .flatten(),
        selected_library_sample: selected_sample,
    };
    unsafe { host.handle_command(request) };
}

unsafe fn sync_summary_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
    let summary = unsafe { host.summary() };
    let sample_count = summary.library_samples.len();
    signals.patch_name.set(summary.patch_name);
    signals.slot_summaries.set(summary.slots);
    signals.library_samples.set(summary.library_samples);
    clamp_library_page_signal(signals, sample_count);
}

unsafe fn request_telemetry_from_controller(host: ResonatorEditorHost) {
    unsafe { host.request_telemetry() };
}

unsafe fn sync_signals_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
    for entry in signals.parameters.entries.iter().flatten() {
        entry.signal.set(unsafe { host.parameter_value(entry.id) });
    }
    unsafe { sync_summary_from_controller(host, signals) };
}

unsafe fn sync_telemetry_from_controller(host: ResonatorEditorHost, signals: EditorSignals) {
    let telemetry = unsafe { host.telemetry() };
    signals.left_peak.set(telemetry.left_peak);
    signals.right_peak.set(telemetry.right_peak);
    signals.left_rms.set(telemetry.left_rms);
    signals.right_rms.set(telemetry.right_rms);
    signals.active_voices.set(telemetry.active_voices);
    signals.sidechain_required.set(telemetry.sidechain_required);
    signals
        .sidechain_input_detected
        .set(telemetry.sidechain_input_detected);
    signals
        .sidechain_signal_active
        .set(telemetry.sidechain_signal_active);
    signals.audio_note_detected.set(telemetry.audio_note_detected);
    signals
        .audio_note_pitch_confidence
        .set(telemetry.audio_note_pitch_confidence);
}
