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
            sidechain_required: Signal::new(values.telemetry.sidechain_required),
            sidechain_input_detected: Signal::new(values.telemetry.sidechain_input_detected),
            sidechain_signal_active: Signal::new(values.telemetry.sidechain_signal_active),
            audio_note_detected: Signal::new(values.telemetry.audio_note_detected),
            audio_note_pitch_confidence: Signal::new(
                values.telemetry.audio_note_pitch_confidence,
            ),
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
            live_input_column(cx, signals);
        })
        .height(Pixels(568.0))
        .horizontal_gap(Pixels(10.0));

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

fn live_input_column(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, |cx| {
        HStack::new(cx, |cx| {
            Svg::new(cx, ICON_ACTIVITY).class("toolbar-icon");
            Label::new(cx, "Live Input").class("section-title");
            Spacer::new(cx);
            Label::new(cx, sidechain_status_text(signals)).class("muted");
        })
        .height(Pixels(22.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(8.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Element::new(cx)
                    .class("chip")
                    .toggle_class("chip-on", signals.sidechain_input_detected)
                    .width(Pixels(52.0))
                    .height(Pixels(20.0))
                    .text("Input");
                Element::new(cx)
                    .class("chip")
                    .toggle_class("chip-on", signals.sidechain_signal_active)
                    .toggle_class("chip-warm", sidechain_warning(signals))
                    .width(Pixels(52.0))
                    .height(Pixels(20.0))
                    .text("Signal");
                Element::new(cx)
                    .class("chip")
                    .toggle_class("chip-on", signals.audio_note_detected)
                    .width(Pixels(48.0))
                    .height(Pixels(20.0))
                    .text("Note");
                Label::new(
                    cx,
                    pitch_confidence_text(
                        signals.audio_note_detected,
                        signals.audio_note_pitch_confidence,
                    ),
                )
                .class("value-label")
                .width(Stretch(1.0));
            })
            .height(Pixels(24.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(6.0));

            parameter_segmented(cx, signals.parameter(ResonatorEditorSurfaceSlot::AudioInputMode));
            parameter_segmented(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationMode),
            );
        })
        .class("strip")
        .height(Pixels(100.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(7.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Note Detection").class("value-label");
                Spacer::new(cx);
                Label::new(cx, "Audio").class("muted");
            })
            .height(Pixels(18.0))
            .alignment(Alignment::Center);
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteOnsetSensitivity),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteReleaseFloor),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteMinimumLength),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNotePitchConfidence),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioNoteVelocityAmount),
            );
        })
        .class("strip")
        .height(Pixels(128.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(6.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Expression").class("value-label");
                Spacer::new(cx);
                binary_switch(
                    cx,
                    signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionEnable),
                );
            })
            .height(Pixels(26.0))
            .alignment(Alignment::Center)
            .horizontal_gap(Pixels(8.0));
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPitchRange),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPressureFloor),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionPressureCeiling),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionBrightnessFloor),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::AudioExpressionBrightnessCeiling),
            );
        })
        .class("strip")
        .height(Pixels(136.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(6.0));

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Excitation").class("value-label");
                Spacer::new(cx);
                let gain = signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationGain);
                Label::new(cx, value_text(gain)).class("value-label");
            })
            .height(Pixels(18.0))
            .alignment(Alignment::Center);
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationGain),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchWindow),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchPreRoll),
            );
            parameter_slider(
                cx,
                signals.parameter(ResonatorEditorSurfaceSlot::LiveExcitationLatchFade),
            );
        })
        .class("strip")
        .height(Pixels(116.0))
        .padding(Pixels(10.0))
        .vertical_gap(Pixels(6.0));
    })
    .class("panel")
    .width(Pixels(260.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}
