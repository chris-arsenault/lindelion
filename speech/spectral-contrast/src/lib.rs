//! Spectral Contrast: raise spectral peak-to-valley contrast, gated by speech presence.
//!
//! Ports hot-mic's `Enhance-Spectral-Contrast.md`. Per bin, a local spectral-envelope average is
//! computed; bins above it (peaks) are boosted and bins below it (valleys) attenuated by raising
//! the ratio to a contrast exponent. The amount is scaled by SpeechPresence (computed inline), so
//! contrast is applied on speech and relaxed in silence. Runs in the shared allocation-free STFT.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::stft::StftProcessor;
use lindelion_effect::{Effect, EffectParam};
use lindelion_speech_signals::SpeechPresence;

pub const PARAM_AMOUNT_PCT: u32 = 0;

const FRAME_SIZE: usize = 1_024;
const BINS: usize = FRAME_SIZE / 2 + 1;
const ENVELOPE_RADIUS: usize = 16;
const MAX_EXPONENT: f32 = 0.6;

const PARAMS: &[EffectParam] = &[EffectParam {
    index: PARAM_AMOUNT_PCT,
    name: "Amount",
    min: 0.0,
    max: 100.0,
    default: 50.0,
    unit: "%",
}];

/// Presence-gated spectral contrast enhancement.
pub struct SpectralContrast {
    amount_pct: f32,
    bypassed: bool,
    stft: StftProcessor,
    presence: SpeechPresence,
    last_presence: f32,
    mag: Vec<f32>,
}

impl SpectralContrast {
    pub fn new() -> Self {
        Self {
            amount_pct: 50.0,
            bypassed: false,
            stft: StftProcessor::new(FRAME_SIZE),
            presence: SpeechPresence::new(),
            last_presence: 0.0,
            mag: vec![0.0; BINS],
        }
    }

    fn exponent(&self) -> f32 {
        (self.amount_pct / 100.0) * MAX_EXPONENT
    }
}

impl Default for SpectralContrast {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for SpectralContrast {
    fn name(&self) -> &str {
        "Spectral Contrast"
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
        self.stft.reset();
        self.presence.prepare(sample_rate);
        self.last_presence = 0.0;
        self.mag.iter_mut().for_each(|m| *m = 0.0);
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed {
            return;
        }
        let mut presence = self.last_presence;
        for &sample in buffer.iter() {
            presence = self.presence.process(sample);
        }
        self.last_presence = presence;
        let exponent = self.exponent() * presence;

        let Self { stft, mag, .. } = self;
        stft.process(buffer, |spectrum| {
            for (i, bin) in spectrum.iter().enumerate() {
                mag[i] = bin.norm();
            }
            for (i, bin) in spectrum.iter_mut().enumerate() {
                let lo = i.saturating_sub(ENVELOPE_RADIUS);
                let hi = (i + ENVELOPE_RADIUS).min(BINS - 1);
                let mut sum = 0.0;
                for &m in &mag[lo..=hi] {
                    sum += m;
                }
                let avg = sum / (hi - lo + 1) as f32;
                let gain = (mag[i] / (avg + 1.0e-9)).powf(exponent);
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
        self.presence.reset();
        self.last_presence = 0.0;
        self.mag.iter_mut().for_each(|m| *m = 0.0);
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

    fn noise(amp: f32, n: usize) -> Vec<f32> {
        let mut s: u32 = 0x2468_ACE0;
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
    fn raises_peak_to_valley_contrast() {
        let mut effect = SpectralContrast::new();
        effect.set_parameter(PARAM_AMOUNT_PCT, 100.0);
        effect.prepare(48_000.0, 1_024);
        let n = 32_768;
        // Harmonic peaks at 400/600/800 Hz over a broadband noise valley floor.
        let mut input = noise(0.05, n);
        for (i, s) in input.iter_mut().enumerate() {
            let t = std::f32::consts::TAU * i as f32 / 48_000.0;
            *s += 0.3 * (400.0 * t).sin() + 0.3 * (600.0 * t).sin() + 0.3 * (800.0 * t).sin();
        }
        let mut buffer = input.clone();
        effect.process(&mut buffer);
        let tail = n / 2;
        let peak = |sig: &[f32]| windowed_dft_magnitude_at(sig, 48_000.0, 600.0);
        let valley = |sig: &[f32]| windowed_dft_magnitude_at(sig, 48_000.0, 500.0);
        let contrast_in = peak(&input[tail..]) / valley(&input[tail..]).max(f32::MIN_POSITIVE);
        let contrast_out = peak(&buffer[tail..]) / valley(&buffer[tail..]).max(f32::MIN_POSITIVE);
        assert!(
            contrast_out > contrast_in * 1.1,
            "contrast not raised: {contrast_in} -> {contrast_out}"
        );
    }
}
