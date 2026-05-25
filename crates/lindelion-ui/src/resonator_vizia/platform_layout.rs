fn build_editor(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        resonator_top_strip(cx, signals);
        HStack::new(cx, move |cx| {
            resonator_source_section(cx, signals);
            resonator_stack_section(cx, signals);
            resonator_output_section(cx, signals);
        })
        .height(Stretch(1.0))
        .horizontal_gap(Pixels(10.0));
        HStack::new(cx, move |cx| {
            resonator_library_section(cx, signals);
            resonator_modulation_section(cx, signals);
        })
        .height(Pixels(224.0))
        .horizontal_gap(Pixels(10.0));
    })
    .class("root")
    .class("ll-shell")
    .padding(Pixels(12.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(10.0));
}

fn resonator_top_strip(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "LAMATH").class("title");
            Label::new(cx, patch_detail_text(signals)).class("muted");
        })
        .width(Stretch(1.0))
        .vertical_gap(Pixels(2.0));
        crate::vizia_controls::metric(cx, "voices", voices_text(signals.active_voices));
        crate::vizia_controls::metric(cx, "pitch", pitch_confidence_text(signals.audio_note_pitch_confidence));
        sidechain_chip(cx, "side req", signals.sidechain_required);
        sidechain_chip(cx, "input", signals.sidechain_input_detected);
        sidechain_chip(cx, "active", signals.sidechain_signal_active);
        HStack::new(cx, move |cx| {
            resonator_tool_button(cx, ICON_FOLDER_OPEN, "Load patch", UiCommand::LoadPatch);
            resonator_tool_button(cx, ICON_DOWNLOAD, "Export patch with samples", UiCommand::ExportPatchWithSamples);
            resonator_tool_button(cx, ICON_LIBRARY, "Open library", UiCommand::OpenLibrary);
        })
        .horizontal_gap(Pixels(5.0));
    })
    .class("topbar")
    .class("ll-top-strip")
    .height(Pixels(60.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(8.0));
}

fn resonator_source_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Source / Excite",
            "midi, audio and latch input",
            crate::vizia_controls::Accent::Audio,
        );
        WaveformStrip::new(cx, 0.64)
            .class("ll-visual-frame")
            .class("ll-visual-audio")
            .height(Pixels(86.0));
        resonator_parameter_control(
            cx,
            signals.parameter(ResonatorEditorSurfaceSlot::AudioInputMode),
            crate::vizia_controls::Accent::Audio,
        );
        resonator_parameter_control(
            cx,
            signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationMode),
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionEnable),
                crate::vizia_controls::Accent::Audio,
            );
            resonator_tool_button(cx, ICON_ACTIVITY, "Load selected excitation slot", UiCommand::LoadSelectedExcitationSlot);
            resonator_tool_button(cx, ICON_TRASH, "Clear selected excitation slot", UiCommand::ClearSelectedExcitationSlot);
        })
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
        HStack::new(cx, move |cx| {
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteOnsetSensitivity),
                crate::vizia_controls::Accent::Audio,
            );
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteMinimumLength),
                crate::vizia_controls::Accent::Audio,
            );
        })
        .horizontal_gap(Pixels(4.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-audio")
    .width(Pixels(286.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn resonator_stack_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Resonator Stack",
            "dual modal and waveguide lanes",
            crate::vizia_controls::Accent::Tone,
        );
        ResonatorScope::new(cx, signals.left_rms, signals.right_rms, signals.active_voices)
            .class("ll-visual-frame")
            .class("ll-visual-tone")
            .height(Pixels(126.0));
        HStack::new(cx, move |cx| {
            resonator_lane_a(cx, signals);
            resonator_lane_b(cx, signals);
        })
        .height(Stretch(1.0))
        .horizontal_gap(Pixels(10.0));
        HStack::new(cx, move |cx| {
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::Routing),
                crate::vizia_controls::Accent::Mod,
            );
            resonator_parameter_control(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::RetriggerResonators),
                crate::vizia_controls::Accent::Tone,
            );
        })
        .height(Pixels(36.0))
        .horizontal_gap(Pixels(10.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-tone")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn resonator_lane_a(cx: &mut Context, signals: EditorSignals) {
    resonator_lane(
        cx,
        "A",
        [
            ResonatorEditorSurfaceSlot::ResonatorAModel,
            ResonatorEditorSurfaceSlot::ResonatorAWaveguideStyle,
        ],
        [
            ResonatorEditorSurfaceSlot::ResonatorAPreset,
            ResonatorEditorSurfaceSlot::ResonatorABrightness,
            ResonatorEditorSurfaceSlot::ResonatorADecay,
            ResonatorEditorSurfaceSlot::ResonatorABoundaryReflection,
        ],
        signals,
    );
}

fn resonator_lane_b(cx: &mut Context, signals: EditorSignals) {
    resonator_lane(
        cx,
        "B",
        [
            ResonatorEditorSurfaceSlot::ResonatorBModel,
            ResonatorEditorSurfaceSlot::ResonatorBWaveguideStyle,
        ],
        [
            ResonatorEditorSurfaceSlot::ResonatorBLoopFilter,
            ResonatorEditorSurfaceSlot::ResonatorBLoopGain,
            ResonatorEditorSurfaceSlot::ResonatorBNonlinearity,
            ResonatorEditorSurfaceSlot::ResonatorBBoundaryReflection,
        ],
        signals,
    );
}

fn resonator_lane(
    cx: &mut Context,
    title: &'static str,
    header_slots: [ResonatorEditorSurfaceSlot; 2],
    knob_slots: [ResonatorEditorSurfaceSlot; 4],
    signals: EditorSignals,
) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, title).class("section-title").width(Pixels(22.0));
            ResonatorBadge::new(cx, signals.parameter(header_slots[0]).signal)
                .class("ll-visual-frame")
                .width(Pixels(54.0))
                .height(Pixels(18.0));
            resonator_parameter_control(
                cx,
                signals.parameter(header_slots[0]),
                crate::vizia_controls::Accent::Tone,
            );
            resonator_parameter_control(
                cx,
                signals.parameter(header_slots[1]),
                crate::vizia_controls::Accent::Tone,
            );
        })
        .height(Pixels(44.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
        HStack::new(cx, move |cx| {
            for slot in knob_slots {
                resonator_parameter_control(
                    cx,
                    signals.parameter(slot),
                    crate::vizia_controls::Accent::Tone,
                );
            }
        })
        .horizontal_gap(Pixels(4.0));
    })
    .class("strip")
    .class("ll-panel")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(6.0));
}

fn resonator_output_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::static_section_header(
            cx,
            "Output",
            "filter, level and meter",
            crate::vizia_controls::Accent::Tone,
        );
        LevelMeter::new(cx, signals.left_peak, signals.right_peak)
            .class("ll-visual-frame")
            .class("ll-visual-tone")
            .height(Pixels(70.0));
        HStack::new(cx, move |cx| {
            resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Master), crate::vizia_controls::Accent::Tone);
            resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Pan), crate::vizia_controls::Accent::Tone);
            resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Saturation), crate::vizia_controls::Accent::Tone);
        })
        .horizontal_gap(Pixels(3.0));
        HStack::new(cx, move |cx| {
            resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Cutoff), crate::vizia_controls::Accent::Tone);
            resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Resonance), crate::vizia_controls::Accent::Tone);
        })
        .horizontal_gap(Pixels(3.0));
        resonator_parameter_control(
            cx,
            signals.parameter(ResonatorEditorSurfaceSlot::FilterMode),
            crate::vizia_controls::Accent::Tone,
        );
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-tone")
    .width(Pixels(254.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn resonator_library_section(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        crate::vizia_controls::section_header(
            cx,
            "Library / Slots",
            selected_slot_text(signals),
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            for index in 0..4 {
                resonator_slot_button(cx, signals, index);
            }
        })
        .height(Pixels(72.0))
        .horizontal_gap(Pixels(6.0));
        HStack::new(cx, move |cx| {
            for index in 0..3 {
                resonator_sample_row(cx, signals, index);
            }
        })
        .height(Stretch(1.0))
        .horizontal_gap(Pixels(6.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-audio")
    .width(Pixels(520.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn resonator_slot_button(cx: &mut Context, signals: EditorSignals, index: usize) {
    let slot = PadId((index + 1) as u8);
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, slot_label_text(signals.slot_summaries, index)).class("ll-control-value");
            Label::new(cx, slot_detail_text(signals.slot_summaries, index)).class("ll-section-subtitle");
            MiniWaveform::new(cx, Memo::new(move |_| index as f32 * 0.21))
                .class("ll-visual-frame")
                .class("ll-visual-audio")
                .height(Pixels(22.0));
        })
        .vertical_gap(Pixels(2.0))
    })
    .on_press(move |cx| cx.emit(EditorEvent::Command(UiCommand::SelectExcitationSlot(slot))))
    .class("ll-pad-button")
    .toggle_class(
        "ll-pad-selected",
        signals.selected_slot.map(move |selected| (*selected).round() as usize == index),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn resonator_sample_row(cx: &mut Context, signals: EditorSignals, index: usize) {
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, sample_label_text(signals.library_samples, index)).class("ll-control-value");
            Label::new(cx, sample_detail_text(signals.library_samples, index)).class("ll-section-subtitle");
            LibraryWaveform::new(cx, signals.library_samples, index)
                .class("ll-visual-frame")
                .class("ll-visual-audio")
                .height(Pixels(36.0));
        })
        .vertical_gap(Pixels(2.0))
    })
    .on_press(move |cx| cx.emit(EditorEvent::SelectLibrarySample(index)))
    .class("sample-row")
    .class("ll-pad-button")
    .toggle_class(
        "ll-pad-selected",
        signals.selected_sample.map(move |selected| (*selected).round() as usize == index),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn resonator_modulation_section(cx: &mut Context, signals: EditorSignals) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            crate::vizia_controls::static_section_header(
                cx,
                "Envelope",
                "amp shape",
                crate::vizia_controls::Accent::Tone,
            );
            HStack::new(cx, move |cx| {
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::AmpAttack), crate::vizia_controls::Accent::Tone);
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::AmpRelease), crate::vizia_controls::Accent::Tone);
            })
            .horizontal_gap(Pixels(4.0));
        })
        .class("ll-panel")
        .class("ll-panel-tone")
        .width(Pixels(220.0))
        .height(Stretch(1.0))
        .vertical_gap(Pixels(8.0));
        VStack::new(cx, move |cx| {
            crate::vizia_controls::static_section_header(
                cx,
                "Modulation",
                "lfo and first slot",
                crate::vizia_controls::Accent::Mod,
            );
            HStack::new(cx, move |cx| {
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::LfoRate), crate::vizia_controls::Accent::Mod);
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::LfoShape), crate::vizia_controls::Accent::Mod);
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Mod1Enabled), crate::vizia_controls::Accent::Mod);
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Mod1Amount), crate::vizia_controls::Accent::Mod);
            })
            .horizontal_gap(Pixels(4.0));
            HStack::new(cx, move |cx| {
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Mod1Source), crate::vizia_controls::Accent::Mod);
                resonator_parameter_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::Mod1Destination), crate::vizia_controls::Accent::Mod);
            })
            .height(Pixels(34.0))
            .horizontal_gap(Pixels(8.0));
        })
        .class("ll-panel")
        .class("ll-panel-mod")
        .width(Stretch(1.0))
        .height(Stretch(1.0))
        .vertical_gap(Pixels(8.0));
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-mod")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .horizontal_gap(Pixels(8.0));
}

fn sidechain_chip(cx: &mut Context, label: &'static str, signal: Signal<bool>) {
    Label::new(cx, sidechain_text(label, signal))
        .class("ll-status-chip")
        .toggle_class("ll-chip-ready", signal)
        .height(Pixels(22.0))
        .alignment(Alignment::Center);
}

fn patch_detail_text(signals: EditorSignals) -> Memo<String> {
    Memo::new(move |_| {
        let command = signals
            .command_status
            .get()
            .map(|command| command_label(Some(command)))
            .unwrap_or("Ready");
        format!("{} / {command}", signals.patch_name.get())
    })
}

fn voices_text(active_voices: Signal<f32>) -> Memo<String> {
    Memo::new(move |_| format!("{:.0}", active_voices.get().max(0.0)))
}

fn pitch_confidence_text(confidence: Signal<f32>) -> Memo<String> {
    Memo::new(move |_| format!("{:.0}%", confidence.get().clamp(0.0, 1.0) * 100.0))
}

fn sidechain_text(label: &'static str, signal: Signal<bool>) -> Memo<String> {
    Memo::new(move |_| format!("{label} {}", if signal.get() { "on" } else { "off" }))
}

fn selected_slot_text(signals: EditorSignals) -> Memo<String> {
    Memo::new(move |_| format!("slot {}", signals.selected_slot.get().round() as usize + 1))
}

fn slot_label_text(
    summaries: Signal<[ResonatorEditorSlotSummary; 4]>,
    index: usize,
) -> Memo<String> {
    Memo::new(move |_| summaries.get()[index].label.clone())
}

fn slot_detail_text(
    summaries: Signal<[ResonatorEditorSlotSummary; 4]>,
    index: usize,
) -> Memo<String> {
    Memo::new(move |_| summaries.get()[index].detail.clone())
}

fn sample_label_text(
    samples: Signal<Vec<ResonatorEditorSampleSummary>>,
    index: usize,
) -> Memo<String> {
    Memo::new(move |_| {
        samples
            .get()
            .get(index)
            .map(|sample| sample.label.clone())
            .unwrap_or_else(|| "Empty".to_string())
    })
}

fn sample_detail_text(
    samples: Signal<Vec<ResonatorEditorSampleSummary>>,
    index: usize,
) -> Memo<String> {
    Memo::new(move |_| {
        samples
            .get()
            .get(index)
            .map(|sample| sample.detail.clone())
            .unwrap_or_else(|| "No sample".to_string())
    })
}
