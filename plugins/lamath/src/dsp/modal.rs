use lindelion_dsp_utils::math;

use crate::{
    ModalPreset,
    dsp::constants::{DSP_FALLBACK_SAMPLE_RATE, STRIKE_POSITION},
};

/// Soft ceiling for the modal bank output. Struck play peaks well below this
/// (~60 worst case), so normal play passes transparently; a sustained tone parked
/// on a mode frequency would otherwise ring up `∝ 1/(1-r)` (tens of thousands), and
/// this bounds it so the downstream chain stays in a sane range.
const MODAL_OUTPUT_CEILING: f32 = 256.0;

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
        let sample_rate = sanitize_sample_rate(sample_rate);
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
            let raw_frequency_hz = params.fundamental_hz * base.ratio * stretch.max(0.05);
            // `retune` keeps a stable mode set (preserving state), so out-of-band
            // partials are silenced rather than clamped onto a single near-Nyquist
            // frequency, matching how `configure` drops them.
            let out_of_band = raw_frequency_hz >= nyquist * 0.95;
            let frequency_hz = raw_frequency_hz.min(nyquist * 0.95);
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
            let gain = if out_of_band {
                0.0
            } else {
                base.gain * strike_gain * brightness_gain
            };

            mode.retune(self.sample_rate, frequency_hz, decay_seconds, gain);
        }
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let input = math::snap_to_zero(input);
        let sum = self
            .modes
            .iter_mut()
            .map(|mode| mode.process_sample(input))
            .sum::<f32>();

        // Soft ceiling limiter: transparent for struck play (peaks far below the
        // ceiling), bounds the sustained-resonance runaway (peak gain ∝ 1/(1-r)).
        let scaled = sum * self.output_scale;
        let limited = MODAL_OUTPUT_CEILING * (scaled / MODAL_OUTPUT_CEILING).tanh();
        math::snap_to_zero(limited)
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
        let sample_rate = sanitize_sample_rate(sample_rate);
        let frequency_hz = math::finite_clamp(frequency_hz, 1.0, sample_rate * 0.49, 1.0);
        let decay_seconds = math::finite_or(decay_seconds, 0.001).max(0.001);
        let radius = (-1.0 / (decay_seconds * sample_rate)).exp();
        let omega = std::f32::consts::TAU * frequency_hz / sample_rate;

        Self {
            frequency_hz,
            radius,
            coefficient: 2.0 * radius * omega.cos(),
            radius_squared: radius * radius,
            gain: math::snap_to_zero(gain),
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn retune(&mut self, sample_rate: f32, frequency_hz: f32, decay_seconds: f32, gain: f32) {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let frequency_hz = math::finite_clamp(frequency_hz, 1.0, sample_rate * 0.49, 1.0);
        let decay_seconds = math::finite_or(decay_seconds, 0.001).max(0.001);
        let radius = (-1.0 / (decay_seconds * sample_rate)).exp();
        let omega = std::f32::consts::TAU * frequency_hz / sample_rate;

        self.frequency_hz = frequency_hz;
        self.radius = radius;
        self.coefficient = 2.0 * radius * omega.cos();
        self.radius_squared = radius * radius;
        self.gain = math::snap_to_zero(gain);
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let input = math::snap_to_zero(input);
        self.y1 = math::snap_to_zero(self.y1);
        self.y2 = math::snap_to_zero(self.y2);
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

fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        DSP_FALLBACK_SAMPLE_RATE
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
        let ratio = self.special_ratios.get(index).copied().unwrap_or_else(|| {
            let power_law = (index as f32 + 1.0).powf(self.harmonicity);
            // Continue smoothly from the last tabulated ratio, but never above the
            // natural power-law series. Presets whose partials already track the
            // power law (e.g. Marimba) are unchanged; inharmonic tables that end well
            // below it (e.g. Bell) continue smoothly instead of jumping up to it.
            match self.special_ratios.last().copied() {
                Some(last) => {
                    let anchor = self.special_ratios.len() as f32;
                    power_law.min(last * ((index as f32 + 1.0) / anchor).powf(self.harmonicity))
                }
                None => power_law,
            }
        });
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

    #[test]
    fn modal_bank_output_limiter_bounds_sustained_resonance() {
        // A sustained tone parked on a mode frequency would otherwise ring up to
        // tens of thousands x (sympathetic resonance, peak gain ∝ 1/(1-r)). The soft
        // ceiling limiter on the bank output bounds it while leaving struck play
        // (peaks well below the ceiling) transparent.
        let sample_rate = 48_000.0;
        let frequency_hz = 220.0;
        let mut bank = ModalBank::new(
            sample_rate,
            ModalBankParams {
                fundamental_hz: frequency_hz,
                mode_count: 8,
                preset: ModalPreset::GenericStrike,
                inharmonicity: 0.0,
                brightness: 0.5,
                decay_global: 2.0,
                decay_tilt: 0.0,
                position_of_strike: 0.37,
            },
        );
        let total = (sample_rate * 1.0) as usize;
        let mut peak = 0.0_f32;
        for index in 0..total {
            let drive = (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate).sin();
            let output = bank.process_sample(drive);
            if index >= total / 2 {
                peak = peak.max(output.abs());
            }
        }

        assert!(peak.is_finite());
        assert!(
            peak <= MODAL_OUTPUT_CEILING * 1.02,
            "sustained resonance should be bounded by the limiter; peak={peak}"
        );
    }

    #[test]
    fn retune_silences_out_of_band_modes_without_nyquist_pileup() {
        let sample_rate = 48_000.0;
        let nyquist = sample_rate * 0.5;
        let low = ModalBankParams {
            fundamental_hz: 110.0,
            mode_count: 32,
            preset: ModalPreset::Bell,
            inharmonicity: 0.0,
            brightness: 0.5,
            decay_global: 1.0,
            decay_tilt: 0.0,
            position_of_strike: 0.37,
        };
        let mut bank = ModalBank::new(sample_rate, low);
        let mode_count_before = bank.modes().len();

        // Retune up to a high fundamental: most partials now exceed Nyquist. They
        // must be silenced, not clamped onto a single near-Nyquist frequency.
        bank.retune(ModalBankParams {
            fundamental_hz: 3_000.0,
            ..low
        });
        assert_eq!(bank.modes().len(), mode_count_before);

        let mut output = Vec::new();
        for index in 0..8192 {
            output.push(bank.process_sample(if index == 0 { 1.0 } else { 0.0 }));
        }
        assert_all_finite(&output);
        let pileup = dft_magnitude_at(&output, sample_rate, nyquist * 0.95);
        assert!(pileup < 0.01, "near-Nyquist pileup energy={pileup}");
    }

    #[test]
    fn bell_partial_series_has_no_large_gap_beyond_special_ratios() {
        // The Bell special ratios end at 3.76 (index 7); the fallback for higher
        // modes must continue smoothly rather than jumping to (index+1)^harmonicity.
        let template = template_for(ModalPreset::Bell);
        let ratios: Vec<f32> = (0..16).map(|index| template.mode(index).ratio).collect();
        for pair in ratios.windows(2) {
            assert!(pair[1] > pair[0], "ratios not monotonic: {ratios:?}");
            assert!(
                pair[1] / pair[0] < 2.0,
                "large gap in partial series: {ratios:?}"
            );
        }
    }

    #[test]
    fn modal_mode_recovers_from_non_finite_state_and_input() {
        let mut mode = ModalMode::new(48_000.0, 440.0, 1.0, 1.0);
        mode.y1 = f32::NAN;
        mode.y2 = f32::INFINITY;

        assert_eq!(mode.process_sample(f32::NAN), 0.0);
        assert!(mode.process_sample(0.25).is_finite());
    }

    /// Emits docs/plots/data/modal_impulse.csv for the ModalBank doc.
    #[test]
    fn export_modal_bank_impulse_csv() {
        use std::fs::{File, create_dir_all};
        use std::io::Write;
        use std::path::PathBuf;

        let sample_rate = 48_000.0_f32;
        let mut bank = ModalBank::new(
            sample_rate,
            ModalBankParams {
                fundamental_hz: 220.0,
                mode_count: 32,
                preset: ModalPreset::Marimba,
                inharmonicity: 0.05,
                brightness: 0.6,
                decay_global: 1.2,
                decay_tilt: 0.4,
                position_of_strike: 0.21,
            },
        );

        let n_samples = 16_384_usize;
        let mut output = Vec::with_capacity(n_samples);
        output.push(bank.process_sample(1.0));
        for _ in 1..n_samples {
            output.push(bank.process_sample(0.0));
        }

        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("plugins dir")
            .parent()
            .expect("workspace root")
            .join("docs")
            .join("plots")
            .join("data");
        create_dir_all(&dir).expect("create data dir");

        let mut file = File::create(dir.join("modal_impulse.csv")).expect("create csv");
        writeln!(file, "time_s,value").unwrap();
        for (i, v) in output.iter().enumerate() {
            let t = i as f32 / sample_rate;
            writeln!(file, "{:.6},{:.6}", t, v).unwrap();
        }
    }
}
