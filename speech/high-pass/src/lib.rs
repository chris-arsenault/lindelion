//! High-Pass Filter: cascaded Butterworth high-pass for speech rumble removal.
//!
//! Ports hot-mic's `HighPassFilterPlugin` (Cutoff, Slope). The slope is realized as a cascade of
//! 12 dB/oct biquad high-pass stages (`stages = round(slope / 12)`), so the selectable slope is
//! quantized to 12 dB/oct steps — a pure biquad cascade as the plan specifies.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_effect::{Effect, EffectParam};

/// Parameter index: cutoff frequency (Hz).
pub const PARAM_CUTOFF_HZ: u32 = 0;
/// Parameter index: slope (dB/oct).
pub const PARAM_SLOPE_DB_OCT: u32 = 1;

const MIN_CUTOFF: f32 = 40.0;
const MAX_CUTOFF: f32 = 200.0;
const MIN_SLOPE: f32 = 12.0;
const MAX_SLOPE: f32 = 48.0;
const Q: f32 = 0.707;
const MAX_STAGES: usize = 4;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_CUTOFF_HZ,
        name: "Cutoff",
        min: MIN_CUTOFF,
        max: MAX_CUTOFF,
        default: 100.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_SLOPE_DB_OCT,
        name: "Slope",
        min: MIN_SLOPE,
        max: MAX_SLOPE,
        default: 24.0,
        unit: "dB/oct",
    },
];

/// Cascaded Butterworth high-pass.
pub struct HighPass {
    cutoff_hz: f32,
    slope_db_oct: f32,
    bypassed: bool,
    sample_rate: f32,
    stages: [Biquad; MAX_STAGES],
    active: usize,
}

impl HighPass {
    /// Create a high-pass at 100 Hz, 24 dB/oct.
    pub fn new() -> Self {
        let mut hp = Self {
            cutoff_hz: 100.0,
            slope_db_oct: 24.0,
            bypassed: false,
            sample_rate: 48_000.0,
            stages: [Biquad::new(BiquadCoefficients::identity()); MAX_STAGES],
            active: 2,
        };
        hp.reconfigure();
        hp
    }

    fn reconfigure(&mut self) {
        self.active = ((self.slope_db_oct / 12.0).round() as usize).clamp(1, MAX_STAGES);
        let coeffs = BiquadCoefficients::highpass(self.sample_rate, self.cutoff_hz, Q);
        for stage in self.stages.iter_mut() {
            stage.set_coefficients(coeffs);
            stage.reset();
        }
    }
}

impl Default for HighPass {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for HighPass {
    fn name(&self) -> &str {
        "High-Pass Filter"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_CUTOFF_HZ => self.cutoff_hz = value.clamp(MIN_CUTOFF, MAX_CUTOFF),
            PARAM_SLOPE_DB_OCT => self.slope_db_oct = value.clamp(MIN_SLOPE, MAX_SLOPE),
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
            let mut x = *sample;
            for stage in self.stages[..self.active].iter_mut() {
                x = stage.process(x);
            }
            *sample = x;
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
        for stage in self.stages.iter_mut() {
            stage.reset();
        }
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8);
        bytes.extend_from_slice(&self.cutoff_hz.to_le_bytes());
        bytes.extend_from_slice(&self.slope_db_oct.to_le_bytes());
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 4 {
            self.cutoff_hz = f32::from_le_bytes([state[0], state[1], state[2], state[3]])
                .clamp(MIN_CUTOFF, MAX_CUTOFF);
        }
        if state.len() >= 8 {
            self.slope_db_oct = f32::from_le_bytes([state[4], state[5], state[6], state[7]])
                .clamp(MIN_SLOPE, MAX_SLOPE);
        }
        self.reconfigure();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::peak_abs;

    fn response_db(freq: f32) -> f32 {
        let mut hp = HighPass::new();
        hp.prepare(48_000.0, 8_192);
        let input: Vec<f32> = (0..8_192)
            .map(|n| (std::f32::consts::TAU * freq * n as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        hp.process(&mut buffer);
        let half = input.len() / 2;
        let in_peak = peak_abs(&input[half..]);
        let out_peak = peak_abs(&buffer[half..]);
        20.0 * (out_peak / in_peak).log10()
    }

    #[test]
    fn passband_unity_stopband_attenuated() {
        assert!(response_db(4_000.0).abs() < 1.0, "passband not unity");
        assert!(response_db(30.0) < -15.0, "stopband not attenuated");
    }
}
