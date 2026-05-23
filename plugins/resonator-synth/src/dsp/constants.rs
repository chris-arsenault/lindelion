#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloatRange {
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) default: f32,
}

impl FloatRange {
    pub(crate) const fn new(min: f32, max: f32, default: f32) -> Self {
        Self { min, max, default }
    }

    pub(crate) fn clamp(self, value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(self.min, self.max)
        } else {
            self.default
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResonanceQ {
    pub(crate) base: f32,
    pub(crate) scale: f32,
}

impl ResonanceQ {
    pub(crate) const fn new(base: f32, scale: f32) -> Self {
        Self { base, scale }
    }

    pub(crate) fn from_resonance(self, resonance: f32) -> f32 {
        self.base + FILTER_RESONANCE.clamp(resonance) * self.scale
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TubeBoundaryModel {
    pub(crate) reflection: FloatRange,
    pub(crate) excitation_loss_per_reflection: f32,
    pub(crate) min_excitation_gain: f32,
    pub(crate) output_base_gain: f32,
    pub(crate) output_reflection_gain: f32,
}

impl TubeBoundaryModel {
    pub(crate) const fn new(
        reflection: FloatRange,
        excitation_loss_per_reflection: f32,
        min_excitation_gain: f32,
        output_base_gain: f32,
        output_reflection_gain: f32,
    ) -> Self {
        Self {
            reflection,
            excitation_loss_per_reflection,
            min_excitation_gain,
            output_base_gain,
            output_reflection_gain,
        }
    }

    pub(crate) fn reflection(self, value: f32) -> f32 {
        self.reflection.clamp(value)
    }

    pub(crate) fn feedback_gain(self, value: f32) -> f32 {
        self.reflection(value)
    }

    pub(crate) fn excitation_gain(self, value: f32) -> f32 {
        let reflection = self.reflection(value).abs();
        (1.0 - reflection * self.excitation_loss_per_reflection)
            .clamp(self.min_excitation_gain, 1.0)
    }

    pub(crate) fn output_gain(self, value: f32) -> f32 {
        let reflection = self.reflection(value).abs();
        self.output_base_gain + reflection * self.output_reflection_gain
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SeriesConditionerParams {
    pub(crate) highpass_cutoff_hz: f32,
    pub(crate) highpass_q: f32,
    pub(crate) fast_env_alpha: f32,
    pub(crate) slow_env_alpha: f32,
    pub(crate) envelope_epsilon: f32,
    pub(crate) output_base_gain: f32,
    pub(crate) output_transient_gain: f32,
}

impl SeriesConditionerParams {
    pub(crate) const fn new(
        highpass_cutoff_hz: f32,
        highpass_q: f32,
        fast_env_alpha: f32,
        slow_env_alpha: f32,
        envelope_epsilon: f32,
        output_base_gain: f32,
        output_transient_gain: f32,
    ) -> Self {
        Self {
            highpass_cutoff_hz,
            highpass_q,
            fast_env_alpha,
            slow_env_alpha,
            envelope_epsilon,
            output_base_gain,
            output_transient_gain,
        }
    }

    pub(crate) fn next_fast_env(self, current: f32, magnitude: f32) -> f32 {
        current + self.fast_env_alpha * (magnitude - current)
    }

    pub(crate) fn next_slow_env(self, current: f32, magnitude: f32) -> f32 {
        current + self.slow_env_alpha * (magnitude - current)
    }

    pub(crate) fn transient_bias(self, fast_env: f32, slow_env: f32) -> f32 {
        ((fast_env - slow_env) / (fast_env + self.envelope_epsilon)).clamp(0.0, 1.0)
    }

    pub(crate) fn output_gain(self, transient_bias: f32) -> f32 {
        self.output_base_gain + transient_bias * self.output_transient_gain
    }
}

pub(crate) const DSP_FALLBACK_SAMPLE_RATE: f32 = 48_000.0;
pub(crate) const LOWEST_RESONATOR_FREQUENCY_HZ: f32 = 20.0;

pub(crate) const DEFAULT_BIQUAD_Q: f32 = 0.707;

pub(crate) const MASTER_GAIN_DB: FloatRange = FloatRange::new(-60.0, 12.0, 0.0);
pub(crate) const MASTER_GAIN_LINEAR: FloatRange = FloatRange::new(0.001, 3.981_071_7, 1.0);
pub(crate) const OUTPUT_FILTER_CUTOFF_HZ: FloatRange = FloatRange::new(20.0, 20_000.0, 20_000.0);
pub(crate) const WAVEGUIDE_LOOP_FILTER_CUTOFF_HZ: FloatRange =
    FloatRange::new(20.0, 20_000.0, 8_000.0);
pub(crate) const FILTER_RESONANCE: FloatRange = FloatRange::new(0.0, 0.999, 0.0);
pub(crate) const WAVEGUIDE_LOOP_GAIN: FloatRange = FloatRange::new(0.0, 0.999, 0.92);
pub(crate) const STRIKE_POSITION: FloatRange = FloatRange::new(0.001, 0.999, 0.5);

pub(crate) const OUTPUT_FILTER_Q: ResonanceQ = ResonanceQ::new(DEFAULT_BIQUAD_Q, 8.0);
pub(crate) const WAVEGUIDE_LOOP_FILTER_Q: ResonanceQ = ResonanceQ::new(0.55, 4.0);
pub(crate) const WAVEGUIDE_RESONANCE_GAIN_COMPENSATION_DEPTH: f32 = 0.45;

pub(crate) const FILTER_CUTOFF_MOD_OCTAVES: f32 = 4.0;
pub(crate) const MODAL_DAMPING_MOD_OCTAVES: f32 = 2.0;
pub(crate) const RESONATOR_POSITION_MOD_DEPTH: f32 = 0.5;
pub(crate) const WAVEGUIDE_DAMPING_MOD_DEPTH: f32 = 0.25;

pub(crate) const TUBE_BOUNDARY: TubeBoundaryModel =
    TubeBoundaryModel::new(FloatRange::new(-1.0, 1.0, 0.75), 0.25, 0.5, 0.8, 0.2);

pub(crate) const SERIES_CONDITIONER: SeriesConditionerParams =
    SeriesConditionerParams::new(80.0, DEFAULT_BIQUAD_Q, 0.01, 0.000_2, 1.0e-6, 0.04, 0.96);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_model_numerics_are_pinned() {
        assert_eq!(TUBE_BOUNDARY.reflection(f32::NAN), 0.75);
        assert_eq!(TUBE_BOUNDARY.feedback_gain(2.0), 1.0);
        assert_eq!(TUBE_BOUNDARY.excitation_gain(0.75), 0.8125);
        assert!((TUBE_BOUNDARY.output_gain(0.75) - 0.95).abs() < 0.000_001);
    }

    #[test]
    fn series_conditioner_envelope_numerics_are_pinned() {
        let fast = SERIES_CONDITIONER.next_fast_env(0.0, 1.0);
        let slow = SERIES_CONDITIONER.next_slow_env(0.0, 1.0);
        let bias = SERIES_CONDITIONER.transient_bias(fast, slow);
        let gain = SERIES_CONDITIONER.output_gain(bias);

        assert!((fast - 0.01).abs() < 0.000_001);
        assert!((slow - 0.000_2).abs() < 0.000_001);
        assert!((bias - 0.979_902).abs() < 0.000_001);
        assert!((gain - 0.980_706).abs() < 0.000_001);
    }
}
