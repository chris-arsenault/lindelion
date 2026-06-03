//! The level/LUFS + analysis-signal panel (Windows Vizia): labelled [`meter_row`]s bound to
//! [`MeterPanelState`], refreshed by Cenedril's single editor timer (off the audio thread).

use std::sync::Arc;

use vizia::prelude::*;

use crate::cenedril_vizia::MeterSource;
use crate::cenedril_vizia::meters::{
    MeterReading, crest_reading, flux_reading, hnr_reading, level_reading, lufs_reading,
    pitch_reading, unit_reading, voicing_label,
};
use crate::vizia_meter::{MeterTone, meter_row};

/// One meter row's reactive state: a `0..=1` fill and a formatted value string the bar binds to.
#[derive(Clone, Copy)]
struct Row {
    fill: Signal<f32>,
    text: Signal<String>,
}

impl Row {
    fn new() -> Self {
        Self {
            fill: Signal::new(0.0),
            text: Signal::new(String::new()),
        }
    }

    fn set(&self, reading: MeterReading) {
        self.fill.set(reading.fill);
        self.text.set(reading.text);
    }
}

/// The reactive state behind every meter/analysis row. Created in the editor build (so the `Signal`s
/// live in the reactive context); the rows bind to them and [`MeterPanelState`] sets them each tick.
#[derive(Clone, Copy)]
struct MeterSignals {
    peak: Row,
    rms: Row,
    crest: Row,
    lufs_m: Row,
    lufs_s: Row,
    lufs_i: Row,
    voicing: Row,
    speech: Row,
    flux: Row,
    hnr: Row,
    pitch: Row,
}

impl MeterSignals {
    fn new() -> Self {
        Self {
            peak: Row::new(),
            rms: Row::new(),
            crest: Row::new(),
            lufs_m: Row::new(),
            lufs_s: Row::new(),
            lufs_i: Row::new(),
            voicing: Row::new(),
            speech: Row::new(),
            flux: Row::new(),
            hnr: Row::new(),
            pitch: Row::new(),
        }
    }

    /// Pull the latest readouts and run the per-metric value→fill/text mappings into the `Signal`s.
    fn update(&self, source: &dyn MeterSource) {
        let m = source.meters();
        self.peak.set(level_reading(m.peak));
        self.rms.set(level_reading(m.rms));
        self.crest.set(crest_reading(m.crest));
        self.lufs_m.set(lufs_reading(m.lufs_momentary));
        self.lufs_s.set(lufs_reading(m.lufs_short));
        self.lufs_i.set(lufs_reading(m.lufs_integrated));

        let s = source.signals();
        self.voicing.fill.set(unit_reading(s.voicing_score).fill);
        self.voicing
            .text
            .set(voicing_label(s.voicing_state).to_string());
        self.speech.set(unit_reading(s.speech_presence));
        self.flux.set(flux_reading(s.spectral_flux));
        self.hnr.set(hnr_reading(s.hnr_db));
        self.pitch.set(pitch_reading(s.pitch_hz));
    }
}

/// Meter panel state updated by the editor's shared refresh timer.
#[derive(Clone)]
pub(super) struct MeterPanelState {
    signals: MeterSignals,
    source: Arc<dyn MeterSource>,
}

impl MeterPanelState {
    pub(super) fn new(source: Arc<dyn MeterSource>) -> Self {
        let state = Self {
            signals: MeterSignals::new(),
            source,
        };
        state.update();
        state
    }

    pub(super) fn update(&self) {
        self.signals.update(self.source.as_ref());
    }
}

/// Build the level/LUFS + analysis-signal panel as ordinary layout, not a custom [`View`].
///
/// Cenedril's custom spectrogram needs a bespoke draw implementation, but the meter panel is just
/// shared rows. Keeping it as plain layout matches Lúmedir and avoids nested custom view
/// construction during `IPlugView::attached`.
pub(super) fn meter_panel(cx: &mut Context, state: MeterPanelState) -> Handle<'_, VStack> {
    crate::vizia_window::debug_log("cenedril-vizia: meter panel begin");
    let signals = state.signals;
    VStack::new(cx, move |cx| {
        crate::vizia_window::debug_log("cenedril-vizia: meter panel layout begin");
        Label::new(cx, "Levels").class("cenedril-section");
        crate::vizia_window::debug_log("cenedril-vizia: meter section levels done");
        meter_row(
            cx,
            "Peak",
            signals.peak.fill,
            signals.peak.text,
            MeterTone::Level,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row peak done");
        meter_row(
            cx,
            "RMS",
            signals.rms.fill,
            signals.rms.text,
            MeterTone::Level,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row rms done");
        meter_row(
            cx,
            "Crest",
            signals.crest.fill,
            signals.crest.text,
            MeterTone::Info,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row crest done");
        meter_row(
            cx,
            "LUFS-M",
            signals.lufs_m.fill,
            signals.lufs_m.text,
            MeterTone::Level,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row lufs-m done");
        meter_row(
            cx,
            "LUFS-S",
            signals.lufs_s.fill,
            signals.lufs_s.text,
            MeterTone::Level,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row lufs-s done");
        meter_row(
            cx,
            "LUFS-I",
            signals.lufs_i.fill,
            signals.lufs_i.text,
            MeterTone::Level,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row lufs-i done");

        Label::new(cx, "Analysis").class("cenedril-section");
        crate::vizia_window::debug_log("cenedril-vizia: meter section analysis done");
        meter_row(
            cx,
            "Voicing",
            signals.voicing.fill,
            signals.voicing.text,
            MeterTone::Good,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row voicing done");
        meter_row(
            cx,
            "Speech",
            signals.speech.fill,
            signals.speech.text,
            MeterTone::Good,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row speech done");
        meter_row(
            cx,
            "Flux",
            signals.flux.fill,
            signals.flux.text,
            MeterTone::Warn,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row flux done");
        meter_row(
            cx,
            "HNR",
            signals.hnr.fill,
            signals.hnr.text,
            MeterTone::Info,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row hnr done");
        meter_row(
            cx,
            "Pitch",
            signals.pitch.fill,
            signals.pitch.text,
            MeterTone::Info,
        );
        crate::vizia_window::debug_log("cenedril-vizia: meter row pitch done");

        crate::vizia_window::debug_log("cenedril-vizia: meter panel layout done");
    })
}
