//! Shared meter/gauge row for the Windows-only VST editors.
//!
//! Cenedril (level/LUFS meters + analysis-signal readouts) and Lúmedir (delivery gauges) both render
//! the same shape — a labelled horizontal bar with a proportional fill and a formatted value
//! (`[label] [====    ] [value]`). This is the single shared widget for that shape, so neither
//! editor rebuilds a meter; the per-metric value formatting and the value→fill mapping stay with the
//! caller (each metric's display range is plugin-specific).
//!
//! Windows-only: `vizia` is a Windows-target dependency for these VSTs (ADR-0023), so this module
//! compiles under the Windows build (cargo-xwin), not Linux `make ci`. Like the other Vizia view
//! code it is verified by compile-check; visual treatment is verified on the Windows build/host.

use vizia::prelude::*;

/// Fill colour for a meter row, by role. The concrete colours are set in [`METER_STYLE`].
#[derive(Clone, Copy)]
pub enum MeterTone {
    /// A signal level / loudness bar (cyan).
    Level,
    /// A "healthy / in-band" reading (green).
    Good,
    /// A "high / out-of-band" reading (amber).
    Warn,
    /// A neutral informational reading (slate).
    Info,
}

/// Stylesheet for the meter classes. Editors add this once (`cx.add_stylesheet(METER_STYLE)`)
/// alongside their own styles; it only defines the `ll-meter-*` classes this module emits.
pub const METER_STYLE: &str = r#"
    .ll-meter-row {
        height: 22px;
        col-between: 8px;
        alignment: center;
    }
    .ll-meter-label {
        width: 116px;
        color: #9aa7ad;
        font-size: 12px;
    }
    .ll-meter-track {
        width: 1s;
        height: 10px;
        background-color: #1b2226;
        border-radius: 5px;
        border-width: 1px;
        border-color: #2c3438;
    }
    .ll-meter-fill {
        height: 1s;
        border-radius: 5px;
    }
    .ll-meter-fill-level { background-color: #2bb8c6; }
    .ll-meter-fill-good { background-color: #41b46f; }
    .ll-meter-fill-warn { background-color: #d29a3a; }
    .ll-meter-fill-info { background-color: #6f7d96; }
    .ll-meter-value {
        width: 92px;
        color: #d8e0e4;
        font-size: 12px;
        alignment: right;
    }
"#;

/// A labelled horizontal meter row: a name on the left, a proportional fill bar, and a formatted
/// value on the right.
///
/// `fill` is a reactive `0.0..=1.0` fraction (clamped here) that drives the bar width, so the row
/// updates live as the editor's model changes — the same `Signal<f32>` primitive the other controls
/// bind to. `value` is the formatted reading — a reactive `Res<String>` (e.g. a `Signal` mapped to a
/// label) for a live value, or a plain `String` for a static one. Style via [`METER_STYLE`].
pub fn meter_row(
    cx: &mut Context,
    label: &'static str,
    fill: Signal<f32>,
    value: impl Res<String> + Clone + 'static,
    tone: MeterTone,
) {
    HStack::new(cx, move |cx| {
        Label::new(cx, label).class("ll-meter-label");
        HStack::new(cx, move |cx| {
            HStack::new(cx, |_cx| {})
                .class("ll-meter-fill")
                .class(meter_tone_class(tone))
                .width(fill.map(|f| Percentage((*f).clamp(0.0, 1.0) * 100.0)));
        })
        .class("ll-meter-track");
        Label::new(cx, value).class("ll-meter-value");
    })
    .class("ll-meter-row");
}

fn meter_tone_class(tone: MeterTone) -> &'static str {
    match tone {
        MeterTone::Level => "ll-meter-fill-level",
        MeterTone::Good => "ll-meter-fill-good",
        MeterTone::Warn => "ll-meter-fill-warn",
        MeterTone::Info => "ll-meter-fill-info",
    }
}
