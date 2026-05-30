//! Gain: linear gain in decibels with optional phase invert.
//!
//! Ports the intent of `hot-mic`'s `GainPlugin`: a smoothed dB gain plus a phase-invert switch.
//! Speech-tuned defaults are an M1 concern; this crate carries only the correct gain behavior on
//! the host-agnostic [`Effect`] contract, reusing `lindelion-dsp-utils` smoothing.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::db_to_gain;
use lindelion_dsp_utils::smoothing::LinearSmoother;
use lindelion_effect::{Effect, EffectParam};

/// Parameter index: gain in decibels.
pub const PARAM_GAIN_DB: u32 = 0;
/// Parameter index: phase invert (>= 0.5 inverts).
pub const PARAM_PHASE_INVERT: u32 = 1;

const GAIN_MIN_DB: f32 = -24.0;
const GAIN_MAX_DB: f32 = 24.0;
const SMOOTHING_MS: f32 = 5.0;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_GAIN_DB,
        name: "Gain",
        min: GAIN_MIN_DB,
        max: GAIN_MAX_DB,
        default: 0.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_PHASE_INVERT,
        name: "Phase",
        min: 0.0,
        max: 1.0,
        default: 0.0,
        unit: "",
    },
];

/// Smoothed dB gain with optional phase invert.
pub struct Gain {
    gain_db: f32,
    phase_invert: bool,
    bypassed: bool,
    sample_rate: f32,
    smoothing_samples: usize,
    smoother: LinearSmoother,
}

impl Gain {
    /// Create a Gain at unity (0 dB, no invert).
    pub fn new() -> Self {
        let mut gain = Self {
            gain_db: 0.0,
            phase_invert: false,
            bypassed: false,
            sample_rate: 48_000.0,
            smoothing_samples: 0,
            smoother: LinearSmoother::new(1.0),
        };
        gain.smoother = LinearSmoother::new(gain.target_linear());
        gain
    }

    fn target_linear(&self) -> f32 {
        let sign = if self.phase_invert { -1.0 } else { 1.0 };
        db_to_gain(self.gain_db) * sign
    }
}

impl Default for Gain {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Gain {
    fn name(&self) -> &str {
        "Gain"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_GAIN_DB => self.gain_db = value.clamp(GAIN_MIN_DB, GAIN_MAX_DB),
            PARAM_PHASE_INVERT => self.phase_invert = value >= 0.5,
            _ => return,
        }
        self.smoother
            .set_target(self.target_linear(), self.smoothing_samples);
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.smoothing_samples = ((SMOOTHING_MS / 1_000.0) * sample_rate).round() as usize;
        // Settle to target so playback starts at the intended gain.
        self.smoother = LinearSmoother::new(self.target_linear());
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        for sample in buffer.iter_mut() {
            *sample *= self.smoother.next_sample();
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
        self.smoother = LinearSmoother::new(self.target_linear());
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&self.gain_db.to_le_bytes());
        let phase = if self.phase_invert { 1.0_f32 } else { 0.0 };
        bytes.extend_from_slice(&phase.to_le_bytes());
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.gain_db = f32::from_le_bytes([state[0], state[1], state[2], state[3]])
                .clamp(GAIN_MIN_DB, GAIN_MAX_DB);
        }
        if state.len() >= 8 {
            self.phase_invert = f32::from_le_bytes([state[4], state[5], state[6], state[7]]) >= 0.5;
        }
        self.smoother
            .set_target(self.target_linear(), self.smoothing_samples);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plus_six_db_roughly_doubles() {
        let mut gain = Gain::new();
        gain.set_parameter(PARAM_GAIN_DB, 6.0);
        gain.prepare(48_000.0, 64);
        let mut buffer = [1.0_f32; 64];
        gain.process(&mut buffer);
        let expected = db_to_gain(6.0);
        for sample in buffer {
            assert!(
                (sample - expected).abs() < 1.0e-4,
                "got {sample}, want {expected}"
            );
        }
    }

    #[test]
    fn bypass_is_identity() {
        let mut gain = Gain::new();
        gain.set_parameter(PARAM_GAIN_DB, 6.0);
        gain.prepare(48_000.0, 64);
        gain.set_bypassed(true);
        let input = [0.3_f32; 64];
        let mut buffer = input;
        gain.process(&mut buffer);
        assert_eq!(buffer, input);
    }

    #[test]
    fn state_roundtrips() {
        let mut gain = Gain::new();
        gain.set_parameter(PARAM_GAIN_DB, -9.0);
        gain.set_parameter(PARAM_PHASE_INVERT, 1.0);
        let state = gain.save_state();

        let mut restored = Gain::new();
        restored.load_state(&state);
        assert_eq!(restored.gain_db, -9.0);
        assert!(restored.phase_invert);
    }
}
