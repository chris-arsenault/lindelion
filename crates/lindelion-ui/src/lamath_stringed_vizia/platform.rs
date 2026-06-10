use std::{ffi::c_void, path::Path, sync::Arc, task::Poll, time::Duration};

use vizia::{ParentWindow, WindowHandle, WindowScalePolicy, prelude::*};

use super::{
    LAMATH_STRINGED_EDITOR_HEIGHT, LAMATH_STRINGED_EDITOR_WIDTH, LamathStringedBodyId,
    LamathStringedControlSurface, LamathStringedDriverId, LamathStringedEditorHost,
    LamathStringedEditorSize, LamathStringedKnob, LamathStringedKnobGroup,
    LamathStringedModelSwitch, LamathStringedSwitchId,
};
use crate::{
    audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListHost, AudioFileSlotListView, AudioFileSource,
    },
    vizia_file_dialogs::{PendingFileDialog, wav_audio_dialog},
};

mod style;
use style::STYLE;

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

/// The layout tells the instrument's physical story: a player drives the
/// string through a pick or bow, the string speaks through a body. Each card
/// is one stage of that chain; the driver card shows only the controls the
/// selected driver actually has.
fn build_editor(cx: &mut Context, signals: Signals) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, "Lamath Stringed").class("stringed-title");
                Label::new(
                    cx,
                    "a single string, bowed or picked, voiced through a body",
                )
                .class("stringed-subtitle");
            })
            .class("stringed-header-text");
            knob_cell_by_id(cx, signals, knob_id_for(signals, KnobRole::Output));
        })
        .class("stringed-header");

        HStack::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, "DRIVER").class("stringed-section");
                HStack::new(cx, move |cx| {
                    driver_button(cx, signals, LamathStringedDriverId::None, "None");
                    driver_button(cx, signals, LamathStringedDriverId::Pick, "Pick");
                    driver_button(cx, signals, LamathStringedDriverId::Bow, "Bow");
                })
                .class("stringed-path");
                HStack::new(cx, move |cx| {
                    for slot in 0..4 {
                        group_knob_cell(cx, signals, LamathStringedKnobGroup::Bow, slot);
                    }
                    switch_cell_by_id(cx, signals, LamathStringedSwitchId::BowDrive);
                })
                .class("stringed-knobs")
                .display(Memo::new(move |_| {
                    signals.driver.get() == LamathStringedDriverId::Bow
                }));
                Label::new(
                    cx,
                    "Pick hardness and contact follow Brightness and Stiffness.",
                )
                .class("stringed-hint")
                .display(Memo::new(move |_| {
                    signals.driver.get() == LamathStringedDriverId::Pick
                }));
                Label::new(cx, "Articulation samples excite the string directly.")
                    .class("stringed-hint")
                    .display(Memo::new(move |_| {
                        signals.driver.get() == LamathStringedDriverId::None
                    }));
            })
            .class("stringed-strip")
            .class("stringed-driver-card");

            VStack::new(cx, move |cx| {
                Label::new(cx, "PLAYER").class("stringed-section");
                HStack::new(cx, move |cx| {
                    for slot in 0..3 {
                        group_knob_cell(cx, signals, LamathStringedKnobGroup::Player, slot);
                    }
                })
                .class("stringed-knobs");
                Label::new(
                    cx,
                    "Half way is a steady player; beyond it gets theatrical.",
                )
                .class("stringed-hint");
            })
            .class("stringed-strip")
            .class("stringed-player-card");
        })
        .class("stringed-card-row");

        VStack::new(cx, move |cx| {
            Label::new(cx, "STRING").class("stringed-section");
            HStack::new(cx, move |cx| {
                for slot in 0..6 {
                    group_knob_cell(cx, signals, LamathStringedKnobGroup::String, slot);
                }
                switch_cell_by_id(cx, signals, LamathStringedSwitchId::Tension);
            })
            .class("stringed-knobs");
        })
        .class("stringed-strip");

        VStack::new(cx, move |cx| {
            Label::new(cx, "BODY").class("stringed-section");
            HStack::new(cx, move |cx| {
                body_button(cx, signals, LamathStringedBodyId::Disabled, "Disabled");
                body_button(cx, signals, LamathStringedBodyId::Guitar, "Guitar");
                body_button(cx, signals, LamathStringedBodyId::Violin, "Violin");
                switch_cell_by_id(cx, signals, LamathStringedSwitchId::BodyContact);
            })
            .class("stringed-path");
        })
        .class("stringed-strip");

        VStack::new(cx, move |cx| {
            Label::new(cx, "ARTICULATIONS").class("stringed-section");
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum KnobRole {
    Output,
}

fn knob_id_for(signals: Signals, role: KnobRole) -> u32 {
    let group = match role {
        KnobRole::Output => LamathStringedKnobGroup::Output,
    };
    signals
        .knobs
        .get()
        .iter()
        .find(|knob| knob.group == group)
        .map(|knob| knob.id)
        .unwrap_or(u32::MAX)
}

/// The `slot`-th knob of a group, in host-parameter order.
fn group_knob_cell(
    cx: &mut Context,
    signals: Signals,
    group: LamathStringedKnobGroup,
    slot: usize,
) {
    let id = Memo::new(move |_| {
        signals
            .knobs
            .get()
            .iter()
            .filter(|knob| knob.group == group)
            .nth(slot)
            .map(|knob| knob.id)
            .unwrap_or(u32::MAX)
    });
    knob_cell_by_memo_id(cx, signals, id);
}

fn knob_cell_by_id(cx: &mut Context, signals: Signals, id: u32) {
    let id = Memo::new(move |_| id);
    knob_cell_by_memo_id(cx, signals, id);
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

fn knob_cell_by_memo_id(cx: &mut Context, signals: Signals, id: Memo<u32>) {
    let knob = move || {
        let wanted = id.get();
        signals
            .knobs
            .get()
            .iter()
            .copied()
            .find(|knob| knob.id == wanted)
    };
    VStack::new(cx, move |cx| {
        Knob::new(
            cx,
            0.5,
            Memo::new(move |_| knob().map(|knob| knob.normalized).unwrap_or(0.0)),
            false,
        )
        .class("stringed-knob")
        .on_change(move |cx, normalized| {
            if let Some(knob) = knob() {
                cx.emit(UiEvent::SetKnob {
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
        .class("stringed-label");
        Label::new(
            cx,
            Memo::new(move |_| knob().map(format_knob).unwrap_or_default()),
        )
        .class("stringed-value");
    })
    .class("stringed-knob-cell")
    .display(Memo::new(move |_| knob().is_some()));
}

fn switch_cell_by_id(cx: &mut Context, signals: Signals, id: LamathStringedSwitchId) {
    let switch = move || {
        signals
            .switches
            .get()
            .iter()
            .copied()
            .find(|switch| switch.id == id)
    };
    Button::new(cx, move |cx| {
        Label::new(
            cx,
            Memo::new(move |_| {
                switch()
                    .map(|switch| switch.label.to_string())
                    .unwrap_or_default()
            }),
        )
    })
    .class("stringed-switch")
    .toggle_class(
        "stringed-switch-on",
        Memo::new(move |_| switch().map(|switch| switch.enabled).unwrap_or(false)),
    )
    .display(Memo::new(move |_| switch().is_some()))
    .on_press(move |cx| {
        if let Some(switch) = switch()
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
