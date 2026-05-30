//! Dynamic EQ: voiced/fricative-driven low/high shelf shaping.
//!
//! Ports hot-mic's `DynamicEqPlugin` (Low Boost, High Boost, Scale, Smoothing). Low-shelf boost
//! tracks VoicingScore (more low warmth on voiced speech); high-shelf boost backs off with
//! FricativeActivity (tame harsh fricatives). VoicingScore comes from the off-thread analysis
//! worker; FricativeActivity is derived inline. Shelf gains are smoothed and updated per block.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::{AnalysisWorker, FricativeActivity};

pub const PARAM_LOW_BOOST_DB: u32 = 0;
pub const PARAM_HIGH_BOOST_DB: u32 = 1;
pub const PARAM_SCALE: u32 = 2;
pub const PARAM_SMOOTHING_MS: u32 = 3;

const LOW_SHELF_HZ: f32 = 200.0;
const HIGH_SHELF_HZ: f32 = 4_000.0;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_LOW_BOOST_DB,
        name: "Low Boost",
        min: -6.0,
        max: 6.0,
        default: 2.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_HIGH_BOOST_DB,
        name: "High Boost",
        min: -6.0,
        max: 6.0,
        default: 2.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_SCALE,
        name: "Scale",
        min: 0.0,
        max: 1.0,
        default: 1.0,
        unit: "",
    },
    EffectParam {
        index: PARAM_SMOOTHING_MS,
        name: "Smoothing",
        min: 20.0,
        max: 200.0,
        default: 80.0,
        unit: "ms",
    },
];

/// Low-shelf target gain (dB) for a voicing score in `0..1`.
pub fn low_target_db(low_boost_db: f32, scale: f32, voicing_score: f32) -> f32 {
    low_boost_db * scale * voicing_score.clamp(0.0, 1.0)
}

/// High-shelf target gain (dB) for a fricative activity in `0..1`.
pub fn high_target_db(high_boost_db: f32, scale: f32, fricative: f32) -> f32 {
    high_boost_db * scale * (1.0 - fricative.clamp(0.0, 1.0))
}

/// Voiced/fricative-driven dynamic EQ.
pub struct DynamicEq {
    low_boost_db: f32,
    high_boost_db: f32,
    scale: f32,
    smoothing_ms: f32,
    bypassed: bool,
    sample_rate: f32,
    worker: Option<AnalysisWorker>,
    fricative: FricativeActivity,
    last_fricative: f32,
    low_shelf: Biquad,
    high_shelf: Biquad,
    low_gain_db: f32,
    high_gain_db: f32,
}

impl DynamicEq {
    pub fn new() -> Self {
        Self {
            low_boost_db: 2.0,
            high_boost_db: 2.0,
            scale: 1.0,
            smoothing_ms: 80.0,
            bypassed: false,
            sample_rate: 48_000.0,
            worker: None,
            fricative: FricativeActivity::new(),
            last_fricative: 0.0,
            low_shelf: Biquad::new(BiquadCoefficients::identity()),
            high_shelf: Biquad::new(BiquadCoefficients::identity()),
            low_gain_db: 0.0,
            high_gain_db: 0.0,
        }
    }

    fn block_smooth(&self, block_len: usize) -> f32 {
        let tau = (self.smoothing_ms / 1_000.0) * self.sample_rate;
        if tau <= 0.0 {
            0.0
        } else {
            (-(block_len as f32) / tau).exp()
        }
    }
}

impl Default for DynamicEq {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for DynamicEq {
    fn name(&self) -> &str {
        "Dynamic EQ"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_LOW_BOOST_DB => self.low_boost_db = value.clamp(-6.0, 6.0),
            PARAM_HIGH_BOOST_DB => self.high_boost_db = value.clamp(-6.0, 6.0),
            PARAM_SCALE => self.scale = value.clamp(0.0, 1.0),
            PARAM_SMOOTHING_MS => self.smoothing_ms = value.clamp(20.0, 200.0),
            _ => {}
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.fricative.prepare(sample_rate);
        self.worker = Some(AnalysisWorker::new(sample_rate as u32));
        self.low_shelf.reset();
        self.high_shelf.reset();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed || buffer.is_empty() {
            return;
        }
        if let Some(worker) = &self.worker {
            worker.push(buffer);
        }
        let voicing = self
            .worker
            .as_ref()
            .map_or(0.0, |w| w.latest().voicing_score);

        let target_low = low_target_db(self.low_boost_db, self.scale, voicing);
        let target_high = high_target_db(self.high_boost_db, self.scale, self.last_fricative);
        let smooth = self.block_smooth(buffer.len());
        self.low_gain_db = target_low + smooth * (self.low_gain_db - target_low);
        self.high_gain_db = target_high + smooth * (self.high_gain_db - target_high);
        self.low_shelf
            .set_coefficients(BiquadCoefficients::low_shelf(
                self.sample_rate,
                LOW_SHELF_HZ,
                self.low_gain_db,
            ));
        self.high_shelf
            .set_coefficients(BiquadCoefficients::high_shelf(
                self.sample_rate,
                HIGH_SHELF_HZ,
                self.high_gain_db,
            ));

        let mut fricative = self.last_fricative;
        for sample in buffer.iter_mut() {
            fricative = self.fricative.process(*sample);
            let low = self.low_shelf.process(*sample);
            *sample = self.high_shelf.process(low);
        }
        self.last_fricative = fricative;
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
        self.fricative.reset();
        self.last_fricative = 0.0;
        self.low_shelf.reset();
        self.high_shelf.reset();
        self.low_gain_db = 0.0;
        self.high_gain_db = 0.0;
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        for value in [
            self.low_boost_db,
            self.high_boost_db,
            self.scale,
            self.smoothing_ms,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let mut idx = 0;
        for setter in [
            PARAM_LOW_BOOST_DB,
            PARAM_HIGH_BOOST_DB,
            PARAM_SCALE,
            PARAM_SMOOTHING_MS,
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

    #[test]
    fn voiced_boosts_lows_more_than_unvoiced() {
        // Low-shelf gain rises with voicing; high-shelf gain falls with fricative.
        assert!(low_target_db(2.0, 1.0, 1.0) > low_target_db(2.0, 1.0, 0.0));
        assert!(high_target_db(2.0, 1.0, 0.0) > high_target_db(2.0, 1.0, 1.0));
    }
}
