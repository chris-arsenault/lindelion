//! Noise Gate: level-driven downward gate with hysteresis and hold.
//!
//! Ports hot-mic's `NoiseGatePlugin` (Threshold, Hysteresis, Attack, Hold, Release). A peak
//! envelope detector drives a hysteretic open/close decision; the gate stays open for `Hold`
//! after the level falls below the close threshold, then releases. Gain ramps with attack
//! (opening) and release (closing) coefficients.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::db_to_gain;
use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_THRESHOLD_DB: u32 = 0;
pub const PARAM_HYSTERESIS_DB: u32 = 1;
pub const PARAM_ATTACK_MS: u32 = 2;
pub const PARAM_HOLD_MS: u32 = 3;
pub const PARAM_RELEASE_MS: u32 = 4;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_THRESHOLD_DB,
        name: "Threshold",
        min: -80.0,
        max: 0.0,
        default: -40.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_HYSTERESIS_DB,
        name: "Hysteresis",
        min: 0.0,
        max: 12.0,
        default: 4.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_ATTACK_MS,
        name: "Attack",
        min: 0.1,
        max: 50.0,
        default: 1.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_HOLD_MS,
        name: "Hold",
        min: 0.0,
        max: 500.0,
        default: 50.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_RELEASE_MS,
        name: "Release",
        min: 10.0,
        max: 500.0,
        default: 100.0,
        unit: "ms",
    },
];

fn time_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s <= 0.0 || sample_rate <= 0.0 {
        0.0
    } else {
        (-1.0 / (time_s * sample_rate)).exp()
    }
}

/// Level-driven noise gate.
pub struct NoiseGate {
    threshold_db: f32,
    hysteresis_db: f32,
    attack_ms: f32,
    hold_ms: f32,
    release_ms: f32,
    bypassed: bool,
    sample_rate: f32,
    detector: EnvelopeFollower,
    gain: f32,
    gate_open: bool,
    hold_counter: usize,
    hold_samples: usize,
    attack_coeff: f32,
    release_coeff: f32,
    open_lin: f32,
    close_lin: f32,
}

impl NoiseGate {
    pub fn new() -> Self {
        let mut gate = Self {
            threshold_db: -40.0,
            hysteresis_db: 4.0,
            attack_ms: 1.0,
            hold_ms: 50.0,
            release_ms: 100.0,
            bypassed: false,
            sample_rate: 48_000.0,
            detector: EnvelopeFollower::new(DetectorMode::Peak),
            gain: 0.0,
            gate_open: false,
            hold_counter: 0,
            hold_samples: 0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            open_lin: 0.0,
            close_lin: 0.0,
        };
        gate.reconfigure();
        gate
    }

    fn reconfigure(&mut self) {
        self.detector.set_times(0.0005, 0.01, self.sample_rate);
        self.attack_coeff = time_coeff(self.attack_ms / 1_000.0, self.sample_rate);
        self.release_coeff = time_coeff(self.release_ms / 1_000.0, self.sample_rate);
        self.hold_samples = ((self.hold_ms / 1_000.0) * self.sample_rate).round() as usize;
        self.open_lin = db_to_gain(self.threshold_db);
        self.close_lin = db_to_gain(self.threshold_db - self.hysteresis_db);
    }
}

impl Default for NoiseGate {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for NoiseGate {
    fn name(&self) -> &str {
        "Noise Gate"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_THRESHOLD_DB => self.threshold_db = value.clamp(-80.0, 0.0),
            PARAM_HYSTERESIS_DB => self.hysteresis_db = value.clamp(0.0, 12.0),
            PARAM_ATTACK_MS => self.attack_ms = value.clamp(0.1, 50.0),
            PARAM_HOLD_MS => self.hold_ms = value.clamp(0.0, 500.0),
            PARAM_RELEASE_MS => self.release_ms = value.clamp(10.0, 500.0),
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
            let level = self.detector.process(input);
            if level > self.open_lin {
                self.gate_open = true;
                self.hold_counter = self.hold_samples;
            } else if level < self.close_lin {
                if self.hold_counter > 0 {
                    self.hold_counter -= 1;
                } else {
                    self.gate_open = false;
                }
            }
            let target = if self.gate_open { 1.0 } else { 0.0 };
            let coeff = if target > self.gain {
                self.attack_coeff
            } else {
                self.release_coeff
            };
            self.gain = target + coeff * (self.gain - target);
            *sample = input * self.gain;
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
        self.gain = 0.0;
        self.gate_open = false;
        self.hold_counter = 0;
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(20);
        for value in [
            self.threshold_db,
            self.hysteresis_db,
            self.attack_ms,
            self.hold_ms,
            self.release_ms,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let mut idx = 0;
        for setter in [
            PARAM_THRESHOLD_DB,
            PARAM_HYSTERESIS_DB,
            PARAM_ATTACK_MS,
            PARAM_HOLD_MS,
            PARAM_RELEASE_MS,
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

    fn gate_gain(amp: f32) -> f32 {
        let mut gate = NoiseGate::new();
        gate.prepare(48_000.0, 1_024);
        let n = 20_000;
        let input: Vec<f32> = (0..n)
            .map(|i| amp * (std::f32::consts::TAU * 300.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        gate.process(&mut buffer);
        let tail = n * 3 / 4;
        peak_abs(&buffer[tail..]) / peak_abs(&input[tail..]).max(f32::MIN_POSITIVE)
    }

    #[test]
    fn opens_for_loud_closes_for_quiet() {
        assert!(gate_gain(0.3) > 0.9, "loud signal should pass");
        assert!(gate_gain(0.003) < 0.1, "quiet signal should be gated");
    }
}
