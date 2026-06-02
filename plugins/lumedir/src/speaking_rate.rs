//! Speaking-rate (syllable-nuclei) estimator.
//!
//! An envelope-peak syllable detector that yields a speaking rate over a sliding window. The method
//! follows De Jong & Wempe's "detect syllable nuclei": take the intensity envelope, then keep its
//! local maxima that are loud enough (relative to the window's intensity maximum) and **separated
//! by an intensity dip** of at least `min_dip_db` — peaks not separated by a real valley belong to
//! the same syllable and are merged. Plugin-local per the M0 decision; it reuses only the pure
//! [`EnvelopeFollower`] primitive.
//!
//! It runs off the audio thread (M3 wires it into the analysis worker), so it is not required to be
//! allocation-free; it keeps a bounded intensity-contour ring (one dB value per hop) so the live
//! path stays clean, and picks nuclei over that ring on query.

use std::collections::VecDeque;

use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};

/// Default syllables-per-word factor for the WPM derivation. English averages ≈ 1.4–1.5 syllables
/// per word; the factor is configurable (the editor/state layer can override it).
pub const DEFAULT_SYLLABLES_PER_WORD: f32 = 1.5;

/// One intensity sample is stored every `HOP` audio samples (≈2.7 ms at 48 kHz) — fine enough to
/// resolve ~150 ms syllables while keeping the contour ring small.
const HOP: usize = 128;

/// Tuning for the syllable-nuclei detector. Defaults are validated and tuned against the
/// spoken-word fixtures (see `tests/rate_fixtures.rs`).
#[derive(Debug, Clone, Copy)]
pub struct SpeakingRateConfig {
    /// Intensity-envelope attack time (seconds).
    pub envelope_attack_s: f32,
    /// Intensity-envelope release time (seconds).
    pub envelope_release_s: f32,
    /// A peak counts only if it is within this many dB of the window's intensity maximum.
    pub silence_threshold_db: f32,
    /// Two peaks are distinct nuclei only if the valley between them is at least this many dB below
    /// the lower of the two; otherwise they belong to one syllable and are merged.
    pub min_dip_db: f32,
    /// Minimum spacing between successive nuclei (seconds) — a refractory bound on the max rate.
    pub min_nucleus_interval_s: f32,
    /// Sliding window over which the rate is reported (seconds).
    pub window_s: f32,
}

impl Default for SpeakingRateConfig {
    fn default() -> Self {
        Self {
            envelope_attack_s: 0.008,
            envelope_release_s: 0.030,
            silence_threshold_db: 25.0,
            min_dip_db: 2.5,
            min_nucleus_interval_s: 0.09,
            window_s: 8.0,
        }
    }
}

/// Streaming syllable-nuclei speaking-rate estimator.
pub struct SpeakingRateEstimator {
    sample_rate: f32,
    config: SpeakingRateConfig,
    follower: EnvelopeFollower,
    /// Intensity contour in dB, one sample per [`HOP`] audio samples, bounded to the window.
    contour_db: VecDeque<f32>,
    contour_capacity: usize,
    hop_counter: usize,
    samples_processed: u64,
}

const DB_FLOOR: f32 = -120.0;
const EPS: f32 = 1.0e-9;

fn amplitude_to_db(amplitude: f32) -> f32 {
    (20.0 * (amplitude + EPS).log10()).max(DB_FLOOR)
}

impl SpeakingRateEstimator {
    pub fn new(sample_rate: f32, config: SpeakingRateConfig) -> Self {
        let mut follower = EnvelopeFollower::new(DetectorMode::Rms);
        follower.set_times(
            config.envelope_attack_s,
            config.envelope_release_s,
            sample_rate,
        );
        let contour_capacity = ((config.window_s * sample_rate) as usize / HOP).max(3);
        Self {
            sample_rate,
            config,
            follower,
            contour_db: VecDeque::with_capacity(contour_capacity + 1),
            contour_capacity,
            hop_counter: 0,
            samples_processed: 0,
        }
    }

    /// Clear all state (keeps the configuration and sample rate).
    pub fn reset(&mut self) {
        self.follower.reset();
        self.contour_db.clear();
        self.hop_counter = 0;
        self.samples_processed = 0;
    }

    /// Feed a block of mono audio, decimating the smoothed intensity into the contour ring.
    pub fn push(&mut self, block: &[f32]) {
        for &sample in block {
            let env = self.follower.process(sample);
            self.hop_counter += 1;
            if self.hop_counter >= HOP {
                self.hop_counter = 0;
                self.contour_db.push_back(amplitude_to_db(env));
                if self.contour_db.len() > self.contour_capacity {
                    self.contour_db.pop_front();
                }
            }
            self.samples_processed += 1;
        }
    }

    /// Number of syllable nuclei currently in the contour window: prominence-based peak picking.
    fn nucleus_count(&self) -> usize {
        let n = self.contour_db.len();
        if n < 3 {
            return 0;
        }
        let contour: Vec<f32> = self.contour_db.iter().copied().collect();
        let global_max = contour.iter().copied().fold(DB_FLOOR, f32::max);
        let floor = global_max - self.config.silence_threshold_db;
        let refractory_hops =
            ((self.config.min_nucleus_interval_s * self.sample_rate) as usize / HOP).max(1);

        let mut count = 0usize;
        let mut last_peak_idx: Option<usize> = None;
        let mut last_peak_db = DB_FLOOR;
        // Lowest contour value since the last accepted nucleus (the valley for the dip test).
        let mut valley = f32::MAX;

        for i in 1..n - 1 {
            valley = valley.min(contour[i]);
            let is_local_max = contour[i] >= contour[i - 1] && contour[i] > contour[i + 1];
            if !is_local_max || contour[i] < floor {
                continue;
            }
            match last_peak_idx {
                None => {
                    count += 1;
                    last_peak_idx = Some(i);
                    last_peak_db = contour[i];
                    valley = contour[i];
                }
                Some(last) => {
                    let dip = last_peak_db.min(contour[i]) - valley;
                    if dip >= self.config.min_dip_db && i - last >= refractory_hops {
                        count += 1;
                        last_peak_idx = Some(i);
                        last_peak_db = contour[i];
                        valley = contour[i];
                    } else if contour[i] > last_peak_db {
                        // Same syllable (no real valley yet): keep the higher peak as the reference.
                        last_peak_idx = Some(i);
                        last_peak_db = contour[i];
                        valley = contour[i];
                    }
                }
            }
        }
        count
    }

    fn window_samples(&self) -> u64 {
        (self.config.window_s * self.sample_rate) as u64
    }

    /// Speaking rate over the sliding window, in syllable nuclei per second.
    pub fn syllables_per_second(&self) -> f32 {
        let elapsed = self.samples_processed.min(self.window_samples());
        if elapsed == 0 {
            return 0.0;
        }
        let elapsed_seconds = elapsed as f32 / self.sample_rate;
        self.nucleus_count() as f32 / elapsed_seconds
    }

    /// Speaking rate in syllable nuclei per minute.
    pub fn syllables_per_minute(&self) -> f32 {
        self.syllables_per_second() * 60.0
    }

    /// Words per minute = syllables/min ÷ the syllables-per-word factor. A non-positive factor is
    /// clamped to a tiny positive value to avoid a divide-by-zero.
    pub fn words_per_minute(&self, syllables_per_word: f32) -> f32 {
        self.syllables_per_minute() / syllables_per_word.max(EPS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Synthesize a voiced carrier amplitude-modulated by `n_bumps` raised-cosine "syllable" bumps
    /// spread over `secs` seconds, with a gap (dip) between bumps. Expected rate = `n_bumps / secs`.
    fn am_speech(secs: f32, n_bumps: usize) -> Vec<f32> {
        let total = (secs * SR) as usize;
        let spacing = secs / n_bumps as f32;
        let width = 0.6 * spacing; // leaves a dip between bumps and trailing room after the last
        let carrier = 160.0;
        (0..total)
            .map(|i| {
                let t = i as f32 / SR;
                let mut amp = 0.0_f32;
                for k in 0..n_bumps {
                    let center = (k as f32 + 0.5) * spacing;
                    let d = t - center;
                    if d.abs() <= width * 0.5 {
                        // Raised-cosine window, peak 1.0 at the center.
                        amp += 0.5 * (1.0 + (std::f32::consts::TAU * d / width).cos());
                    }
                }
                amp * (std::f32::consts::TAU * carrier * t).sin()
            })
            .collect()
    }

    fn measure(secs: f32, n_bumps: usize) -> f32 {
        let mut estimator = SpeakingRateEstimator::new(SR, SpeakingRateConfig::default());
        estimator.push(&am_speech(secs, n_bumps));
        estimator.syllables_per_second()
    }

    fn estimator_for(secs: f32, n_bumps: usize) -> SpeakingRateEstimator {
        let mut estimator = SpeakingRateEstimator::new(SR, SpeakingRateConfig::default());
        estimator.push(&am_speech(secs, n_bumps));
        estimator
    }

    #[test]
    fn tracks_a_known_syllable_rate() {
        // 16 bumps over 4 s → 4.0 syllables/s.
        let rate = measure(4.0, 16);
        assert!((rate - 4.0).abs() < 0.5, "expected ~4.0 syl/s, got {rate}");
    }

    #[test]
    fn ranks_a_faster_rate_above_a_slower_one() {
        let slow = measure(4.0, 16); // 4.0/s
        let fast = measure(4.0, 24); // 6.0/s
        assert!(fast > slow, "fast {fast} should exceed slow {slow}");
        assert!((fast - 6.0).abs() < 0.8, "expected ~6.0 syl/s, got {fast}");
    }

    #[test]
    fn wpm_derives_from_the_rate_and_factor() {
        let estimator = estimator_for(4.0, 16); // ~4.0 syl/s → ~240 syl/min
        let spm = estimator.syllables_per_minute();
        // WPM = syllables/min ÷ factor.
        assert!(
            (estimator.words_per_minute(DEFAULT_SYLLABLES_PER_WORD) - spm / 1.5).abs() < 1.0e-3,
            "WPM should equal syllables/min ÷ 1.5"
        );
        assert!(
            (estimator.words_per_minute(DEFAULT_SYLLABLES_PER_WORD) - 160.0).abs() < 20.0,
            "expected ~160 WPM for ~4 syl/s at factor 1.5, got {}",
            estimator.words_per_minute(DEFAULT_SYLLABLES_PER_WORD)
        );
        // A smaller factor yields a larger WPM.
        assert!(estimator.words_per_minute(1.0) > estimator.words_per_minute(2.0));
    }
}
