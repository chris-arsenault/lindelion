use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math,
};

use super::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY, core, tube::ReedTubeParams};

const TUBE_AIR_COLUMN_BODY_HZ: f32 = 280.0;
const TUBE_CLARINET_RING_HZ: f32 = 1_180.0;
const TUBE_CLARINET_RING_PARTIAL: f32 = 3.0;
const TUBE_CLARINET_RING_MIN_HZ: f32 = 760.0;
const TUBE_CLARINET_RING_MAX_HZ: f32 = 1_650.0;
const TUBE_VOICING_SHIFT_OCTAVES: f32 = 0.55;

#[derive(Debug, Clone, PartialEq)]
pub struct TubeBody {
    sample_rate: f32,
    highpass: Biquad,
    lowpass: Biquad,
    low_resonance: Biquad,
    high_resonance: Biquad,
    prepared: Option<(ReedTubeParams, BodyProfile)>,
}

impl TubeBody {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            highpass: Biquad::new(BiquadCoefficients::identity()),
            lowpass: Biquad::new(BiquadCoefficients::identity()),
            low_resonance: Biquad::new(BiquadCoefficients::identity()),
            high_resonance: Biquad::new(BiquadCoefficients::identity()),
            prepared: None,
        }
    }

    pub fn reset(&mut self) {
        self.highpass.reset();
        self.lowpass.reset();
        self.low_resonance.reset();
        self.high_resonance.reset();
        self.prepared = None;
    }

    pub fn process_sample(&mut self, input: f32, params: ReedTubeParams) -> f32 {
        let profile = self.prepared_profile(params);
        let input = math::snap_to_zero(input);
        let radiating_input = self.highpass.process(input);
        let direct = self.lowpass.process(radiating_input) * profile.direct_gain;
        let low_body = self.low_resonance.process(radiating_input) * profile.low_resonance_gain;
        let high_body = self.high_resonance.process(radiating_input) * profile.high_resonance_gain;

        math::snap_to_zero((direct + low_body + high_body) * profile.output_gain)
    }

    fn prepared_profile(&mut self, params: ReedTubeParams) -> BodyProfile {
        if let Some((cached_params, profile)) = self.prepared
            && cached_params == params
        {
            return profile;
        }

        let profile = BodyProfile::from_params(self.sample_rate, params);
        self.highpass.set_coefficients(profile.highpass);
        self.lowpass.set_coefficients(profile.lowpass);
        self.low_resonance.set_coefficients(profile.low_resonance);
        self.high_resonance.set_coefficients(profile.high_resonance);
        self.prepared = Some((params, profile));
        profile
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyProfile {
    highpass: BiquadCoefficients,
    lowpass: BiquadCoefficients,
    low_resonance: BiquadCoefficients,
    high_resonance: BiquadCoefficients,
    direct_gain: f32,
    low_resonance_gain: f32,
    high_resonance_gain: f32,
    output_gain: f32,
}

impl BodyProfile {
    fn from_params(sample_rate: f32, params: ReedTubeParams) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        let formant = unit(params.body_formant);
        let loop_cutoff = math::finite_clamp(
            params.loop_filter_cutoff_hz,
            20.0,
            sample_rate * 0.45,
            8_000.0,
        );
        let bore_cutoff =
            core::bore_hf_loss_cutoff_hz(sample_rate, params.frequency_hz, loop_cutoff);
        let radiation_cutoff = math::finite_clamp(
            bore_cutoff * (1.55 - 0.50 * formant),
            1_450.0,
            sample_rate * 0.45,
            3_000.0,
        );
        let tracked_ring = math::finite_clamp(
            params.frequency_hz * TUBE_CLARINET_RING_PARTIAL,
            TUBE_CLARINET_RING_MIN_HZ,
            TUBE_CLARINET_RING_MAX_HZ,
            TUBE_CLARINET_RING_HZ,
        );
        let ring_hz = lerp(TUBE_CLARINET_RING_HZ, tracked_ring, formant);
        let voicing = math::finite_clamp(params.body_formant_shift, -2.0, 2.0, 0.0);
        let ring_hz = math::finite_clamp(
            ring_hz * 2.0_f32.powf(voicing * TUBE_VOICING_SHIFT_OCTAVES),
            260.0,
            sample_rate * 0.45,
            ring_hz,
        );
        let ring_q =
            lerp(2.4, 5.2, formant) * math::finite_clamp(1.0 + 0.20 * voicing, 0.35, 1.80, 1.0);
        let high_gain = lerp(0.34, 4.20, formant)
            * math::finite_clamp(1.0 + 0.50 * voicing, 0.10, 2.60, 1.0);

        Self {
            highpass: BiquadCoefficients::highpass(sample_rate, 45.0, DEFAULT_BIQUAD_Q),
            lowpass: BiquadCoefficients::lowpass(sample_rate, radiation_cutoff, DEFAULT_BIQUAD_Q),
            low_resonance: BiquadCoefficients::bandpass(sample_rate, TUBE_AIR_COLUMN_BODY_HZ, 2.0),
            high_resonance: BiquadCoefficients::bandpass(sample_rate, ring_hz, ring_q),
            direct_gain: lerp(0.62, 0.08, formant),
            low_resonance_gain: lerp(0.14, 0.20, formant),
            high_resonance_gain: high_gain,
            output_gain: TUBE_BOUNDARY.output_gain(params.boundary_reflection),
        }
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
