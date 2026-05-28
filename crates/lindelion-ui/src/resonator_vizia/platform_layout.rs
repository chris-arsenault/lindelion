fn build_editor(cx: &mut Context, signals: EditorSignals) {
    ZStack::new(cx, move |cx| {
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
            .height(Pixels(264.0))
            .horizontal_gap(Pixels(10.0));
        })
        .padding(Pixels(12.0))
        .width(Stretch(1.0))
        .height(Stretch(1.0))
        .vertical_gap(Pixels(10.0));
        resonator_settings_overlay(cx, signals);
    })
    .class("root")
    .class("ll-shell")
    .width(Stretch(1.0))
    .height(Stretch(1.0));
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
            crate::vizia_controls::icon_tool_button(cx, ICON_PLUS, "Add sample to library")
                .on_press(|cx| cx.emit(EditorEvent::AddLibrarySample));
            crate::vizia_controls::icon_tool_button(cx, ICON_SETTINGS, "Settings")
                .on_press(|cx| cx.emit(EditorEvent::ToggleSettings));
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
            crate::vizia_controls::icon_tool_button(cx, ICON_FOLDER_OPEN, "Load sample to selected layer")
                .on_press(|cx| cx.emit(EditorEvent::ChooseSampleFile));
            resonator_tool_button(cx, ICON_TRASH, "Clear selected layer", UiCommand::ClearSelectedExcitationSlot);
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
            resonator_mix_column(cx, signals);
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
        ResonatorEditorSurfaceSlot::ResonatorAModel,
        ResonatorEditorSurfaceSlot::ResonatorAWaveguideStyle,
        [
            ResonatorEditorSurfaceSlot::ResonatorAPreset,
            ResonatorEditorSurfaceSlot::ResonatorABrightness,
            ResonatorEditorSurfaceSlot::ResonatorADecay,
        ],
        [
            ResonatorEditorSurfaceSlot::ResonatorALoopFilter,
            ResonatorEditorSurfaceSlot::ResonatorALoopGain,
            ResonatorEditorSurfaceSlot::ResonatorANonlinearity,
            ResonatorEditorSurfaceSlot::ResonatorABoundaryReflection,
        ],
        signals,
    );
}

fn resonator_lane_b(cx: &mut Context, signals: EditorSignals) {
    resonator_lane(
        cx,
        "B",
        ResonatorEditorSurfaceSlot::ResonatorBModel,
        ResonatorEditorSurfaceSlot::ResonatorBWaveguideStyle,
        [
            ResonatorEditorSurfaceSlot::ResonatorBPreset,
            ResonatorEditorSurfaceSlot::ResonatorBBrightness,
            ResonatorEditorSurfaceSlot::ResonatorBDecay,
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

fn resonator_mix_column(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        Spacer::new(cx);
        resonator_mix_control(cx, signals.parameter(ResonatorEditorSurfaceSlot::ResonatorMix));
        Spacer::new(cx);
    })
    .width(Pixels(76.0))
    .height(Stretch(1.0))
    .alignment(Alignment::Center);
}

fn resonator_lane(
    cx: &mut Context,
    title: &'static str,
    model_slot: ResonatorEditorSurfaceSlot,
    waveguide_style_slot: ResonatorEditorSurfaceSlot,
    modal_slots: [ResonatorEditorSurfaceSlot; 3],
    waveguide_slots: [ResonatorEditorSurfaceSlot; 4],
    signals: EditorSignals,
) {
    let model_signal = signals.parameter(model_slot).signal;
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Label::new(cx, title).class("section-title").width(Pixels(22.0));
            resonator_compact_binary_control(cx, signals.parameter(model_slot));
            HStack::new(cx, move |cx| {
                resonator_compact_binary_control(cx, signals.parameter(waveguide_style_slot));
            })
            .display(resonator_model_display(
                model_signal,
                ResonatorLaneModel::Waveguide,
            ));
        })
        .height(Pixels(44.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0));
        HStack::new(cx, move |cx| {
            for slot in modal_slots {
                resonator_parameter_control(
                    cx,
                    signals.parameter(slot),
                    crate::vizia_controls::Accent::Tone,
                );
            }
        })
        .display(resonator_model_display(
            model_signal,
            ResonatorLaneModel::Modal,
        ))
        .horizontal_gap(Pixels(4.0));
        HStack::new(cx, move |cx| {
            for slot in waveguide_slots {
                resonator_parameter_control(
                    cx,
                    signals.parameter(slot),
                    crate::vizia_controls::Accent::Tone,
                );
            }
        })
        .display(resonator_model_display(
            model_signal,
            ResonatorLaneModel::Waveguide,
        ))
        .horizontal_gap(Pixels(4.0));
    })
    .class("strip")
    .class("ll-panel")
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(6.0));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResonatorLaneModel {
    Modal,
    Waveguide,
}

fn resonator_model_display(
    model_signal: Signal<f32>,
    model: ResonatorLaneModel,
) -> impl Res<Display> + Clone {
    model_signal.map(move |value| {
        let active_model = if *value >= 0.5 {
            ResonatorLaneModel::Waveguide
        } else {
            ResonatorLaneModel::Modal
        };
        if active_model == model {
            Display::Flex
        } else {
            Display::None
        }
    })
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
            "Exciter",
            selected_layer_text(signals),
            crate::vizia_controls::Accent::Audio,
        );
        HStack::new(cx, move |cx| {
            for index in 0..4 {
                resonator_layer_button(cx, signals, index);
            }
        })
        .height(Pixels(58.0))
        .horizontal_gap(Pixels(6.0));
        resonator_library_browser(cx, signals);
    })
    .class("panel")
    .class("ll-panel")
    .class("ll-panel-audio")
    .width(Pixels(520.0))
    .height(Stretch(1.0))
    .vertical_gap(Pixels(8.0));
}

fn resonator_library_browser(cx: &mut Context, signals: EditorSignals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, library_browser_title(signals)).class("ll-control-value");
                Label::new(cx, signals.library_location).class("ll-section-subtitle");
            })
            .width(Stretch(1.0))
            .min_width(Pixels(0.0))
            .overflow(Overflow::Hidden)
            .vertical_gap(Pixels(1.0));
            crate::vizia_controls::icon_tool_button(cx, ICON_FOLDER_OPEN, "Load sample to selected layer")
                .on_press(|cx| cx.emit(EditorEvent::ChooseSampleFile));
            crate::vizia_controls::icon_tool_button(cx, ICON_PLUS, "Add sample to library")
                .on_press(|cx| cx.emit(EditorEvent::AddLibrarySample));
            resonator_tool_button(cx, ICON_LIBRARY, "Refresh library", UiCommand::OpenLibrary);
            resonator_tool_button(
                cx,
                ICON_TRASH,
                "Clear selected layer",
                UiCommand::ClearSelectedExcitationSlot,
            );
            crate::vizia_controls::icon_tool_button(cx, ICON_ARROW_BACK, "Previous library page")
                .on_press(|cx| cx.emit(EditorEvent::LibraryPagePrevious));
            crate::vizia_controls::icon_tool_button(cx, ICON_ARROW_FORWARD, "Next library page")
                .on_press(|cx| cx.emit(EditorEvent::LibraryPageNext));
        })
        .height(Pixels(30.0))
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(5.0));
        VStack::new(cx, move |cx| {
            for row in 0..LIBRARY_BROWSER_ROWS {
                resonator_sample_row(cx, signals, row);
            }
        })
        .height(Stretch(1.0))
        .vertical_gap(Pixels(4.0));
    })
    .height(Stretch(1.0))
    .vertical_gap(Pixels(6.0));
}

fn resonator_layer_button(cx: &mut Context, signals: EditorSignals, index: usize) {
    let slot = PadId((index + 1) as u8);
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, layer_label(index)).class("ll-control-value");
            Label::new(cx, layer_detail_text(signals.slot_summaries, index)).class("ll-section-subtitle");
            MiniWaveform::new(cx, Memo::new(move |_| index as f32 * 0.21))
                .class("ll-visual-frame")
                .class("ll-visual-audio")
                .height(Pixels(18.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .overflow(Overflow::Hidden)
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

fn resonator_sample_row(cx: &mut Context, signals: EditorSignals, row: usize) {
    Button::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, sample_label_text(signals, row)).class("ll-control-value");
                Label::new(cx, sample_detail_text(signals, row)).class("ll-section-subtitle");
            })
            .width(Pixels(190.0))
            .min_width(Pixels(0.0))
            .overflow(Overflow::Hidden)
            .vertical_gap(Pixels(1.0));
            LibraryWaveform::new(cx, signals.library_samples, signals.library_page_start, row)
                .class("ll-visual-frame")
                .class("ll-visual-audio")
                .width(Stretch(1.0))
                .overflow(Overflow::Hidden)
                .height(Pixels(24.0));
        })
        .width(Stretch(1.0))
        .min_width(Pixels(0.0))
        .overflow(Overflow::Hidden)
        .alignment(Alignment::Center)
        .horizontal_gap(Pixels(6.0))
    })
    .class("sample-row")
    .class("ll-pad-button")
    .on_press(move |cx| {
        cx.emit(EditorEvent::UseLibrarySample(
            signals.library_page_start.get().saturating_add(row),
        ));
    })
    .toggle_class(
        "ll-pad-selected",
        Memo::new(move |_| {
            selected_sample_matches(
                signals.selected_sample.get(),
                signals.library_page_start.get().saturating_add(row),
            )
        }),
    )
    .width(Stretch(1.0))
    .min_width(Pixels(0.0))
    .overflow(Overflow::Hidden)
    .height(Pixels(30.0));
}

fn selected_sample_matches(selected: f32, index: usize) -> bool {
    selected.is_finite() && selected >= 0.0 && selected.round() as usize == index
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
