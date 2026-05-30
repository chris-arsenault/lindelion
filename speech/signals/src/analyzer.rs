//! Off-thread analysis: the heavy signals (SwiftF0 voicing + FFT-based flux/HNR).
//!
//! This runs in the analysis worker, never on the audio thread, so it may allocate. Voicing
//! comes straight off SwiftF0's `PitchFrame`; broadband and high-band spectral flux come from the
//! onset-detect flux engine; HNR is `−10·log10(spectral flatness)` over a windowed frame.

use std::sync::Arc;

use lindelion_dsp_utils::filters::{Biquad, BiquadCoefficients};
use lindelion_dsp_utils::window;
use lindelion_onset_detect::StreamingSpectralFlux;
use lindelion_pitch_detect::{
    PitchDetectionConfig, StreamingPitchTracker, SwiftF0StreamingPitchTracker,
};
use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

const FRAME: usize = 1_024;
const HOP: usize = 256;
const HIGH_BAND_HZ: f32 = 2_000.0;
const SILENCE_RMS: f32 = 0.001;
const HNR_CLAMP_DB: f32 = 120.0;

/// The latest value of every heavy analysis signal.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SignalSnapshot {
    pub pitch_hz: f32,
    pub pitch_confidence: f32,
    pub voicing_score: f32,
    /// 0 = silence, 1 = unvoiced, 2 = voiced.
    pub voicing_state: f32,
    pub onset_flux_high: f32,
    pub spectral_flux: f32,
    pub hnr_db: f32,
}

/// Computes the heavy signals from blocks of source-rate audio. Allocates; off-thread only.
pub struct SignalAnalyzer {
    pitch: SwiftF0StreamingPitchTracker,
    flux: StreamingSpectralFlux,
    flux_high: StreamingSpectralFlux,
    high_pass: Biquad,
    fft: Arc<dyn RealToComplex<f32>>,
    fft_input: Vec<f32>,
    fft_output: Vec<Complex32>,
    hp_scratch: Vec<f32>,
    hnr_buffer: Vec<f32>,
    last: SignalSnapshot,
}

impl SignalAnalyzer {
    pub fn new(source_sample_rate: u32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FRAME);
        let fft_input = fft.make_input_vec();
        let fft_output = fft.make_output_vec();
        Self {
            pitch: SwiftF0StreamingPitchTracker::new(
                source_sample_rate,
                PitchDetectionConfig::default(),
            ),
            flux: StreamingSpectralFlux::new(FRAME, HOP, 1),
            flux_high: StreamingSpectralFlux::new(FRAME, HOP, 1),
            high_pass: Biquad::new(BiquadCoefficients::highpass(
                source_sample_rate as f32,
                HIGH_BAND_HZ,
                0.707,
            )),
            fft,
            fft_input,
            fft_output,
            hp_scratch: Vec::with_capacity(FRAME * 4),
            hnr_buffer: Vec::with_capacity(FRAME * 2),
            last: SignalSnapshot::default(),
        }
    }

    /// Consume a block of source-rate audio and return the updated snapshot.
    pub fn process(&mut self, block: &[f32]) -> SignalSnapshot {
        if let Ok(frames) = self.pitch.next_block(block)
            && let Some(frame) = frames.last()
        {
            self.last.pitch_hz = frame.f0_hz.unwrap_or(0.0);
            self.last.pitch_confidence = frame.confidence;
            self.last.voicing_score = frame.confidence;
            self.last.voicing_state = if frame.rms < SILENCE_RMS {
                0.0
            } else if frame.voiced {
                2.0
            } else {
                1.0
            };
        }

        if let Some(frame) = self.flux.next_block(block).last() {
            self.last.spectral_flux = frame.flux;
        }

        self.hp_scratch.clear();
        for &sample in block {
            self.hp_scratch.push(self.high_pass.process(sample));
        }
        if let Some(frame) = self.flux_high.next_block(&self.hp_scratch).last() {
            self.last.onset_flux_high = frame.flux;
        }

        self.hnr_buffer.extend_from_slice(block);
        if self.hnr_buffer.len() > FRAME {
            let excess = self.hnr_buffer.len() - FRAME;
            self.hnr_buffer.drain(0..excess);
        }
        if self.hnr_buffer.len() == FRAME {
            self.last.hnr_db = self.compute_hnr();
        }

        self.last
    }

    fn compute_hnr(&mut self) -> f32 {
        for (i, slot) in self.fft_input.iter_mut().enumerate() {
            *slot = self.hnr_buffer[i] * window::hann(i, FRAME);
        }
        if self
            .fft
            .process(&mut self.fft_input, &mut self.fft_output)
            .is_err()
        {
            return self.last.hnr_db;
        }
        let bins = self.fft_output.len() as f32;
        let mut log_sum = 0.0;
        let mut lin_sum = 0.0;
        for bin in &self.fft_output {
            let magnitude = bin.norm() + 1.0e-9;
            log_sum += magnitude.ln();
            lin_sum += magnitude;
        }
        let geometric = (log_sum / bins).exp();
        let arithmetic = lin_sum / bins + 1.0e-9;
        let flatness = (geometric / arithmetic).clamp(1.0e-9, 1.0);
        (-10.0 * flatness.log10()).clamp(-HNR_CLAMP_DB, HNR_CLAMP_DB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn voiced_tone(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = std::f32::consts::TAU * i as f32 / SR as f32;
                0.5 * (150.0 * t).sin() + 0.25 * (300.0 * t).sin() + 0.12 * (450.0 * t).sin()
            })
            .collect()
    }

    fn noise(n: usize) -> Vec<f32> {
        let mut s: u32 = 0x9E37_79B9;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                0.5 * ((s as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn voiced_tone_reads_voiced_and_pitched() {
        let mut analyzer = SignalAnalyzer::new(SR);
        let snapshot = analyzer.process(&voiced_tone(16_384));
        assert_eq!(snapshot.voicing_state, 2.0, "expected voiced");
        assert!(
            (120.0..180.0).contains(&snapshot.pitch_hz),
            "pitch off: {}",
            snapshot.pitch_hz
        );
        assert!(snapshot.pitch_confidence > 0.4, "low confidence");
    }

    #[test]
    fn silence_reads_silence_state() {
        let mut analyzer = SignalAnalyzer::new(SR);
        let snapshot = analyzer.process(&vec![0.0_f32; 16_384]);
        assert_eq!(snapshot.voicing_state, 0.0, "expected silence");
    }

    #[test]
    fn hnr_higher_for_tone_than_noise() {
        let mut on_tone = SignalAnalyzer::new(SR);
        let hnr_tone = on_tone.process(&voiced_tone(16_384)).hnr_db;
        let mut on_noise = SignalAnalyzer::new(SR);
        let hnr_noise = on_noise.process(&noise(16_384)).hnr_db;
        assert!(
            hnr_tone > hnr_noise + 3.0,
            "HNR not discriminating: tone {hnr_tone} vs noise {hnr_noise}"
        );
    }

    #[test]
    fn flux_responds_to_onset() {
        let mut analyzer = SignalAnalyzer::new(SR);
        let mut max_flux = 0.0_f32;
        analyzer.process(&vec![0.0_f32; 8_192]);
        for chunk in noise(8_192).chunks(256) {
            max_flux = max_flux.max(analyzer.process(chunk).onset_flux_high);
        }
        assert!(
            max_flux > 0.0 && max_flux.is_finite(),
            "no flux on onset: {max_flux}"
        );
    }
}
