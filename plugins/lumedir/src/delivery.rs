//! Delivery snapshot + aggregator: the complete set of delivery metrics, assembled from the four
//! estimators.
//!
//! Pure assembly — no threading. Each `update` consumes one `(audio chunk, SignalSnapshot)` pair and
//! routes it to the estimators that need it: rate/pauses get the audio; dynamism gets the voiced f0;
//! clarity gets the voicing state + onset flux. The [`DeliveryWorker`](crate::DeliveryWorker) drives
//! this off the audio thread.

use lindelion_speech_signals::SignalSnapshot;

use crate::clarity::{Clarity, ClarityConfig};
use crate::pause_structure::{PauseStructure, PauseStructureConfig};
use crate::pitch_dynamism::{PitchDynamism, PitchDynamismConfig};
use crate::speaking_rate::{DEFAULT_SYLLABLES_PER_WORD, SpeakingRateConfig, SpeakingRateEstimator};

/// The complete delivery readout.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DeliverySnapshot {
    pub syllables_per_second: f32,
    pub words_per_minute: f32,
    pub pitch_dynamism_semitones: f32,
    pub pause_fraction: f32,
    pub pause_count: u32,
    pub clarity: f32,
}

/// Tuning for the delivery aggregator.
#[derive(Debug, Clone, Copy)]
pub struct DeliveryConfig {
    /// Syllables-per-word factor for the WPM derivation.
    pub syllables_per_word: f32,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            syllables_per_word: DEFAULT_SYLLABLES_PER_WORD,
        }
    }
}

/// Owns the four delivery estimators and assembles a [`DeliverySnapshot`].
pub struct DeliveryAggregator {
    config: DeliveryConfig,
    rate: SpeakingRateEstimator,
    dynamism: PitchDynamism,
    pauses: PauseStructure,
    clarity: Clarity,
}

impl DeliveryAggregator {
    pub fn new(sample_rate: f32, config: DeliveryConfig) -> Self {
        Self {
            config,
            rate: SpeakingRateEstimator::new(sample_rate, SpeakingRateConfig::default()),
            dynamism: PitchDynamism::new(PitchDynamismConfig::default()),
            pauses: PauseStructure::new(sample_rate, PauseStructureConfig::default()),
            clarity: Clarity::new(ClarityConfig::default()),
        }
    }

    /// Clear all estimator state (keeps the configuration).
    pub fn reset(&mut self) {
        self.rate.reset();
        self.dynamism.reset();
        self.pauses.reset();
        self.clarity.reset();
    }

    /// Consume one analysis step: the chunk audio plus the analyzer's snapshot for that chunk.
    pub fn update(&mut self, audio: &[f32], signal: &SignalSnapshot) {
        self.rate.push(audio);
        self.pauses.push(audio);
        // Voiced, pitched frame ⇒ feed its f0 to the dynamism ring; otherwise nothing.
        let f0 = (signal.voicing_state >= 1.5).then_some(signal.pitch_hz);
        self.dynamism.push_frame(f0);
        self.clarity
            .update(signal.voicing_state, signal.onset_flux_high);
    }

    /// Assemble the current delivery snapshot.
    pub fn snapshot(&self) -> DeliverySnapshot {
        DeliverySnapshot {
            syllables_per_second: self.rate.syllables_per_second(),
            words_per_minute: self.rate.words_per_minute(self.config.syllables_per_word),
            pitch_dynamism_semitones: self.dynamism.semitone_std(),
            pause_fraction: self.pauses.pause_fraction(),
            pause_count: self.pauses.pause_count() as u32,
            clarity: self.clarity.clarity(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn voiced_block(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 150.0 * i as f32 / SR).sin())
            .collect()
    }

    #[test]
    fn aggregates_a_complete_snapshot() {
        let mut aggregator = DeliveryAggregator::new(SR, DeliveryConfig::default());
        let block = voiced_block(2_048);
        // Drive several voiced frames with a slightly varying pitch (so dynamism > 0) and onsets.
        for i in 0..400 {
            let signal = SignalSnapshot {
                voicing_state: 2.0,
                pitch_hz: if i % 2 == 0 { 150.0 } else { 160.0 },
                onset_flux_high: 4.0,
                ..SignalSnapshot::default()
            };
            aggregator.update(&block, &signal);
        }
        let snapshot = aggregator.snapshot();

        // WPM derives from the rate via the factor.
        assert!(
            (snapshot.words_per_minute - snapshot.syllables_per_second * 60.0 / 1.5).abs() < 1.0e-3,
            "WPM should equal syllables/min ÷ factor"
        );
        // Dynamism responds to the alternating pitch.
        assert!(
            snapshot.pitch_dynamism_semitones > 0.0,
            "dynamism should be > 0 for varying pitch"
        );
        // Clarity is in range and elevated (all voiced + onsets).
        assert!(
            (0.0..=1.0).contains(&snapshot.clarity) && snapshot.clarity > 0.5,
            "clarity {} out of expected range",
            snapshot.clarity
        );
        // Pause fraction is a valid fraction.
        assert!((0.0..=1.0).contains(&snapshot.pause_fraction));
        // Every field is finite.
        assert!(
            snapshot.syllables_per_second.is_finite()
                && snapshot.words_per_minute.is_finite()
                && snapshot.pitch_dynamism_semitones.is_finite()
                && snapshot.pause_fraction.is_finite()
                && snapshot.clarity.is_finite()
        );
    }

    #[test]
    fn reset_clears_the_snapshot() {
        let mut aggregator = DeliveryAggregator::new(SR, DeliveryConfig::default());
        let block = voiced_block(2_048);
        for _ in 0..50 {
            let signal = SignalSnapshot {
                voicing_state: 2.0,
                pitch_hz: 150.0,
                onset_flux_high: 4.0,
                ..SignalSnapshot::default()
            };
            aggregator.update(&block, &signal);
        }
        aggregator.reset();
        assert_eq!(aggregator.snapshot(), DeliverySnapshot::default());
    }
}
