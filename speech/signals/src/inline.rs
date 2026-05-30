//! Inline, allocation-free analysis signals computed on the audio thread.
//!
//! Definitions from hot-mic `docs/technical/Analysis-Signals.md`:
//! - SpeechPresence — envelope follower on the signal, `clamp((envDb − (−50)) / 30)`.
//! - FricativeActivity — high-pass ~2.5 kHz energy normalized by the full-band envelope.
//! - SibilanceEnergy — band-pass ~6.5 kHz (Q≈1.2) energy normalized by the full-band envelope.

use lindelion_dsp_utils::envelope_follower::{DetectorMode, EnvelopeFollower};
use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::gain_to_db;

const FRICATIVE_HZ: f32 = 2_500.0;
const SIBILANCE_HZ: f32 = 6_500.0;
const SIBILANCE_Q: f32 = 1.2;
const NORM_EPS: f32 = 1.0e-6;

/// Smoothed speech-likelihood envelope in `0..1`.
#[derive(Debug, Clone, Copy)]
pub struct SpeechPresence {
    env: EnvelopeFollower,
}

impl SpeechPresence {
    pub fn new() -> Self {
        Self {
            env: EnvelopeFollower::new(DetectorMode::Rms),
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.env.set_times(0.005, 0.050, sample_rate);
    }

    pub fn reset(&mut self) {
        self.env.reset();
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let env_db = gain_to_db(self.env.process(input));
        ((env_db + 50.0) / 30.0).clamp(0.0, 1.0)
    }
}

impl Default for SpeechPresence {
    fn default() -> Self {
        Self::new()
    }
}

// A band envelope normalized by the full-band envelope, in `0..1`.
#[derive(Debug, Clone, Copy)]
struct NormalizedBand {
    band: Biquad,
    band_env: EnvelopeFollower,
    full_env: EnvelopeFollower,
}

impl NormalizedBand {
    fn new() -> Self {
        Self {
            band: Biquad::new(BiquadCoefficients::identity()),
            band_env: EnvelopeFollower::new(DetectorMode::Rms),
            full_env: EnvelopeFollower::new(DetectorMode::Rms),
        }
    }

    fn prepare(&mut self, coeffs: BiquadCoefficients, sample_rate: f32) {
        self.band.set_coefficients(coeffs);
        self.band_env.set_times(0.003, 0.030, sample_rate);
        self.full_env.set_times(0.003, 0.030, sample_rate);
    }

    fn reset(&mut self) {
        self.band.reset();
        self.band_env.reset();
        self.full_env.reset();
    }

    fn process(&mut self, input: f32) -> f32 {
        let band = self.band_env.process(self.band.process(input));
        let full = self.full_env.process(input);
        (band / (full + NORM_EPS)).clamp(0.0, 1.0)
    }
}

/// High-frequency aperiodic activity (fricative energy proxy) in `0..1`.
#[derive(Debug, Clone, Copy)]
pub struct FricativeActivity {
    band: NormalizedBand,
}

impl FricativeActivity {
    pub fn new() -> Self {
        Self {
            band: NormalizedBand::new(),
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.band.prepare(
            BiquadCoefficients::highpass(sample_rate, FRICATIVE_HZ, 0.707),
            sample_rate,
        );
    }

    pub fn reset(&mut self) {
        self.band.reset();
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.band.process(input)
    }
}

impl Default for FricativeActivity {
    fn default() -> Self {
        Self::new()
    }
}

/// Narrow-band sibilance energy around the sibilant band in `0..1`.
#[derive(Debug, Clone, Copy)]
pub struct SibilanceEnergy {
    band: NormalizedBand,
}

impl SibilanceEnergy {
    pub fn new() -> Self {
        Self {
            band: NormalizedBand::new(),
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.band.prepare(
            BiquadCoefficients::bandpass(sample_rate, SIBILANCE_HZ, SIBILANCE_Q),
            sample_rate,
        );
    }

    pub fn reset(&mut self) {
        self.band.reset();
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.band.process(input)
    }
}

impl Default for SibilanceEnergy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SR: f32 = 48_000.0;

    fn tone(freq: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / SR).sin())
            .collect()
    }

    fn white_noise(amp: f32, n: usize) -> Vec<f32> {
        let mut s: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                amp * ((s as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect()
    }

    fn vocal_spoken() -> (Vec<f32>, f32) {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/audio/vocal_spoken.wav");
        let decoded = lindelion_sample_library::decode_wav_mono(&path).expect("decode fixture");
        (decoded.samples, decoded.sample_rate as f32)
    }

    #[test]
    fn speech_presence_high_on_voice_low_on_silence() {
        let (clip, sr) = vocal_spoken();
        let mut presence = SpeechPresence::new();
        presence.prepare(sr);
        let mut max_during = 0.0_f32;
        for &s in &clip {
            max_during = max_during.max(presence.process(s));
        }
        // Trailing silence: presence must decay toward zero.
        let mut last = 1.0_f32;
        for _ in 0..(sr as usize) {
            last = presence.process(0.0);
        }
        assert!(max_during > 0.5, "voice presence too low: {max_during}");
        assert!(last < 0.1, "presence did not decay in silence: {last}");
    }

    #[test]
    fn fricative_higher_for_noise_than_tone() {
        let n = 8_192;
        let mut on_noise = FricativeActivity::new();
        on_noise.prepare(SR);
        let noise = white_noise(0.5, n);
        let mut fric_noise = 0.0;
        for &s in &noise {
            fric_noise = on_noise.process(s);
        }

        let mut on_tone = FricativeActivity::new();
        on_tone.prepare(SR);
        let low = tone(200.0, 0.5, n);
        let mut fric_tone = 0.0;
        for &s in &low {
            fric_tone = on_tone.process(s);
        }

        assert!(
            fric_noise > fric_tone + 0.2,
            "fricative not discriminating: noise {fric_noise} vs tone {fric_tone}"
        );
    }

    #[test]
    fn sibilance_higher_in_band_than_low() {
        let n = 8_192;
        let mut on_band = SibilanceEnergy::new();
        on_band.prepare(SR);
        let band = tone(SIBILANCE_HZ, 0.5, n);
        let mut sib_band = 0.0;
        for &s in &band {
            sib_band = on_band.process(s);
        }

        let mut on_low = SibilanceEnergy::new();
        on_low.prepare(SR);
        let low = tone(200.0, 0.5, n);
        let mut sib_low = 0.0;
        for &s in &low {
            sib_low = on_low.process(s);
        }

        assert!(
            sib_band > sib_low + 0.2,
            "sibilance not discriminating: band {sib_band} vs low {sib_low}"
        );
    }

    #[test]
    fn inline_signals_are_allocation_free() {
        let mut presence = SpeechPresence::new();
        let mut fricative = FricativeActivity::new();
        let mut sibilance = SibilanceEnergy::new();
        presence.prepare(SR);
        fricative.prepare(SR);
        sibilance.prepare(SR);
        lindelion_test_allocator::assert_no_allocations("inline signals", || {
            for i in 0..256 {
                let x = (i as f32 * 0.01).sin();
                presence.process(x);
                fricative.process(x);
                sibilance.process(x);
            }
        });
    }
}
