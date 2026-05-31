//! Allocation-free streaming STFT processor (weighted overlap-add).
//!
//! Frames the input with a sqrt-Hann analysis window, transforms with realfft, hands the spectrum
//! to a caller-supplied closure, inverse-transforms, applies a sqrt-Hann synthesis window, and
//! overlap-adds. All buffers and FFT scratch are preallocated, so [`StftProcessor::process`] does
//! not allocate (uses realfft `process_with_scratch`). Latency equals the frame size. This is a
//! new build, not a lift: `pitch-shift/spectral.rs` and `onset-detect/spectral_flux.rs` plan-
//! allocate a `RealFftPlanner` per call.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::{ola, window};

/// Streaming weighted-overlap-add STFT processor.
pub struct StftProcessor {
    frame_size: usize,
    hop: usize,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    win: Vec<f32>,
    norm: f32,
    in_ring: Vec<f32>,
    in_pos: usize,
    hop_countdown: usize,
    frame: Vec<f32>,
    spectrum: Vec<Complex32>,
    fwd_scratch: Vec<Complex32>,
    inv_scratch: Vec<Complex32>,
    ifft: Vec<f32>,
    ola: Vec<f32>,
    out: Vec<f32>,
    out_read: usize,
    out_write: usize,
}

impl StftProcessor {
    /// Create a processor with the given `frame_size` (power of two) and 75% overlap.
    pub fn new(frame_size: usize) -> Self {
        let hop = frame_size / 4;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(frame_size);
        let inverse = planner.plan_fft_inverse(frame_size);

        let win: Vec<f32> = (0..frame_size)
            .map(|i| window::sqrt_hann_f64(i, frame_size) as f32)
            .collect();
        // Exact COLA constant for the sqrt-Hann (squared = Hann) overlap at this hop.
        let win64: Vec<f64> = win.iter().map(|&w| w as f64).collect();
        let cola = ola::steady_state_squared_window_sum(&win64, hop)[frame_size / 2].max(1e-9);
        // iFFT is unnormalized (scales by frame_size); the windowed overlap-add sums to `cola`.
        let norm = 1.0 / (frame_size as f32 * cola as f32);

        let spectrum = forward.make_output_vec();
        let fwd_scratch = forward.make_scratch_vec();
        let inv_scratch = inverse.make_scratch_vec();
        // Output FIFO: primed with `frame_size` zeros (the latency) plus headroom for a block.
        let out_capacity = frame_size * 8;
        let out = vec![0.0_f32; out_capacity];
        // Prime `hop` zeros so the read pointer trails the first flush, giving latency = frame_size.
        let out_write = hop;

        Self {
            frame_size,
            hop,
            forward,
            inverse,
            win,
            norm,
            in_ring: vec![0.0; frame_size],
            in_pos: 0,
            hop_countdown: hop,
            frame: vec![0.0; frame_size],
            spectrum,
            fwd_scratch,
            inv_scratch,
            ifft: vec![0.0; frame_size],
            ola: vec![0.0; frame_size],
            out,
            out_read: 0,
            out_write,
        }
    }

    /// Processing latency in samples (equal to the frame size).
    pub fn latency_samples(&self) -> usize {
        self.frame_size
    }

    /// Clear all state and re-prime the output latency.
    pub fn reset(&mut self) {
        self.in_ring.iter_mut().for_each(|s| *s = 0.0);
        self.in_pos = 0;
        self.hop_countdown = self.hop;
        self.ola.iter_mut().for_each(|s| *s = 0.0);
        self.out.iter_mut().for_each(|s| *s = 0.0);
        self.out_read = 0;
        self.out_write = self.hop;
    }

    /// Process a block in place. `frame_fn` receives each analysis spectrum (length
    /// `frame_size/2 + 1`) to modify before resynthesis. Allocation-free.
    pub fn process(&mut self, buffer: &mut [f32], mut frame_fn: impl FnMut(&mut [Complex32])) {
        for sample in buffer.iter_mut() {
            self.in_ring[self.in_pos] = *sample;
            self.in_pos = (self.in_pos + 1) % self.frame_size;
            self.hop_countdown -= 1;
            if self.hop_countdown == 0 {
                self.hop_countdown = self.hop;
                self.process_frame(&mut frame_fn);
            }
            *sample = self.out[self.out_read];
            self.out[self.out_read] = 0.0;
            self.out_read = (self.out_read + 1) % self.out.len();
        }
    }

    fn process_frame(&mut self, frame_fn: &mut impl FnMut(&mut [Complex32])) {
        // Read the frame oldest-to-newest, apply the analysis window.
        for i in 0..self.frame_size {
            let idx = (self.in_pos + i) % self.frame_size;
            self.frame[i] = self.in_ring[idx] * self.win[i];
        }
        let _ = self.forward.process_with_scratch(
            &mut self.frame,
            &mut self.spectrum,
            &mut self.fwd_scratch,
        );
        frame_fn(&mut self.spectrum);
        let _ = self.inverse.process_with_scratch(
            &mut self.spectrum,
            &mut self.ifft,
            &mut self.inv_scratch,
        );
        // Synthesis window + normalize, overlap-add into the accumulator.
        for i in 0..self.frame_size {
            self.ola[i] += self.ifft[i] * self.win[i] * self.norm;
        }
        // Flush the first `hop` finished samples to the output FIFO; slide the accumulator.
        for i in 0..self.hop {
            self.out[self.out_write] = self.ola[i];
            self.out_write = (self.out_write + 1) % self.out.len();
        }
        self.ola.copy_within(self.hop..self.frame_size, 0);
        for s in self.ola[self.frame_size - self.hop..].iter_mut() {
            *s = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_round_trips_with_frame_latency() {
        let frame = 1_024;
        let mut stft = StftProcessor::new(frame);
        let n = 8_192;
        let input: Vec<f32> = (0..n)
            .map(|i| 0.5 * (std::f32::consts::TAU * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        let mut buffer = input.clone();
        stft.process(&mut buffer, |_spectrum| {});
        // Output equals the input delayed by the latency, after the priming region.
        let latency = stft.latency_samples();
        let mut err = 0.0_f64;
        let mut energy = 0.0_f64;
        let compare_from = latency + frame; // skip the initial ramp-in
        for i in compare_from..n {
            let diff = (buffer[i] - input[i - latency]) as f64;
            err += diff * diff;
            energy += (input[i - latency] as f64).powi(2);
        }
        let rel = (err / energy.max(1e-12)).sqrt();
        assert!(rel < 0.02, "round-trip relative error too high: {rel}");
    }

    #[test]
    fn process_is_allocation_free() {
        let mut stft = StftProcessor::new(1_024);
        let mut buffer = vec![0.1_f32; 512];
        // Warm up outside the measured region.
        stft.process(&mut buffer, |_s| {});
        lindelion_test_allocator::assert_no_allocations("stft process", || {
            stft.process(&mut buffer, |_s| {});
        });
    }
}
