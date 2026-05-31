//! Dereverberation: spectral suppression of late-reverb energy.
//!
//! Ports hot-mic's `Enhance-Dereverberation.md`. Late reverberation is a delayed, decayed copy of
//! recent spectral energy, so per bin a `decay × previous-frame magnitude` estimate is subtracted
//! from the current magnitude (phase kept). Decaying tails are strongly suppressed while onsets
//! and rising energy pass. Runs in the shared allocation-free STFT.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::stft::StftProcessor;
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_AMOUNT_PCT: u32 = 0;

const FRAME_SIZE: usize = 1_024;
const MAX_DECAY: f32 = 0.9;
const GAIN_FLOOR: f32 = 0.1;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 60.0,
    unit: "%",
}];

/// Spectral dereverberation.
pub struct Dereverberation {
    amount_pct: f32,
    bypassed: bool,
    stft: StftProcessor,
    prev_mag: Vec<f32>,
}

impl Dereverberation {
    pub fn new() -> Self {
        Self {
            amount_pct: 60.0,
            bypassed: false,
            stft: StftProcessor::new(FRAME_SIZE),
            prev_mag: vec![0.0; FRAME_SIZE / 2 + 1],
        }
    }

    fn decay(&self) -> f32 {
        (self.amount_pct / 100.0) * MAX_DECAY
    }
}

impl Default for Dereverberation {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Dereverberation {
    fn name(&self) -> &str {
        "Dereverberation"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        if index == PARAM_AMOUNT_PCT {
            self.amount_pct = value.clamp(0.0, 100.0);
        }
    }

    fn prepare(&mut self, _sample_rate: f32, _max_block: usize) {
        self.stft.reset();
        self.prev_mag.iter_mut().for_each(|m| *m = 0.0);
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let decay = self.decay();
        let Self { stft, prev_mag, .. } = self;
        stft.process(buffer, |spectrum| {
            for (bin, prev) in spectrum.iter_mut().zip(prev_mag.iter_mut()) {
                let mag = bin.norm();
                let gain = ((mag - decay * *prev) / (mag + 1.0e-9)).clamp(GAIN_FLOOR, 1.0);
                *bin *= gain;
                *prev = mag;
            }
        });
    }

    fn latency_samples(&self) -> usize {
        self.stft.latency_samples()
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.stft.reset();
        self.prev_mag.iter_mut().for_each(|m| *m = 0.0);
    }

    fn save_state(&self) -> Vec<u8> {
        self.amount_pct.to_le_bytes().to_vec()
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.set_parameter(
                PARAM_AMOUNT_PCT,
                f32::from_le_bytes([state[0], state[1], state[2], state[3]]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::rms;

    #[test]
    fn decaying_tail_reduced_more_than_steady() {
        let mut effect = Dereverberation::new();
        effect.prepare(48_000.0, 1_024);
        let n = 24_000;
        let half = n / 2;
        let input: Vec<f32> = (0..n)
            .map(|i| {
                let env = if i < half {
                    0.5
                } else {
                    0.5 * (-((i - half) as f32) / 1_500.0).exp()
                };
                env * (std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin()
            })
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let lat = effect.latency_samples();
        let steady = (n / 4, half - 2_000);
        let tail = (half + 2_000, n - 2_000);
        let ratio = |r: (usize, usize)| {
            rms(&buffer[r.0 + lat..r.1 + lat]) / rms(&input[r.0..r.1]).max(f32::MIN_POSITIVE)
        };
        let steady_ratio = ratio(steady);
        let tail_ratio = ratio(tail);
        assert!(
            tail_ratio < steady_ratio * 0.9,
            "tail not reduced relative to direct: steady {steady_ratio} vs tail {tail_ratio}"
        );
    }
}
