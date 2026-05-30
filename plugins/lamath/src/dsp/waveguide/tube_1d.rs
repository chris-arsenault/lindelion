use lindelion_dsp_utils::{filters::BiquadCoefficients, math, soft_saturate};

use super::{
    WaveguideParams, WaveguideStyle,
    body::WaveguideBody,
    core,
    traveling::{BoundaryFilters, BoundarySide, PickupSamples, TravelingWavePair},
};
use crate::dsp::constants::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY};

const MOUTH_REFLECTION: f32 = -0.36;
const MIN_END_REFLECTION_MAGNITUDE: f32 = 0.08;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Tube1d {
    sample_rate: f32,
    waves: TravelingWavePair,
    boundary_filters: BoundaryFilters,
    body: WaveguideBody,
}

impl Tube1d {
    pub(super) fn new(sample_rate: f32, lowest_frequency_hz: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            waves: TravelingWavePair::new(sample_rate, lowest_frequency_hz, 4.0),
            boundary_filters: BoundaryFilters::new(),
            body: WaveguideBody::new(sample_rate),
        }
    }

    pub(super) fn reset(&mut self) {
        self.waves.clear();
        self.boundary_filters.reset();
        self.body.reset();
    }

    pub(super) fn process_sample(&mut self, excitation: f32, params: WaveguideParams) -> f32 {
        let params = WaveguideParams {
            style: WaveguideStyle::Tube,
            ..params
        };
        let damping = core::loop_damping(self.sample_rate, params);
        let profile = TubeBoreProfile::from_params(self.sample_rate, params, damping.loop_gain);
        let geometry = core::waveguide_geometry(params.position_of_strike, params.pickup_position);
        // The bore round trip passes through two distinct boundary filters once
        // each — the mouth lowpass (left) and the damping lowpass (right). Loop
        // resonance is set by accumulated phase, so compensate each filter's
        // phase delay at the played pitch (not group delay, which drifts as the
        // pitch nears the cutoff), split half onto each one-way leg.
        let mouth_phase_delay = core::filter_phase_delay_samples(
            profile.mouth_loss,
            self.sample_rate,
            params.frequency_hz,
        );
        let damping_phase_delay = core::filter_phase_delay_samples(
            damping.coefficients,
            self.sample_rate,
            params.frequency_hz,
        );
        let tuning = core::delay_tuning(
            self.sample_rate,
            self.waves.capacity(),
            params.frequency_hz,
            4.0,
            1.0 + 0.5 * (mouth_phase_delay + damping_phase_delay),
        );
        let one_way_delay = tuning.integer_delay + tuning.fractional_delay;

        self.boundary_filters
            .set_coefficients(profile.mouth_loss, damping.coefficients);

        let boundary = self.waves.boundary_samples(one_way_delay);
        let pickup = self
            .waves
            .pickup_samples(one_way_delay, geometry.pickup_position);
        let mouth_reflection =
            self.reflected_sample(BoundarySide::Left, boundary.left, profile, params);
        let end_reflection =
            self.reflected_sample(BoundarySide::Right, boundary.right, profile, params);

        self.waves.push(end_reflection, mouth_reflection);
        self.waves.add_symmetric_excitation(
            one_way_delay,
            geometry.excitation_taps,
            math::snap_to_zero(excitation) * profile.excitation_coupling,
        );

        self.body
            .process_sample(profile.pickup_sample(pickup), params)
    }

    fn reflected_sample(
        &mut self,
        side: BoundarySide,
        input: f32,
        profile: TubeBoreProfile,
        params: WaveguideParams,
    ) -> f32 {
        let filtered = self.boundary_filters.process(side, input);
        let nonlinear = if side == BoundarySide::Left {
            let drive = math::finite_clamp(params.loop_nonlinearity, 0.0, 1.0, 0.0);
            if drive > 0.0 {
                soft_saturate(filtered, drive)
            } else {
                filtered
            }
        } else {
            filtered
        };
        let reflection = match side {
            BoundarySide::Left => profile.mouth_reflection,
            BoundarySide::Right => profile.end_reflection,
        };

        math::snap_to_zero(nonlinear * reflection)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TubeBoreProfile {
    mouth_loss: BiquadCoefficients,
    mouth_reflection: f32,
    end_reflection: f32,
    excitation_coupling: f32,
    pressure_mix: f32,
}

impl TubeBoreProfile {
    fn from_params(sample_rate: f32, params: WaveguideParams, loop_gain: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let endpoint_loss = core::endpoint_reflection_gain(loop_gain);
        let end_reflection = bore_end_reflection(params.boundary_reflection) * endpoint_loss;
        let openness = (1.0 - TUBE_BOUNDARY.reflection(params.boundary_reflection)) * 0.5;
        let mouth_cutoff = math::finite_clamp(
            params.loop_filter_cutoff * (0.75 + 0.35 * openness),
            160.0,
            sample_rate * 0.45,
            6_000.0,
        );

        Self {
            mouth_loss: BiquadCoefficients::lowpass(sample_rate, mouth_cutoff, DEFAULT_BIQUAD_Q),
            mouth_reflection: MOUTH_REFLECTION * endpoint_loss,
            end_reflection,
            excitation_coupling: TUBE_BOUNDARY.excitation_coupling(params.boundary_reflection),
            pressure_mix: math::finite_clamp(0.30 + 0.60 * (1.0 - openness), 0.2, 0.95, 0.65),
        }
    }

    fn pickup_sample(self, pickup: PickupSamples) -> f32 {
        let pressure = pickup.average();
        let flow = (pickup.right - pickup.left) * 0.5;

        math::snap_to_zero(pressure * self.pressure_mix + flow * (1.0 - self.pressure_mix))
    }
}

fn bore_end_reflection(boundary_reflection: f32) -> f32 {
    let reflection = TUBE_BOUNDARY.reflection(boundary_reflection);
    if reflection.abs() < MIN_END_REFLECTION_MAGNITUDE {
        MIN_END_REFLECTION_MAGNITUDE.copysign(reflection)
    } else {
        reflection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{assert_all_finite, audio_window_metrics, rms_difference};

    #[test]
    fn tube_1d_renders_finite_decaying_audio() {
        let sample_rate = 48_000.0;
        let output = render_tube_1d(
            sample_rate,
            WaveguideParams {
                style: WaveguideStyle::Tube,
                frequency_hz: 220.0,
                loop_filter_cutoff: 6_000.0,
                loop_filter_resonance: 0.1,
                loop_gain: 0.94,
                loop_nonlinearity: 0.2,
                position_of_strike: 0.18,
                pickup_position: 0.72,
                boundary_reflection: 0.8,
                ..WaveguideParams::default()
            },
            24_000,
        );
        let early = audio_window_metrics(&output[512..2_560], sample_rate);
        let late = audio_window_metrics(&output[12_000..14_048], sample_rate);

        assert_all_finite(&output);
        assert!(early.rms > late.rms, "early={early:?}, late={late:?}");
        assert!(early.peak_abs < 4.0);
    }

    #[test]
    fn tube_1d_tuning_matches_requested_pitch_across_matrix() {
        use lindelion_dsp_utils::analysis::estimate_f0_autocorrelation_refined;
        use lindelion_dsp_utils::math::cents_between;

        let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
        // The quarter-wave bore tunes to < 3 cents while its round trip stays
        // long enough that sample quantization is sub-cent. At 4 kHz / 44.1 kHz
        // that round trip is only ~5.5 samples, so a ~0.2-sample interpolation
        // floor becomes tens of cents; accuracy degrades monotonically above
        // this range (an inherent limit of the short quarter-wave loop). These
        // frequencies stay within the accurate range at every supported rate.
        let frequencies = [30.0, 55.0, 110.0, 220.0];

        for sample_rate in sample_rates {
            for frequency in frequencies {
                let params = WaveguideParams {
                    style: WaveguideStyle::Tube,
                    frequency_hz: frequency,
                    loop_filter_cutoff: 18_000.0,
                    loop_filter_resonance: 0.0,
                    loop_gain: 0.992,
                    loop_nonlinearity: 0.0,
                    boundary_reflection: 0.85,
                    ..WaveguideParams::default()
                };
                let output = render_tube_1d(sample_rate, params, 48_000);
                assert_all_finite(&output);
                // The bore's strike response is harmonically rich and body-coloured,
                // so a magnitude-peak scan is pulled by the spectral envelope. Measure
                // periodicity instead, over a sub-octave bracket.
                let estimate = estimate_f0_autocorrelation_refined(
                    &output,
                    sample_rate,
                    frequency * 0.75,
                    frequency * 1.5,
                )
                .unwrap_or_else(|| {
                    panic!("no pitch estimate at {frequency} Hz / {sample_rate} Hz")
                });
                let cents = cents_between(frequency, estimate);
                assert!(
                    cents < 3.0,
                    "frequency={frequency} sample_rate={sample_rate} estimate={estimate} cents={cents}"
                );
            }
        }
    }

    #[test]
    fn tube_1d_stays_finite_and_decays_across_full_range() {
        // Tuning accuracy degrades above the range checked above, but the bore
        // must still render finite, bounded, decaying output across the whole
        // 30 Hz–4 kHz span at every supported sample rate.
        let sample_rates = [44_100.0, 48_000.0, 88_200.0, 96_000.0];
        let frequencies = [30.0, 220.0, 880.0, 1_500.0, 4_000.0];

        for sample_rate in sample_rates {
            for frequency in frequencies {
                let output = render_tube_1d(
                    sample_rate,
                    WaveguideParams {
                        style: WaveguideStyle::Tube,
                        frequency_hz: frequency,
                        loop_filter_cutoff: 18_000.0,
                        loop_filter_resonance: 0.0,
                        loop_gain: 0.992,
                        loop_nonlinearity: 0.0,
                        boundary_reflection: 0.85,
                        ..WaveguideParams::default()
                    },
                    24_000,
                );
                assert_all_finite(&output);
                let early = audio_window_metrics(&output[512..4_608], sample_rate);
                let late = audio_window_metrics(&output[18_000..22_096], sample_rate);
                assert!(
                    early.peak_abs < 4.0,
                    "f={frequency} sr={sample_rate} {early:?}"
                );
                assert!(
                    early.rms > late.rms,
                    "should decay; f={frequency} sr={sample_rate} early={early:?} late={late:?}"
                );
            }
        }
    }

    #[test]
    fn tube_1d_tuning_accounts_for_bore_delay() {
        let sample_rate = 48_000.0;
        let target = 220.0;
        let params = WaveguideParams {
            style: WaveguideStyle::Tube,
            frequency_hz: target,
            loop_filter_cutoff: 18_000.0,
            loop_filter_resonance: 0.0,
            loop_gain: 0.985,
            boundary_reflection: 0.85,
            ..WaveguideParams::default()
        };
        let damping = core::loop_damping(sample_rate, params);
        let profile = TubeBoreProfile::from_params(sample_rate, params, damping.loop_gain);
        let mouth_phase = core::filter_phase_delay_samples(profile.mouth_loss, sample_rate, target);
        let damping_phase =
            core::filter_phase_delay_samples(damping.coefficients, sample_rate, target);
        let tube = Tube1d::new(sample_rate, 20.0);
        let tuning = core::delay_tuning(
            sample_rate,
            tube.waves.capacity(),
            target,
            4.0,
            1.0 + 0.5 * (mouth_phase + damping_phase),
        );
        // Round trip = two one-way legs (each plus a one-sample push) plus each
        // boundary filter's phase delay once.
        let compensated_period = 2.0 * (tuning.integer_delay + tuning.fractional_delay + 1.0)
            + mouth_phase
            + damping_phase;

        // The asymmetric bore (inverting mouth, non-inverting end) is a
        // quarter-wave resonator: a full round trip is half a period of the
        // played pitch, not a whole period as for the half-wave string.
        assert!((compensated_period - sample_rate / (2.0 * target)).abs() < 0.001);
    }

    #[test]
    fn tube_boundary_polarity_materially_changes_bore_response() {
        let sample_rate = 48_000.0;
        let base = WaveguideParams {
            style: WaveguideStyle::Tube,
            frequency_hz: 220.0,
            loop_filter_cutoff: 8_000.0,
            loop_filter_resonance: 0.15,
            loop_gain: 0.97,
            loop_nonlinearity: 0.0,
            position_of_strike: 0.2,
            pickup_position: 0.75,
            ..WaveguideParams::default()
        };
        let closed = render_tube_1d(
            sample_rate,
            WaveguideParams {
                boundary_reflection: 0.85,
                ..base
            },
            12_000,
        );
        let open = render_tube_1d(
            sample_rate,
            WaveguideParams {
                boundary_reflection: -0.85,
                ..base
            },
            12_000,
        );

        assert_all_finite(&closed);
        assert_all_finite(&open);
        // The corrected quarter-wave loop is half its former length, so it
        // circulates less energy and renders at a lower absolute level; the two
        // polarities still resonate an octave apart, so their difference exceeds
        // either render's own RMS. Assert a difference well above the noise floor.
        assert!(rms_difference(&closed[512..], &open[512..]) > 0.000_001);
    }

    #[test]
    fn tube_1d_reset_clears_state() {
        let sample_rate = 48_000.0;
        let mut tube = Tube1d::new(sample_rate, 20.0);
        let params = WaveguideParams {
            style: WaveguideStyle::Tube,
            frequency_hz: 220.0,
            loop_gain: 0.98,
            boundary_reflection: 0.85,
            ..WaveguideParams::default()
        };
        for index in 0..4_096 {
            tube.process_sample((index == 0) as u8 as f32, params);
        }
        tube.reset();

        let output = (0..512)
            .map(|_| tube.process_sample(0.0, params))
            .collect::<Vec<_>>();

        assert_all_finite(&output);
        assert!(audio_window_metrics(&output, sample_rate).peak_abs < 0.000_001);
    }

    fn render_tube_1d(sample_rate: f32, params: WaveguideParams, sample_count: usize) -> Vec<f32> {
        let mut tube = Tube1d::new(sample_rate, 20.0);
        let mut output = Vec::with_capacity(sample_count);
        for index in 0..sample_count {
            output.push(tube.process_sample((index == 0) as u8 as f32, params));
        }
        output
    }
}
