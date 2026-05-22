use ahara_dsp_utils::{delay::DelayLine, filters::OnePoleLowpass, math, soft_saturate};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveguideParams {
    pub frequency_hz: f32,
    pub loop_filter_cutoff: f32,
    pub loop_gain: f32,
    pub loop_nonlinearity: f32,
    pub position_of_strike: f32,
}

impl Default for WaveguideParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            loop_filter_cutoff: 8_000.0,
            loop_gain: 0.92,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveguideResonator {
    sample_rate: f32,
    delay: DelayLine,
    loop_filter: OnePoleLowpass,
}

impl WaveguideResonator {
    pub fn new(sample_rate: f32, lowest_frequency_hz: f32) -> Self {
        let max_delay = (sample_rate / lowest_frequency_hz.max(1.0)).ceil() as usize + 8;
        Self {
            sample_rate,
            delay: DelayLine::new(max_delay),
            loop_filter: OnePoleLowpass::new(8_000.0, sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.loop_filter.reset();
    }

    pub fn process_sample(&mut self, excitation: f32, params: WaveguideParams) -> f32 {
        let frequency_hz = params.frequency_hz.clamp(
            self.sample_rate / self.delay.capacity() as f32,
            self.sample_rate * 0.45,
        );
        let delay_samples =
            (self.sample_rate / frequency_hz - 1.0).clamp(1.0, self.delay.capacity() as f32 - 3.0);

        self.loop_filter
            .set_cutoff(params.loop_filter_cutoff, self.sample_rate);

        let delayed = self.delay.read(delay_samples);
        let damped = self.loop_filter.process(delayed);
        let nonlinear = if params.loop_nonlinearity > 0.0 {
            soft_saturate(damped, params.loop_nonlinearity)
        } else {
            damped
        };

        let strike_gain = (std::f32::consts::PI * params.position_of_strike.clamp(0.001, 0.999))
            .sin()
            .abs()
            .max(0.05);
        let feedback = nonlinear * params.loop_gain.clamp(0.0, 0.999);
        self.delay
            .push(math::snap_to_zero(feedback + excitation * strike_gain));

        math::snap_to_zero(delayed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahara_dsp_utils::analysis::{
        assert_all_finite, estimate_frequency_zero_crossings, peak_abs, rms,
    };

    #[test]
    fn impulse_produces_decaying_output() {
        let mut waveguide = WaveguideResonator::new(48_000.0, 20.0);
        let params = WaveguideParams {
            frequency_hz: 240.0,
            loop_filter_cutoff: 12_000.0,
            loop_gain: 0.95,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
        };
        let mut output = Vec::new();

        for index in 0..8_000 {
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        assert_all_finite(&output);
        assert!(rms(&output[500..2_000]) > rms(&output[6_000..]));
    }

    #[test]
    fn impulse_frequency_tracks_delay_length() {
        let sample_rate = 48_000.0;
        let target = 440.0;
        let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
        let params = WaveguideParams {
            frequency_hz: target,
            loop_filter_cutoff: 20_000.0,
            loop_gain: 0.99,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.5,
        };
        let mut output = Vec::new();

        for index in 0..10_000 {
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        let estimate = estimate_frequency_zero_crossings(&output[500..], sample_rate).unwrap();
        assert!((estimate - target).abs() < 12.0, "estimate={estimate}");
    }

    #[test]
    fn stable_across_parameter_sweep() {
        let sample_rate = 48_000.0;
        let mut waveguide = WaveguideResonator::new(sample_rate, 20.0);
        let mut output = Vec::new();

        for index in 0..20_000 {
            let t = index as f32 / 19_999.0;
            let params = WaveguideParams {
                frequency_hz: 40.0 + t * 4_000.0,
                loop_filter_cutoff: 200.0 + t * 18_000.0,
                loop_gain: 0.2 + t * 0.799,
                loop_nonlinearity: t,
                position_of_strike: 0.1 + 0.8 * t,
            };
            output.push(waveguide.process_sample(if index == 0 { 1.0 } else { 0.0 }, params));
        }

        assert_all_finite(&output);
        assert!(peak_abs(&output) < 10.0);
    }
}
