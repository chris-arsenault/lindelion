//! FFT Noise Removal: spectral subtraction against a per-bin noise estimate.
//!
//! Ports hot-mic's `FFTNoiseRemovalPlugin` (`Cleanup.md`). A per-bin noise estimate tracks the
//! lower magnitude envelope (falls moderately toward the floor, rises slowly so steady tones are
//! not learned as noise) and is subtracted from each frame's magnitude; phase is kept. Runs in
//! the shared allocation-free STFT.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::stft::StftProcessor;
use lindelion_effect::{Effect, EffectParam};

pub const PARAM_AMOUNT_PCT: u32 = 0;

const FRAME_SIZE: usize = 1_024;
const NOISE_FALL: f32 = 0.05; // track downward toward the noise floor at a moderate rate
const NOISE_RISE: f32 = 0.0005; // adapt upward slowly so steady tones are not learned as noise
const GAIN_FLOOR: f32 = 0.1;
const MAX_REDUCTION: f32 = 3.0;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 60.0,
    unit: "%",
}];

/// Spectral-subtraction noise removal.
pub struct FftNoiseRemoval {
    amount_pct: f32,
    bypassed: bool,
    stft: StftProcessor,
    noise_mag: Vec<f32>,
}

impl FftNoiseRemoval {
    pub fn new() -> Self {
        Self {
            amount_pct: 60.0,
            bypassed: false,
            stft: StftProcessor::new(FRAME_SIZE),
            noise_mag: vec![0.0; FRAME_SIZE / 2 + 1],
        }
    }

    fn reduction(&self) -> f32 {
        (self.amount_pct / 100.0) * MAX_REDUCTION
    }
}

impl Default for FftNoiseRemoval {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for FftNoiseRemoval {
    fn name(&self) -> &str {
        "FFT Noise Removal"
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
        self.noise_mag.iter_mut().for_each(|n| *n = 0.0);
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let reduction = self.reduction();
        let Self {
            stft, noise_mag, ..
        } = self;
        stft.process(buffer, |spectrum| {
            for (bin, noise) in spectrum.iter_mut().zip(noise_mag.iter_mut()) {
                let mag = bin.norm();
                if *noise <= 0.0 {
                    *noise = mag; // seed the estimate on the first frame
                } else {
                    let alpha = if mag < *noise { NOISE_FALL } else { NOISE_RISE };
                    *noise += alpha * (mag - *noise);
                }
                let gain = ((mag - reduction * *noise) / (mag + 1.0e-9)).max(GAIN_FLOOR);
                *bin *= gain;
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
        self.noise_mag.iter_mut().for_each(|n| *n = 0.0);
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
    use lindelion_dsp_utils::analysis::{rms, windowed_dft_magnitude_at};

    fn noise(amp: f32, n: usize) -> Vec<f32> {
        let mut s: u32 = 0x1357_9BDF;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                amp * ((s as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn reduces_broadband_noise() {
        let mut effect = FftNoiseRemoval::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let input = noise(0.2, 32_768);
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let tail = input.len() / 2;
        assert!(
            rms(&buffer[tail..]) < rms(&input[tail..]) * 0.85,
            "noise not reduced: {} -> {}",
            rms(&input[tail..]),
            rms(&buffer[tail..])
        );
    }

    #[test]
    fn preserves_high_snr_tone() {
        let mut effect = FftNoiseRemoval::new();
        effect.prepare(48_000.0, 1_024);
        let n = 32_768;
        let mut input = noise(0.02, n);
        // Tone only in the second half, after the noise floor has been learned.
        for (i, s) in input.iter_mut().enumerate().skip(n / 2) {
            *s += 0.4 * (std::f32::consts::TAU * 1_000.0 * i as f32 / 48_000.0).sin();
        }
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let tail = n / 2;
        let in_mag = windowed_dft_magnitude_at(&input[tail..], 48_000.0, 1_000.0);
        let out_mag = windowed_dft_magnitude_at(&buffer[tail..], 48_000.0, 1_000.0);
        assert!(
            out_mag > in_mag * 0.7,
            "tone not preserved: {in_mag} -> {out_mag}"
        );
    }
}
