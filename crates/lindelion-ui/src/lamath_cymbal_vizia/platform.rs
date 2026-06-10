use std::{ffi::c_void, path::Path, sync::Arc, task::Poll, time::Duration};

use vizia::{WindowScalePolicy, prelude::*};

use super::{
    LAMATH_CYMBAL_EDITOR_HEIGHT, LAMATH_CYMBAL_EDITOR_WIDTH, LamathCymbalControlSurface,
    LamathCymbalEditorHost, LamathCymbalEditorSize, LamathCymbalKnob, LamathCymbalPerformance,
    LamathCymbalPreset,
};
use crate::{
    audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListHost, AudioFileSlotListView, AudioFileSource,
    },
    vizia_file_dialogs::{PendingFileDialog, wav_audio_dialog},
    vizia_window::ViziaWindowEditor,
};

mod theme;

use theme::{CYMBAL_SVG, STYLE};

#[derive(Clone, Copy)]
struct CymbalSignals {
    knobs: Signal<Vec<LamathCymbalKnob>>,
    slots: Signal<AudioFileSlotListView>,
    presets: Signal<Vec<LamathCymbalPreset>>,
    preset_names: Signal<Vec<String>>,
    active_preset: Signal<Option<usize>>,
    performance: Signal<LamathCymbalPerformance>,
    tab: Signal<usize>,
}

impl CymbalSignals {
    fn from_host(host: &LamathCymbalEditorHost) -> Self {
        let presets = host.controls.presets();
        let preset_names = presets
            .iter()
            .map(|preset| preset.name.to_string())
            .collect();
        Self {
            knobs: Signal::new(host.controls.knobs()),
            slots: Signal::new(host.strikers.surface.slot_list_view()),
            presets: Signal::new(presets),
            preset_names: Signal::new(preset_names),
            active_preset: Signal::new(host.controls.active_preset()),
            performance: Signal::new(host.controls.performance()),
            tab: Signal::new(0),
        }
    }
}

enum CymbalEvent {
    SetKnob { id: u32, normalized: f32 },
    ApplyPreset(usize),
    SelectSlot(AudioFileSlotId),
    OpenSlotDialog(AudioFileSlotId),
    ClearSlot(AudioFileSlotId),
    ResetPerformance,
    Sync,
}

struct CymbalModel {
    controls: Arc<dyn LamathCymbalControlSurface>,
    strikers: AudioFileSlotListHost,
    signals: CymbalSignals,
    pending_dialog: Option<(AudioFileSlotId, PendingFileDialog)>,
}

impl Model for CymbalModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|cymbal_event, _| match cymbal_event {
            CymbalEvent::SetKnob { id, normalized } => {
                self.controls.set_knob_normalized(*id, *normalized);
                self.signals.knobs.set(self.controls.knobs());
                self.signals
                    .active_preset
                    .set(self.controls.active_preset());
            }
            CymbalEvent::ApplyPreset(index) => {
                self.controls.apply_preset(*index);
                self.signals.knobs.set(self.controls.knobs());
                self.signals
                    .active_preset
                    .set(self.controls.active_preset());
            }
            CymbalEvent::SelectSlot(slot) => {
                self.strikers.surface.select_slot(*slot);
                self.signals
                    .slots
                    .set(self.strikers.surface.slot_list_view());
            }
            CymbalEvent::OpenSlotDialog(slot) => {
                self.strikers.surface.select_slot(*slot);
                self.signals
                    .slots
                    .set(self.strikers.surface.slot_list_view());
                self.pending_dialog = Some((
                    *slot,
                    PendingFileDialog::pick_file(wav_audio_dialog(Path::new("."), None)),
                ));
            }
            CymbalEvent::ClearSlot(slot) => {
                self.strikers.surface.clear_audio_file(*slot);
                self.signals
                    .slots
                    .set(self.strikers.surface.slot_list_view());
            }
            CymbalEvent::ResetPerformance => {
                self.controls.reset_performance();
                self.signals.performance.set(self.controls.performance());
            }
            CymbalEvent::Sync => {
                // The audio-thread load changes independently of UI events, so the performance
                // signal is the one thing that must be polled every tick.
                let performance = self.controls.performance();
                if self.signals.performance.get() != performance {
                    self.signals.performance.set(performance);
                }
                if let Some((slot, dialog)) = self.pending_dialog.as_mut() {
                    match dialog.poll_path() {
                        Poll::Ready(Some(path)) => {
                            self.strikers.surface.load_audio_file(*slot, &path);
                            self.signals
                                .slots
                                .set(self.strikers.surface.slot_list_view());
                            self.signals.knobs.set(self.controls.knobs());
                            self.signals
                                .active_preset
                                .set(self.controls.active_preset());
                            self.pending_dialog = None;
                        }
                        Poll::Ready(None) => self.pending_dialog = None,
                        Poll::Pending => {}
                    }
                }
            }
        });
    }
}

fn build_editor(cx: &mut Context, signals: CymbalSignals) {
    VStack::new(cx, move |cx| {
        header(cx, signals);
        basic_pane(cx, signals);
        advanced_pane(cx, signals);
    })
    .class("cymbal-root");
}

fn header(cx: &mut Context, signals: CymbalSignals) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, |cx| {
            Label::new(cx, "Lamath Cymbal").class("cymbal-title");
            Label::new(cx, "Shared body idiophone").class("cymbal-subtitle");
        })
        .class("cymbal-title-block");
        performance_indicator(cx, signals);
        tab_strip(cx, signals);
    })
    .class("cymbal-header")
    .alignment(Alignment::Center);
}

/// Audio-thread load meter. A low bar with sound problems points at the voice; a full bar or a
/// rising xrun count points at buffer underruns. Severity classes recolor the bar/text.
fn performance_indicator(cx: &mut Context, signals: CymbalSignals) {
    let perf = signals.performance;
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "DSP LOAD").class("cymbal-perf-cap");
            HStack::new(cx, move |cx| {
                Element::new(cx)
                    .class("cymbal-perf-fill")
                    .toggle_class(
                        "cymbal-perf-warn",
                        Memo::new(move |_| perf_level(perf) == 1),
                    )
                    .toggle_class("cymbal-perf-bad", Memo::new(move |_| perf_level(perf) == 2))
                    .width(Memo::new(move |_| {
                        Units::Percentage((perf.get().load * 100.0).clamp(0.0, 100.0))
                    }));
            })
            .class("cymbal-perf-track");
            Label::new(cx, Memo::new(move |_| perf_text(perf.get())))
                .class("cymbal-perf-text")
                .toggle_class(
                    "cymbal-perf-warn",
                    Memo::new(move |_| perf_level(perf) == 1),
                )
                .toggle_class("cymbal-perf-bad", Memo::new(move |_| perf_level(perf) == 2));
        })
        .class("cymbal-perf-readout");
        Button::new(cx, |cx| {
            Label::new(cx, "Reset").alignment(Alignment::Center)
        })
        .class("cymbal-perf-reset")
        .on_press(|cx| cx.emit(CymbalEvent::ResetPerformance));
    })
    .class("cymbal-perf")
    .alignment(Alignment::Center);
}

/// Severity of the current load reading: 0 healthy, 1 tight, 2 overrunning. Any recorded dropout
/// latches the worst level so a transient xrun stays visible.
fn perf_level(perf: Signal<LamathCymbalPerformance>) -> u32 {
    let perf = perf.get();
    if perf.xruns > 0 || perf.peak_load >= 1.0 {
        2
    } else if perf.load >= 0.7 || perf.peak_load >= 0.85 {
        1
    } else {
        0
    }
}

fn perf_text(perf: LamathCymbalPerformance) -> String {
    format!(
        "{:.0}%  ·  pk {:.0}%  ·  xruns {}",
        perf.load * 100.0,
        perf.peak_load * 100.0,
        perf.xruns
    )
}

fn tab_strip(cx: &mut Context, signals: CymbalSignals) {
    HStack::new(cx, move |cx| {
        tab_button(cx, signals, 0, "Basic");
        tab_button(cx, signals, 1, "Advanced");
    })
    .class("cymbal-tabs")
    .alignment(Alignment::Center);
}

fn tab_button(cx: &mut Context, signals: CymbalSignals, index: usize, label: &'static str) {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .class("cymbal-tab")
    .toggle_class(
        "cymbal-tab-active",
        Memo::new(move |_| signals.tab.get() == index),
    )
    .on_press(move |_| signals.tab.set(index));
}

fn basic_pane(cx: &mut Context, signals: CymbalSignals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, |cx| {
                Svg::new(cx, CYMBAL_SVG).class("cymbal-image");
            })
            .class("cymbal-image-frame");

            VStack::new(cx, move |cx| {
                Label::new(cx, "Voice").class("cymbal-section");
                Select::new(cx, signals.preset_names, signals.active_preset, true)
                    .class("cymbal-select")
                    .placeholder("Custom")
                    .on_select(move |cx, index| cx.emit(CymbalEvent::ApplyPreset(index)));
                Label::new(cx, preset_description_memo(signals)).class("cymbal-voice-desc");
                Label::new(
                    cx,
                    "Pick a cymbal voice, then fine-tune it on the Advanced tab.",
                )
                .class("cymbal-voice-hint");
            })
            .class("cymbal-voice-col");
        })
        .class("cymbal-basic-top");

        strikers_panel(cx, signals);
    })
    .class("cymbal-pane")
    .display(Memo::new(move |_| signals.tab.get() == 0));
}

fn preset_description_memo(signals: CymbalSignals) -> impl Res<String> + Clone {
    Memo::new(move |_| match signals.active_preset.get() {
        Some(index) => signals
            .presets
            .get()
            .get(index)
            .map(|preset| preset.description.to_string())
            .unwrap_or_default(),
        None => "Custom voice — adjust the controls on the Advanced tab.".to_string(),
    })
}

fn advanced_pane(cx: &mut Context, signals: CymbalSignals) {
    VStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "Body").class("cymbal-section");
            HStack::new(cx, move |cx| {
                for index in 0..8 {
                    knob_cell(cx, signals, index);
                }
            })
            .class("cymbal-knob-grid");
        })
        .class("cymbal-panel");

        strikers_panel(cx, signals);
    })
    .class("cymbal-pane")
    .display(Memo::new(move |_| signals.tab.get() == 1));
}

/// The shared striker/mallet panel. It lives on both tabs: a preset only changes the body knobs, so
/// the player should always be able to pick and load strikers.
fn strikers_panel(cx: &mut Context, signals: CymbalSignals) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Sticks / Mallets").class("cymbal-section");
        HStack::new(cx, move |cx| {
            for index in 0..4 {
                slot_cell(cx, signals, index);
            }
        })
        .class("cymbal-slot-row");
        selected_slot_actions(cx, signals);
    })
    .class("cymbal-panel");
}

fn knob_cell(cx: &mut Context, signals: CymbalSignals, index: usize) {
    VStack::new(cx, move |cx| {
        Knob::new(
            cx,
            0.5,
            Memo::new(move |_| {
                signals
                    .knobs
                    .get()
                    .get(index)
                    .map(|knob| knob.normalized)
                    .unwrap_or(0.0)
            }),
            false,
        )
        .class("cymbal-knob")
        .on_change(move |cx, normalized| {
            if let Some(knob) = signals.knobs.get().get(index) {
                cx.emit(CymbalEvent::SetKnob {
                    id: knob.id,
                    normalized,
                });
            }
        });
        Label::new(
            cx,
            Memo::new(move |_| {
                signals
                    .knobs
                    .get()
                    .get(index)
                    .map(|knob| knob.label.to_string())
                    .unwrap_or_default()
            }),
        )
        .class("cymbal-knob-label");
        Label::new(
            cx,
            Memo::new(move |_| {
                signals
                    .knobs
                    .get()
                    .get(index)
                    .map(|knob| format!("{:.2} {}", knob.plain, knob.units))
                    .unwrap_or_default()
            }),
        )
        .class("cymbal-knob-value");
    })
    .class("cymbal-knob-cell")
    .display(Memo::new(move |_| signals.knobs.get().len() > index));
}

fn slot_cell(cx: &mut Context, signals: CymbalSignals, index: usize) {
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, slot_key_label(index)).class("cymbal-slot-key");
            Label::new(
                cx,
                Memo::new(move |_| {
                    signals
                        .slots
                        .get()
                        .slots
                        .get(index)
                        .map(|slot| slot.label.clone())
                        .unwrap_or_default()
                }),
            )
            .class("cymbal-slot-label");
            Label::new(
                cx,
                Memo::new(move |_| {
                    signals
                        .slots
                        .get()
                        .slots
                        .get(index)
                        .map(|slot| source_label(slot.source))
                        .unwrap_or_default()
                }),
            )
            .class("cymbal-slot-source");
        })
    })
    .class("cymbal-slot")
    .toggle_class(
        "cymbal-slot-selected",
        Memo::new(move |_| signals.slots.get().selected == AudioFileSlotId(index)),
    )
    .display(Memo::new(move |_| signals.slots.get().slots.len() > index))
    .on_press(move |cx| cx.emit(CymbalEvent::SelectSlot(AudioFileSlotId(index))));
}

fn selected_slot_actions(cx: &mut Context, signals: CymbalSignals) {
    HStack::new(cx, move |cx| {
        Label::new(
            cx,
            Memo::new(move |_| {
                let view = signals.slots.get();
                let index = view.selected.0;
                view.slots
                    .get(index)
                    .map(|slot| format!("{}  {}", slot_key_label(index), slot.label))
                    .unwrap_or_default()
            }),
        )
        .class("cymbal-selected-label");
        Button::new(cx, |cx| Label::new(cx, "Load").alignment(Alignment::Center))
            .class("cymbal-button")
            .on_press(move |cx| cx.emit(CymbalEvent::OpenSlotDialog(signals.slots.get().selected)));
        Button::new(cx, |cx| {
            Label::new(cx, "Clear").alignment(Alignment::Center)
        })
        .class("cymbal-button")
        .on_press(move |cx| cx.emit(CymbalEvent::ClearSlot(signals.slots.get().selected)));
    })
    .class("cymbal-action-row")
    .alignment(Alignment::Center);
}

fn source_label(source: AudioFileSource) -> String {
    match source {
        AudioFileSource::BuiltIn => "Built-in".to_string(),
        AudioFileSource::Loaded => "Loaded".to_string(),
    }
}

fn slot_key_label(index: usize) -> &'static str {
    match index {
        0 => "C-2",
        1 => "C#-2",
        2 => "D-2",
        _ => "D#-2",
    }
}

fn build_application(
    host: LamathCymbalEditorHost,
    size: LamathCymbalEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LAMATH_CYMBAL_EDITOR_WIDTH) as u32;
    let height = size.height.max(LAMATH_CYMBAL_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add lamath cymbal editor style");
        let signals = CymbalSignals::from_host(&host);
        CymbalModel {
            controls: Arc::clone(&host.controls),
            strikers: host.strikers.clone(),
            signals,
            pending_dialog: None,
        }
        .build(cx);
        let sync_timer = cx.add_timer(Duration::from_millis(66), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(CymbalEvent::Sync);
            }
        });
        cx.start_timer(sync_timer);
        build_editor(cx, signals);
    })
    .ignore_default_theme()
    .title("Lamath Cymbal")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

pub struct LamathCymbalViziaEditor(#[allow(dead_code)] ViziaWindowEditor);

impl LamathCymbalViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle.
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LamathCymbalEditorHost,
        size: LamathCymbalEditorSize,
    ) -> Self {
        crate::vizia_window::debug_log(format!(
            "lamath-cymbal-vizia: attach begin parent=0x{:x} size={}x{}",
            parent as usize, size.width, size.height
        ));
        let application = build_application(host, size);
        crate::vizia_window::debug_log("lamath-cymbal-vizia: application built");
        Self(unsafe { ViziaWindowEditor::attach(parent, application) })
    }
}
