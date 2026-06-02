//! Clarity estimator: how clearly articulated the speech is, from voicing ratio + onset sharpness.
//!
//! Two cues, both from `SignalAnalyzer`'s per-frame outputs: the **voicing ratio** (the share of
//! phonated frames that are voiced — silence excluded so pauses don't penalize clarity), which drops
//! when noise or breathiness erodes confident voicing; and **onset sharpness** (a normalized
//! aggregate of `onset_flux_high`), which rises with crisp consonant articulation. Clarity is a
//! bounded [0, 1] blend of the two.
//!
//! Plugin-local per the M0 decision. Pure per-frame accumulator; runs off the audio thread.

/// Tuning for the clarity estimator.
#[derive(Debug, Clone, Copy)]
pub struct ClarityConfig {
    /// Weight of the voicing ratio in the blend.
    pub voicing_weight: f32,
    /// Weight of the normalized onset sharpness in the blend.
    pub onset_weight: f32,
    /// Reference flux for the onset-sharpness soft saturation `x / (x + reference)`. Tuned against
    /// the fixtures.
    pub onset_reference: f32,
}

impl Default for ClarityConfig {
    fn default() -> Self {
        // Voicing ratio is the noise-robust clarity cue; onset flux is partly inflated by broadband
        // noise (it rewards any high-band energy), so it carries the smaller weight.
        Self {
            voicing_weight: 0.7,
            onset_weight: 0.3,
            onset_reference: 1.0,
        }
    }
}

/// Streaming clarity estimator. Cumulative over the fed frames; call [`reset`](Self::reset) to start
/// a new session.
pub struct Clarity {
    config: ClarityConfig,
    voiced_frames: u64,
    unvoiced_frames: u64,
    onset_sum: f64,
    non_silent_frames: u64,
}

impl Clarity {
    pub fn new(config: ClarityConfig) -> Self {
        Self {
            config,
            voiced_frames: 0,
            unvoiced_frames: 0,
            onset_sum: 0.0,
            non_silent_frames: 0,
        }
    }

    /// Clear all state (keeps the configuration).
    pub fn reset(&mut self) {
        self.voiced_frames = 0;
        self.unvoiced_frames = 0;
        self.onset_sum = 0.0;
        self.non_silent_frames = 0;
    }

    /// Consume one analysis frame: `voicing_state` is 0 = silence, 1 = unvoiced, 2 = voiced;
    /// `onset_flux_high` is the high-band onset flux for the frame.
    pub fn update(&mut self, voicing_state: f32, onset_flux_high: f32) {
        // Nearest-state classification (voicing_state is published as a float).
        if voicing_state >= 1.5 {
            self.voiced_frames += 1;
        } else if voicing_state >= 0.5 {
            self.unvoiced_frames += 1;
        } else {
            // Silence: contributes to neither the voicing ratio nor the onset aggregate.
            return;
        }
        self.onset_sum += onset_flux_high.max(0.0) as f64;
        self.non_silent_frames += 1;
    }

    /// Clarity score in [0, 1]. Returns 0 with no non-silent frames.
    pub fn clarity(&self) -> f32 {
        let phonated = self.voiced_frames + self.unvoiced_frames;
        if phonated == 0 {
            return 0.0;
        }
        let voicing_ratio = self.voiced_frames as f32 / phonated as f32;

        let onset_mean = if self.non_silent_frames == 0 {
            0.0
        } else {
            (self.onset_sum / self.non_silent_frames as f64) as f32
        };
        let onset_norm = onset_mean / (onset_mean + self.config.onset_reference.max(f32::EPSILON));

        (self.config.voicing_weight * voicing_ratio + self.config.onset_weight * onset_norm)
            .clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed(frames: &[(f32, f32)]) -> Clarity {
        let mut clarity = Clarity::new(ClarityConfig::default());
        for &(state, flux) in frames {
            clarity.update(state, flux);
        }
        clarity
    }

    #[test]
    fn clear_speech_reads_high() {
        // All voiced, strong onsets.
        let frames: Vec<(f32, f32)> = std::iter::repeat_n((2.0_f32, 10.0_f32), 200).collect();
        let c = fed(&frames).clarity();
        assert!(c > 0.7, "expected high clarity, got {c}");
        assert!((0.0..=1.0).contains(&c));
    }

    #[test]
    fn unclear_speech_reads_low() {
        // Mostly unvoiced, weak onsets.
        let frames: Vec<(f32, f32)> = std::iter::repeat_n((1.0_f32, 0.1_f32), 200).collect();
        let c = fed(&frames).clarity();
        assert!(c < 0.4, "expected low clarity, got {c}");
        assert!((0.0..=1.0).contains(&c));
    }

    #[test]
    fn clearer_reads_above_less_clear() {
        let clear =
            fed(&std::iter::repeat_n((2.0_f32, 8.0_f32), 200).collect::<Vec<_>>()).clarity();
        let less = fed(&(0..200)
            .map(|i| {
                if i % 4 == 0 {
                    (2.0_f32, 1.0_f32)
                } else {
                    (1.0_f32, 0.5_f32)
                }
            })
            .collect::<Vec<_>>())
        .clarity();
        assert!(
            clear > less,
            "clear {clear} should exceed less-clear {less}"
        );
    }

    #[test]
    fn silence_only_reads_zero() {
        let c = fed(&std::iter::repeat_n((0.0_f32, 0.0_f32), 50).collect::<Vec<_>>()).clarity();
        assert_eq!(c, 0.0);
    }
}
