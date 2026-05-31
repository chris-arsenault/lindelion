use std::f32::consts::PI;

use lindelion_dsp_utils::math::finite_or;

/// Half-band FIR half-order: taps span `[-HALF, HALF]`, so the filter is
/// `2*HALF + 1` long with a group delay of `HALF` samples at its own clock.
/// Small enough for low latency, long enough to reject the upper half-band.
const HALF: usize = 8;
const LEN: usize = 2 * HALF + 1;

/// Symmetric linear-phase half-band FIR low-pass with cutoff at a quarter of its
/// own sample rate (`Fs/4`). At the 2x clock that is the host Nyquist, so it is
/// the interpolation/decimation filter for a 2x oversampler. Fixed-size and
/// allocation-free; coefficients computed once at construction.
#[derive(Debug, Clone)]
pub(super) struct HalfbandFir {
    coefficients: [f32; LEN],
    state: [f32; LEN],
    write: usize,
}

impl HalfbandFir {
    /// Build the windowed-sinc half-band. `gain` scales the DC-normalized taps:
    /// `1.0` for the decimator, `2.0` for the interpolator (to restore the level
    /// halved by zero-stuffing).
    pub(super) fn new(gain: f32) -> Self {
        let mut coefficients = [0.0_f32; LEN];
        let mut dc_sum = 0.0_f32;
        for (index, coefficient) in coefficients.iter_mut().enumerate() {
            let n = index as isize - HALF as isize;
            // Ideal half-band low-pass: h[n] = 0.5 * sinc(n/2); even n (!=0) are
            // the half-band zeros, odd n are +/-1/(pi*n), n==0 is 0.5.
            let ideal = if n == 0 {
                0.5
            } else {
                (PI * n as f32 / 2.0).sin() / (PI * n as f32)
            };
            let window = blackman(index);
            let tap = ideal * window;
            *coefficient = tap;
            dc_sum += tap;
        }

        // Normalize to unity DC gain, then apply the interpolation/decimation gain.
        let normalize = if dc_sum.abs() > f32::EPSILON {
            gain / dc_sum
        } else {
            gain
        };
        for coefficient in coefficients.iter_mut() {
            *coefficient *= normalize;
        }

        Self {
            coefficients,
            state: [0.0; LEN],
            write: 0,
        }
    }

    pub(super) fn reset(&mut self) {
        self.state = [0.0; LEN];
        self.write = 0;
    }

    /// Convolve one input sample. `y[m] = sum_k coefficients[k] * x[m-k]`.
    pub(super) fn process(&mut self, input: f32) -> f32 {
        self.state[self.write] = finite_or(input, 0.0);
        let mut accumulator = 0.0_f32;
        let mut index = self.write;
        for coefficient in self.coefficients.iter() {
            accumulator += coefficient * self.state[index];
            index = if index == 0 { LEN - 1 } else { index - 1 };
        }
        self.write = (self.write + 1) % LEN;
        accumulator
    }
}

impl Default for Oversampler2x {
    fn default() -> Self {
        Self::new()
    }
}

fn blackman(index: usize) -> f32 {
    let denominator = (LEN - 1) as f32;
    let phase = 2.0 * PI * index as f32 / denominator;
    0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos()
}

/// Reusable, allocation-free 2x oversampling stage (ADR-0016): upsample one host
/// sample to two via a half-band interpolator, run the core twice at the doubled
/// clock, then half-band decimate back to one host sample. The wrapper is
/// identity-equivalent within filter tolerance when the core is linear, so it is
/// the shared substrate for the dynamic-response nonlinear stages (M4–M6).
///
/// Candidate extraction (ADR-0003): single consumer today, kept local in Lamath.
#[derive(Debug, Clone)]
pub struct Oversampler2x {
    interpolator: HalfbandFir,
    decimator: HalfbandFir,
}

impl Oversampler2x {
    /// Fixed latency the wrapper adds, in host samples: the interpolator and
    /// decimator each contribute `HALF` samples of group delay at the 2x clock,
    /// so `2*HALF` at 2x equals `HALF` host samples. Reported to the host for
    /// plugin-delay compensation.
    pub const LATENCY_SAMPLES: u32 = HALF as u32;

    pub fn new() -> Self {
        Self {
            // The interpolator carries 2x gain to restore the level halved by
            // zero-stuffing; the decimator is unity.
            interpolator: HalfbandFir::new(2.0),
            decimator: HalfbandFir::new(1.0),
        }
    }

    pub fn reset(&mut self) {
        self.interpolator.reset();
        self.decimator.reset();
    }

    /// Run `core` at twice the host rate for one host sample.
    pub fn process(&mut self, input: f32, mut core: impl FnMut(f32) -> f32) -> f32 {
        // Upsample: the real sample then a stuffed zero, half-band interpolated.
        let high = self.interpolator.process(input);
        let low = self.interpolator.process(0.0);
        // Core at 2x.
        let high = core(high);
        let low = core(low);
        // Decimate: filter both phases, keep one host sample.
        self.decimator.process(high);
        self.decimator.process(low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_no_allocations;
    use lindelion_dsp_utils::analysis::{gain_fitted_rms_difference, rms};

    #[test]
    fn passes_dc_at_unity_gain() {
        let mut filter = HalfbandFir::new(1.0);
        let mut output = 0.0;
        for _ in 0..LEN * 4 {
            output = filter.process(1.0);
        }
        assert!((output - 1.0).abs() < 1.0e-4, "dc gain output={output}");
    }

    #[test]
    fn rejects_nyquist_tone() {
        let mut filter = HalfbandFir::new(1.0);
        // Alternating +/-1 is the 2x-clock Nyquist tone, deep in the stop-band.
        let mut peak = 0.0_f32;
        for index in 0..LEN * 4 {
            let input = if index % 2 == 0 { 1.0 } else { -1.0 };
            let output = filter.process(input);
            if index >= LEN {
                peak = peak.max(output.abs());
            }
        }
        assert!(peak < 0.05, "nyquist leakage peak={peak}");
    }

    #[test]
    fn reset_clears_state() {
        let mut filter = HalfbandFir::new(1.0);
        for _ in 0..LEN {
            filter.process(0.9);
        }
        filter.reset();
        assert_eq!(filter.process(0.0), 0.0);
    }

    #[test]
    fn process_does_not_allocate() {
        let mut filter = HalfbandFir::new(2.0);
        assert_no_allocations("halfband_process", || {
            for index in 0..512 {
                filter.process((index % 2) as f32 * 0.5 - 0.25);
            }
        });
    }

    #[test]
    fn identity_core_reproduces_held_input() {
        let mut oversampler = Oversampler2x::new();
        let mut output = 0.0;
        for _ in 0..LEN * 8 {
            output = oversampler.process(0.5, |x| x);
        }
        assert!(
            (output - 0.5).abs() < 1.0e-3,
            "transparency output={output}"
        );
    }

    #[test]
    fn linear_core_matches_base_rate_within_tolerance() {
        // A one-pole low-pass driven well inside the passband should render the
        // same through the oversampler (core run at 2x) as at the base rate.
        let signal: Vec<f32> = (0..2_048)
            .map(|index| (2.0 * PI * 4.0 * index as f32 / 2_048.0).sin() * 0.5)
            .collect();

        let base_pole = 0.2_f32;
        let mut base_state = 0.0_f32;
        let base: Vec<f32> = signal
            .iter()
            .map(|&x| {
                base_state += base_pole * (x - base_state);
                base_state
            })
            .collect();

        // The same time constant at 2x uses half the per-sample coefficient.
        let mut oversampler = Oversampler2x::new();
        let mut over_state = 0.0_f32;
        let over: Vec<f32> = signal
            .iter()
            .map(|&x| {
                oversampler.process(x, |s| {
                    over_state += (base_pole * 0.5) * (s - over_state);
                    over_state
                })
            })
            .collect();

        // Compare gain-fitted tails (skip the filter fill / latency region).
        let reference = &base[LEN..];
        let rendered = &over[LEN..];
        let difference = gain_fitted_rms_difference(reference, rendered);
        let normalization = rms(rendered).max(1.0e-6);
        assert!(
            difference / normalization < 0.1,
            "normalized difference={}",
            difference / normalization
        );
    }

    #[test]
    fn reported_latency_is_nonzero_and_surfaced_to_host() {
        const { assert!(Oversampler2x::LATENCY_SAMPLES > 0) };
        assert_eq!(
            crate::dsp::RESONATOR_OVERSAMPLING_LATENCY_SAMPLES,
            Oversampler2x::LATENCY_SAMPLES
        );
    }

    #[test]
    fn reset_clears_oversampler_state() {
        let mut oversampler = Oversampler2x::new();
        for _ in 0..LEN {
            oversampler.process(0.7, |x| x);
        }
        oversampler.reset();
        assert_eq!(oversampler.process(0.0, |x| x), 0.0);
    }

    #[test]
    fn oversampler_process_does_not_allocate() {
        let mut oversampler = Oversampler2x::new();
        let mut state = 0.0_f32;
        assert_no_allocations("oversampler_process", || {
            for index in 0..512 {
                oversampler.process((index % 2) as f32 * 0.5 - 0.25, |x| {
                    state += 0.1 * (x - state);
                    state
                });
            }
        });
    }
}
