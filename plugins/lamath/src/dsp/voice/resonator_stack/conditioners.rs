//! Series conditioner and body-color exciter — the signal-conditioning helpers for
//! the Series and BodyColor resonator routings. Split out of `resonator_stack.rs` to
//! keep each file within the 600-line size cap; behavior is unchanged.

use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math::{finite_clamp, finite_or, snap_to_zero},
};

use crate::dsp::constants::{DSP_FALLBACK_SAMPLE_RATE, SERIES_CONDITIONER};

const BODY_COLOR_WINDOW_MS: f32 = 35.0;
const BODY_COLOR_RETRIGGER_MS: f32 = 80.0;
const BODY_COLOR_TRIGGER_THRESHOLD: f32 = 1.0e-5;
const BODY_COLOR_TRIGGER_RATIO: f32 = 1.5;
const BODY_COLOR_EXCITATION_GAIN: f32 = 0.006;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SeriesConditioner {
    highpass: Biquad,
    pub(crate) fast_env: f32,
    pub(crate) slow_env: f32,
}

impl SeriesConditioner {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            highpass: Biquad::new(BiquadCoefficients::highpass(
                sample_rate,
                SERIES_CONDITIONER.highpass_cutoff_hz,
                SERIES_CONDITIONER.highpass_q,
            )),
            fast_env: 0.0,
            slow_env: 0.0,
        }
    }

    pub(crate) fn reset(&mut self, sample_rate: f32) {
        self.highpass.set_coefficients(BiquadCoefficients::highpass(
            sample_rate,
            SERIES_CONDITIONER.highpass_cutoff_hz,
            SERIES_CONDITIONER.highpass_q,
        ));
        self.highpass.reset();
        self.fast_env = 0.0;
        self.slow_env = 0.0;
    }

    pub(crate) fn process_sample(&mut self, input: f32) -> f32 {
        let highpassed = snap_to_zero(self.highpass.process(input));
        let magnitude = highpassed.abs();

        self.fast_env =
            snap_to_zero(SERIES_CONDITIONER.next_fast_env(snap_to_zero(self.fast_env), magnitude));
        self.slow_env =
            snap_to_zero(SERIES_CONDITIONER.next_slow_env(snap_to_zero(self.slow_env), magnitude));

        let transient_bias = SERIES_CONDITIONER.transient_bias(self.fast_env, self.slow_env);
        snap_to_zero(highpassed * SERIES_CONDITIONER.output_gain(transient_bias))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BodyColorExciter {
    pub(crate) window_env: f32,
    pub(crate) trigger_peak: f32,
    window_decay: f32,
    trigger_decay: f32,
}

impl BodyColorExciter {
    pub(crate) fn new(sample_rate: f32) -> Self {
        let mut exciter = Self {
            window_env: 0.0,
            trigger_peak: 0.0,
            window_decay: 0.0,
            trigger_decay: 0.0,
        };
        exciter.reset(sample_rate);
        exciter
    }

    pub(crate) fn reset(&mut self, sample_rate: f32) {
        self.window_env = 0.0;
        self.trigger_peak = 0.0;
        self.window_decay = decay_to_floor(sample_rate, BODY_COLOR_WINDOW_MS);
        self.trigger_decay = decay_to_floor(sample_rate, BODY_COLOR_RETRIGGER_MS);
    }

    pub(crate) fn process_sample(&mut self, excitation: f32, color_sample: f32) -> f32 {
        let excitation = snap_to_zero(excitation);
        let color_sample = snap_to_zero(color_sample);
        self.window_env = finite_clamp(self.window_env, 0.0, 1.0, 0.0);
        self.trigger_peak = snap_to_zero(self.trigger_peak).max(0.0);
        let magnitude = excitation.abs();
        let trigger_threshold =
            BODY_COLOR_TRIGGER_THRESHOLD.max(self.trigger_peak * BODY_COLOR_TRIGGER_RATIO);
        if magnitude > trigger_threshold {
            self.window_env = 1.0;
        }
        self.trigger_peak = self.trigger_peak.max(magnitude) * self.trigger_decay;

        let colored = color_sample * self.window_env * BODY_COLOR_EXCITATION_GAIN;
        self.window_env *= self.window_decay;
        if self.window_env < BODY_COLOR_TRIGGER_THRESHOLD {
            self.window_env = 0.0;
        }

        snap_to_zero(colored)
    }
}

fn decay_to_floor(sample_rate: f32, duration_ms: f32) -> f32 {
    let sample_rate = finite_or(sample_rate, DSP_FALLBACK_SAMPLE_RATE);
    let duration_ms = finite_or(duration_ms, 0.0).max(0.0);
    let samples = (sample_rate * duration_ms * 0.001).max(1.0);
    0.001_f32.powf(1.0 / samples)
}
