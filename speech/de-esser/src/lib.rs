//! De-Esser: dynamic narrowband attenuation of sibilance.
//!
//! Ports hot-mic's `DeEsserPlugin` (Center, Bandwidth, Threshold, Reduction, Max Range). A
//! band-pass tap isolates the sibilant band; its envelope (the self-derived SibilanceEnergy)
//! drives a dynamic gain. The reduced band is subtracted from the dry signal, so only the
//! sibilant band ducks when it is over threshold and the rest of the signal is untouched.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::{db_to_gain, gain_to_db};
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_CENTER_HZ: u32 = 0;
pub const PARAM_BANDWIDTH_HZ: u32 = 1;
pub const PARAM_THRESHOLD_DB: u32 = 2;
pub const PARAM_REDUCTION_DB: u32 = 3;
pub const PARAM_MAX_RANGE_DB: u32 = 4;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_CENTER_HZ,
        name: "Center Freq",
        min: 4_000.0,
        max: 9_000.0,
        default: 6_000.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_BANDWIDTH_HZ,
        name: "Bandwidth",
        min: 1_000.0,
        max: 4_000.0,
        default: 2_000.0,
        unit: "Hz",
    },
    EffectParam {
        index: PARAM_THRESHOLD_DB,
        name: "Threshold",
        min: -40.0,
        max: 0.0,
        default: -30.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_REDUCTION_DB,
        name: "Reduction",
        min: 0.0,
        max: 12.0,
        default: 6.0,
        unit: "dB",
    },
    EffectParam {
        index: PARAM_MAX_RANGE_DB,
        name: "Max Range",
        min: 0.0,
        max: 20.0,
        default: 10.0,
        unit: "dB",
    },
];

/// Dynamic de-esser.
pub struct DeEsser {
    center_hz: f32,
    bandwidth_hz: f32,
    threshold_db: f32,
    reduction_db: f32,
    max_range_db: f32,
    bypassed: bool,
    sample_rate: f32,
    band: Biquad,
    detector: EnvelopeFollower,
}

impl DeEsser {
    pub fn new() -> Self {
        let mut de = Self {
            center_hz: 6_000.0,
            bandwidth_hz: 2_000.0,
            threshold_db: -30.0,
            reduction_db: 6.0,
            max_range_db: 10.0,
            bypassed: false,
            sample_rate: 48_000.0,
            band: Biquad::new(BiquadCoefficients::identity()),
            detector: EnvelopeFollower::new(DetectorMode::Peak),
        };
        de.reconfigure();
        de
    }

    fn reconfigure(&mut self) {
        let q = (self.center_hz / self.bandwidth_hz).max(0.1);
        self.band.set_coefficients(BiquadCoefficients::bandpass(
            self.sample_rate,
            self.center_hz,
            q,
        ));
        self.detector.set_times(0.001, 0.05, self.sample_rate);
    }
}

impl Default for DeEsser {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for DeEsser {
    fn name(&self) -> &str {
        "De-Esser"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_CENTER_HZ => self.center_hz = value.clamp(4_000.0, 9_000.0),
            PARAM_BANDWIDTH_HZ => self.bandwidth_hz = value.clamp(1_000.0, 4_000.0),
            PARAM_THRESHOLD_DB => self.threshold_db = value.clamp(-40.0, 0.0),
            PARAM_REDUCTION_DB => self.reduction_db = value.clamp(0.0, 12.0),
            PARAM_MAX_RANGE_DB => self.max_range_db = value.clamp(0.0, 20.0),
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
        let cap = self.reduction_db.min(self.max_range_db);
        for sample in buffer.iter_mut() {
            let input = *sample;
            let band = self.band.process(input);
            let env = self.detector.process(band);
            let over = gain_to_db(env) - self.threshold_db;
            let reduction = over.max(0.0).min(cap);
            let band_gain = db_to_gain(-reduction);
            *sample = input - (1.0 - band_gain) * band;
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
        self.band.reset();
        self.detector.reset();
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(20);
        for value in [
            self.center_hz,
            self.bandwidth_hz,
            self.threshold_db,
            self.reduction_db,
            self.max_range_db,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let mut idx = 0;
        for setter in [
            PARAM_CENTER_HZ,
            PARAM_BANDWIDTH_HZ,
            PARAM_THRESHOLD_DB,
            PARAM_REDUCTION_DB,
            PARAM_MAX_RANGE_DB,
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
    use lindelion_dsp_utils::analysis::windowed_dft_magnitude_at;

    fn render(freq: f32) -> (f32, f32) {
        let mut de = DeEsser::new();
        de.prepare(48_000.0, 1_024);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * freq * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        de.process(&mut buffer);
        let half = n / 2;
        (
            windowed_dft_magnitude_at(&input[half..], 48_000.0, freq),
            windowed_dft_magnitude_at(&buffer[half..], 48_000.0, freq),
        )
    }

    #[test]
    fn sibilant_band_is_reduced() {
        let (in_mag, out_mag) = render(6_000.0);
        assert!(
            out_mag < in_mag * 0.8,
            "sibilant band not reduced: {in_mag} -> {out_mag}"
        );
    }

    #[test]
    fn low_band_is_untouched() {
        let (in_mag, out_mag) = render(300.0);
        assert!(
            (out_mag - in_mag).abs() < in_mag * 0.1,
            "low band changed: {in_mag} -> {out_mag}"
        );
    }
}
