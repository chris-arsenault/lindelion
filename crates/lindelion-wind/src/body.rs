use lindelion_dsp_utils::{
    filters::{Biquad, BiquadCoefficients},
    math,
};

use super::{DEFAULT_BIQUAD_Q, TUBE_BOUNDARY, core, tube::ReedTubeParams};

const TUBE_AIR_COLUMN_BODY_HZ: f32 = 190.0;
const TUBE_CLARINET_RING_HZ: f32 = 1_180.0;
const TUBE_CLARINET_RING_PARTIAL: f32 = 3.0;
const TUBE_CLARINET_RING_MIN_HZ: f32 = 360.0;
const TUBE_CLARINET_RING_MAX_HZ: f32 = 1_650.0;
const TUBE_CLARINET_UPPER_RING_PARTIAL: f32 = 5.0;
const TUBE_CLARINET_UPPER_RING_MIN_HZ: f32 = 650.0;
const TUBE_CLARINET_UPPER_RING_MAX_HZ: f32 = 2_400.0;
const TUBE_CLARINET_EDGE_RING_PARTIAL: f32 = 7.0;
const TUBE_CLARINET_EDGE_RING_MIN_HZ: f32 = 850.0;
const TUBE_CLARINET_EDGE_RING_MAX_HZ: f32 = 3_100.0;
const TUBE_CLARINET_TAIL9_RING_PARTIAL: f32 = 9.0;
const TUBE_CLARINET_TAIL11_RING_PARTIAL: f32 = 11.0;
const TUBE_CLARINET_TAIL13_RING_PARTIAL: f32 = 13.0;
const TUBE_REGISTER_MODE_THRESHOLD: f32 = 1.5;
const TUBE_VOICING_SHIFT_OCTAVES: f32 = 0.55;
const TUBE_EVEN_MODE_REJECTION_Q: f32 = 12.0;
const TUBE_BODY_REACTION_LIMIT: f32 = 0.42;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TubeBodySample {
    pub output: f32,
    pub reaction_flow: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TubeBody {
    sample_rate: f32,
    highpass: Biquad,
    lowpass: Biquad,
    even_h2_rejection: Biquad,
    even_h4_rejection: Biquad,
    low_resonance: Biquad,
    high_resonance: Biquad,
    upper_resonance: Biquad,
    edge_resonance: Biquad,
    tail9_resonance: Biquad,
    tail11_resonance: Biquad,
    tail13_resonance: Biquad,
    prepared: Option<(ReedTubeParams, BodyProfile)>,
}

impl TubeBody {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = core::sanitize_sample_rate(sample_rate);
        Self {
            sample_rate,
            highpass: Biquad::new(BiquadCoefficients::identity()),
            lowpass: Biquad::new(BiquadCoefficients::identity()),
            even_h2_rejection: Biquad::new(BiquadCoefficients::identity()),
            even_h4_rejection: Biquad::new(BiquadCoefficients::identity()),
            low_resonance: Biquad::new(BiquadCoefficients::identity()),
            high_resonance: Biquad::new(BiquadCoefficients::identity()),
            upper_resonance: Biquad::new(BiquadCoefficients::identity()),
            edge_resonance: Biquad::new(BiquadCoefficients::identity()),
            tail9_resonance: Biquad::new(BiquadCoefficients::identity()),
            tail11_resonance: Biquad::new(BiquadCoefficients::identity()),
            tail13_resonance: Biquad::new(BiquadCoefficients::identity()),
            prepared: None,
        }
    }

    pub fn reset(&mut self) {
        self.highpass.reset();
        self.lowpass.reset();
        self.even_h2_rejection.reset();
        self.even_h4_rejection.reset();
        self.low_resonance.reset();
        self.high_resonance.reset();
        self.upper_resonance.reset();
        self.edge_resonance.reset();
        self.tail9_resonance.reset();
        self.tail11_resonance.reset();
        self.tail13_resonance.reset();
        self.prepared = None;
    }

    pub fn process_sample(
        &mut self,
        input: f32,
        coupling_position: f32,
        params: ReedTubeParams,
    ) -> TubeBodySample {
        let profile = self.prepared_profile(params);
        let input = math::snap_to_zero(input);
        let coupling_position = math::finite_clamp(coupling_position, 0.0, 1.0, 0.5);
        let radiating_input = self.highpass.process(input);
        let odd_admittance = self
            .even_h4_rejection
            .process(self.even_h2_rejection.process(radiating_input));
        let direct = self.lowpass.process(odd_admittance) * profile.direct_gain;
        let low_mode = self.low_resonance.process(odd_admittance);
        let high_mode = self.high_resonance.process(odd_admittance);
        let upper_mode = self.upper_resonance.process(odd_admittance);
        let edge_mode = self.edge_resonance.process(odd_admittance);
        let tail9_mode = self.tail9_resonance.process(odd_admittance);
        let tail11_mode = self.tail11_resonance.process(odd_admittance);
        let tail13_mode = self.tail13_resonance.process(odd_admittance);

        let low_body = low_mode * profile.low_resonance_gain;
        let high_body = high_mode * profile.high_resonance_gain;
        let upper_body = upper_mode * profile.upper_resonance_gain;
        let edge_body = edge_mode * profile.edge_resonance_gain;
        let tail_body = tail9_mode * profile.tail9_resonance_gain
            + tail11_mode * profile.tail11_resonance_gain
            + tail13_mode * profile.tail13_resonance_gain;
        let modal_body = low_body + high_body + upper_body + edge_body + tail_body;
        let modal_reaction = low_body * 0.35
            + high_body * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_RING_PARTIAL)
            + upper_body
                * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_UPPER_RING_PARTIAL)
            + edge_body
                * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_EDGE_RING_PARTIAL)
            + tail9_mode
                * profile.tail9_resonance_gain
                * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_TAIL9_RING_PARTIAL)
            + tail11_mode
                * profile.tail11_resonance_gain
                * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_TAIL11_RING_PARTIAL)
            + tail13_mode
                * profile.tail13_resonance_gain
                * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_TAIL13_RING_PARTIAL);
        let reaction_flow = math::finite_clamp(
            modal_reaction * profile.reaction_gain,
            -TUBE_BODY_REACTION_LIMIT,
            TUBE_BODY_REACTION_LIMIT,
            0.0,
        );

        TubeBodySample {
            output: math::snap_to_zero((direct + modal_body) * profile.output_gain),
            reaction_flow: math::snap_to_zero(reaction_flow),
        }
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
        self.even_h2_rejection
            .set_coefficients(profile.even_h2_rejection);
        self.even_h4_rejection
            .set_coefficients(profile.even_h4_rejection);
        self.low_resonance.set_coefficients(profile.low_resonance);
        self.high_resonance.set_coefficients(profile.high_resonance);
        self.upper_resonance
            .set_coefficients(profile.upper_resonance);
        self.edge_resonance.set_coefficients(profile.edge_resonance);
        self.tail9_resonance
            .set_coefficients(profile.tail9_resonance);
        self.tail11_resonance
            .set_coefficients(profile.tail11_resonance);
        self.tail13_resonance
            .set_coefficients(profile.tail13_resonance);
        self.prepared = Some((params, profile));
        profile
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BodyProfile {
    highpass: BiquadCoefficients,
    lowpass: BiquadCoefficients,
    even_h2_rejection: BiquadCoefficients,
    even_h4_rejection: BiquadCoefficients,
    low_resonance: BiquadCoefficients,
    high_resonance: BiquadCoefficients,
    upper_resonance: BiquadCoefficients,
    edge_resonance: BiquadCoefficients,
    tail9_resonance: BiquadCoefficients,
    tail11_resonance: BiquadCoefficients,
    tail13_resonance: BiquadCoefficients,
    direct_gain: f32,
    low_resonance_gain: f32,
    high_resonance_gain: f32,
    upper_resonance_gain: f32,
    edge_resonance_gain: f32,
    tail9_resonance_gain: f32,
    tail11_resonance_gain: f32,
    tail13_resonance_gain: f32,
    reaction_gain: f32,
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
            lerp(2.4, 8.0, formant) * math::finite_clamp(1.0 + 0.20 * voicing, 0.35, 1.80, 1.0);
        let high_gain =
            lerp(0.05, 0.10, formant) * math::finite_clamp(1.0 + 0.50 * voicing, 0.10, 2.60, 1.0);
        let register_mode =
            math::finite_or(params.register_mode_ratio, 1.0) > TUBE_REGISTER_MODE_THRESHOLD;
        let primary_body_hz = if register_mode {
            params.frequency_hz
        } else {
            TUBE_AIR_COLUMN_BODY_HZ
        };
        let primary_body_q = if register_mode { 4.0 } else { 2.0 };
        let primary_body_gain = if register_mode {
            lerp(0.10, 0.95, formant)
        } else {
            lerp(0.10, 0.03, formant)
        };
        let upper_ring_hz = math::finite_clamp(
            params.frequency_hz * TUBE_CLARINET_UPPER_RING_PARTIAL,
            TUBE_CLARINET_UPPER_RING_MIN_HZ,
            TUBE_CLARINET_UPPER_RING_MAX_HZ.min(sample_rate * 0.45),
            TUBE_CLARINET_UPPER_RING_MIN_HZ,
        );
        let edge_ring_hz = math::finite_clamp(
            params.frequency_hz * TUBE_CLARINET_EDGE_RING_PARTIAL,
            TUBE_CLARINET_EDGE_RING_MIN_HZ,
            TUBE_CLARINET_EDGE_RING_MAX_HZ.min(sample_rate * 0.45),
            TUBE_CLARINET_EDGE_RING_MIN_HZ,
        );
        let upper_q = lerp(2.2, 11.0, formant);
        let edge_q = lerp(2.0, 10.0, formant);
        let tail_strength = if register_mode {
            0.0
        } else {
            formant * unit(params.body_upper_odd_modes)
        };
        let reaction_gain = if register_mode {
            lerp(0.0, 0.16, formant)
        } else {
            lerp(0.0, 0.28, formant)
        };

        Self {
            highpass: BiquadCoefficients::highpass(sample_rate, 45.0, DEFAULT_BIQUAD_Q),
            lowpass: BiquadCoefficients::lowpass(sample_rate, radiation_cutoff, DEFAULT_BIQUAD_Q),
            even_h2_rejection: BiquadCoefficients::notch(
                sample_rate,
                params.frequency_hz * 2.0,
                TUBE_EVEN_MODE_REJECTION_Q,
            ),
            even_h4_rejection: BiquadCoefficients::notch(
                sample_rate,
                params.frequency_hz * 4.0,
                TUBE_EVEN_MODE_REJECTION_Q,
            ),
            low_resonance: BiquadCoefficients::bandpass(
                sample_rate,
                primary_body_hz,
                primary_body_q,
            ),
            high_resonance: BiquadCoefficients::bandpass(sample_rate, ring_hz, ring_q),
            upper_resonance: BiquadCoefficients::bandpass(sample_rate, upper_ring_hz, upper_q),
            edge_resonance: BiquadCoefficients::bandpass(sample_rate, edge_ring_hz, edge_q),
            tail9_resonance: odd_tail_resonance(
                sample_rate,
                params.frequency_hz,
                TUBE_CLARINET_TAIL9_RING_PARTIAL,
                8.0,
            ),
            tail11_resonance: odd_tail_resonance(
                sample_rate,
                params.frequency_hz,
                TUBE_CLARINET_TAIL11_RING_PARTIAL,
                7.0,
            ),
            tail13_resonance: odd_tail_resonance(
                sample_rate,
                params.frequency_hz,
                TUBE_CLARINET_TAIL13_RING_PARTIAL,
                6.0,
            ),
            direct_gain: lerp(0.62, 0.028, formant),
            low_resonance_gain: primary_body_gain,
            high_resonance_gain: high_gain * 2.8,
            upper_resonance_gain: lerp(0.0, 1.60, formant),
            edge_resonance_gain: lerp(0.0, 1.10, formant),
            tail9_resonance_gain: tail_strength * 1.35,
            tail11_resonance_gain: tail_strength * 1.35,
            tail13_resonance_gain: tail_strength * 2.80,
            reaction_gain,
            output_gain: TUBE_BOUNDARY.output_gain(params.boundary_reflection),
        }
    }
}

fn odd_tail_resonance(
    sample_rate: f32,
    frequency_hz: f32,
    harmonic: f32,
    q: f32,
) -> BiquadCoefficients {
    BiquadCoefficients::bandpass(
        sample_rate,
        math::finite_clamp(
            frequency_hz * harmonic,
            20.0,
            sample_rate * 0.45,
            frequency_hz,
        ),
        q,
    )
}

fn closed_open_mode_coupling(position: f32, harmonic: f32) -> f32 {
    let position = math::finite_clamp(position, 0.0, 1.0, 0.5);
    let harmonic = math::finite_or(harmonic, 1.0).max(1.0);
    let pressure_shape = (0.5 * std::f32::consts::PI * harmonic * position).cos();
    pressure_shape * pressure_shape
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
