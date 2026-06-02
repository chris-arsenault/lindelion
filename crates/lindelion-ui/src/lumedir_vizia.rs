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

/// Everything the editor needs from the plugin. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct LumedirEditorHost {
    pub source: Arc<dyn DeliverySource>,
}

impl LumedirEditorHost {
    pub fn new(source: Arc<dyn DeliverySource>) -> Self {
        Self { source }
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

    #[test]
    fn editor_host_exposes_every_delivery_metric_from_its_source() {
        let host = LumedirEditorHost::new(Arc::new(StubSource));
        assert_eq!(host.source.syllables_per_second(), 3.2);
        assert_eq!(host.source.words_per_minute(), 128.0);
        assert_eq!(host.source.pitch_dynamism_semitones(), 4.5);
        assert_eq!(host.source.pause_fraction(), 0.2);
        assert_eq!(host.source.pause_count(), 7);
        assert_eq!(host.source.clarity(), 0.8);
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
