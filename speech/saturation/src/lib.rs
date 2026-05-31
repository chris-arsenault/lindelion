//! Saturation: asymmetric tanh warmth with dry/wet blend.
//!
//! Ports hot-mic's `SaturationPlugin` (Warmth, Blend). Warmth scales the drive and asymmetry of
//! the shared `dsp-utils::saturation` shaper; Blend mixes the shaped signal back with the dry
//! signal. At Blend 0 the effect is identity.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::saturation::soft_clip;
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_WARMTH_PCT: u32 = 0;
pub const PARAM_BLEND_PCT: u32 = 1;

const MAX_DRIVE: f32 = 5.0;
const MAX_ASYMMETRY: f32 = 0.3;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_WARMTH_PCT,
        name: "Warmth",
        min: 0.0,
        max: 100.0,
        default: 50.0,
        unit: "%",
    },
    EffectParam {
        index: PARAM_BLEND_PCT,
        name: "Blend",
        min: 0.0,
        max: 100.0,
        default: 100.0,
        unit: "%",
    },
];

/// Asymmetric tanh saturation with dry/wet blend.
pub struct Saturation {
    warmth_pct: f32,
    blend_pct: f32,
    bypassed: bool,
    drive: f32,
    asymmetry: f32,
    wet: f32,
}

impl Saturation {
    pub fn new() -> Self {
        let mut sat = Self {
            warmth_pct: 50.0,
            blend_pct: 100.0,
            bypassed: false,
            drive: 0.0,
            asymmetry: 0.0,
            wet: 1.0,
        };
        sat.reconfigure();
        sat
    }

    fn reconfigure(&mut self) {
        let warmth = self.warmth_pct / 100.0;
        self.drive = warmth * MAX_DRIVE;
        self.asymmetry = warmth * MAX_ASYMMETRY;
        self.wet = self.blend_pct / 100.0;
    }
}

impl Default for Saturation {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for Saturation {
    fn name(&self) -> &str {
        "Saturation"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_WARMTH_PCT => self.warmth_pct = value.clamp(0.0, 100.0),
            PARAM_BLEND_PCT => self.blend_pct = value.clamp(0.0, 100.0),
            _ => return,
        }
        self.reconfigure();
    }

    fn prepare(&mut self, _sample_rate: f32, _max_block: usize) {
        self.reconfigure();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let dry = 1.0 - self.wet;
        for sample in buffer.iter_mut() {
            let input = *sample;
            let shaped = soft_clip(input, self.drive, self.asymmetry);
            *sample = dry * input + self.wet * shaped;
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

    fn reset(&mut self) {}

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&self.warmth_pct.to_le_bytes());
        bytes.extend_from_slice(&self.blend_pct.to_le_bytes());
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.set_parameter(
                PARAM_WARMTH_PCT,
                f32::from_le_bytes([state[0], state[1], state[2], state[3]]),
            );
        }
        if state.len() >= 8 {
            self.set_parameter(
                PARAM_BLEND_PCT,
                f32::from_le_bytes([state[4], state[5], state[6], state[7]]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::windowed_dft_magnitude_at;

    #[test]
    fn blend_zero_is_identity() {
        let mut sat = Saturation::new();
        sat.set_parameter(PARAM_BLEND_PCT, 0.0);
        sat.prepare(48_000.0, 64);
        let input = [0.4_f32; 64];
        let mut buffer = input;
        sat.process(&mut buffer);
        assert_eq!(buffer, input);
    }

    #[test]
    fn adds_harmonics() {
        let mut sat = Saturation::new();
        sat.prepare(48_000.0, 4_096);
        let input: Vec<f32> = (0..4_096)
            .map(|n| 0.8 * (std::f32::consts::TAU * 1_000.0 * n as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        sat.process(&mut buffer);
        let h3_in = windowed_dft_magnitude_at(&input, 48_000.0, 3_000.0);
        let h3_out = windowed_dft_magnitude_at(&buffer, 48_000.0, 3_000.0);
        assert!(
            h3_out > h3_in + 0.01,
            "THD should rise: {h3_in} -> {h3_out}"
        );
    }
}
