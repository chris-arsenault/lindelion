use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

use vizia::{ParentWindow, WindowHandle, WindowScalePolicy, prelude::*};

use super::{
    CALOMA_EDITOR_HEIGHT, CALOMA_EDITOR_WIDTH, CALOMA_LEVEL_DB_MAX, CALOMA_LEVEL_DB_MIN,
    CalomaControlSurface, CalomaEditorHost, CalomaEditorSize,
};

/// The catalog has 20 slots; an order uses a subset. The editor builds this many fixed effect rows
/// and hides the ones the current order does not use (this vizia rev renders dynamic collections as
/// fixed rows reading a `Signal<Vec<_>>` by index, rather than a dynamic `List`).
const CALOMA_MAX_ROWS: usize = 20;

const STYLE: &str = r#"
    .caloma-root {
        background-color: #11161a;
        width: 1s;
        height: 1s;
        padding: 14px;
        vertical-gap: 12px;
    }
    .caloma-title { color: #d8e0e4; font-size: 22px; }
    .caloma-section { color: #7c8a90; font-size: 13px; }
    .caloma-order-row { horizontal-gap: 8px; height: auto; }
    .caloma-order-btn {
        background-color: #1c2329;
        border-radius: 5px;
        border-width: 1px;
        border-color: #303a40;
        padding: 6px;
        color: #c4ced3;
    }
    .caloma-order-btn-active {
        background-color: #2a7f6f;
        border-color: #38c3a6;
        color: #ecfbf6;
    }
    .caloma-level-row { horizontal-gap: 10px; height: auto; alignment: center; }
    .caloma-level-label { color: #aab6bb; width: 70px; }
    .caloma-level-value { color: #7c8a90; width: 80px; }
    .caloma-slot-row {
        horizontal-gap: 10px;
        height: auto;
        alignment: center;
        background-color: #161c1e;
        border-radius: 5px;
        border-width: 1px;
        border-color: #2c3437;
        padding: 6px;
    }
    .caloma-slot-label { color: #c4ced3; width: 150px; }
    .caloma-slot-toggle {
        background-color: #1c2329;
        border-radius: 4px;
        padding: 4px;
        color: #8b979c;
        width: 56px;
    }
    .caloma-slot-toggle-on { background-color: #2a7f6f; color: #ecfbf6; }
    .caloma-knob { width: 44px; height: 44px; }
"#;

/// One editor row mirrored from the control surface.
#[derive(Clone)]
struct SlotRow {
    index: usize,
    label: String,
    enabled: bool,
    intensity: f32,
}

/// The editor's reactive cells. `Copy` (every field is a `Signal` handle) so it can be shared by the
/// model and every view builder.
#[derive(Clone, Copy)]
struct CalomaSignals {
    order_index: Signal<usize>,
    input_db: Signal<f32>,
    output_db: Signal<f32>,
    slots: Signal<Vec<SlotRow>>,
}

impl CalomaSignals {
    fn from_surface(surface: &dyn CalomaControlSurface) -> Self {
        Self {
            order_index: Signal::new(surface.order_index() as usize),
            input_db: Signal::new(surface.input_level_db()),
            output_db: Signal::new(surface.output_level_db()),
            slots: Signal::new(read_slots(surface)),
        }
    }

    /// Re-read every cell from the surface (after an edit re-targets the rows, or on the sync tick).
    fn sync(self, surface: &dyn CalomaControlSurface) {
        self.order_index.set(surface.order_index() as usize);
        self.input_db.set(surface.input_level_db());
        self.output_db.set(surface.output_level_db());
        self.slots.set(read_slots(surface));
    }
}

fn read_slots(surface: &dyn CalomaControlSurface) -> Vec<SlotRow> {
    surface
        .active_slots()
        .into_iter()
        .map(|view| SlotRow {
            index: view.index,
            label: view.label,
            enabled: view.enabled,
            intensity: view.intensity,
        })
        .collect()
}

enum CalomaEvent {
    SelectOrder(usize),
    SetInput(f32),
    SetOutput(f32),
    ToggleRow(usize),
    SetRowIntensity(usize, f32),
    Sync,
}

struct CalomaModel {
    surface: Arc<dyn CalomaControlSurface>,
    signals: CalomaSignals,
}

impl Model for CalomaModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|caloma_event, _| match caloma_event {
            CalomaEvent::SelectOrder(index) => {
                self.surface.select_order(*index as u32);
                self.signals.sync(self.surface.as_ref());
            }
            CalomaEvent::SetInput(db) => {
                self.surface.set_input_level_db(*db);
                self.signals.input_db.set(*db);
            }
            CalomaEvent::SetOutput(db) => {
                self.surface.set_output_level_db(*db);
                self.signals.output_db.set(*db);
            }
            CalomaEvent::ToggleRow(row) => {
                if let Some(slot) = self.signals.slots.get().get(*row) {
                    self.surface.set_slot_enabled(slot.index, !slot.enabled);
                }
                self.signals.sync(self.surface.as_ref());
            }
            CalomaEvent::SetRowIntensity(row, value) => {
                if let Some(slot) = self.signals.slots.get().get(*row) {
                    self.surface.set_slot_intensity(slot.index, *value);
                }
                self.signals.sync(self.surface.as_ref());
            }
            CalomaEvent::Sync => self.signals.sync(self.surface.as_ref()),
        });
    }
}

/// Map a 0..1 knob position to the dB level range, and back.
fn knob_to_db(position: f32) -> f32 {
    CALOMA_LEVEL_DB_MIN + position.clamp(0.0, 1.0) * (CALOMA_LEVEL_DB_MAX - CALOMA_LEVEL_DB_MIN)
}

fn db_to_knob(db: f32) -> f32 {
    ((db - CALOMA_LEVEL_DB_MIN) / (CALOMA_LEVEL_DB_MAX - CALOMA_LEVEL_DB_MIN)).clamp(0.0, 1.0)
}

fn build_editor(cx: &mut Context, signals: CalomaSignals, order_labels: Vec<String>) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Calóma").class("caloma-title");

        Label::new(cx, "Signal order").class("caloma-section");
        HStack::new(cx, move |cx| {
            for (index, label) in order_labels.iter().cloned().enumerate() {
                Button::new(cx, move |cx| Label::new(cx, label.clone()))
                    .class("caloma-order-btn")
                    .toggle_class(
                        "caloma-order-btn-active",
                        Memo::new(move |_| signals.order_index.get() == index),
                    )
                    .on_press(move |cx| cx.emit(CalomaEvent::SelectOrder(index)));
            }
        })
        .class("caloma-order-row");

        Label::new(cx, "Levels").class("caloma-section");
        level_row(cx, "Input", signals.input_db, CalomaEvent::SetInput);
        level_row(cx, "Output", signals.output_db, CalomaEvent::SetOutput);

        Label::new(cx, "Effects").class("caloma-section");
        ScrollView::new(cx, move |cx| {
            for row in 0..CALOMA_MAX_ROWS {
                slot_row(cx, signals, row);
            }
        });
    })
    .class("caloma-root");
}

fn level_row(
    cx: &mut Context,
    label: &'static str,
    db_signal: Signal<f32>,
    make_event: impl Fn(f32) -> CalomaEvent + 'static + Copy + Send,
) {
    HStack::new(cx, move |cx| {
        Label::new(cx, label).class("caloma-level-label");
        Knob::new(
            cx,
            0.5,
            Memo::new(move |_| db_to_knob(db_signal.get())),
            true,
        )
        .class("caloma-knob")
        .on_change(move |cx, position| cx.emit(make_event(knob_to_db(position))));
        Label::new(
            cx,
            Memo::new(move |_| format!("{:+.1} dB", db_signal.get())),
        )
        .class("caloma-level-value");
    })
    .class("caloma-level-row");
}

fn slot_row(cx: &mut Context, signals: CalomaSignals, row: usize) {
    HStack::new(cx, move |cx| {
        Label::new(
            cx,
            Memo::new(move |_| {
                signals
                    .slots
                    .get()
                    .get(row)
                    .map(|slot| slot.label.clone())
                    .unwrap_or_default()
            }),
        )
        .class("caloma-slot-label");

        Button::new(cx, move |cx| {
            Label::new(
                cx,
                Memo::new(move |_| {
                    let on = signals
                        .slots
                        .get()
                        .get(row)
                        .map(|slot| slot.enabled)
                        .unwrap_or(false);
                    if on { "On" } else { "Off" }.to_string()
                }),
            )
        })
        .class("caloma-slot-toggle")
        .toggle_class(
            "caloma-slot-toggle-on",
            Memo::new(move |_| {
                signals
                    .slots
                    .get()
                    .get(row)
                    .map(|slot| slot.enabled)
                    .unwrap_or(false)
            }),
        )
        .on_press(move |cx| cx.emit(CalomaEvent::ToggleRow(row)));

        Knob::new(
            cx,
            1.0,
            Memo::new(move |_| {
                signals
                    .slots
                    .get()
                    .get(row)
                    .map(|slot| slot.intensity)
                    .unwrap_or(1.0)
            }),
            false,
        )
        .class("caloma-knob")
        .on_change(move |cx, value| cx.emit(CalomaEvent::SetRowIntensity(row, value)));
    })
    .class("caloma-slot-row")
    .display(Memo::new(move |_| signals.slots.get().len() > row));
}

fn build_caloma_application(
    host: CalomaEditorHost,
    size: CalomaEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(CALOMA_EDITOR_WIDTH) as u32;
    let height = size.height.max(CALOMA_EDITOR_HEIGHT) as u32;
    let surface = host.surface;
    let order_labels: Vec<String> = surface
        .order_labels()
        .iter()
        .map(|label| (*label).to_string())
        .collect();
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add caloma editor style");
        let signals = CalomaSignals::from_surface(surface.as_ref());
        CalomaModel {
            surface: Arc::clone(&surface),
            signals,
        }
        .build(cx);
        // Pick up external changes (host `setState`, order-default loads) on a light timer.
        let sync_timer = cx.add_timer(Duration::from_millis(66), None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(CalomaEvent::Sync);
            }
        });
        cx.start_timer(sync_timer);
        build_editor(cx, signals, order_labels.clone());
    })
    .ignore_default_theme()
    .title("Calóma")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

pub struct CalomaViziaEditor {
    window: WindowHandle,
}

impl CalomaViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle (an `HWND` on Windows).
    pub unsafe fn attach(
        parent: *mut c_void,
        host: CalomaEditorHost,
        size: CalomaEditorSize,
    ) -> Self {
        let parent = ParentWindow(parent);
        let window = build_caloma_application(host, size).open_parented(&parent);
        Self { window }
    }
}

impl Drop for CalomaViziaEditor {
    fn drop(&mut self) {
        if self.window.is_open() {
            self.window.close();
        }
    }
}
