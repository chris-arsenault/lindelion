//! 5-Band EQ: high-pass + low shelf + two peaking mids + high shelf, in series.
//!
//! Ports hot-mic's `FiveBandEqPlugin`. Reuses the dsp-utils high-pass, low/high shelf, and
//! peaking biquad coefficients. Speech-context defaults: HPF 80 Hz, low shelf +3 dB @120 Hz,
//! low-mid −3 dB @300 Hz (Q 1), high-mid +3 dB @3 kHz (Q 1), high shelf +2 dB @10 kHz.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_HPF_FREQ: u32 = 0;
pub const PARAM_LOW_SHELF_GAIN: u32 = 1;
pub const PARAM_LOW_SHELF_FREQ: u32 = 2;
pub const PARAM_LOW_MID_GAIN: u32 = 3;
pub const PARAM_LOW_MID_FREQ: u32 = 4;
pub const PARAM_LOW_MID_Q: u32 = 5;
pub const PARAM_HIGH_MID_GAIN: u32 = 6;
pub const PARAM_HIGH_MID_FREQ: u32 = 7;
pub const PARAM_HIGH_MID_Q: u32 = 8;
pub const PARAM_HIGH_SHELF_GAIN: u32 = 9;
pub const PARAM_HIGH_SHELF_FREQ: u32 = 10;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_HPF_FREQ,
        name: "HPF Freq",
        min: 40.0,
        max: 200.0,
        default: 80.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_LOW_SHELF_GAIN,
        name: "Low Shelf Gain",
        min: -24.0,
        max: 24.0,
        default: 3.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_LOW_SHELF_FREQ,
        name: "Low Shelf Freq",
        min: 60.0,
        max: 300.0,
        default: 120.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_LOW_MID_GAIN,
        name: "Low-Mid Gain",
        min: -24.0,
        max: 24.0,
        default: -3.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_LOW_MID_FREQ,
        name: "Low-Mid Freq",
        min: 150.0,
        max: 800.0,
        default: 300.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_LOW_MID_Q,
        name: "Low-Mid Q",
        min: 0.5,
        max: 4.0,
        default: 1.0,
        unit: "",
    },
    EffectParam {
        index: PARAM_HIGH_MID_GAIN,
        name: "High-Mid Gain",
        min: -24.0,
        max: 24.0,
        default: 3.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_HIGH_MID_FREQ,
        name: "High-Mid Freq",
        min: 1_000.0,
        max: 6_000.0,
        default: 3_000.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_HIGH_MID_Q,
        name: "High-Mid Q",
        min: 0.5,
        max: 4.0,
        default: 1.0,
        unit: "",
    },
    EffectParam {
        index: PARAM_HIGH_SHELF_GAIN,
        name: "High Shelf Gain",
        min: -24.0,
        max: 24.0,
        default: 2.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_HIGH_SHELF_FREQ,
        name: "High Shelf Freq",
        min: 6_000.0,
        max: 16_000.0,
        default: 10_000.0,
        unit: "Hz",
    },
];

const PARAM_COUNT: usize = 11;

/// Five-band parametric EQ.
pub struct FiveBandEq {
    values: [f32; PARAM_COUNT],
    bypassed: bool,
    sample_rate: f32,
    hpf: Biquad,
    low_shelf: Biquad,
    low_mid: Biquad,
    high_mid: Biquad,
    high_shelf: Biquad,
}

impl FiveBandEq {
    pub fn new() -> Self {
        let mut values = [0.0; PARAM_COUNT];
        for param in PARAMS {
            values[param.index as usize] = param.default;
        }
        let mut eq = Self {
            values,
            bypassed: false,
            sample_rate: 48_000.0,
            hpf: Biquad::new(BiquadCoefficients::identity()),
            low_shelf: Biquad::new(BiquadCoefficients::identity()),
            low_mid: Biquad::new(BiquadCoefficients::identity()),
            high_mid: Biquad::new(BiquadCoefficients::identity()),
            high_shelf: Biquad::new(BiquadCoefficients::identity()),
        };
        eq.reconfigure();
        eq
    }

    fn reconfigure(&mut self) {
        let sr = self.sample_rate;
        let v = &self.values;
        self.hpf
            .set_coefficients(BiquadCoefficients::highpass(sr, v[0], 0.707));
        self.low_shelf
            .set_coefficients(BiquadCoefficients::low_shelf(sr, v[2], v[1]));
        self.low_mid
            .set_coefficients(BiquadCoefficients::peaking(sr, v[4], v[5], v[3]));
        self.high_mid
            .set_coefficients(BiquadCoefficients::peaking(sr, v[7], v[8], v[6]));
        self.high_shelf
            .set_coefficients(BiquadCoefficients::high_shelf(sr, v[10], v[9]));
    }
}

impl Default for FiveBandEq {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for FiveBandEq {
    fn name(&self) -> &str {
        "5-Band EQ"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        let Some(param) = PARAMS.iter().find(|p| p.index == index) else {
            return;
        };
        self.values[index as usize] = value.clamp(param.min, param.max);
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
            let mut x = self.hpf.process(*sample);
            x = self.low_shelf.process(x);
            x = self.low_mid.process(x);
            x = self.high_mid.process(x);
            x = self.high_shelf.process(x);
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
        self.hpf.reset();
        self.low_shelf.reset();
        self.low_mid.reset();
        self.high_mid.reset();
        self.high_shelf.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PARAM_COUNT * 4);
        for value in self.values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        for (i, param) in PARAMS.iter().enumerate() {
            let off = i * 4;
            if state.len() >= off + 4 {
                let v = f32::from_le_bytes([
                    state[off],
                    state[off + 1],
                    state[off + 2],
                    state[off + 3],
                ]);
                self.values[param.index as usize] = v.clamp(param.min, param.max);
            }
        }
        self.reconfigure();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::peak_abs;

    fn response_db(freq: f32) -> f32 {
        let mut eq = FiveBandEq::new();
        eq.prepare(48_000.0, 8_192);
        let input: Vec<f32> = (0..8_192)
            .map(|n| (std::f32::consts::TAU * freq * n as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        eq.process(&mut buffer);
        let half = input.len() / 2;
        20.0 * (peak_abs(&buffer[half..]) / peak_abs(&input[half..])).log10()
    }

    #[test]
    fn band_centers_respond() {
        assert!((response_db(3_000.0) - 3.0).abs() < 1.5, "high-mid +3 dB");
        assert!(response_db(30.0) < -8.0, "HPF stopband");
        assert!(response_db(10_000.0) > 0.5, "high shelf boost");
    }
}
