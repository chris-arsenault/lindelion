use std::{ffi::c_void, path::Path, sync::Arc, task::Poll, time::Duration};

use vizia::{ParentWindow, WindowHandle, WindowScalePolicy, prelude::*};

use super::{
    LAMATH_CYMBAL_EDITOR_HEIGHT, LAMATH_CYMBAL_EDITOR_WIDTH, LamathCymbalControlSurface,
    LamathCymbalEditorHost, LamathCymbalEditorSize, LamathCymbalKnob,
};
use crate::{
    audio_file_slot::{
        AudioFileSlotId, AudioFileSlotListHost, AudioFileSlotListView, AudioFileSource,
    },
    vizia_file_dialogs::{PendingFileDialog, wav_audio_dialog},
};

const STYLE: &str = r#"
    .cymbal-root {
        background-color: #101418;
        width: 1s;
        height: 1s;
        padding: 16px;
        vertical-gap: 14px;
    }
    .cymbal-title { color: #dce2e0; font-size: 22px; }
    .cymbal-subtitle { color: #8e9a98; font-size: 12px; }
    .cymbal-panel {
        background-color: #171d20;
        border-width: 1px;
        border-color: #30383a;
        border-radius: 6px;
        padding: 10px;
        vertical-gap: 8px;
    }
    .cymbal-section { color: #9aa8a4; font-size: 13px; }
    .cymbal-knob-grid { horizontal-gap: 12px; vertical-gap: 10px; }
    .cymbal-knob-cell { width: 70px; vertical-gap: 4px; alignment: center; }
    .cymbal-knob { width: 44px; height: 44px; }
    .cymbal-knob .knob-track { color: #c99c45; background-color: #2b3032; }
    .cymbal-knob .knob-head {
        background-color: #1e2426;
        border-width: 1px;
        border-color: #65706d;
        color: #f1e8d4;
    }
    .cymbal-knob:hover .knob-head { border-color: #e2d6be; }
    .cymbal-knob .knob-tick {
        background-color: #f1e8d4;
        width: 2px;
        height: 9px;
        corner-radius: 1px;
    }
    .cymbal-knob-label { color: #cbd3d0; font-size: 11px; text-align: center; }
    .cymbal-knob-value { color: #8e9a98; font-size: 10px; text-align: center; }
    .cymbal-slot-row { horizontal-gap: 8px; }
    .cymbal-slot {
        background-color: #202729;
        border-width: 1px;
        border-color: #3a4446;
        border-radius: 5px;
        width: 92px;
        height: 58px;
        padding: 5px;
        vertical-gap: 2px;
    }
    .cymbal-slot-selected { border-color: #c99c45; }
    .cymbal-slot-key { color: #92a09c; font-size: 10px; text-align: center; }
    .cymbal-slot-label { color: #dde5e0; font-size: 10px; text-align: center; }
    .cymbal-slot-source { color: #7f908a; font-size: 10px; text-align: center; }
    .cymbal-action-row { horizontal-gap: 8px; alignment: center; }
    .cymbal-selected-label { color: #d3ded8; width: 300px; }
    .cymbal-button {
        background-color: #232a2d;
        border-radius: 5px;
        border-width: 1px;
        border-color: #3a4446;
        color: #dce2e0;
        padding: 6px;
        width: 72px;
    }
    .cymbal-button:hover { border-color: #c99c45; }
"#;

#[derive(Clone, Copy)]
struct CymbalSignals {
    knobs: Signal<Vec<LamathCymbalKnob>>,
    slots: Signal<AudioFileSlotListView>,
}

impl CymbalSignals {
    fn from_host(host: &LamathCymbalEditorHost) -> Self {
        Self {
            knobs: Signal::new(host.controls.knobs()),
            slots: Signal::new(host.strikers.surface.slot_list_view()),
        }
    }

    fn sync(self, host: &LamathCymbalEditorHost) {
        self.knobs.set(host.controls.knobs());
        self.slots.set(host.strikers.surface.slot_list_view());
    }
}

enum CymbalEvent {
    SetKnob { id: u32, normalized: f32 },
    SelectSlot(AudioFileSlotId),
    OpenSlotDialog(AudioFileSlotId),
    ClearSlot(AudioFileSlotId),
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
            }
            CymbalEvent::SelectSlot(slot) => {
                self.strikers.surface.select_slot(*slot);
                self.signals
                    .slots
                    .set(self.strikers.surface.slot_list_view());
            }
            CymbalEvent::OpenSlotDialog(slot) => {
                self.strikers.surface.select_slot(*slot);
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
            CymbalEvent::Sync => {
                if let Some((slot, dialog)) = self.pending_dialog.as_mut() {
                    match dialog.poll_path() {
                        Poll::Ready(Some(path)) => {
                            self.strikers.surface.load_audio_file(*slot, &path);
                            self.pending_dialog = None;
                        }
                        Poll::Ready(None) => self.pending_dialog = None,
                        Poll::Pending => {}
                    }
                }
                let host = LamathCymbalEditorHost {
                    controls: Arc::clone(&self.controls),
                    strikers: self.strikers.clone(),
                };
                self.signals.sync(&host);
            }
        });
    }
}

fn build_editor(cx: &mut Context, signals: CymbalSignals) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Lamath Cymbal").class("cymbal-title");
        Label::new(cx, "Shared body idiophone").class("cymbal-subtitle");

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
    })
    .class("cymbal-root");
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
        });
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
        Button::new(cx, |cx| Label::new(cx, "Load"))
            .class("cymbal-button")
            .on_press(move |cx| cx.emit(CymbalEvent::OpenSlotDialog(signals.slots.get().selected)));
        Button::new(cx, |cx| Label::new(cx, "Clear"))
            .class("cymbal-button")
            .on_press(move |cx| cx.emit(CymbalEvent::ClearSlot(signals.slots.get().selected)));
    })
    .class("cymbal-action-row");
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

pub struct LamathCymbalViziaEditor {
    window: WindowHandle,
}

impl LamathCymbalViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle.
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LamathCymbalEditorHost,
        size: LamathCymbalEditorSize,
    ) -> Self {
        let parent = ParentWindow(parent);
        let window = build_application(host, size).open_parented(&parent);
        Self { window }
    }
}

impl Drop for LamathCymbalViziaEditor {
    fn drop(&mut self) {
        if self.window.is_open() {
            self.window.close();
        }
    }
}
