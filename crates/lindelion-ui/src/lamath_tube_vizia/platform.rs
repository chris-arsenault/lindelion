use std::{ffi::c_void, path::Path, sync::Arc, task::Poll, time::Duration};

use vizia::{ParentWindow, WindowHandle, WindowScalePolicy, prelude::*};

use super::{
    LAMATH_TUBE_EDITOR_HEIGHT, LAMATH_TUBE_EDITOR_WIDTH, LamathTubeControlSurface,
    LamathTubeEditorHost, LamathTubeEditorSize, LamathTubeKnob, LamathTubeKnobGroup,
    LamathTubeModelSwitch, LamathTubeSwitchId,
};
use crate::{
    audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListHost, AudioFileSlotListView, AudioFileSource,
    },
    vizia_file_dialogs::{PendingFileDialog, wav_audio_dialog},
};

const STYLE: &str = r#"
    .tube-root {
        background-color: #101315;
        width: 1s;
        height: 1s;
        padding: 16px;
        vertical-gap: 12px;
    }
    .tube-header { horizontal-gap: 12px; }
    .tube-header-text { width: 1s; vertical-gap: 2px; }
    .tube-card-row { horizontal-gap: 12px; }
    .tube-reed-card { width: 1s; }
    .tube-bore-card { width: auto; }
    .tube-player-card { width: 1s; }
    .tube-model-card { width: auto; vertical-gap: 6px; }
    .tube-hint { color: #7e887e; font-size: 10px; }
    .tube-title { color: #e3e7e2; font-size: 22px; }
    .tube-subtitle { color: #91a19b; font-size: 12px; }
    .tube-panel {
        background-color: #171c1d;
        border-width: 1px;
        border-color: #313a3b;
        border-radius: 6px;
        padding: 10px;
        vertical-gap: 8px;
    }
    .tube-section { color: #aab6b0; font-size: 13px; }
    .tube-knob-row { horizontal-gap: 10px; }
    .tube-knob-cell { width: 78px; vertical-gap: 4px; alignment: center; }
    .tube-knob { width: 44px; height: 44px; }
    .tube-knob .knob-track { color: #d6a94f; background-color: #2a3031; }
    .tube-knob .knob-head {
        background-color: #202728;
        border-width: 1px;
        border-color: #667370;
        color: #f5ead2;
    }
    .tube-knob:hover .knob-head { border-color: #e0c27a; }
    .tube-knob .knob-tick {
        background-color: #f5ead2;
        width: 2px;
        height: 9px;
        corner-radius: 1px;
    }
    .tube-knob-label { color: #d5ddd8; font-size: 11px; text-align: center; }
    .tube-knob-value { color: #8d9a96; font-size: 10px; text-align: center; }
    .tube-switch-row { horizontal-gap: 8px; }
    .tube-switch {
        background-color: #22292a;
        border-width: 1px;
        border-color: #3b4647;
        border-radius: 5px;
        color: #b8c4bf;
        height: 30px;
        width: 142px;
    }
    .tube-switch-on { border-color: #d6a94f; color: #f1e5cb; }
    .tube-switch-locked { border-color: #496158; color: #9fc9b8; }
    .tube-slot-row { horizontal-gap: 6px; }
    .tube-slot {
        background-color: #202728;
        border-width: 1px;
        border-color: #394445;
        border-radius: 5px;
        width: 68px;
        height: 58px;
        padding: 5px;
        vertical-gap: 2px;
    }
    .tube-slot-selected { border-color: #d6a94f; }
    .tube-slot-key { color: #95a39e; font-size: 10px; text-align: center; }
    .tube-slot-label { color: #dde5e0; font-size: 10px; text-align: center; }
    .tube-slot-source { color: #7f908a; font-size: 10px; text-align: center; }
    .tube-action-row { horizontal-gap: 8px; alignment: center; }
    .tube-selected-label { color: #d3ded8; width: 300px; }
    .tube-button {
        background-color: #232b2c;
        border-radius: 5px;
        border-width: 1px;
        border-color: #3c4748;
        color: #dce4df;
        padding: 6px;
        width: 76px;
    }
    .tube-button:hover { border-color: #d6a94f; }
"#;

#[derive(Clone, Copy)]
struct TubeSignals {
    knobs: Signal<Vec<LamathTubeKnob>>,
    switches: Signal<Vec<LamathTubeModelSwitch>>,
    slots: Signal<AudioFileSlotListView>,
}

impl TubeSignals {
    fn from_host(host: &LamathTubeEditorHost) -> Self {
        Self {
            knobs: Signal::new(host.controls.knobs()),
            switches: Signal::new(host.controls.model_switches()),
            slots: Signal::new(host.articulations.surface.slot_list_view()),
        }
    }
}

enum TubeEvent {
    SetKnob {
        id: u32,
        normalized: f32,
    },
    SetSwitch {
        id: LamathTubeSwitchId,
        enabled: bool,
    },
    SelectSlot(AudioFileSlotId),
    OpenSlotDialog(AudioFileSlotId),
    ClearSlot(AudioFileSlotId),
    Sync,
}

struct TubeModel {
    controls: Arc<dyn LamathTubeControlSurface>,
    articulations: AudioFileSlotListHost,
    signals: TubeSignals,
    pending_dialog: Option<(AudioFileSlotId, PendingFileDialog)>,
}

impl Model for TubeModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|tube_event, _| match tube_event {
            TubeEvent::SetKnob { id, normalized } => {
                self.controls.set_knob_normalized(*id, *normalized);
                self.signals.knobs.set(self.controls.knobs());
            }
            TubeEvent::SetSwitch { id, enabled } => {
                self.controls.set_model_switch(*id, *enabled);
                self.signals.switches.set(self.controls.model_switches());
            }
            TubeEvent::SelectSlot(slot) => {
                self.articulations.surface.select_slot(*slot);
                self.signals
                    .slots
                    .set(self.articulations.surface.slot_list_view());
            }
            TubeEvent::OpenSlotDialog(slot) => {
                self.articulations.surface.select_slot(*slot);
                self.signals
                    .slots
                    .set(self.articulations.surface.slot_list_view());
                self.pending_dialog = Some((
                    *slot,
                    PendingFileDialog::pick_file(wav_audio_dialog(Path::new("."), None)),
                ));
            }
            TubeEvent::ClearSlot(slot) => {
                self.articulations.surface.clear_audio_file(*slot);
                self.signals
                    .slots
                    .set(self.articulations.surface.slot_list_view());
            }
            TubeEvent::Sync => {
                if let Some((slot, dialog)) = self.pending_dialog.as_mut() {
                    match dialog.poll_path() {
                        Poll::Ready(Some(path)) => {
                            self.articulations.surface.load_audio_file(*slot, &path);
                            self.signals
                                .slots
                                .set(self.articulations.surface.slot_list_view());
                            self.signals.knobs.set(self.controls.knobs());
                            self.signals.switches.set(self.controls.model_switches());
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

/// The layout tells the instrument's physical story: a player's breath drives
/// the reed, the reed speaks through the bore. Each card is one stage.
fn build_editor(cx: &mut Context, signals: TubeSignals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, "Lamath Tube").class("tube-title");
                Label::new(cx, "a reed-driven bore, breathed through a register key")
                    .class("tube-subtitle");
            })
            .class("tube-header-text");
            group_knob_cell(cx, signals, LamathTubeKnobGroup::Output, 0);
        })
        .class("tube-header");

        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, "REED").class("tube-section");
                HStack::new(cx, move |cx| {
                    for slot in 0..4 {
                        group_knob_cell(cx, signals, LamathTubeKnobGroup::Reed, slot);
                    }
                })
                .class("tube-knob-row");
            })
            .class("tube-panel")
            .class("tube-reed-card");

            VStack::new(cx, move |cx| {
                Label::new(cx, "BORE").class("tube-section");
                HStack::new(cx, move |cx| {
                    for slot in 0..3 {
                        group_knob_cell(cx, signals, LamathTubeKnobGroup::Bore, slot);
                    }
                })
                .class("tube-knob-row");
            })
            .class("tube-panel")
            .class("tube-bore-card");
        })
        .class("tube-card-row");

        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, "PLAYER").class("tube-section");
                HStack::new(cx, move |cx| {
                    for slot in 0..3 {
                        group_knob_cell(cx, signals, LamathTubeKnobGroup::Player, slot);
                    }
                })
                .class("tube-knob-row");
                Label::new(
                    cx,
                    "Half way is a steady player; beyond it gets theatrical.",
                )
                .class("tube-hint");
            })
            .class("tube-panel")
            .class("tube-player-card");

            VStack::new(cx, move |cx| {
                Label::new(cx, "MODEL").class("tube-section");
                HStack::new(cx, move |cx| {
                    for index in 0..2 {
                        switch_cell(cx, signals, index);
                    }
                })
                .class("tube-switch-row");
                HStack::new(cx, move |cx| {
                    for index in 2..4 {
                        switch_cell(cx, signals, index);
                    }
                })
                .class("tube-switch-row");
            })
            .class("tube-panel")
            .class("tube-model-card");
        })
        .class("tube-card-row");

        VStack::new(cx, move |cx| {
            Label::new(cx, "ARTICULATIONS").class("tube-section");
            HStack::new(cx, move |cx| {
                for index in 0..8 {
                    slot_cell(cx, signals, index);
                }
            })
            .class("tube-slot-row");
            selected_slot_actions(cx, signals);
        })
        .class("tube-panel");
    })
    .class("tube-root");
}

/// The `slot`-th knob of a group, in host-parameter order.
fn group_knob_cell(
    cx: &mut Context,
    signals: TubeSignals,
    group: LamathTubeKnobGroup,
    slot: usize,
) {
    let knob = move || {
        signals
            .knobs
            .get()
            .iter()
            .copied()
            .filter(|knob| knob.group == group)
            .nth(slot)
    };
    VStack::new(cx, move |cx| {
        Knob::new(
            cx,
            0.5,
            Memo::new(move |_| knob().map(|knob| knob.normalized).unwrap_or(0.0)),
            false,
        )
        .class("tube-knob")
        .on_change(move |cx, normalized| {
            if let Some(knob) = knob() {
                cx.emit(TubeEvent::SetKnob {
                    id: knob.id,
                    normalized,
                });
            }
        });
        Label::new(
            cx,
            Memo::new(move |_| {
                knob()
                    .map(|knob| knob.label.to_string())
                    .unwrap_or_default()
            }),
        )
        .class("tube-knob-label");
        Label::new(
            cx,
            Memo::new(move |_| {
                knob()
                    .map(|knob| format!("{:.2} {}", knob.plain, knob.units))
                    .unwrap_or_default()
            }),
        )
        .class("tube-knob-value");
    })
    .class("tube-knob-cell")
    .display(Memo::new(move |_| knob().is_some()));
}

fn switch_cell(cx: &mut Context, signals: TubeSignals, index: usize) {
    Button::new(cx, move |cx| {
        Label::new(
            cx,
            Memo::new(move |_| {
                signals
                    .switches
                    .get()
                    .get(index)
                    .map(|switch| switch.label.to_string())
                    .unwrap_or_default()
            }),
        )
    })
    .class("tube-switch")
    .toggle_class(
        "tube-switch-on",
        Memo::new(move |_| {
            signals
                .switches
                .get()
                .get(index)
                .map(|switch| switch.enabled)
                .unwrap_or(false)
        }),
    )
    .toggle_class(
        "tube-switch-locked",
        Memo::new(move |_| {
            signals
                .switches
                .get()
                .get(index)
                .map(|switch| !switch.editable)
                .unwrap_or(false)
        }),
    )
    .display(Memo::new(move |_| signals.switches.get().len() > index))
    .on_press(move |cx| {
        if let Some(switch) = signals.switches.get().get(index)
            && switch.editable
        {
            cx.emit(TubeEvent::SetSwitch {
                id: switch.id,
                enabled: !switch.enabled,
            });
        }
    });
}

fn slot_cell(cx: &mut Context, signals: TubeSignals, index: usize) {
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, slot_key_label(index)).class("tube-slot-key");
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
            .class("tube-slot-label");
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
            .class("tube-slot-source");
        })
    })
    .class("tube-slot")
    .toggle_class(
        "tube-slot-selected",
        Memo::new(move |_| signals.slots.get().selected == AudioFileSlotId(index)),
    )
    .display(Memo::new(move |_| signals.slots.get().slots.len() > index))
    .on_press(move |cx| cx.emit(TubeEvent::SelectSlot(AudioFileSlotId(index))));
}

fn selected_slot_actions(cx: &mut Context, signals: TubeSignals) {
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
        .class("tube-selected-label");
        Button::new(cx, |cx| Label::new(cx, "Load"))
            .class("tube-button")
            .on_press(move |cx| cx.emit(TubeEvent::OpenSlotDialog(signals.slots.get().selected)));
        Button::new(cx, |cx| Label::new(cx, "Clear"))
            .class("tube-button")
            .on_press(move |cx| cx.emit(TubeEvent::ClearSlot(signals.slots.get().selected)));
    })
    .class("tube-action-row");
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
        3 => "D#-2",
        4 => "E-2",
        5 => "F-2",
        6 => "F#-2",
        _ => "G-2",
    }
}

fn build_application(
    host: LamathTubeEditorHost,
    size: LamathTubeEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LAMATH_TUBE_EDITOR_WIDTH) as u32;
    let height = size.height.max(LAMATH_TUBE_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add lamath tube editor style");
        let signals = TubeSignals::from_host(&host);
        TubeModel {
            controls: Arc::clone(&host.controls),
            articulations: host.articulations.clone(),
            signals,
            pending_dialog: None,
        }
        .build(cx);
        let sync_timer = cx.add_timer(Duration::from_millis(66), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(TubeEvent::Sync);
            }
        });
        cx.start_timer(sync_timer);
        build_editor(cx, signals);
    })
    .ignore_default_theme()
    .title("Lamath Tube")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

pub struct LamathTubeViziaEditor {
    window: WindowHandle,
    #[cfg(target_os = "macos")]
    drop_targets: Option<crate::vizia_audio_file_drop::NativeAudioFileDropTargets>,
}

impl LamathTubeViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle.
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LamathTubeEditorHost,
        size: LamathTubeEditorSize,
    ) -> Self {
        let drop_host = host.clone();
        let parent = ParentWindow(parent);
        let window = build_application(host, size).open_parented(&parent);
        #[cfg(target_os = "macos")]
        let drop_targets = crate::vizia_audio_file_drop::NativeAudioFileDropTargets::install(
            &window,
            Arc::clone(&drop_host.articulations.surface),
            crate::vizia_audio_file_drop::AudioDropGrid {
                left: 26.0,
                top: 332.0,
                width: 588.0,
                height: 58.0,
                gap: 6.0,
                slot_count: 8,
            },
        );
        Self {
            window,
            #[cfg(target_os = "macos")]
            drop_targets,
        }
    }
}

impl Drop for LamathTubeViziaEditor {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.drop_targets.take();
        }
        if self.window.is_open() {
            self.window.close();
        }
    }
}
