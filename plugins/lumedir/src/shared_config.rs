//! `SharedConfig` — the live coaching config shared between Lúmedir's Vizia editor (UI thread, the
//! writer) and the delivery worker / `state()` (the readers). Lúmedir surfaces **no
//! host-automatable parameters** (ADR-0023's self-contained design); the editor edits these atomics
//! directly.
//!
//! Every field is an atomic accessed `Relaxed`: the config fields are independent scalars with no
//! cross-field ordering invariant, the worker only ever *reads* the factor, and the editor only ever
//! *writes* — so there is nothing to order between them (ADR-0001: no locks on the audio path). f32
//! fields are stored as their `to_bits`/`from_bits` representation in an `AtomicU32`, mirroring
//! Calóma's `SharedControls` and the `speech/signals` analysis worker.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::config::{LumedirConfig, TargetBands};

fn load_f32(cell: &AtomicU32) -> f32 {
    f32::from_bits(cell.load(Ordering::Relaxed))
}

fn store_f32(cell: &AtomicU32, value: f32) {
    cell.store(value.to_bits(), Ordering::Relaxed);
}

/// Lock-free live coaching config: the syllables-per-word factor (read by the worker for WPM) and
/// the target bands (read by the editor for scoring). The editor writes; the worker and `state()`
/// read.
pub struct SharedConfig {
    syllables_per_word: AtomicU32,
    rate_syl_per_s_min: AtomicU32,
    rate_syl_per_s_max: AtomicU32,
    wpm_min: AtomicU32,
    wpm_max: AtomicU32,
    dynamism_semitones_min: AtomicU32,
    pause_fraction_min: AtomicU32,
    pause_fraction_max: AtomicU32,
    clarity_min: AtomicU32,
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self::from_config(&LumedirConfig::default())
    }
}

impl SharedConfig {
    /// Build the shared config seeded from `config` (the editor and worker then share this object).
    pub fn from_config(config: &LumedirConfig) -> Self {
        let shared = Self {
            syllables_per_word: AtomicU32::new(0),
            rate_syl_per_s_min: AtomicU32::new(0),
            rate_syl_per_s_max: AtomicU32::new(0),
            wpm_min: AtomicU32::new(0),
            wpm_max: AtomicU32::new(0),
            dynamism_semitones_min: AtomicU32::new(0),
            pause_fraction_min: AtomicU32::new(0),
            pause_fraction_max: AtomicU32::new(0),
            clarity_min: AtomicU32::new(0),
        };
        shared.load_from_config(config);
        shared
    }

    /// Overwrite every field from `config` (e.g. after `setState`).
    pub fn load_from_config(&self, config: &LumedirConfig) {
        store_f32(&self.syllables_per_word, config.syllables_per_word);
        let bands = &config.bands;
        store_f32(&self.rate_syl_per_s_min, bands.rate_syl_per_s_min);
        store_f32(&self.rate_syl_per_s_max, bands.rate_syl_per_s_max);
        store_f32(&self.wpm_min, bands.wpm_min);
        store_f32(&self.wpm_max, bands.wpm_max);
        store_f32(&self.dynamism_semitones_min, bands.dynamism_semitones_min);
        store_f32(&self.pause_fraction_min, bands.pause_fraction_min);
        store_f32(&self.pause_fraction_max, bands.pause_fraction_max);
        store_f32(&self.clarity_min, bands.clarity_min);
    }

    /// Snapshot every field into a [`LumedirConfig`] (e.g. for `state()`).
    pub fn to_config(&self) -> LumedirConfig {
        LumedirConfig {
            syllables_per_word: self.syllables_per_word(),
            bands: self.bands(),
        }
    }

    /// The syllables-per-word factor (read by the worker each publish for the WPM derivation).
    pub fn syllables_per_word(&self) -> f32 {
        load_f32(&self.syllables_per_word)
    }

    /// Set the syllables-per-word factor (editor).
    pub fn set_syllables_per_word(&self, value: f32) {
        store_f32(&self.syllables_per_word, value);
    }

    /// Snapshot the target bands (editor reads for scoring).
    pub fn bands(&self) -> TargetBands {
        TargetBands {
            rate_syl_per_s_min: load_f32(&self.rate_syl_per_s_min),
            rate_syl_per_s_max: load_f32(&self.rate_syl_per_s_max),
            wpm_min: load_f32(&self.wpm_min),
            wpm_max: load_f32(&self.wpm_max),
            dynamism_semitones_min: load_f32(&self.dynamism_semitones_min),
            pause_fraction_min: load_f32(&self.pause_fraction_min),
            pause_fraction_max: load_f32(&self.pause_fraction_max),
            clarity_min: load_f32(&self.clarity_min),
        }
    }

    /// Overwrite the target bands (editor; single-writer, so the per-field stores need no ordering).
    pub fn set_bands(&self, bands: TargetBands) {
        store_f32(&self.rate_syl_per_s_min, bands.rate_syl_per_s_min);
        store_f32(&self.rate_syl_per_s_max, bands.rate_syl_per_s_max);
        store_f32(&self.wpm_min, bands.wpm_min);
        store_f32(&self.wpm_max, bands.wpm_max);
        store_f32(&self.dynamism_semitones_min, bands.dynamism_semitones_min);
        store_f32(&self.pause_fraction_min, bands.pause_fraction_min);
        store_f32(&self.pause_fraction_max, bands.pause_fraction_max);
        store_f32(&self.clarity_min, bands.clarity_min);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_config() {
        let config = LumedirConfig {
            syllables_per_word: 1.8,
            bands: TargetBands {
                rate_syl_per_s_min: 2.6,
                rate_syl_per_s_max: 4.2,
                wpm_min: 115.0,
                wpm_max: 165.0,
                dynamism_semitones_min: 2.8,
                pause_fraction_min: 0.08,
                pause_fraction_max: 0.32,
                clarity_min: 0.58,
            },
        };
        let shared = SharedConfig::from_config(&config);
        assert_eq!(shared.to_config(), config);
    }

    #[test]
    fn factor_and_bands_setters_take_effect() {
        let shared = SharedConfig::default();
        shared.set_syllables_per_word(2.0);
        assert_eq!(shared.syllables_per_word(), 2.0);

        let mut bands = shared.bands();
        bands.wpm_max = 200.0;
        bands.clarity_min = 0.7;
        shared.set_bands(bands);
        assert_eq!(shared.bands().wpm_max, 200.0);
        assert_eq!(shared.bands().clarity_min, 0.7);
    }
}
