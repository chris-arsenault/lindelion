use std::{ffi::c_void, path::Path, sync::Arc, task::Poll, time::Duration};

use vizia::{ParentWindow, WindowHandle, WindowScalePolicy, prelude::*};

use super::{
    LAMATH_STRINGED_EDITOR_HEIGHT, LAMATH_STRINGED_EDITOR_WIDTH, LamathStringedBodyId,
    LamathStringedControlSurface, LamathStringedDriverId, LamathStringedEditorHost,
    LamathStringedEditorSize, LamathStringedKnob, LamathStringedModelSwitch,
    LamathStringedSwitchId,
};
use crate::{
    audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListHost, AudioFileSlotListView, AudioFileSource,
    },
    vizia_file_dialogs::{PendingFileDialog, wav_audio_dialog},
};

const STYLE: &str = r#"
    .stringed-root {
        background-color: #101214;
        width: 1s;
        height: 1s;
        padding: 16px;
        vertical-gap: 12px;
    }
    .stringed-title { color: #e6e3d6; font-size: 22px; }
    .stringed-subtitle { color: #9da69c; font-size: 12px; }
    .stringed-strip {
        background-color: #171a1d;
        border-color: #34383c;
        border-width: 1px;
        border-radius: 6px;
        padding: 10px;
        vertical-gap: 8px;
    }
    .stringed-section { color: #bbc0b8; font-size: 12px; }
    .stringed-path { horizontal-gap: 10px; }
    .stringed-choice {
        background-color: #23272b;
        border-color: #44494e;
        border-width: 1px;
        border-radius: 5px;
        color: #cbd1c8;
        width: 96px;
        height: 30px;
    }
    .stringed-choice-on { border-color: #c68b4a; color: #f2dfc4; }
    .stringed-knobs { horizontal-gap: 10px; }
    .stringed-knob-cell { width: 82px; vertical-gap: 4px; alignment: center; }
    .stringed-knob { width: 44px; height: 44px; }
    .stringed-knob .knob-track { color: #c68b4a; background-color: #2b3033; }
    .stringed-knob .knob-head {
        background-color: #202427;
        border-color: #6a7068;
        border-width: 1px;
        color: #f3e2ca;
    }
    .stringed-knob .knob-tick {
        background-color: #f3e2ca;
        width: 2px;
        height: 9px;
        corner-radius: 1px;
    }
    .stringed-label { color: #d9ddd2; font-size: 11px; text-align: center; }
    .stringed-value { color: #919890; font-size: 10px; text-align: center; }
    .stringed-switch-row { horizontal-gap: 8px; }
    .stringed-switch {
        background-color: #22262a;
        border-color: #42484d;
        border-width: 1px;
        border-radius: 5px;
        color: #bec5bc;
        width: 132px;
        height: 30px;
    }
    .stringed-switch-on { border-color: #8eb782; color: #d8ead0; }
    .stringed-slot-row { horizontal-gap: 6px; }
    .stringed-slot {
        background-color: #202428;
        border-color: #3d4448;
        border-width: 1px;
        border-radius: 5px;
        width: 72px;
        height: 58px;
        padding: 5px;
        vertical-gap: 2px;
    }
    .stringed-slot-on { border-color: #c68b4a; }
    .stringed-slot-key { color: #9ba39b; font-size: 10px; text-align: center; }
    .stringed-slot-name { color: #e0e4d8; font-size: 10px; text-align: center; }
    .stringed-slot-source { color: #858f86; font-size: 10px; text-align: center; }
    .stringed-actions { horizontal-gap: 8px; alignment: center; }
    .stringed-current { color: #d9ddd2; width: 330px; }
    .stringed-button {
        background-color: #24282c;
        border-color: #42484d;
        border-width: 1px;
        border-radius: 5px;
        color: #dce2d8;
        width: 76px;
        padding: 6px;
    }
    .stringed-button:hover { border-color: #c68b4a; }
"#;

#[derive(Clone, Copy)]
struct Signals {
    knobs: Signal<Vec<LamathStringedKnob>>,
    driver: Signal<LamathStringedDriverId>,
    body: Signal<LamathStringedBodyId>,
    switches: Signal<Vec<LamathStringedModelSwitch>>,
    slots: Signal<AudioFileSlotListView>,
}

impl Signals {
    fn new(host: &LamathStringedEditorHost) -> Self {
        Self {
            knobs: Signal::new(host.controls.knobs()),
            driver: Signal::new(host.controls.selected_driver()),
            body: Signal::new(host.controls.selected_body()),
            switches: Signal::new(host.controls.model_switches()),
            slots: Signal::new(host.articulations.surface.slot_list_view()),
        }
    }
}

enum UiEvent {
    SetKnob {
        id: u32,
        normalized: f32,
    },
    SetDriver(LamathStringedDriverId),
    SetBody(LamathStringedBodyId),
    SetSwitch {
        id: LamathStringedSwitchId,
        enabled: bool,
    },
    SelectSlot(AudioFileSlotId),
    OpenSlot(AudioFileSlotId),
    ClearSlot(AudioFileSlotId),
    Sync,
}

struct UiModel {
    controls: Arc<dyn LamathStringedControlSurface>,
    slots: AudioFileSlotListHost,
    signals: Signals,
    pending_dialog: Option<(AudioFileSlotId, PendingFileDialog)>,
}

impl Model for UiModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|ui_event, _| match ui_event {
            UiEvent::SetKnob { id, normalized } => {
                self.controls.set_knob_normalized(*id, *normalized);
                self.signals.knobs.set(self.controls.knobs());
            }
            UiEvent::SetDriver(driver) => {
                self.controls.set_driver(*driver);
                self.signals.driver.set(self.controls.selected_driver());
            }
            UiEvent::SetBody(body) => {
                self.controls.set_body(*body);
                self.signals.body.set(self.controls.selected_body());
            }
            UiEvent::SetSwitch { id, enabled } => {
                self.controls.set_model_switch(*id, *enabled);
                self.signals.switches.set(self.controls.model_switches());
            }
            UiEvent::SelectSlot(slot) => {
                self.slots.surface.select_slot(*slot);
                self.signals.slots.set(self.slots.surface.slot_list_view());
            }
            UiEvent::OpenSlot(slot) => {
                self.slots.surface.select_slot(*slot);
                self.signals.slots.set(self.slots.surface.slot_list_view());
                self.pending_dialog = Some((
                    *slot,
                    PendingFileDialog::pick_file(wav_audio_dialog(Path::new("."), None)),
                ));
            }
            UiEvent::ClearSlot(slot) => {
                self.slots.surface.clear_audio_file(*slot);
                self.signals.slots.set(self.slots.surface.slot_list_view());
            }
            UiEvent::Sync => {
                if let Some((slot, dialog)) = self.pending_dialog.as_mut() {
                    match dialog.poll_path() {
                        Poll::Ready(Some(path)) => {
                            self.slots.surface.load_audio_file(*slot, &path);
                            self.signals.slots.set(self.slots.surface.slot_list_view());
                            self.signals.knobs.set(self.controls.knobs());
                            self.signals.driver.set(self.controls.selected_driver());
                            self.signals.body.set(self.controls.selected_body());
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

fn build_editor(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Lamath Stringed").class("stringed-title");
        Label::new(cx, "single string body and articulation slots").class("stringed-subtitle");

        VStack::new(cx, move |cx| {
            Label::new(cx, "Driver").class("stringed-section");
            HStack::new(cx, move |cx| {
                driver_button(cx, signals, LamathStringedDriverId::None, "None");
                driver_button(cx, signals, LamathStringedDriverId::Pick, "Pick");
                driver_button(cx, signals, LamathStringedDriverId::Bow, "Bow");
            })
            .class("stringed-path");
        })
        .class("stringed-strip");

        VStack::new(cx, move |cx| {
            Label::new(cx, "String").class("stringed-section");
            HStack::new(cx, move |cx| {
                for index in 0..7 {
                    knob_cell(cx, signals, index);
                }
            })
            .class("stringed-knobs");
        })
        .class("stringed-strip");

        VStack::new(cx, move |cx| {
            Label::new(cx, "Body").class("stringed-section");
            HStack::new(cx, move |cx| {
                body_button(cx, signals, LamathStringedBodyId::Disabled, "Disabled");
                body_button(cx, signals, LamathStringedBodyId::Guitar, "Guitar");
                body_button(cx, signals, LamathStringedBodyId::Violin, "Violin");
            })
            .class("stringed-path");
        })
        .class("stringed-strip");

        VStack::new(cx, move |cx| {
            Label::new(cx, "Physical switches").class("stringed-section");
            HStack::new(cx, move |cx| {
                for index in 0..3 {
                    switch_cell(cx, signals, index);
                }
            })
            .class("stringed-switch-row");
        })
        .class("stringed-strip");

        VStack::new(cx, move |cx| {
            Label::new(cx, "Articulations").class("stringed-section");
            HStack::new(cx, move |cx| {
                for index in 0..8 {
                    slot_cell(cx, signals, index);
                }
            })
            .class("stringed-slot-row");
            slot_actions(cx, signals);
        })
        .class("stringed-strip");
    })
    .class("stringed-root");
}

fn driver_button(
    cx: &mut Context,
    signals: Signals,
    id: LamathStringedDriverId,
    label: &'static str,
) {
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("stringed-choice")
        .toggle_class(
            "stringed-choice-on",
            Memo::new(move |_| signals.driver.get() == id),
        )
        .on_press(move |cx| cx.emit(UiEvent::SetDriver(id)));
}

fn body_button(cx: &mut Context, signals: Signals, id: LamathStringedBodyId, label: &'static str) {
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("stringed-choice")
        .toggle_class(
            "stringed-choice-on",
            Memo::new(move |_| signals.body.get() == id),
        )
        .on_press(move |cx| cx.emit(UiEvent::SetBody(id)));
}

fn knob_cell(cx: &mut Context, signals: Signals, index: usize) {
    VStack::new(cx, move |cx| {
        Knob::new(
            cx,
            0.5,
            Memo::new(move |_| {
                knob_at(signals, index)
                    .map(|knob| knob.normalized)
                    .unwrap_or(0.0)
            }),
            false,
        )
        .class("stringed-knob")
        .on_change(move |cx, normalized| {
            if let Some(knob) = knob_at(signals, index) {
                cx.emit(UiEvent::SetKnob {
                    id: knob.id,
                    normalized,
                });
            }
        });
        Label::new(
            cx,
            Memo::new(move |_| {
                knob_at(signals, index)
                    .map(|knob| knob.label.to_string())
                    .unwrap_or_default()
            }),
        )
        .class("stringed-label");
        Label::new(
            cx,
            Memo::new(move |_| knob_at(signals, index).map(format_knob).unwrap_or_default()),
        )
        .class("stringed-value");
    })
    .class("stringed-knob-cell")
    .display(Memo::new(move |_| signals.knobs.get().len() > index));
}

fn switch_cell(cx: &mut Context, signals: Signals, index: usize) {
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
    .class("stringed-switch")
    .toggle_class(
        "stringed-switch-on",
        Memo::new(move |_| {
            signals
                .switches
                .get()
                .get(index)
                .map(|switch| switch.enabled)
                .unwrap_or(false)
        }),
    )
    .display(Memo::new(move |_| signals.switches.get().len() > index))
    .on_press(move |cx| {
        if let Some(switch) = signals.switches.get().get(index)
            && switch.editable
        {
            cx.emit(UiEvent::SetSwitch {
                id: switch.id,
                enabled: !switch.enabled,
            });
        }
    });
}

fn slot_cell(cx: &mut Context, signals: Signals, index: usize) {
    Button::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, slot_key(index)).class("stringed-slot-key");
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
            .class("stringed-slot-name");
            Label::new(
                cx,
                Memo::new(move |_| {
                    signals
                        .slots
                        .get()
                        .slots
                        .get(index)
                        .map(|slot| source_name(slot.source).to_string())
                        .unwrap_or_default()
                }),
            )
            .class("stringed-slot-source");
        })
    })
    .class("stringed-slot")
    .toggle_class(
        "stringed-slot-on",
        Memo::new(move |_| signals.slots.get().selected == AudioFileSlotId(index)),
    )
    .display(Memo::new(move |_| signals.slots.get().slots.len() > index))
    .on_press(move |cx| cx.emit(UiEvent::SelectSlot(AudioFileSlotId(index))));
}

fn slot_actions(cx: &mut Context, signals: Signals) {
    HStack::new(cx, move |cx| {
        Label::new(
            cx,
            Memo::new(move |_| {
                let view = signals.slots.get();
                let index = view.selected.0;
                view.slots
                    .get(index)
                    .map(|slot| format!("{}  {}", slot_key(index), slot.label))
                    .unwrap_or_default()
            }),
        )
        .class("stringed-current");
        Button::new(cx, |cx| Label::new(cx, "Load"))
            .class("stringed-button")
            .on_press(move |cx| cx.emit(UiEvent::OpenSlot(signals.slots.get().selected)));
        Button::new(cx, |cx| Label::new(cx, "Clear"))
            .class("stringed-button")
            .on_press(move |cx| cx.emit(UiEvent::ClearSlot(signals.slots.get().selected)));
    })
    .class("stringed-actions");
}

fn knob_at(signals: Signals, index: usize) -> Option<LamathStringedKnob> {
    signals.knobs.get().get(index).copied()
}

fn format_knob(knob: LamathStringedKnob) -> String {
    if knob.units.is_empty() {
        format!("{:.2}", knob.plain)
    } else {
        format!("{:.1} {}", knob.plain, knob.units)
    }
}

fn source_name(source: AudioFileSource) -> &'static str {
    match source {
        AudioFileSource::BuiltIn => "Built-in",
        AudioFileSource::Loaded => "Loaded",
    }
}

fn slot_key(index: usize) -> &'static str {
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
    host: LamathStringedEditorHost,
    size: LamathStringedEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LAMATH_STRINGED_EDITOR_WIDTH) as u32;
    let height = size.height.max(LAMATH_STRINGED_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add lamath stringed editor style");
        let signals = Signals::new(&host);
        UiModel {
            controls: Arc::clone(&host.controls),
            slots: host.articulations.clone(),
            signals,
            pending_dialog: None,
        }
        .build(cx);
        let timer = cx.add_timer(Duration::from_millis(66), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(UiEvent::Sync);
            }
        });
        cx.start_timer(timer);
        build_editor(cx, signals);
    })
    .ignore_default_theme()
    .title("Lamath Stringed")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

pub struct LamathStringedViziaEditor {
    window: WindowHandle,
    #[cfg(target_os = "macos")]
    drop_targets: Option<crate::vizia_audio_file_drop::NativeAudioFileDropTargets>,
}

impl LamathStringedViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle.
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LamathStringedEditorHost,
        size: LamathStringedEditorSize,
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
                top: 392.0,
                width: 624.0,
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

impl Drop for LamathStringedViziaEditor {
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
