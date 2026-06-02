//! Pitch-dynamism estimator: the spread of voiced pitch over a window, in semitones.
//!
//! A pure windowed-statistics component. Each voiced, pitched frame's f0 is converted to semitones
//! (`12·log2(f0)`; any reference cancels in a standard deviation) and kept in a bounded ring; the
//! dynamism is the standard deviation of those values — small for flat/monotone delivery, large for
//! animated/expressive delivery. The f0 source is SwiftF0, supplied by the caller (the fixture test
//! and, in M3, the live worker); unvoiced/unpitched frames (`None`) contribute nothing.
//!
//! Plugin-local per the M0 decision. It runs off the audio thread, so it is not required to be
//! allocation-free; the ring is bounded so the live path stays clean.

use std::collections::VecDeque;

/// Tuning for the pitch-dynamism estimator.
#[derive(Debug, Clone, Copy)]
pub struct PitchDynamismConfig {
    /// Number of recent voiced frames over which the dynamism is reported.
    pub window_voiced_frames: usize,
}

impl Default for PitchDynamismConfig {
    fn default() -> Self {
        // ~2000 frames covers the ~5 s fixtures whole and bounds the live window.
        Self {
            window_voiced_frames: 2000,
        }
    }
}

/// Streaming pitch-dynamism estimator.
pub struct PitchDynamism {
    config: PitchDynamismConfig,
    /// Recent voiced pitches in semitones (`12·log2(f0)`), bounded to the window.
    semitones: VecDeque<f32>,
}

impl PitchDynamism {
    pub fn new(config: PitchDynamismConfig) -> Self {
        Self {
            config,
            semitones: VecDeque::with_capacity(config.window_voiced_frames + 1),
        }
    }

    /// Clear all state (keeps the configuration).
    pub fn reset(&mut self) {
        self.semitones.clear();
    }

    /// Consume one analysis frame. A voiced, pitched frame (`Some(f0_hz)`, `f0_hz > 0`) contributes
    /// its semitone value; unvoiced/unpitched frames are ignored.
    pub fn push_frame(&mut self, f0_hz: Option<f32>) {
        let Some(f0) = f0_hz else { return };
        if f0 <= 0.0 || !f0.is_finite() {
            return;
        }
        self.semitones.push_back(12.0 * f0.log2());
        if self.semitones.len() > self.config.window_voiced_frames {
            self.semitones.pop_front();
        }
    }

    /// Standard deviation of the windowed voiced pitch, in semitones. Returns 0 with fewer than two
    /// voiced frames.
    pub fn semitone_std(&self) -> f32 {
        let n = self.semitones.len();
        if n < 2 {
            return 0.0;
        }
        let mean = self.semitones.iter().sum::<f32>() / n as f32;
        let variance = self
            .semitones
            .iter()
            .map(|s| {
                let d = s - mean;
                d * d
            })
            .sum::<f32>()
            / n as f32;
        variance.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fed(frames: &[Option<f32>]) -> PitchDynamism {
        let mut dynamism = PitchDynamism::new(PitchDynamismConfig::default());
        for &f in frames {
            dynamism.push_frame(f);
        }
        dynamism
    }

    #[test]
    fn constant_pitch_reads_near_zero() {
        let frames: Vec<Option<f32>> = std::iter::repeat_n(Some(200.0_f32), 500).collect();
        assert!(fed(&frames).semitone_std() < 1.0e-3);
    }

    #[test]
    fn alternating_pitches_read_the_expected_std() {
        // Alternating 200/220 Hz → std = half the interval = 12·log2(1.1)/2 ≈ 0.826 st.
        let frames: Vec<Option<f32>> = (0..500)
            .map(|i| Some(if i % 2 == 0 { 200.0 } else { 220.0 }))
            .collect();
        let std = fed(&frames).semitone_std();
        let expected = 12.0 * 1.1_f32.log2() / 2.0;
        assert!(
            (std - expected).abs() < 0.05,
            "expected ~{expected} st, got {std}"
        );
    }

    #[test]
    fn animated_reads_above_flat() {
        // Flat: small jitter around 150 Hz. Animated: wide swings.
        let flat: Vec<Option<f32>> = (0..400)
            .map(|i| Some(150.0 + if i % 2 == 0 { 1.0 } else { -1.0 }))
            .collect();
        let animated: Vec<Option<f32>> = (0..400)
            .map(|i| Some(if i % 2 == 0 { 110.0 } else { 240.0 }))
            .collect();
        assert!(
            fed(&animated).semitone_std() > fed(&flat).semitone_std(),
            "animated should read above flat"
        );
    }

    #[test]
    fn unvoiced_frames_are_ignored() {
        // None frames interleaved must not change the std of the voiced frames.
        let voiced: Vec<Option<f32>> = (0..100)
            .map(|i| Some(if i % 2 == 0 { 200.0 } else { 220.0 }))
            .collect();
        let mut interleaved: Vec<Option<f32>> = Vec::new();
        for f in &voiced {
            interleaved.push(*f);
            interleaved.push(None);
        }
        assert!((fed(&voiced).semitone_std() - fed(&interleaved).semitone_std()).abs() < 1.0e-6);
    }
}
