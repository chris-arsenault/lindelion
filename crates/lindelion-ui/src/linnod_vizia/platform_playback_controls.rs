fn linnod_playback_panel(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            playback_scope_button(cx, signals.control_scope, "global", ControlScope::Global);
            playback_scope_button(cx, signals.control_scope, "selected", ControlScope::Selected);
            Spacer::new(cx);
            Label::new(cx, playback_scope_detail(signals.summary, signals.control_scope))
                .class("ll-section-subtitle");
        })
        .height(Pixels(24.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(5.0));
        playback_global_panel(cx, signals).display(signals.control_scope.map(|scope| {
            if *scope == ControlScope::Global {
                Display::Flex
            } else {
                Display::None
            }
        }));
        playback_selected_panel(cx, signals).display(signals.control_scope.map(|scope| {
            if *scope == ControlScope::Selected {
                Display::Flex
            } else {
                Display::None
            }
        }));
    })
    .class("ll-playback-panel")
    .height(Pixels(124.0))
    .vertical_gap(Pixels(6.0));
}

fn playback_global_panel<'a>(cx: &'a mut Context, signals: EditorSignals) -> Handle<'a, VStack> {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            playback_mode_selector(cx, signals.summary, PlaybackEditTarget::Global);
            Spacer::new(cx);
            Label::new(cx, global_playback_mode_text(signals.summary)).class("ll-control-value");
        })
        .height(Pixels(24.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
        envelope_controls(cx, signals.summary, PlaybackEditTarget::Global);
    })
    .vertical_gap(Pixels(6.0))
}

fn playback_selected_panel<'a>(cx: &'a mut Context, signals: EditorSignals) -> Handle<'a, VStack> {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            playback_override_toggle(cx, signals.summary);
            playback_mode_selector(cx, signals.summary, PlaybackEditTarget::Selected);
            crate::vizia_controls::compact_text_button(cx, "rev", "Toggle reverse")
                .on_press(move |cx| cx.emit(slice_reverse_event(signals.summary)))
                .toggle_class(
                    "ll-seg-active",
                    signals.summary.map(|summary| selected_slice(summary).reverse),
                )
                .height(Pixels(24.0));
        })
        .height(Pixels(24.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
        envelope_controls(cx, signals.summary, PlaybackEditTarget::Selected);
    })
    .vertical_gap(Pixels(6.0))
}

fn playback_scope_button(
    cx: &mut Context,
    scope_signal: Signal<ControlScope>,
    label: &'static str,
    scope: ControlScope,
) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| cx.emit(EditorEvent::SetControlScope(scope)))
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class(
        "ll-seg-active",
        scope_signal.map(move |active| *active == scope),
    )
    .width(Pixels(72.0))
    .height(Pixels(22.0));
}

fn playback_override_toggle(cx: &mut Context, summary: Signal<LinnodEditorPatchSummary>) {
    Button::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Element::new(cx)
                .class("ll-check-indicator")
                .toggle_class(
                    "ll-check-indicator-on",
                    summary.map(|summary| selected_slice(summary).use_playback_override),
                );
            Label::new(cx, "override").class("ll-control-label");
        })
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(5.0))
    })
    .on_press(move |cx| cx.emit(slice_playback_override_event(summary)))
    .class("ll-check-button")
    .toggle_class(
        "ll-check-on",
        summary.map(|summary| selected_slice(summary).use_playback_override),
    )
    .width(Pixels(86.0))
    .height(Pixels(24.0));
}

fn playback_mode_selector(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
) {
    HStack::new(cx, move |cx| {
        playback_mode_button(cx, summary, target, "one", LinnodEditorPlaybackMode::OneShot);
        playback_mode_button(cx, summary, target, "gate", LinnodEditorPlaybackMode::Gated);
        playback_mode_button(cx, summary, target, "loop", LinnodEditorPlaybackMode::Looped);
        playback_mode_button(cx, summary, target, "cont", LinnodEditorPlaybackMode::Continue);
    })
    .class("segmented")
    .class("ll-segmented")
    .height(Pixels(24.0))
    .width(Pixels(178.0))
    .horizontal_gap(Pixels(2.0));
}

fn playback_mode_button(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
    label: &'static str,
    mode: LinnodEditorPlaybackMode,
) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| emit_playback_mode_edit(cx, summary, target, mode))
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class(
        "ll-seg-active",
        summary.map(move |summary| playback_mode_for_target(summary, target) == mode),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn envelope_controls(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
) {
    HStack::new(cx, move |cx| {
        envelope_drag(cx, summary, target, EnvelopeField::Attack, "A");
        envelope_drag(cx, summary, target, EnvelopeField::Decay, "D");
        envelope_drag(cx, summary, target, EnvelopeField::Sustain, "S");
        envelope_drag(cx, summary, target, EnvelopeField::Release, "R");
    })
    .height(Pixels(44.0))
    .horizontal_gap(Pixels(5.0));
}

fn envelope_drag(
    cx: &mut Context,
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
    field: EnvelopeField,
    label: &'static str,
) {
    let (min, max, default, step, fine, width) = match field {
        EnvelopeField::Sustain => (0.0, 1.0, 1.0, 0.02, 0.005, 58.0),
        _ => (0.0, 5_000.0, 0.0, 5.0, 0.5, 58.0),
    };
    crate::vizia_controls::drag_value(
        cx,
        label,
        envelope_field_text(summary, target, field),
        envelope_field_value(summary, target, field),
        crate::vizia_controls::DragValueSpec::new(
            min,
            max,
            default,
            step,
            fine,
            width,
            crate::vizia_controls::Accent::Tone,
        ),
        move |cx, value| emit_envelope_field_edit(cx, summary, target, field, value),
    );
}

#[derive(Clone, Copy)]
enum PlaybackEditTarget {
    Global,
    Selected,
}

#[derive(Clone, Copy)]
enum EnvelopeField {
    Attack,
    Decay,
    Sustain,
    Release,
}

fn slice_playback_override_event(summary: Signal<LinnodEditorPatchSummary>) -> EditorEvent {
    let slice = selected_slice(&summary.get());
    EditorEvent::SliceEdit(LinnodEditorSliceEdit::PlaybackOverride {
        slice_index: slice.index,
        enabled: !slice.use_playback_override,
    })
}

fn emit_playback_mode_edit(
    cx: &mut EventContext,
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
    mode: LinnodEditorPlaybackMode,
) {
    match target {
        PlaybackEditTarget::Global => {
            cx.emit(EditorEvent::PlaybackEdit(LinnodEditorPlaybackEdit::Mode {
                mode,
            }));
        }
        PlaybackEditTarget::Selected => {
            let slice = selected_slice(&summary.get());
            if !slice.use_playback_override {
                cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::PlaybackOverride {
                    slice_index: slice.index,
                    enabled: true,
                }));
            }
            cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::PlaybackMode {
                slice_index: slice.index,
                mode,
            }));
        }
    }
}

fn emit_envelope_field_edit(
    cx: &mut EventContext,
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
    field: EnvelopeField,
    value: f32,
) {
    let summary_value = summary.get();
    let mut envelope = envelope_for_target(&summary_value, target);
    set_envelope_field(&mut envelope, field, value);
    match target {
        PlaybackEditTarget::Global => {
            cx.emit(EditorEvent::PlaybackEdit(
                LinnodEditorPlaybackEdit::Envelope { envelope },
            ));
        }
        PlaybackEditTarget::Selected => {
            let slice = selected_slice(&summary_value);
            if !slice.use_playback_override {
                cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::PlaybackOverride {
                    slice_index: slice.index,
                    enabled: true,
                }));
            }
            cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::Envelope {
                slice_index: slice.index,
                envelope,
            }));
        }
    }
}

fn playback_mode_for_target(
    summary: &LinnodEditorPatchSummary,
    target: PlaybackEditTarget,
) -> LinnodEditorPlaybackMode {
    match target {
        PlaybackEditTarget::Global => summary.playback.mode,
        PlaybackEditTarget::Selected => {
            let slice = selected_slice(summary);
            if slice.use_playback_override {
                slice.playback_mode
            } else {
                summary.playback.mode
            }
        }
    }
}

fn envelope_for_target(
    summary: &LinnodEditorPatchSummary,
    target: PlaybackEditTarget,
) -> LinnodEditorEnvelope {
    match target {
        PlaybackEditTarget::Global => summary.playback.envelope,
        PlaybackEditTarget::Selected => {
            let slice = selected_slice(summary);
            if slice.use_playback_override {
                slice.envelope
            } else {
                summary.playback.envelope
            }
        }
    }
}

fn set_envelope_field(envelope: &mut LinnodEditorEnvelope, field: EnvelopeField, value: f32) {
    match field {
        EnvelopeField::Attack => envelope.attack_ms = value,
        EnvelopeField::Decay => envelope.decay_ms = value,
        EnvelopeField::Sustain => envelope.sustain = value,
        EnvelopeField::Release => envelope.release_ms = value,
    }
}

fn envelope_field_value(
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
    field: EnvelopeField,
) -> Memo<f32> {
    Memo::new(move |_| envelope_field(envelope_for_target(&summary.get(), target), field))
}

fn envelope_field_text(
    summary: Signal<LinnodEditorPatchSummary>,
    target: PlaybackEditTarget,
    field: EnvelopeField,
) -> Memo<String> {
    Memo::new(move |_| {
        let value = envelope_field(envelope_for_target(&summary.get(), target), field);
        match field {
            EnvelopeField::Sustain => format!("{:.0}%", value * 100.0),
            _ => format!("{value:.0} ms"),
        }
    })
}

fn envelope_field(envelope: LinnodEditorEnvelope, field: EnvelopeField) -> f32 {
    match field {
        EnvelopeField::Attack => envelope.attack_ms,
        EnvelopeField::Decay => envelope.decay_ms,
        EnvelopeField::Sustain => envelope.sustain,
        EnvelopeField::Release => envelope.release_ms,
    }
}

fn playback_scope_detail(
    summary: Signal<LinnodEditorPatchSummary>,
    scope: Signal<ControlScope>,
) -> Memo<String> {
    Memo::new(move |_| {
        let summary = summary.get();
        match scope.get() {
            ControlScope::Global => {
                format!("all slices / {}", playback_mode_label(summary.playback.mode))
            }
            ControlScope::Selected => {
                let slice = selected_slice(&summary);
                let source = if slice.use_playback_override {
                    "override"
                } else {
                    "global"
                };
                format!(
                    "{source} / {}",
                    playback_mode_label(playback_mode_for_target(
                        &summary,
                        PlaybackEditTarget::Selected
                    ))
                )
            }
        }
    })
}

fn global_playback_mode_text(summary: Signal<LinnodEditorPatchSummary>) -> Memo<String> {
    Memo::new(move |_| playback_mode_label(summary.get().playback.mode).to_string())
}

fn playback_mode_label(mode: LinnodEditorPlaybackMode) -> &'static str {
    match mode {
        LinnodEditorPlaybackMode::OneShot => "one-shot",
        LinnodEditorPlaybackMode::Gated => "gate",
        LinnodEditorPlaybackMode::Looped => "loop",
        LinnodEditorPlaybackMode::Continue => "continue",
    }
}
