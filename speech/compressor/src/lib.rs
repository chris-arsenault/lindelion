//! Compressor: feed-forward dynamics with a soft knee and an internal detection high-pass.
//!
//! Ports hot-mic's `CompressorPlugin` (Threshold, Ratio, Attack, Release, Knee, Makeup). A peak
//! envelope detector — fed from a high-passed copy of the signal so low-frequency energy does not
//! pump the gain — smooths the level with the attack/release times; a soft-knee gain computer
//! turns level over threshold into gain reduction; makeup gain is applied after.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::{db_to_gain, gain_to_db};
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_THRESHOLD_DB: u32 = 0;
pub const PARAM_RATIO: u32 = 1;
pub const PARAM_ATTACK_MS: u32 = 2;
pub const PARAM_RELEASE_MS: u32 = 3;
pub const PARAM_KNEE_DB: u32 = 4;
pub const PARAM_MAKEUP_DB: u32 = 5;

const DETECTOR_HPF_HZ: f32 = 85.0;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_THRESHOLD_DB,
        name: "Threshold",
        min: -60.0,
        max: 0.0,
        default: -20.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_RATIO,
        name: "Ratio",
        min: 1.0,
        max: 20.0,
        default: 4.0,
        unit: ":1",
    },
    EffectParam {
        index: PARAM_ATTACK_MS,
        name: "Attack",
        min: 0.1,
        max: 100.0,
        default: 10.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_RELEASE_MS,
        name: "Release",
        min: 10.0,
        max: 1000.0,
        default: 100.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_KNEE_DB,
        name: "Knee",
        min: 0.0,
        max: 12.0,
        default: 6.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_MAKEUP_DB,
        name: "Makeup",
        min: 0.0,
        max: 24.0,
        default: 0.0,
        unit: "dB",
    },
];

// Soft-knee gain-reduction (dB, >= 0) for a level `over` dB above threshold.
fn gain_reduction_db(over: f32, ratio: f32, knee: f32) -> f32 {
    let slope = 1.0 - 1.0 / ratio;
    if knee > 0.0 && over > -knee / 2.0 && over < knee / 2.0 {
        let x = over + knee / 2.0;
        slope * x * x / (2.0 * knee)
    } else if over >= knee / 2.0 {
        slope * over
    } else {
        0.0
    }
}

/// Feed-forward compressor.
pub struct Compressor {
    threshold_db: f32,
    ratio: f32,
    attack_ms: f32,
    release_ms: f32,
    knee_db: f32,
    makeup_db: f32,
    bypassed: bool,
    sample_rate: f32,
    detector: EnvelopeFollower,
    detector_hpf: Biquad,
}

impl Compressor {
    pub fn new() -> Self {
        let mut comp = Self {
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            knee_db: 6.0,
            makeup_db: 0.0,
            bypassed: false,
            sample_rate: 48_000.0,
            detector: EnvelopeFollower::new(DetectorMode::Peak),
            detector_hpf: Biquad::new(BiquadCoefficients::identity()),
        };
        comp.reconfigure();
        comp
    }

    fn reconfigure(&mut self) {
        self.detector.set_times(
            self.attack_ms / 1_000.0,
            self.release_ms / 1_000.0,
            self.sample_rate,
        );
        self.detector_hpf
            .set_coefficients(BiquadCoefficients::highpass(
                self.sample_rate,
                DETECTOR_HPF_HZ,
                0.707,
            ));
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Compressor {
    fn name(&self) -> &str {
        "Compressor"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_THRESHOLD_DB => self.threshold_db = value.clamp(-60.0, 0.0),
            PARAM_RATIO => self.ratio = value.clamp(1.0, 20.0),
            PARAM_ATTACK_MS => self.attack_ms = value.clamp(0.1, 100.0),
            PARAM_RELEASE_MS => self.release_ms = value.clamp(10.0, 1000.0),
            PARAM_KNEE_DB => self.knee_db = value.clamp(0.0, 12.0),
            PARAM_MAKEUP_DB => self.makeup_db = value.clamp(0.0, 24.0),
            _ => return,
        }
        self.reconfigure();
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.reconfigure();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        for sample in buffer.iter_mut() {
            let input = *sample;
            let detect = self.detector_hpf.process(input);
            let level = self.detector.process(detect);
            let level_db = gain_to_db(level);
            let reduction =
                gain_reduction_db(level_db - self.threshold_db, self.ratio, self.knee_db);
            let gain = db_to_gain(self.makeup_db - reduction);
            *sample = input * gain;
        }
    }

    fn latency_samples(&self) -> usize {
        0
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.detector.reset();
        self.detector_hpf.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24);
        for value in [
            self.threshold_db,
            self.ratio,
            self.attack_ms,
            self.release_ms,
            self.knee_db,
            self.makeup_db,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let mut idx = 0;
        for setter in [
            PARAM_THRESHOLD_DB,
            PARAM_RATIO,
            PARAM_ATTACK_MS,
            PARAM_RELEASE_MS,
            PARAM_KNEE_DB,
            PARAM_MAKEUP_DB,
        ] {
            if state.len() >= idx + 4 {
                let v = f32::from_le_bytes([
                    state[idx],
                    state[idx + 1],
                    state[idx + 2],
                    state[idx + 3],
                ]);
                self.set_parameter(setter, v);
                idx += 4;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::peak_abs;

    fn output_db(input_db: f32) -> f32 {
        let mut comp = Compressor::new();
        comp.set_parameter(PARAM_KNEE_DB, 0.0); // sharp knee for an exact curve check
        comp.prepare(48_000.0, 1_024);
        let amp = db_to_gain(input_db);
        let n = 20_000;
        let input: Vec<f32> = (0..n)
            .map(|i| amp * (std::f32::consts::TAU * 300.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        comp.process(&mut buffer);
        let tail = n * 3 / 4;
        gain_to_db(peak_abs(&buffer[tail..]))
    }

    #[test]
    fn below_threshold_is_unity() {
        assert!((output_db(-30.0) - (-30.0)).abs() < 1.0);
    }

    #[test]
    fn above_threshold_follows_ratio() {
        // -8 dB in, threshold -20, ratio 4: GR = 12 * (1 - 1/4) = 9 dB -> out ~ -17 dB.
        assert!((output_db(-8.0) - (-17.0)).abs() < 1.5);
    }
}
