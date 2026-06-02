use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

use vizia::{WindowScalePolicy, prelude::*};

use super::{
    DeliverySource, LUMEDIR_EDITOR_HEIGHT, LUMEDIR_EDITOR_WIDTH, LumedirEditorHost,
    LumedirEditorSize,
};
use crate::vizia_meter::{METER_STYLE, MeterTone, meter_row};
use crate::vizia_window::ViziaWindowEditor;

/// Editor refresh cadence (≈15 fps), matching the other Lindelion editors.
const REFRESH: Duration = Duration::from_millis(66);

const STYLE: &str = r#"
    .lumedir-root {
        background-color: #11161a;
        width: 1s;
        height: 1s;
        child-space: 16px;
        row-between: 8px;
    }
    .lumedir-title {
        color: #d8e0e4;
        font-size: 22px;
    }
    .lumedir-subtitle {
        color: #7c8a90;
        font-size: 13px;
        bottom: 6px;
    }
"#;

/// The editor's reactive cells. `Copy` (every field is a `Signal` handle) so the model and every
/// view builder can share it. For each delivery metric there are two cells: the raw value (formatted
/// into the row's right-hand label) and the `0.0..=1.0` bar fill (computed from the raw value via the
/// platform-neutral [`super`] mappings).
#[derive(Clone, Copy)]
struct LumedirSignals {
    rate: Signal<f32>,
    rate_bar: Signal<f32>,
    wpm: Signal<f32>,
    wpm_bar: Signal<f32>,
    dynamism: Signal<f32>,
    dynamism_bar: Signal<f32>,
    pause_frac: Signal<f32>,
    pause_frac_bar: Signal<f32>,
    pause_count: Signal<f32>,
    pause_count_bar: Signal<f32>,
    clarity: Signal<f32>,
    clarity_bar: Signal<f32>,
}

impl LumedirSignals {
    fn from_source(source: &dyn DeliverySource) -> Self {
        let signals = Self {
            rate: Signal::new(0.0),
            rate_bar: Signal::new(0.0),
            wpm: Signal::new(0.0),
            wpm_bar: Signal::new(0.0),
            dynamism: Signal::new(0.0),
            dynamism_bar: Signal::new(0.0),
            pause_frac: Signal::new(0.0),
            pause_frac_bar: Signal::new(0.0),
            pause_count: Signal::new(0.0),
            pause_count_bar: Signal::new(0.0),
            clarity: Signal::new(0.0),
            clarity_bar: Signal::new(0.0),
        };
        signals.sync(source);
        signals
    }

    /// Re-read every metric from the source and update both its value and its bar fill.
    fn sync(self, source: &dyn DeliverySource) {
        let rate = source.syllables_per_second();
        self.rate.set(rate);
        self.rate_bar.set(super::rate_fill(rate));

        let wpm = source.words_per_minute();
        self.wpm.set(wpm);
        self.wpm_bar.set(super::wpm_fill(wpm));

        let dynamism = source.pitch_dynamism_semitones();
        self.dynamism.set(dynamism);
        self.dynamism_bar.set(super::dynamism_fill(dynamism));

        let pause_frac = source.pause_fraction();
        self.pause_frac.set(pause_frac);
        self.pause_frac_bar
            .set(super::pause_fraction_fill(pause_frac));

        let pause_count = source.pause_count();
        self.pause_count.set(pause_count as f32);
        self.pause_count_bar
            .set(super::pause_count_fill(pause_count));

        let clarity = source.clarity();
        self.clarity.set(clarity);
        self.clarity_bar.set(super::clarity_fill(clarity));
    }
}

enum LumedirEvent {
    Sync,
}

struct LumedirModel {
    source: Arc<dyn DeliverySource>,
    signals: LumedirSignals,
}

impl Model for LumedirModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|lumedir_event, _| match lumedir_event {
            LumedirEvent::Sync => self.signals.sync(self.source.as_ref()),
        });
    }
}

fn build_readout(cx: &mut Context, signals: LumedirSignals) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Lúmedir — Delivery").class("lumedir-title");
        Label::new(cx, "Live delivery readout").class("lumedir-subtitle");

        meter_row(
            cx,
            "Speaking rate",
            signals.rate_bar,
            signals.rate.map(|v| super::fmt_rate(*v)),
            MeterTone::Level,
        );
        meter_row(
            cx,
            "Words / min",
            signals.wpm_bar,
            signals.wpm.map(|v| super::fmt_wpm(*v)),
            MeterTone::Level,
        );
        meter_row(
            cx,
            "Pitch dynamism",
            signals.dynamism_bar,
            signals.dynamism.map(|v| super::fmt_dynamism(*v)),
            MeterTone::Good,
        );
        meter_row(
            cx,
            "Pause fraction",
            signals.pause_frac_bar,
            signals.pause_frac.map(|v| super::fmt_pause_fraction(*v)),
            MeterTone::Info,
        );
        meter_row(
            cx,
            "Pauses",
            signals.pause_count_bar,
            signals
                .pause_count
                .map(|v| super::fmt_pause_count(*v as u32)),
            MeterTone::Info,
        );
        meter_row(
            cx,
            "Clarity",
            signals.clarity_bar,
            signals.clarity.map(|v| super::fmt_clarity(*v)),
            MeterTone::Good,
        );
    })
    .class("lumedir-root");
}

fn build_lumedir_application(
    host: LumedirEditorHost,
    size: LumedirEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LUMEDIR_EDITOR_WIDTH) as u32;
    let height = size.height.max(LUMEDIR_EDITOR_HEIGHT) as u32;
    let source = host.source;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(METER_STYLE)
            .expect("failed to add shared meter style");
        cx.add_stylesheet(STYLE)
            .expect("failed to add lumedir editor style");
        let signals = LumedirSignals::from_source(source.as_ref());
        LumedirModel {
            source: Arc::clone(&source),
            signals,
        }
        .build(cx);
        // Poll the delivery source off the audio thread on a light timer (the worker publishes
        // snapshots through lock-free atomics; this reads the latest).
        let sync_timer = cx.add_timer(REFRESH, None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(LumedirEvent::Sync);
            }
        });
        cx.start_timer(sync_timer);
        build_readout(cx, signals);
    })
    .ignore_default_theme()
    .title("Lumedir")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

/// The Lúmedir editor: a thin newtype over the shared [`ViziaWindowEditor`], which owns the
/// `IPlugView`→`HWND` attach and the close-on-drop teardown. The inner editor is an RAII guard —
/// held only so its `Drop` closes the host window — hence never read directly.
pub struct LumedirViziaEditor(#[allow(dead_code)] ViziaWindowEditor);

impl LumedirViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle (an `HWND` on Windows).
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LumedirEditorHost,
        size: LumedirEditorSize,
    ) -> Self {
        let application = build_lumedir_application(host, size);
        Self(unsafe { ViziaWindowEditor::attach(parent, application) })
    }
}
