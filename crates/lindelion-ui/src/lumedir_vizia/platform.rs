use std::ffi::c_void;
use std::sync::Arc;
use std::time::Duration;

use vizia::{WindowScalePolicy, prelude::*};

use super::{
    BandStatus, CoachBands, CoachConfigSurface, DeliverySample, DeliverySource,
    LUMEDIR_EDITOR_HEIGHT, LUMEDIR_EDITOR_WIDTH, LumedirEditorHost, LumedirEditorSize,
    SampleStatuses, SessionAccumulator, SessionSummary,
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
        child-space: 14px;
        row-between: 8px;
    }
    .lumedir-title { color: #d8e0e4; font-size: 22px; }
    .lumedir-section { color: #7c8a90; font-size: 13px; top: 4px; }
    .lumedir-row { col-between: 8px; height: auto; alignment: center; }
    .lumedir-chip {
        width: 1s;
        height: 22px;
        alignment: center;
        border-radius: 5px;
        font-size: 12px;
        color: #0c1013;
        background-color: #6f7d96;
    }
    .lumedir-chip-good { background-color: #41b46f; }
    .lumedir-chip-bad { background-color: #d29a3a; }
    .lumedir-control-btn {
        background-color: #1c2329;
        border-radius: 5px;
        border-width: 1px;
        border-color: #303a40;
        child-space: 6px;
        color: #c4ced3;
    }
    .lumedir-control-btn-active { background-color: #2a7f6f; color: #ecfbf6; }
    .lumedir-status-text { color: #9aa7ad; font-size: 12px; }
    .lumedir-summary-line { color: #c4ced3; font-size: 12px; }
    .lumedir-cfg-row { col-between: 6px; height: auto; alignment: center; }
    .lumedir-cfg-label { width: 130px; color: #9aa7ad; font-size: 12px; }
    .lumedir-cfg-value { width: 64px; color: #d8e0e4; font-size: 12px; alignment: center; }
    .lumedir-step-btn {
        width: 26px;
        height: 22px;
        alignment: center;
        background-color: #1c2329;
        border-radius: 4px;
        border-width: 1px;
        border-color: #303a40;
        color: #c4ced3;
    }
"#;

/// One editable target-band edge (used by the config steppers and the [`LumedirEvent::EditBand`]
/// event).
#[derive(Debug, Clone, Copy)]
enum BandField {
    RateMin,
    RateMax,
    WpmMin,
    WpmMax,
    DynamismMin,
    PauseMin,
    PauseMax,
    ClarityMin,
}

#[derive(Debug, Clone, Copy)]
enum LumedirEvent {
    Sync,
    StartSession,
    StopSession,
    /// Adjust the syllables-per-word factor by the given delta.
    StepFactor(f32),
    /// Adjust a band edge by the given delta.
    EditBand(BandField, f32),
}

/// The editor's reactive cells. `Copy` (every field is a `Signal` handle).
#[derive(Clone, Copy)]
struct LumedirSignals {
    // Live gauges (raw value + 0..1 bar fill), as in M4.
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
    // Live per-metric band scoring (drives the status strip).
    status: Signal<SampleStatuses>,
    // Session.
    active: Signal<bool>,
    session: Signal<SessionSummary>,
    // Config (editable).
    factor: Signal<f32>,
    bands: Signal<CoachBands>,
}

impl LumedirSignals {
    fn new(source: &dyn DeliverySource, config: &dyn CoachConfigSurface) -> Self {
        let all_in = SampleStatuses {
            rate: BandStatus::InBand,
            wpm: BandStatus::InBand,
            dynamism: BandStatus::InBand,
            pause_fraction: BandStatus::InBand,
            clarity: BandStatus::InBand,
        };
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
            status: Signal::new(all_in),
            active: Signal::new(false),
            session: Signal::new(SessionSummary::default()),
            factor: Signal::new(config.syllables_per_word()),
            bands: Signal::new(config.bands()),
        };
        signals.sync_live(source, config);
        signals
    }

    /// Read the live snapshot from the source, update the gauges + scoring, and return the sample +
    /// statuses (so the caller can fold them into the session).
    fn sync_live(
        self,
        source: &dyn DeliverySource,
        config: &dyn CoachConfigSurface,
    ) -> (DeliverySample, SampleStatuses) {
        let sample = DeliverySample {
            rate: source.syllables_per_second(),
            wpm: source.words_per_minute(),
            dynamism: source.pitch_dynamism_semitones(),
            pause_fraction: source.pause_fraction(),
            clarity: source.clarity(),
        };
        let pause_count = source.pause_count();

        self.rate.set(sample.rate);
        self.rate_bar.set(super::rate_fill(sample.rate));
        self.wpm.set(sample.wpm);
        self.wpm_bar.set(super::wpm_fill(sample.wpm));
        self.dynamism.set(sample.dynamism);
        self.dynamism_bar.set(super::dynamism_fill(sample.dynamism));
        self.pause_frac.set(sample.pause_fraction);
        self.pause_frac_bar
            .set(super::pause_fraction_fill(sample.pause_fraction));
        self.pause_count.set(pause_count as f32);
        self.pause_count_bar
            .set(super::pause_count_fill(pause_count));
        self.clarity.set(sample.clarity);
        self.clarity_bar.set(super::clarity_fill(sample.clarity));

        let statuses = config.status(sample);
        self.status.set(statuses);
        (sample, statuses)
    }
}

struct LumedirModel {
    source: Arc<dyn DeliverySource>,
    config: Arc<dyn CoachConfigSurface>,
    session: SessionAccumulator,
    signals: LumedirSignals,
}

impl Model for LumedirModel {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|lumedir_event, _| match lumedir_event {
            LumedirEvent::Sync => {
                let (sample, statuses) = self
                    .signals
                    .sync_live(self.source.as_ref(), self.config.as_ref());
                if self.session.is_active() {
                    self.session.record(sample, statuses);
                    self.signals.session.set(self.session.summary());
                }
            }
            LumedirEvent::StartSession => {
                self.session.start();
                self.signals.active.set(true);
                self.signals.session.set(self.session.summary());
            }
            LumedirEvent::StopSession => {
                self.session.stop();
                self.signals.active.set(false);
            }
            LumedirEvent::StepFactor(delta) => {
                let value = (self.config.syllables_per_word() + delta).clamp(0.5, 4.0);
                self.config.set_syllables_per_word(value);
                self.signals.factor.set(value);
            }
            LumedirEvent::EditBand(field, delta) => {
                let mut bands = self.config.bands();
                match field {
                    BandField::RateMin => bands.rate_min = (bands.rate_min + delta).max(0.0),
                    BandField::RateMax => bands.rate_max = (bands.rate_max + delta).max(0.0),
                    BandField::WpmMin => bands.wpm_min = (bands.wpm_min + delta).max(0.0),
                    BandField::WpmMax => bands.wpm_max = (bands.wpm_max + delta).max(0.0),
                    BandField::DynamismMin => {
                        bands.dynamism_min = (bands.dynamism_min + delta).max(0.0)
                    }
                    BandField::PauseMin => {
                        bands.pause_min = (bands.pause_min + delta).clamp(0.0, 1.0)
                    }
                    BandField::PauseMax => {
                        bands.pause_max = (bands.pause_max + delta).clamp(0.0, 1.0)
                    }
                    BandField::ClarityMin => {
                        bands.clarity_min = (bands.clarity_min + delta).clamp(0.0, 1.0)
                    }
                }
                self.config.set_bands(bands);
                self.signals.bands.set(bands);
            }
        });
    }
}

fn build_gauges(cx: &mut Context, s: LumedirSignals) {
    meter_row(
        cx,
        "Speaking rate",
        s.rate_bar,
        s.rate.map(|v| super::fmt_rate(*v)),
        MeterTone::Level,
    );
    meter_row(
        cx,
        "Words / min",
        s.wpm_bar,
        s.wpm.map(|v| super::fmt_wpm(*v)),
        MeterTone::Level,
    );
    meter_row(
        cx,
        "Pitch dynamism",
        s.dynamism_bar,
        s.dynamism.map(|v| super::fmt_dynamism(*v)),
        MeterTone::Good,
    );
    meter_row(
        cx,
        "Pause fraction",
        s.pause_frac_bar,
        s.pause_frac.map(|v| super::fmt_pause_fraction(*v)),
        MeterTone::Info,
    );
    meter_row(
        cx,
        "Pauses",
        s.pause_count_bar,
        s.pause_count.map(|v| super::fmt_pause_count(*v as u32)),
        MeterTone::Info,
    );
    meter_row(
        cx,
        "Clarity",
        s.clarity_bar,
        s.clarity.map(|v| super::fmt_clarity(*v)),
        MeterTone::Good,
    );
}

/// A live in/out-of-band chip per metric, green when in band and amber otherwise.
fn status_chip(
    cx: &mut Context,
    label: &'static str,
    status: Signal<SampleStatuses>,
    pick: fn(&SampleStatuses) -> BandStatus,
) {
    Label::new(cx, label)
        .class("lumedir-chip")
        .toggle_class(
            "lumedir-chip-good",
            Memo::new(move |_| pick(&status.get()) == BandStatus::InBand),
        )
        .toggle_class(
            "lumedir-chip-bad",
            Memo::new(move |_| pick(&status.get()) != BandStatus::InBand),
        );
}

fn build_status_strip(cx: &mut Context, status: Signal<SampleStatuses>) {
    HStack::new(cx, move |cx| {
        status_chip(cx, "Rate", status, |s| s.rate);
        status_chip(cx, "WPM", status, |s| s.wpm);
        status_chip(cx, "Pitch", status, |s| s.dynamism);
        status_chip(cx, "Pause", status, |s| s.pause_fraction);
        status_chip(cx, "Clarity", status, |s| s.clarity);
    })
    .class("lumedir-row");
}

fn build_session_controls(cx: &mut Context, s: LumedirSignals) {
    HStack::new(cx, move |cx| {
        Button::new(cx, |cx| Label::new(cx, "Start"))
            .class("lumedir-control-btn")
            .toggle_class("lumedir-control-btn-active", s.active)
            .on_press(|cx| cx.emit(LumedirEvent::StartSession));
        Button::new(cx, |cx| Label::new(cx, "Stop"))
            .class("lumedir-control-btn")
            .on_press(|cx| cx.emit(LumedirEvent::StopSession));
        Label::new(
            cx,
            s.active
                .map(|a| if *a { "● recording" } else { "○ idle" }.to_string()),
        )
        .class("lumedir-status-text");
        Label::new(
            cx,
            s.session
                .map(|summary| format!("{} samples", summary.sample_count)),
        )
        .class("lumedir-status-text");
    })
    .class("lumedir-row");
}

fn build_summary(cx: &mut Context, session: Signal<SessionSummary>) {
    Label::new(
        cx,
        session.map(|s| {
            format!(
                "Rate {:.1} syl/s · {:.0}% in band",
                s.rate.mean,
                s.rate.in_band_fraction * 100.0
            )
        }),
    )
    .class("lumedir-summary-line");
    Label::new(
        cx,
        session.map(|s| {
            format!(
                "WPM {:.0} · {:.0}% in band",
                s.wpm.mean,
                s.wpm.in_band_fraction * 100.0
            )
        }),
    )
    .class("lumedir-summary-line");
    Label::new(
        cx,
        session.map(|s| {
            format!(
                "Pitch dynamism {:.1} st · {:.0}% in band",
                s.dynamism.mean,
                s.dynamism.in_band_fraction * 100.0
            )
        }),
    )
    .class("lumedir-summary-line");
    Label::new(
        cx,
        session.map(|s| {
            format!(
                "Pause fraction {:.2} · {:.0}% in band",
                s.pause_fraction.mean,
                s.pause_fraction.in_band_fraction * 100.0
            )
        }),
    )
    .class("lumedir-summary-line");
    Label::new(
        cx,
        session.map(|s| {
            format!(
                "Clarity {:.0}% · {:.0}% in band",
                s.clarity.mean * 100.0,
                s.clarity.in_band_fraction * 100.0
            )
        }),
    )
    .class("lumedir-summary-line");
}

fn stepper(
    cx: &mut Context,
    label: &'static str,
    value: impl Res<String> + Clone + 'static,
    dec: LumedirEvent,
    inc: LumedirEvent,
) {
    HStack::new(cx, move |cx| {
        Label::new(cx, label).class("lumedir-cfg-label");
        Button::new(cx, |cx| Label::new(cx, "−"))
            .class("lumedir-step-btn")
            .on_press(move |cx| cx.emit(dec));
        Label::new(cx, value).class("lumedir-cfg-value");
        Button::new(cx, |cx| Label::new(cx, "+"))
            .class("lumedir-step-btn")
            .on_press(move |cx| cx.emit(inc));
    })
    .class("lumedir-cfg-row");
}

fn build_config(cx: &mut Context, s: LumedirSignals) {
    let bands = s.bands;
    stepper(
        cx,
        "Syllables/word",
        s.factor.map(|v| format!("{v:.2}")),
        LumedirEvent::StepFactor(-0.1),
        LumedirEvent::StepFactor(0.1),
    );
    stepper(
        cx,
        "Rate min (syl/s)",
        bands.map(|b| format!("{:.1}", b.rate_min)),
        LumedirEvent::EditBand(BandField::RateMin, -0.1),
        LumedirEvent::EditBand(BandField::RateMin, 0.1),
    );
    stepper(
        cx,
        "Rate max (syl/s)",
        bands.map(|b| format!("{:.1}", b.rate_max)),
        LumedirEvent::EditBand(BandField::RateMax, -0.1),
        LumedirEvent::EditBand(BandField::RateMax, 0.1),
    );
    stepper(
        cx,
        "WPM min",
        bands.map(|b| format!("{:.0}", b.wpm_min)),
        LumedirEvent::EditBand(BandField::WpmMin, -5.0),
        LumedirEvent::EditBand(BandField::WpmMin, 5.0),
    );
    stepper(
        cx,
        "WPM max",
        bands.map(|b| format!("{:.0}", b.wpm_max)),
        LumedirEvent::EditBand(BandField::WpmMax, -5.0),
        LumedirEvent::EditBand(BandField::WpmMax, 5.0),
    );
    stepper(
        cx,
        "Dynamism min (st)",
        bands.map(|b| format!("{:.1}", b.dynamism_min)),
        LumedirEvent::EditBand(BandField::DynamismMin, -0.5),
        LumedirEvent::EditBand(BandField::DynamismMin, 0.5),
    );
    stepper(
        cx,
        "Pause min",
        bands.map(|b| format!("{:.2}", b.pause_min)),
        LumedirEvent::EditBand(BandField::PauseMin, -0.05),
        LumedirEvent::EditBand(BandField::PauseMin, 0.05),
    );
    stepper(
        cx,
        "Pause max",
        bands.map(|b| format!("{:.2}", b.pause_max)),
        LumedirEvent::EditBand(BandField::PauseMax, -0.05),
        LumedirEvent::EditBand(BandField::PauseMax, 0.05),
    );
    stepper(
        cx,
        "Clarity min",
        bands.map(|b| format!("{:.2}", b.clarity_min)),
        LumedirEvent::EditBand(BandField::ClarityMin, -0.05),
        LumedirEvent::EditBand(BandField::ClarityMin, 0.05),
    );
}

fn build_lumedir_application(
    host: LumedirEditorHost,
    size: LumedirEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LUMEDIR_EDITOR_WIDTH) as u32;
    let height = size.height.max(LUMEDIR_EDITOR_HEIGHT) as u32;
    let source = host.source;
    let config = host.config;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(METER_STYLE)
            .expect("failed to add shared meter style");
        cx.add_stylesheet(STYLE)
            .expect("failed to add lumedir editor style");
        let signals = LumedirSignals::new(source.as_ref(), config.as_ref());
        LumedirModel {
            source: Arc::clone(&source),
            config: Arc::clone(&config),
            session: SessionAccumulator::new(),
            signals,
        }
        .build(cx);
        // Poll the delivery source off the audio thread on a light timer.
        let sync_timer = cx.add_timer(REFRESH, None, |cx, action| {
            if matches!(action, TimerAction::Tick(_)) {
                cx.emit(LumedirEvent::Sync);
            }
        });
        cx.start_timer(sync_timer);

        VStack::new(cx, move |cx| {
            Label::new(cx, "Lúmedir — Delivery").class("lumedir-title");
            build_gauges(cx, signals);
            build_status_strip(cx, signals.status);
            Label::new(cx, "Session").class("lumedir-section");
            build_session_controls(cx, signals);
            build_summary(cx, signals.session);
            Label::new(cx, "Target bands").class("lumedir-section");
            build_config(cx, signals);
        })
        .class("lumedir-root");
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
