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
const TUBE_REGISTER_H3_RING_PARTIAL: f32 = 3.0;
const TUBE_REGISTER_H3_RING_Q: f32 = 12.0;
// Source-fed register color gains, set from band measurements of the register-key A4 case
// against the owner reference: the vented bore's standing wave carries no sounding h3 (3x the
// note is not a bore mode), so the h3 register color must radiate from the reed source spectrum
// through the open-hole lattice, like a real clarion register above the lattice cutoff. h3 is
// the reference's strongest harmonic and has no other carrier; the large gain is why the h3
// extraction is a cascaded (4th-order) bandpass — a single biquad's first-order skirts re-inject
// the source's broadband breath noise an octave either side at this gain. h4 stays modest
// because the plugin's reed-radiation layer already carries h4 and above.
const TUBE_REGISTER_H3_SOURCE_GAIN: f32 = 2.0;
// Lattice radiation window over the reed-character lines (sounding h4-h7 at the register-key
// A4 reference): a band-limited broadband radiation of the coherent register source. The edges
// shape the reference's line contour (rising into the lattice region, rolling off above h7);
// the source is the reed's coherent output, so the window radiates lines, not breath.
const TUBE_REGISTER_LATTICE_LOW_HZ: f32 = 1_650.0;
const TUBE_REGISTER_LATTICE_HIGH_HZ: f32 = 2_800.0;
const TUBE_REGISTER_LATTICE_GAIN: f32 = 2.9;
const TUBE_REGISTER_FUNDAMENTAL_GAIN_LEGACY: f32 = 0.95;
const TUBE_REGISTER_FUNDAMENTAL_GAIN: f32 = 0.55;
const TUBE_REGISTER_RING_GAIN_SCALE_LEGACY: f32 = 2.8;
const TUBE_REGISTER_RING_GAIN_SCALE: f32 = 9.0;

/// How strongly the register-aware body/radiation admittance applies: `0.0` below the break,
/// `1.0` when the bore speaks on a register mode. Shared with the odd-mode projection bypass in
/// the tube loop.
pub(super) fn register_body_blend(params: ReedTubeParams) -> f32 {
    let ratio = math::finite_or(params.register_mode_ratio, 1.0);
    if ratio > TUBE_REGISTER_MODE_THRESHOLD {
        1.0
    } else {
        0.0
    }
}

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
    register_h3_resonance: Biquad,
    register_h3_resonance_b: Biquad,
    register_lattice_highpass: Biquad,
    register_lattice_highpass_b: Biquad,
    register_lattice_lowpass: Biquad,
    register_lattice_lowpass_b: Biquad,
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
            register_h3_resonance: Biquad::new(BiquadCoefficients::identity()),
            register_h3_resonance_b: Biquad::new(BiquadCoefficients::identity()),
            register_lattice_highpass: Biquad::new(BiquadCoefficients::identity()),
            register_lattice_highpass_b: Biquad::new(BiquadCoefficients::identity()),
            register_lattice_lowpass: Biquad::new(BiquadCoefficients::identity()),
            register_lattice_lowpass_b: Biquad::new(BiquadCoefficients::identity()),
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
        self.register_h3_resonance.reset();
        self.register_h3_resonance_b.reset();
        self.register_lattice_highpass.reset();
        self.register_lattice_highpass_b.reset();
        self.register_lattice_lowpass.reset();
        self.register_lattice_lowpass_b.reset();
        self.prepared = None;
    }

    /// `input` is the pickup-derived bore pressure feeding the radiating body; `register_source`
    /// is the reed-side source wave the register-mode color radiates from (the vented bore's
    /// standing wave does not carry the sounding h3/h4, so above the break the open-hole lattice
    /// radiates the source spectrum directly). The source path is radiation-only and gained to
    /// zero outside register mode.
    pub fn process_sample(
        &mut self,
        input: f32,
        register_source: f32,
        coupling_position: f32,
        params: ReedTubeParams,
    ) -> TubeBodySample {
        let profile = self.prepared_profile(params);
        let input = math::snap_to_zero(input);
        let register_source = math::snap_to_zero(register_source);
        let coupling_position = math::finite_clamp(coupling_position, 0.0, 1.0, 0.5);
        let radiating_input = self.highpass.process(input);
        let h2_rejected = self.even_h2_rejection.process(radiating_input);
        let h4_rejected = self.even_h4_rejection.process(h2_rejected);
        // Above the break the open register vent breaks the closed-pipe even-cancelling symmetry,
        // so the sounding h4 radiates: blend the h4 notch back out. h2 rejection stays — the
        // 0.8–1.2 kHz band is already over-present in the vented register. Full rejection takes
        // the notched sample directly so the chalumeau register stays bit-exact.
        let odd_admittance = if profile.even_h4_rejection_amount >= 1.0 {
            h4_rejected
        } else {
            h2_rejected + (h4_rejected - h2_rejected) * profile.even_h4_rejection_amount
        };
        let direct = self.lowpass.process(odd_admittance) * profile.direct_gain;
        let low_mode = self.low_resonance.process(odd_admittance);
        let high_mode = self.high_resonance.process(odd_admittance);
        let upper_mode = self.upper_resonance.process(odd_admittance);
        let edge_mode = self.edge_resonance.process(odd_admittance);
        let tail9_mode = self.tail9_resonance.process(odd_admittance);
        let tail11_mode = self.tail11_resonance.process(odd_admittance);
        let tail13_mode = self.tail13_resonance.process(odd_admittance);
        let register_h3_mode = self
            .register_h3_resonance_b
            .process(self.register_h3_resonance.process(register_source));
        // 4th-order window edges: at this gain, 2nd-order skirts leak the sounding h2 below and
        // pass h9+ above the reed-character band.
        let register_lattice_mode = self.register_lattice_lowpass_b.process(
            self.register_lattice_lowpass.process(
                self.register_lattice_highpass_b
                    .process(self.register_lattice_highpass.process(register_source)),
            ),
        );

        let low_body = low_mode * profile.low_resonance_gain;
        let high_body = high_mode * profile.high_resonance_gain;
        let upper_body = upper_mode * profile.upper_resonance_gain;
        let edge_body = edge_mode * profile.edge_resonance_gain;
        let tail_body = tail9_mode * profile.tail9_resonance_gain
            + tail11_mode * profile.tail11_resonance_gain
            + tail13_mode * profile.tail13_resonance_gain;
        // Radiation-only register h3/h4 body color (open-hole lattice admittance over the reed
        // source spectrum); the vent's reaction into the bore is already the junction shunt in
        // the tube loop.
        let register_color = register_h3_mode * profile.register_h3_resonance_gain
            + register_lattice_mode * profile.register_lattice_gain;
        let modal_body = low_body + high_body + upper_body + edge_body + tail_body + register_color;
        // The fundamental and ring reactions onto the bore use their own (legacy) gains: the
        // register-aware admittance rebalance is radiation-only (far-field efficiency), and
        // letting it scale the reactive junction load changes the bore's damping per band —
        // trimming h1 radiation would un-damp the bore fundamental, and boosting the ring would
        // damp the very h3 band it is meant to radiate.
        let modal_reaction = low_mode * profile.low_resonance_reaction_gain * 0.35
            + high_mode
                * profile.high_resonance_reaction_gain
                * closed_open_mode_coupling(coupling_position, TUBE_CLARINET_RING_PARTIAL)
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
        self.register_h3_resonance
            .set_coefficients(profile.register_h3_resonance);
        self.register_h3_resonance_b
            .set_coefficients(profile.register_h3_resonance);
        self.register_lattice_highpass
            .set_coefficients(profile.register_lattice_highpass);
        self.register_lattice_highpass_b
            .set_coefficients(profile.register_lattice_highpass);
        self.register_lattice_lowpass
            .set_coefficients(profile.register_lattice_lowpass);
        self.register_lattice_lowpass_b
            .set_coefficients(profile.register_lattice_lowpass);
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
    register_h3_resonance: BiquadCoefficients,
    register_lattice_highpass: BiquadCoefficients,
    register_lattice_lowpass: BiquadCoefficients,
    direct_gain: f32,
    low_resonance_gain: f32,
    high_resonance_gain: f32,
    upper_resonance_gain: f32,
    edge_resonance_gain: f32,
    tail9_resonance_gain: f32,
    tail11_resonance_gain: f32,
    tail13_resonance_gain: f32,
    register_h3_resonance_gain: f32,
    register_lattice_gain: f32,
    low_resonance_reaction_gain: f32,
    high_resonance_reaction_gain: f32,
    even_h4_rejection_amount: f32,
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
        let (ring_hz, ring_q, high_gain) = ring_voicing(sample_rate, params, formant);
        let voicing = register_voicing(params, formant);
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
                voicing.primary_body_hz,
                voicing.primary_body_q,
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
            register_h3_resonance: register_h3_resonance(sample_rate, params.frequency_hz),
            register_lattice_highpass: BiquadCoefficients::highpass(
                sample_rate,
                TUBE_REGISTER_LATTICE_LOW_HZ,
                DEFAULT_BIQUAD_Q,
            ),
            register_lattice_lowpass: BiquadCoefficients::lowpass(
                sample_rate,
                TUBE_REGISTER_LATTICE_HIGH_HZ,
                DEFAULT_BIQUAD_Q,
            ),
            direct_gain: lerp(0.62, 0.028, formant),
            low_resonance_gain: voicing.primary_body_gain,
            high_resonance_gain: high_gain * voicing.ring_gain_scale,
            upper_resonance_gain: lerp(0.0, 1.60, formant),
            edge_resonance_gain: lerp(0.0, 1.10, formant),
            tail9_resonance_gain: voicing.tail_strength * 1.35,
            tail11_resonance_gain: voicing.tail_strength * 1.35,
            tail13_resonance_gain: voicing.tail_strength * 2.80,
            register_h3_resonance_gain: voicing.register_h3_gain,
            register_lattice_gain: voicing.register_lattice_gain,
            low_resonance_reaction_gain: voicing.primary_body_reaction_gain,
            high_resonance_reaction_gain: high_gain * TUBE_REGISTER_RING_GAIN_SCALE_LEGACY,
            even_h4_rejection_amount: voicing.even_h4_rejection_amount,
            reaction_gain: voicing.reaction_gain,
            output_gain: TUBE_BOUNDARY.output_gain(params.boundary_reflection),
        }
    }
}

/// Register/chalumeau gain and topology choices for the radiating body. Above the break the
/// register-aware admittance applies: radiation efficiency through the open-hole lattice rises
/// with frequency, so the vented note's body color lives in the h3/h4 band, not on h1, the
/// sounding h4 radiates through the broken even symmetry (notch released plus the lattice
/// window), and the reaction gains stay at the chalumeau values so the rebalance is
/// radiation-only.
struct RegisterVoicing {
    primary_body_hz: f32,
    primary_body_q: f32,
    primary_body_gain: f32,
    primary_body_reaction_gain: f32,
    tail_strength: f32,
    reaction_gain: f32,
    ring_gain_scale: f32,
    register_h3_gain: f32,
    register_lattice_gain: f32,
    even_h4_rejection_amount: f32,
}

fn register_voicing(params: ReedTubeParams, formant: f32) -> RegisterVoicing {
    let register_mode =
        math::finite_or(params.register_mode_ratio, 1.0) > TUBE_REGISTER_MODE_THRESHOLD;
    let register_blend = register_body_blend(params);
    if register_mode {
        RegisterVoicing {
            primary_body_hz: params.frequency_hz,
            primary_body_q: 4.0,
            primary_body_gain: lerp(
                0.10,
                lerp(
                    TUBE_REGISTER_FUNDAMENTAL_GAIN_LEGACY,
                    TUBE_REGISTER_FUNDAMENTAL_GAIN,
                    register_blend,
                ),
                formant,
            ),
            primary_body_reaction_gain: lerp(0.10, TUBE_REGISTER_FUNDAMENTAL_GAIN_LEGACY, formant),
            tail_strength: 0.0,
            reaction_gain: lerp(0.0, 0.16, formant),
            ring_gain_scale: lerp(
                TUBE_REGISTER_RING_GAIN_SCALE_LEGACY,
                TUBE_REGISTER_RING_GAIN_SCALE,
                register_blend,
            ),
            register_h3_gain: register_blend * formant * TUBE_REGISTER_H3_SOURCE_GAIN,
            register_lattice_gain: register_blend * formant * TUBE_REGISTER_LATTICE_GAIN,
            even_h4_rejection_amount: 1.0 - register_blend,
        }
    } else {
        RegisterVoicing {
            primary_body_hz: TUBE_AIR_COLUMN_BODY_HZ,
            primary_body_q: 2.0,
            primary_body_gain: lerp(0.10, 0.03, formant),
            primary_body_reaction_gain: lerp(0.10, 0.03, formant),
            tail_strength: formant * unit(params.body_upper_odd_modes),
            reaction_gain: lerp(0.0, 0.28, formant),
            ring_gain_scale: TUBE_REGISTER_RING_GAIN_SCALE_LEGACY,
            register_h3_gain: 0.0,
            register_lattice_gain: 0.0,
            even_h4_rejection_amount: 1.0,
        }
    }
}

/// The tracked h3 ring's center, Q, and base gain, voiced by the formant amount and the
/// body-formant shift.
fn register_h3_resonance(sample_rate: f32, frequency_hz: f32) -> BiquadCoefficients {
    BiquadCoefficients::bandpass(
        sample_rate,
        math::finite_clamp(
            frequency_hz * TUBE_REGISTER_H3_RING_PARTIAL,
            20.0,
            sample_rate * 0.45,
            frequency_hz,
        ),
        TUBE_REGISTER_H3_RING_Q,
    )
}

fn ring_voicing(sample_rate: f32, params: ReedTubeParams, formant: f32) -> (f32, f32, f32) {
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
    (ring_hz, ring_q, high_gain)
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
