//! Air Exciter: keyed high-frequency exciter, de-ess-aware.
//!
//! Ports hot-mic's `Enhance-Air-Exciter.md`. A band-passed copy of the signal (4–8 kHz) is
//! soft-clipped to generate high-frequency harmonics ("air") and blended back in. The
//! gain-normalized `soft_clip` keeps the tap at unity small-signal gain, so Amount adds at most a
//! bounded (≤ +6 dB) presence shelf plus the harmonics — the un-normalized shaper multiplied the
//! band by the drive (~4×, a ~+14 dB shelf rather than excitation). The low-pass leg of the band
//! limit keeps the shaper's dominant (3rd-order) products under Nyquist at 48 kHz; without it,
//! harmonics of content above ~8 kHz fold back as inharmonic fizz. The excitation is keyed down
//! by SibilanceEnergy so it backs off on sibilants (avoids harsh esses). Reuses dsp-utils filters
//! + saturation + the inline sibilance signal.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::saturation::soft_clip;
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::SibilanceEnergy;

pub const PARAM_AMOUNT_PCT: u32 = 0;

const AIR_HPF_HZ: f32 = 4_000.0;
/// Upper edge of the excitation band: 3rd-order products of 8 kHz land at the 48 kHz Nyquist.
const AIR_LPF_HZ: f32 = 8_000.0;
const DRIVE: f32 = 4.0;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 40.0,
    unit: "%",
}];

/// Excitation blend gain for a given amount and sibilance activity (keyed down by sibilance).
pub fn excitation_gain(amount: f32, sibilance: f32) -> f32 {
    amount.max(0.0) * (1.0 - sibilance.clamp(0.0, 1.0))
}

/// De-ess-aware air exciter.
pub struct AirExciter {
    amount_pct: f32,
    bypassed: bool,
    sample_rate: f32,
    hpf: Biquad,
    lpf: Biquad,
    sibilance: SibilanceEnergy,
}

impl AirExciter {
    pub fn new() -> Self {
        Self {
            amount_pct: 40.0,
            bypassed: false,
            sample_rate: 48_000.0,
            hpf: Biquad::new(BiquadCoefficients::identity()),
            lpf: Biquad::new(BiquadCoefficients::identity()),
            sibilance: SibilanceEnergy::new(),
        }
    }

    fn reconfigure(&mut self) {
        self.hpf.set_coefficients(BiquadCoefficients::highpass(
            self.sample_rate,
            AIR_HPF_HZ,
            0.707,
        ));
        self.lpf.set_coefficients(BiquadCoefficients::lowpass(
            self.sample_rate,
            AIR_LPF_HZ,
            0.707,
        ));
        self.sibilance.prepare(self.sample_rate);
    }
}

impl Default for AirExciter {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for AirExciter {
    fn name(&self) -> &str {
        "Air Exciter"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        if index == PARAM_AMOUNT_PCT {
            self.amount_pct = value.clamp(0.0, 100.0);
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.reconfigure();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let amount = self.amount_pct / 100.0;
        for sample in buffer.iter_mut() {
            let dry = *sample;
            let sibilance = self.sibilance.process(dry);
            let band = self.lpf.process(self.hpf.process(dry));
            let air = soft_clip(band, DRIVE, 0.0);
            *sample = dry + excitation_gain(amount, sibilance) * air;
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
        self.lpf.reset();
        self.sibilance.reset();
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
    use lindelion_dsp_utils::analysis::windowed_dft_magnitude_at;

    #[test]
    fn keyed_down_by_sibilance() {
        assert!(excitation_gain(1.0, 0.0) > excitation_gain(1.0, 1.0));
        assert_eq!(excitation_gain(1.0, 1.0), 0.0);
    }

    #[test]
    fn adds_high_frequency_harmonics() {
        let mut effect = AirExciter::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 4_500.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let h3_in = windowed_dft_magnitude_at(&input, 48_000.0, 13_500.0);
        let h3_out = windowed_dft_magnitude_at(&buffer, 48_000.0, 13_500.0);
        // Threshold calibrated to the gain-normalized shaper: its harmonics-only tap sits ~4×
        // below the old un-normalized tap (which also carried a linear-band level boost).
        assert!(
            h3_out > h3_in + 0.002,
            "no HF harmonics added: {h3_in} -> {h3_out}"
        );
    }

    #[test]
    fn band_boost_is_bounded_by_amount() {
        // Regression: the excitation tap must have unity small-signal gain, so Amount 100 % can
        // raise the in-band content by at most ~2x (dry + the compressed band copy). The
        // un-normalized shaper multiplied the band by the drive, shelving it ~3x and more.
        let mut effect = AirExciter::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.4 * (std::f32::consts::TAU * 4_500.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let f_in = windowed_dft_magnitude_at(&input, 48_000.0, 4_500.0);
        let f_out = windowed_dft_magnitude_at(&buffer, 48_000.0, 4_500.0);
        assert!(
            f_out < f_in * 2.0,
            "band shelved beyond the unity-gain bound: {f_in} -> {f_out}"
        );
        assert!(f_out > f_in * 1.1, "no presence added: {f_in} -> {f_out}");
    }
}
