//! Lúmedir coaching configuration: the syllables-per-word factor plus the per-metric target bands a
//! delivery is scored against. Persisted in plugin state (see [`crate::config_io`]) and edited live
//! through [`crate::SharedConfig`].
//!
//! The default band edges are grounded in established public-speaking guidance (signed off in M5):
//! - **Speaking rate 120–160 WPM** — Toastmasters / broad consensus for effective delivery
//!   (~150 comfortable; >200 hurts comprehension). At the default 1.5 syllables-per-word factor that
//!   is **3.0–4.0 syllable nuclei/s**, so the rate and WPM gauges agree.
//! - **Pitch dynamism ≥ 3.0 semitones** — normal conversational F0 variability is ~2–4 semitones;
//!   monotone reads below, engaging delivery at the upper end. Floor only (more expressiveness is not
//!   penalized).
//! - **Pause fraction 0.10–0.30** — speech stays articulation-dominant but strategic pauses matter;
//!   below ≈0.10 reads rushed, above ≈0.30 halting.
//! - **Clarity ≥ 0.6** — calibrated to Lúmedir's internal voicing-ratio + onset-sharpness composite
//!   (not an external literature value).

use lindelion_ui::lumedir_vizia::BandStatus;
use serde::{Deserialize, Serialize};

use crate::speaking_rate::DEFAULT_SYLLABLES_PER_WORD;

/// Per-metric target bands. A two-sided band has a `min` and `max`; a floor-only metric (dynamism,
/// clarity) carries just a `min`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TargetBands {
    /// Speaking-rate band, syllable nuclei per second.
    pub rate_syl_per_s_min: f32,
    pub rate_syl_per_s_max: f32,
    /// Words-per-minute band.
    pub wpm_min: f32,
    pub wpm_max: f32,
    /// Pitch-dynamism floor, semitone standard deviation (no upper limit).
    pub dynamism_semitones_min: f32,
    /// Pause-fraction band (`0.0..=1.0`).
    pub pause_fraction_min: f32,
    pub pause_fraction_max: f32,
    /// Clarity floor (`0.0..=1.0`, no upper limit).
    pub clarity_min: f32,
}

impl Default for TargetBands {
    fn default() -> Self {
        Self {
            rate_syl_per_s_min: 3.0,
            rate_syl_per_s_max: 4.0,
            wpm_min: 120.0,
            wpm_max: 160.0,
            dynamism_semitones_min: 3.0,
            pause_fraction_min: 0.10,
            pause_fraction_max: 0.30,
            clarity_min: 0.6,
        }
    }
}

fn two_sided_status(value: f32, min: f32, max: f32) -> BandStatus {
    if value < min {
        BandStatus::Below
    } else if value > max {
        BandStatus::Above
    } else {
        BandStatus::InBand
    }
}

fn floor_status(value: f32, min: f32) -> BandStatus {
    if value < min {
        BandStatus::Below
    } else {
        BandStatus::InBand
    }
}

impl TargetBands {
    /// Score the syllable-nuclei speaking rate against its two-sided band.
    pub fn rate_status(&self, syllables_per_second: f32) -> BandStatus {
        two_sided_status(
            syllables_per_second,
            self.rate_syl_per_s_min,
            self.rate_syl_per_s_max,
        )
    }

    /// Score words-per-minute against its two-sided band.
    pub fn wpm_status(&self, words_per_minute: f32) -> BandStatus {
        two_sided_status(words_per_minute, self.wpm_min, self.wpm_max)
    }

    /// Score pitch dynamism against its floor (no upper limit).
    pub fn dynamism_status(&self, semitones: f32) -> BandStatus {
        floor_status(semitones, self.dynamism_semitones_min)
    }

    /// Score pause fraction against its two-sided band.
    pub fn pause_status(&self, fraction: f32) -> BandStatus {
        two_sided_status(fraction, self.pause_fraction_min, self.pause_fraction_max)
    }

    /// Score clarity against its floor (no upper limit).
    pub fn clarity_status(&self, clarity: f32) -> BandStatus {
        floor_status(clarity, self.clarity_min)
    }
}

/// The complete coaching configuration: the WPM-derivation factor plus the target bands.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LumedirConfig {
    /// Syllables-per-word factor used to derive WPM from the syllable-nuclei rate.
    pub syllables_per_word: f32,
    pub bands: TargetBands,
}

impl Default for LumedirConfig {
    fn default() -> Self {
        Self {
            syllables_per_word: DEFAULT_SYLLABLES_PER_WORD,
            bands: TargetBands::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sided_metrics_flag_below_in_and_above() {
        let bands = TargetBands::default(); // rate 3.0–4.0, wpm 120–160, pause 0.10–0.30
        assert_eq!(bands.rate_status(2.5), BandStatus::Below);
        assert_eq!(bands.rate_status(3.5), BandStatus::InBand);
        assert_eq!(bands.rate_status(4.5), BandStatus::Above);

        assert_eq!(bands.wpm_status(100.0), BandStatus::Below);
        assert_eq!(bands.wpm_status(140.0), BandStatus::InBand);
        assert_eq!(bands.wpm_status(190.0), BandStatus::Above);

        assert_eq!(bands.pause_status(0.05), BandStatus::Below);
        assert_eq!(bands.pause_status(0.20), BandStatus::InBand);
        assert_eq!(bands.pause_status(0.40), BandStatus::Above);
    }

    #[test]
    fn floor_metrics_flag_below_and_in_but_never_above() {
        let bands = TargetBands::default(); // dynamism floor 3.0, clarity floor 0.6
        assert_eq!(bands.dynamism_status(1.1), BandStatus::Below);
        assert_eq!(bands.dynamism_status(3.0), BandStatus::InBand);
        assert_eq!(bands.dynamism_status(7.4), BandStatus::InBand);

        assert_eq!(bands.clarity_status(0.3), BandStatus::Below);
        assert_eq!(bands.clarity_status(0.6), BandStatus::InBand);
        assert_eq!(bands.clarity_status(0.95), BandStatus::InBand);
    }

    #[test]
    fn default_config_carries_the_signed_off_bands_and_factor() {
        let config = LumedirConfig::default();
        assert_eq!(config.syllables_per_word, DEFAULT_SYLLABLES_PER_WORD);
        let bands = config.bands;
        assert_eq!(bands.rate_syl_per_s_min, 3.0);
        assert_eq!(bands.rate_syl_per_s_max, 4.0);
        assert_eq!(bands.wpm_min, 120.0);
        assert_eq!(bands.wpm_max, 160.0);
        assert_eq!(bands.dynamism_semitones_min, 3.0);
        assert_eq!(bands.pause_fraction_min, 0.10);
        assert_eq!(bands.pause_fraction_max, 0.30);
        assert_eq!(bands.clarity_min, 0.6);
    }
}
