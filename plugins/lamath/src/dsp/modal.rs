use lindelion_dsp_utils::math;

use crate::{ModalPreset, dsp::constants::STRIKE_POSITION};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalBankParams {
    pub fundamental_hz: f32,
    pub mode_count: usize,
    pub preset: ModalPreset,
    pub inharmonicity: f32,
    pub brightness: f32,
    pub decay_global: f32,
    pub decay_tilt: f32,
    pub position_of_strike: f32,
}

impl Default for ModalBankParams {
    fn default() -> Self {
        Self {
            fundamental_hz: 220.0,
            mode_count: 64,
            preset: ModalPreset::Marimba,
            inharmonicity: 0.0,
            brightness: 0.5,
            decay_global: 1.0,
            decay_tilt: 0.5,
            position_of_strike: STRIKE_POSITION.default,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModalBank {
    sample_rate: f32,
    modes: Vec<ModalMode>,
    output_scale: f32,
}

impl ModalBank {
    #[cfg(test)]
    pub fn new(sample_rate: f32, params: ModalBankParams) -> Self {
        Self::with_capacity(sample_rate, params.mode_count.clamp(1, 256), params)
    }

    pub fn with_capacity(sample_rate: f32, max_modes: usize, params: ModalBankParams) -> Self {
        let mut bank = Self {
            sample_rate,
            modes: Vec::with_capacity(max_modes.clamp(1, 256)),
            output_scale: 1.0,
        };
        bank.configure(params);
        bank
    }

    pub fn configure(&mut self, params: ModalBankParams) {
        self.modes.clear();

        let mode_count = params.mode_count.clamp(1, 256);
        if self.modes.capacity() < mode_count {
            self.modes.reserve_exact(mode_count - self.modes.capacity());
        }
        let nyquist = self.sample_rate * 0.5;
        let brightness = params.brightness.clamp(0.0, 1.0);
        let position = STRIKE_POSITION.clamp(params.position_of_strike);
        let template = template_for(params.preset);

        for index in 0..mode_count {
            let base = template.mode(index);
            let stretch = 1.0 + params.inharmonicity * (index as f32 / mode_count as f32).powi(2);
            let frequency_hz = params.fundamental_hz * base.ratio * stretch.max(0.05);

            if frequency_hz >= nyquist * 0.95 {
                continue;
            }

            let normalized_index = if mode_count == 1 {
                0.0
            } else {
                index as f32 / (mode_count - 1) as f32
            };
            let strike_gain = (std::f32::consts::PI * (index as f32 + 1.0) * position)
                .sin()
                .abs();
            let brightness_gain = 10.0_f32.powf((brightness - 0.5) * normalized_index * 2.0);
            let decay_tilt = 1.0 - params.decay_tilt.clamp(0.0, 1.0) * normalized_index * 0.75;
            let decay_seconds =
                base.decay_seconds * params.decay_global.max(0.01) * decay_tilt.max(0.05);
            let gain = base.gain * strike_gain * brightness_gain;

            self.modes.push(ModalMode::new(
                self.sample_rate,
                frequency_hz,
                decay_seconds,
                gain,
            ));
        }

        self.output_scale = if self.modes.is_empty() {
            1.0
        } else {
            1.0 / self.modes.len() as f32
        };
    }

    pub fn retune(&mut self, params: ModalBankParams) {
        let mode_count = params.mode_count.clamp(1, 256);
        let nyquist = self.sample_rate * 0.5;
        let brightness = params.brightness.clamp(0.0, 1.0);
        let position = STRIKE_POSITION.clamp(params.position_of_strike);
        let template = template_for(params.preset);

        for (index, mode) in self.modes.iter_mut().enumerate() {
            let base = template.mode(index);
            let stretch = 1.0 + params.inharmonicity * (index as f32 / mode_count as f32).powi(2);
            let frequency_hz =
                (params.fundamental_hz * base.ratio * stretch.max(0.05)).min(nyquist * 0.95);
            let normalized_index = if mode_count == 1 {
                0.0
            } else {
                index as f32 / (mode_count - 1) as f32
            };
            let strike_gain = (std::f32::consts::PI * (index as f32 + 1.0) * position)
                .sin()
                .abs();
            let brightness_gain = 10.0_f32.powf((brightness - 0.5) * normalized_index * 2.0);
            let decay_tilt = 1.0 - params.decay_tilt.clamp(0.0, 1.0) * normalized_index * 0.75;
            let decay_seconds =
                base.decay_seconds * params.decay_global.max(0.01) * decay_tilt.max(0.05);
            let gain = base.gain * strike_gain * brightness_gain;

            mode.retune(self.sample_rate, frequency_hz, decay_seconds, gain);
        }
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let sum = self
            .modes
            .iter_mut()
            .map(|mode| mode.process_sample(input))
            .sum::<f32>();

        math::snap_to_zero(sum * self.output_scale)
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.reset();
        }
    }

    pub fn modes(&self) -> &[ModalMode] {
        &self.modes
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalMode {
    frequency_hz: f32,
    radius: f32,
    coefficient: f32,
    radius_squared: f32,
    gain: f32,
    y1: f32,
    y2: f32,
}

impl ModalMode {
    pub fn new(sample_rate: f32, frequency_hz: f32, decay_seconds: f32, gain: f32) -> Self {
        let frequency_hz = frequency_hz.clamp(1.0, sample_rate * 0.49);
        let decay_seconds = decay_seconds.max(0.001);
        let radius = (-1.0 / (decay_seconds * sample_rate)).exp();
        let omega = std::f32::consts::TAU * frequency_hz / sample_rate;

        Self {
            frequency_hz,
            radius,
            coefficient: 2.0 * radius * omega.cos(),
            radius_squared: radius * radius,
            gain,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn retune(&mut self, sample_rate: f32, frequency_hz: f32, decay_seconds: f32, gain: f32) {
        let frequency_hz = frequency_hz.clamp(1.0, sample_rate * 0.49);
        let decay_seconds = decay_seconds.max(0.001);
        let radius = (-1.0 / (decay_seconds * sample_rate)).exp();
        let omega = std::f32::consts::TAU * frequency_hz / sample_rate;

        self.frequency_hz = frequency_hz;
        self.radius = radius;
        self.coefficient = 2.0 * radius * omega.cos();
        self.radius_squared = radius * radius;
        self.gain = gain;
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let output = input * self.gain + self.coefficient * self.y1 - self.radius_squared * self.y2;
        self.y2 = self.y1;
        self.y1 = math::snap_to_zero(output);
        self.y1
    }

    pub fn reset(&mut self) {
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
struct Template {
    harmonicity: f32,
    decay_seconds: f32,
    decay_power: f32,
    gain_power: f32,
    special_ratios: &'static [f32],
}

impl Template {
    fn mode(self, index: usize) -> TemplateMode {
        let ratio = self
            .special_ratios
            .get(index)
            .copied()
            .unwrap_or_else(|| (index as f32 + 1.0).powf(self.harmonicity));
        let normalized = index as f32 + 1.0;

        TemplateMode {
            ratio,
            decay_seconds: self.decay_seconds / normalized.powf(self.decay_power),
            gain: 1.0 / normalized.powf(self.gain_power),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TemplateMode {
    ratio: f32,
    decay_seconds: f32,
    gain: f32,
}

fn template_for(preset: ModalPreset) -> Template {
    match preset {
        ModalPreset::Kalimba => Template {
            harmonicity: 1.35,
            decay_seconds: 1.4,
            decay_power: 0.7,
            gain_power: 0.85,
            special_ratios: &[1.0, 2.76, 5.4, 8.93, 13.34],
        },
        ModalPreset::Marimba => Template {
            harmonicity: 1.9,
            decay_seconds: 0.9,
            decay_power: 0.8,
            gain_power: 1.0,
            special_ratios: &[1.0, 3.99, 10.65],
        },
        ModalPreset::Bell => Template {
            harmonicity: 1.45,
            decay_seconds: 3.5,
            decay_power: 0.45,
            gain_power: 0.75,
            special_ratios: &[0.56, 0.92, 1.19, 1.71, 2.0, 2.74, 3.0, 3.76],
        },
        ModalPreset::GlassBowl => Template {
            harmonicity: 1.18,
            decay_seconds: 4.0,
            decay_power: 0.35,
            gain_power: 0.8,
            special_ratios: &[1.0, 1.72, 2.41, 3.16, 4.02, 5.35],
        },
        ModalPreset::MetalBar => Template {
            harmonicity: 1.75,
            decay_seconds: 2.2,
            decay_power: 0.5,
            gain_power: 0.8,
            special_ratios: &[1.0, 2.76, 5.4, 8.93],
        },
        ModalPreset::Woodblock => Template {
            harmonicity: 2.2,
            decay_seconds: 0.25,
            decay_power: 1.0,
            gain_power: 0.9,
            special_ratios: &[1.0, 2.1, 3.9, 6.8],
        },
        ModalPreset::GenericStrike => Template {
            harmonicity: 1.0,
            decay_seconds: 1.0,
            decay_power: 0.75,
            gain_power: 1.0,
            special_ratios: &[],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{
        assert_all_finite, dft_magnitude_at, estimate_frequency_zero_crossings, peak_abs, rms,
    };

    #[test]
    fn single_mode_impulse_rings_near_configured_frequency() {
        let sample_rate = 48_000.0;
        let mut mode = ModalMode::new(sample_rate, 440.0, 1.0, 1.0);
        let mut output = Vec::new();

        for index in 0..8192 {
            output.push(mode.process_sample(if index == 0 { 1.0 } else { 0.0 }));
        }

        assert_all_finite(&output);
        let estimate = estimate_frequency_zero_crossings(&output[64..], sample_rate).unwrap();
        assert!((estimate - 440.0).abs() < 5.0, "estimate={estimate}");
    }

    #[test]
    fn modal_bank_has_energy_at_fundamental() {
        let sample_rate = 48_000.0;
        let mut bank = ModalBank::new(
            sample_rate,
            ModalBankParams {
                fundamental_hz: 220.0,
                mode_count: 8,
                preset: ModalPreset::GenericStrike,
                ..Default::default()
            },
        );
        let mut output = Vec::new();

        for index in 0..8192 {
            output.push(bank.process_sample(if index == 0 { 1.0 } else { 0.0 }));
        }

        assert_all_finite(&output);
        assert!(dft_magnitude_at(&output, sample_rate, 220.0) > 0.000_5);
        assert!(rms(&output) > 0.000_1);
    }

    #[test]
    fn modal_bank_stays_finite_across_parameter_sweep() {
        let sample_rate = 48_000.0;

        for fundamental_hz in [40.0, 110.0, 440.0, 1_760.0, 4_000.0] {
            let mut bank = ModalBank::new(
                sample_rate,
                ModalBankParams {
                    fundamental_hz,
                    mode_count: 64,
                    preset: ModalPreset::Bell,
                    inharmonicity: 0.8,
                    brightness: 1.0,
                    decay_global: 2.0,
                    decay_tilt: 0.0,
                    position_of_strike: 0.37,
                },
            );
            let mut output = Vec::new();

            for index in 0..4096 {
                output.push(bank.process_sample(if index == 0 { 1.0 } else { 0.0 }));
            }

            assert_all_finite(&output);
            assert!(peak_abs(&output) < 20.0);
        }
    }
}
