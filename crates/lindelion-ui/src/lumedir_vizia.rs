//! Lúmedir editor surface.
//!
//! The size/host types and the [`DeliverySource`] trait here are platform-neutral and compile
//! everywhere (including Linux `make ci`). The Vizia view and the Windows `IPlugView`→`HWND` attach
//! live in the `windows`-gated [`platform`] submodule, because `vizia` is a Windows-target
//! dependency for the new VSTs (ADR-0023) — mirroring Cenedril's editor stack.
//!
//! Delivery metrics reach the editor through the [`DeliverySource`] trait, which the plugin
//! implements over its lock-free delivery-worker read handle — so this UI crate never names the
//! plugin's snapshot type and stays free of audio-thread infrastructure.

use std::sync::Arc;

pub const LUMEDIR_EDITOR_WIDTH: i32 = 720;
pub const LUMEDIR_EDITOR_HEIGHT: i32 = 480;

#[derive(Debug, Clone, Copy)]
pub struct LumedirEditorSize {
    pub width: i32,
    pub height: i32,
}

/// The editor's source of live delivery metrics, implemented by the plugin over its lock-free
/// delivery-worker read handle. The view polls it on the UI thread; the worker/handoff types stay
/// in the plugin crate. Accessors return primitives only (no plugin types), so `lindelion-ui` does
/// not depend on the plugin crate.
pub trait DeliverySource: Send + Sync {
    /// Speaking rate in syllable nuclei per second.
    fn syllables_per_second(&self) -> f32;
    /// Words per minute, derived from the rate via the syllables-per-word factor.
    fn words_per_minute(&self) -> f32;
    /// Pitch dynamism: standard deviation of voiced f0 in semitones (flat↔animated).
    fn pitch_dynamism_semitones(&self) -> f32;
    /// Fraction of the analysis window spent in silence (`0.0..=1.0`).
    fn pause_fraction(&self) -> f32;
    /// Number of distinct pauses (silence runs) in the window.
    fn pause_count(&self) -> u32;
    /// Clarity score (`0.0..=1.0`) from voicing ratio + onset sharpness.
    fn clarity(&self) -> f32;
}

/// The eight editable target-band edges, as a UI-side DTO. The plugin's persisted `TargetBands`
/// lives in the plugin crate; this crosses the editor boundary (the surface converts between them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoachBands {
    pub rate_min: f32,
    pub rate_max: f32,
    pub wpm_min: f32,
    pub wpm_max: f32,
    pub dynamism_min: f32,
    pub pause_min: f32,
    pub pause_max: f32,
    pub clarity_min: f32,
}

/// The editor's read/write access to the coaching config plus plugin-side scoring. Implemented by
/// the plugin over its lock-free `SharedConfig` + `TargetBands` (the plugin owns the band schema and
/// the scoring; the editor edits and reads through this trait).
pub trait CoachConfigSurface: Send + Sync {
    fn syllables_per_word(&self) -> f32;
    fn set_syllables_per_word(&self, value: f32);
    fn bands(&self) -> CoachBands;
    fn set_bands(&self, bands: CoachBands);
    /// Score a live sample against the current bands (the plugin owns the scoring logic).
    fn status(&self, sample: DeliverySample) -> SampleStatuses;
}

/// Everything the editor needs from the plugin. Cheap to clone (`Arc`s inside).
#[derive(Clone)]
pub struct LumedirEditorHost {
    pub source: Arc<dyn DeliverySource>,
    pub config: Arc<dyn CoachConfigSurface>,
}

impl LumedirEditorHost {
    pub fn new(source: Arc<dyn DeliverySource>, config: Arc<dyn CoachConfigSurface>) -> Self {
        Self { source, config }
    }
}

// ---------------------------------------------------------------------------------------------
// Metric → meter-fill mappings and value formatters.
//
// These are platform-neutral (so they compile and are unit-tested on Linux `make ci`, the testable
// substance of the readout) and are the single place that maps a delivery metric to a `0.0..=1.0`
// bar fill and to a formatted display string. The Windows view (`platform`) binds the model's
// metric signals through these; the display spans are anchored on the `FIXTURES.md` delivery
// targets (slow 2.8 / fast 3.8 syl/s; flat 1.1 / animated 7.4 semitone pitch-std).
// ---------------------------------------------------------------------------------------------

/// Speaking-rate display span (syllable nuclei per second): conversational speech sits roughly
/// 1–5 syl/s, bracketing the slow (2.8) and fast (3.8) fixtures.
const RATE_MIN_SYL_PER_S: f32 = 1.0;
const RATE_MAX_SYL_PER_S: f32 = 5.0;
/// Words-per-minute display span (derived from the rate); ~60–220 wpm covers slow→fast delivery.
const WPM_MIN: f32 = 60.0;
const WPM_MAX: f32 = 220.0;
/// Pitch-dynamism display span: 0 (monotone) up to 8 semitones, bracketing flat (1.1) and animated
/// (7.4) fixtures.
const DYNAMISM_MAX_SEMITONES: f32 = 8.0;
/// Pause count that fills the bar (more than this clamps to full).
const PAUSE_COUNT_FULL: f32 = 12.0;

/// Linear fill fraction of `value` within `[min, max]`, clamped to `0.0..=1.0`.
fn span_fill(value: f32, min: f32, max: f32) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

/// Speaking-rate → bar fill.
pub fn rate_fill(syllables_per_second: f32) -> f32 {
    span_fill(syllables_per_second, RATE_MIN_SYL_PER_S, RATE_MAX_SYL_PER_S)
}

/// Words-per-minute → bar fill.
pub fn wpm_fill(words_per_minute: f32) -> f32 {
    span_fill(words_per_minute, WPM_MIN, WPM_MAX)
}

/// Pitch dynamism (semitones) → bar fill.
pub fn dynamism_fill(semitones: f32) -> f32 {
    span_fill(semitones, 0.0, DYNAMISM_MAX_SEMITONES)
}

/// Pause fraction (already `0.0..=1.0`) → bar fill.
pub fn pause_fraction_fill(fraction: f32) -> f32 {
    fraction.clamp(0.0, 1.0)
}

/// Pause count → bar fill (over [`PAUSE_COUNT_FULL`]).
pub fn pause_count_fill(count: u32) -> f32 {
    span_fill(count as f32, 0.0, PAUSE_COUNT_FULL)
}

/// Clarity (already `0.0..=1.0`) → bar fill.
pub fn clarity_fill(clarity: f32) -> f32 {
    clarity.clamp(0.0, 1.0)
}

/// `"3.2 syl/s"`.
pub fn fmt_rate(syllables_per_second: f32) -> String {
    format!("{syllables_per_second:.1} syl/s")
}

/// `"128 wpm"`.
pub fn fmt_wpm(words_per_minute: f32) -> String {
    format!("{words_per_minute:.0} wpm")
}

/// `"4.5 st"`.
pub fn fmt_dynamism(semitones: f32) -> String {
    format!("{semitones:.1} st")
}

/// `"20%"`.
pub fn fmt_pause_fraction(fraction: f32) -> String {
    format!("{:.0}%", fraction.clamp(0.0, 1.0) * 100.0)
}

/// `"7"`.
pub fn fmt_pause_count(count: u32) -> String {
    format!("{count}")
}

/// `"80%"`.
pub fn fmt_clarity(clarity: f32) -> String {
    format!("{:.0}%", clarity.clamp(0.0, 1.0) * 100.0)
}

// ---------------------------------------------------------------------------------------------
// Target-band scoring boundary + session accumulation.
// ---------------------------------------------------------------------------------------------

/// Where a metric reading falls relative to its target band. The shared boundary enum: the plugin's
/// `TargetBands` scoring returns it (computed plugin-side, since `lindelion-ui` cannot depend on the
/// plugin crate) and the editor colors its live + session gauges by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandStatus {
    /// Inside the band (good).
    InBand,
    /// Below the band's lower edge / floor.
    Below,
    /// Above the band's upper edge (two-sided bands only).
    Above,
}

/// One sampled delivery datapoint (the five live metrics) the session accumulator folds in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeliverySample {
    pub rate: f32,
    pub wpm: f32,
    pub dynamism: f32,
    pub pause_fraction: f32,
    pub clarity: f32,
}

/// The per-metric band status for one sample, as scored by the plugin. Paired with a
/// [`DeliverySample`] when recording into the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleStatuses {
    pub rate: BandStatus,
    pub wpm: BandStatus,
    pub dynamism: BandStatus,
    pub pause_fraction: BandStatus,
    pub clarity: BandStatus,
}

/// Running mean + in-band fraction for one metric over a session.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MetricSummary {
    pub mean: f32,
    pub in_band_fraction: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct MetricAccumulator {
    sum: f64,
    in_band: u64,
    count: u64,
}

impl MetricAccumulator {
    fn record(&mut self, value: f32, status: BandStatus) {
        self.sum += value as f64;
        if status == BandStatus::InBand {
            self.in_band += 1;
        }
        self.count += 1;
    }

    fn summary(&self) -> MetricSummary {
        if self.count == 0 {
            return MetricSummary::default();
        }
        MetricSummary {
            mean: (self.sum / self.count as f64) as f32,
            in_band_fraction: self.in_band as f32 / self.count as f32,
        }
    }
}

/// End-of-session summary: per-metric mean + fraction of samples that scored in-band, plus the
/// sample count.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SessionSummary {
    pub rate: MetricSummary,
    pub wpm: MetricSummary,
    pub dynamism: MetricSummary,
    pub pause_fraction: MetricSummary,
    pub clarity: MetricSummary,
    pub sample_count: u64,
}

/// Accumulates the live delivery snapshot stream into a session summary, under manual start/stop
/// (the M5 session-boundary decision). The editor calls [`start`](Self::start) to begin a fresh
/// session, [`record`](Self::record) each refresh tick while active, and [`stop`](Self::stop) to
/// freeze it; the running [`summary`](Self::summary) is the end-of-session readout. Band-agnostic:
/// the caller scores each sample (the plugin owns the bands) and passes the statuses in.
#[derive(Debug, Clone, Default)]
pub struct SessionAccumulator {
    active: bool,
    rate: MetricAccumulator,
    wpm: MetricAccumulator,
    dynamism: MetricAccumulator,
    pause_fraction: MetricAccumulator,
    clarity: MetricAccumulator,
    sample_count: u64,
}

impl SessionAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a fresh session: clear all running totals and mark active.
    pub fn start(&mut self) {
        *self = Self {
            active: true,
            ..Self::default()
        };
    }

    /// Freeze the session (stop folding in new samples); the summary is preserved.
    pub fn stop(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Fold one sampled datapoint into the session (ignored when not active).
    pub fn record(&mut self, sample: DeliverySample, statuses: SampleStatuses) {
        if !self.active {
            return;
        }
        self.rate.record(sample.rate, statuses.rate);
        self.wpm.record(sample.wpm, statuses.wpm);
        self.dynamism.record(sample.dynamism, statuses.dynamism);
        self.pause_fraction
            .record(sample.pause_fraction, statuses.pause_fraction);
        self.clarity.record(sample.clarity, statuses.clarity);
        self.sample_count += 1;
    }

    /// The current session summary (per-metric mean + in-band fraction).
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            rate: self.rate.summary(),
            wpm: self.wpm.summary(),
            dynamism: self.dynamism.summary(),
            pause_fraction: self.pause_fraction.summary(),
            clarity: self.clarity.summary(),
            sample_count: self.sample_count,
        }
    }
}

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::LumedirViziaEditor;

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed stub source, so the platform-neutral host wiring is testable on `make ci` (Linux)
    /// without the plugin crate or the Windows view.
    struct StubSource;

    impl DeliverySource for StubSource {
        fn syllables_per_second(&self) -> f32 {
            3.2
        }
        fn words_per_minute(&self) -> f32 {
            128.0
        }
        fn pitch_dynamism_semitones(&self) -> f32 {
            4.5
        }
        fn pause_fraction(&self) -> f32 {
            0.2
        }
        fn pause_count(&self) -> u32 {
            7
        }
        fn clarity(&self) -> f32 {
            0.8
        }
    }

    struct StubConfig;

    impl CoachConfigSurface for StubConfig {
        fn syllables_per_word(&self) -> f32 {
            1.5
        }
        fn set_syllables_per_word(&self, _value: f32) {}
        fn bands(&self) -> CoachBands {
            CoachBands {
                rate_min: 3.0,
                rate_max: 4.0,
                wpm_min: 120.0,
                wpm_max: 160.0,
                dynamism_min: 3.0,
                pause_min: 0.1,
                pause_max: 0.3,
                clarity_min: 0.6,
            }
        }
        fn set_bands(&self, _bands: CoachBands) {}
        fn status(&self, _sample: DeliverySample) -> SampleStatuses {
            statuses_all_in()
        }
    }

    #[test]
    fn editor_host_exposes_every_delivery_metric_from_its_source() {
        let host = LumedirEditorHost::new(Arc::new(StubSource), Arc::new(StubConfig));
        assert_eq!(host.source.syllables_per_second(), 3.2);
        assert_eq!(host.source.words_per_minute(), 128.0);
        assert_eq!(host.source.pitch_dynamism_semitones(), 4.5);
        assert_eq!(host.source.pause_fraction(), 0.2);
        assert_eq!(host.source.pause_count(), 7);
        assert_eq!(host.source.clarity(), 0.8);
        assert_eq!(host.config.syllables_per_word(), 1.5);
        assert_eq!(host.config.bands().wpm_max, 160.0);
    }

    #[test]
    fn editor_size_holds_its_dimensions() {
        let size = LumedirEditorSize {
            width: LUMEDIR_EDITOR_WIDTH,
            height: LUMEDIR_EDITOR_HEIGHT,
        };
        assert_eq!(size.width, LUMEDIR_EDITOR_WIDTH);
        assert_eq!(size.height, LUMEDIR_EDITOR_HEIGHT);
    }

    #[test]
    fn fills_are_clamped_to_the_unit_interval() {
        for fill in [
            rate_fill(-10.0),
            rate_fill(1_000.0),
            wpm_fill(-1.0),
            wpm_fill(10_000.0),
            dynamism_fill(-3.0),
            dynamism_fill(99.0),
            pause_fraction_fill(-0.5),
            pause_fraction_fill(2.0),
            pause_count_fill(9_999),
            clarity_fill(-1.0),
            clarity_fill(5.0),
        ] {
            assert!((0.0..=1.0).contains(&fill), "fill {fill} out of range");
        }
    }

    #[test]
    fn fills_increase_with_their_metric() {
        assert!(
            rate_fill(3.8) > rate_fill(2.8),
            "fast rate fills more than slow"
        );
        assert!(wpm_fill(180.0) > wpm_fill(110.0));
        assert!(
            dynamism_fill(7.4) > dynamism_fill(1.1),
            "animated fills more than flat"
        );
        assert!(pause_fraction_fill(0.26) > pause_fraction_fill(0.12));
        assert!(pause_count_fill(8) > pause_count_fill(2));
        assert!(clarity_fill(0.9) > clarity_fill(0.3));
    }

    #[test]
    fn fixture_targets_land_in_sensible_bands() {
        // Flat (1.1 st) reads low, animated (7.4 st) reads high on the dynamism gauge.
        assert!(dynamism_fill(1.1) < 0.3, "flat dynamism should read low");
        assert!(
            dynamism_fill(7.4) > 0.7,
            "animated dynamism should read high"
        );
        // Slow (2.8) and fast (3.8) syl/s both land mid-gauge (neither pinned to an extreme).
        for rate in [2.8, 3.8] {
            let fill = rate_fill(rate);
            assert!(
                (0.2..=0.8).contains(&fill),
                "rate {rate} fill {fill} not mid-gauge"
            );
        }
    }

    fn statuses(
        rate: BandStatus,
        wpm: BandStatus,
        dynamism: BandStatus,
        pause: BandStatus,
        clarity: BandStatus,
    ) -> SampleStatuses {
        SampleStatuses {
            rate,
            wpm,
            dynamism,
            pause_fraction: pause,
            clarity,
        }
    }

    #[test]
    fn session_accumulates_means_and_in_band_fractions() {
        use BandStatus::{Below, InBand};
        let mut session = SessionAccumulator::new();
        session.start();
        // Two samples: rate 2.0 (below) then 4.0 (in) → mean 3.0, 1/2 in band.
        session.record(
            DeliverySample {
                rate: 2.0,
                wpm: 100.0,
                dynamism: 1.0,
                pause_fraction: 0.4,
                clarity: 0.5,
            },
            statuses(Below, Below, Below, Below, Below),
        );
        session.record(
            DeliverySample {
                rate: 4.0,
                wpm: 140.0,
                dynamism: 5.0,
                pause_fraction: 0.2,
                clarity: 0.9,
            },
            statuses(InBand, InBand, InBand, InBand, InBand),
        );
        let summary = session.summary();
        assert_eq!(summary.sample_count, 2);
        assert!((summary.rate.mean - 3.0).abs() < 1.0e-6);
        assert!((summary.rate.in_band_fraction - 0.5).abs() < 1.0e-6);
        assert!((summary.wpm.mean - 120.0).abs() < 1.0e-6);
        assert!((summary.clarity.in_band_fraction - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn session_ignores_records_when_stopped_and_start_clears() {
        let mut session = SessionAccumulator::new();
        // Not started: records are ignored.
        session.record(DeliverySample::default(), statuses_all_in());
        assert_eq!(session.summary().sample_count, 0);

        session.start();
        session.record(DeliverySample::default(), statuses_all_in());
        session.stop();
        // Stopped: further records ignored, prior count preserved.
        session.record(DeliverySample::default(), statuses_all_in());
        assert_eq!(session.summary().sample_count, 1);
        assert!(!session.is_active());

        // A new session clears the prior totals.
        session.start();
        assert_eq!(session.summary().sample_count, 0);
    }

    fn statuses_all_in() -> SampleStatuses {
        statuses(
            BandStatus::InBand,
            BandStatus::InBand,
            BandStatus::InBand,
            BandStatus::InBand,
            BandStatus::InBand,
        )
    }

    #[test]
    fn formatters_render_the_expected_strings() {
        assert_eq!(fmt_rate(3.2), "3.2 syl/s");
        assert_eq!(fmt_wpm(128.4), "128 wpm");
        assert_eq!(fmt_dynamism(4.5), "4.5 st");
        assert_eq!(fmt_pause_fraction(0.2), "20%");
        assert_eq!(fmt_pause_count(7), "7");
        assert_eq!(fmt_clarity(0.8), "80%");
    }
}
