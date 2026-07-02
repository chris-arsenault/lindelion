use lindelion_dsp_utils::{
    db_to_gain,
    filters::{Biquad, BiquadCoefficients},
    math::{self, midi_note_to_hz},
};
use lindelion_plugin_shell::{ControlEvent, MidiEvent, NoteEvent};
use lindelion_wind::{
    ReedDriver, ReedParams, ReedProcessTaps, ReedTube, ReedTubeParams, ReedTubeSwitches,
    ReedTubeTaps,
};

use lindelion_dsp_utils::phrase::{self, HostExpression, PhraseEngine};

use crate::patch::TubePatch;

pub const ARTICULATION_SLOT_COUNT: usize = 8;
use taps::{FINAL_OUTPUT_TAP_INDEX, FINAL_POST_GAIN_TAP_INDEX, FINAL_PRE_GAIN_TAP_INDEX};
pub use taps::{TUBE_RENDER_TAP_COUNT, TUBE_RENDER_TAP_NAMES, TubeRenderTaps};

mod articulation;
mod excitation;
mod level;
mod note_lifecycle;
mod params;
mod phrasing;
mod taps;
mod variance_source;

use articulation::{ArticulationStyle, HELD_RETURN, ONSET_NOISE_TAU_SECONDS, articulation_style};
use level::VENTED_EFFORT_FLOOR;
use note_lifecycle::HELD_NOTE_CAPACITY;
use params::*;
use phrasing::{PHRASE_DEVELOPMENT_START, phrase_params_for_knobs};
use variance_source::{SteadyVarianceSource, TubeSteadyVariance};

pub use excitation::{ARTICULATION_NAMES, ExcitationSource};
use excitation::{Injector, keyswitch_slot};

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const OVERSAMPLE_FACTOR: usize = 2;
const KEYSWITCH_BASE_NOTE: u8 = 0;
const BUILTIN_EXCITATION_SAMPLE_RATE: f32 = 48_000.0;
const GATE_RAMP_SECONDS: f32 = 0.006;
const MIN_LOOP_GAIN: f32 = 0.90;
const MAX_LOOP_GAIN: f32 = 0.995;
const MIN_BRIGHTNESS_HZ: f32 = 1_200.0;
const MAX_BRIGHTNESS_HZ: f32 = 14_000.0;
const BELL_MIX_EXPONENT: f32 = 1.514_573_2;
const REED_RADIATION_CUTOFF_HZ: f32 = 1_050.0;
const REED_RADIATION_Q: f32 = 0.707;
const REED_RADIATION_EDGE_CUTOFF_HZ: f32 = 1_200.0;
const REED_RADIATION_EDGE_Q: f32 = 0.707;
const REED_RADIATION_ROLLOFF_HZ: f32 = 6_000.0;
const REED_RADIATION_ROLLOFF_Q: f32 = 0.707;
const REED_RADIATION_SLOT_ROLLOFF_HZ: f32 = 3_000.0;
const REED_RADIATION_SLOT_ROLLOFF_Q: f32 = 0.707;
const REED_RADIATION_GAIN: f32 = 0.75;
const REED_RADIATION_LIMIT: f32 = 0.22;
// Sounding-to-bore mode ratio above the break: with the vent choked at the played mode (chimney
// anti-resonance) the bore speaks its natural third mode, measured at 2.994x the bore
// fundamental.
const REGISTER_MODE_RATIO: f32 = 2.994;
const REGISTER_VENT_ADMITTANCE: f32 = 1.20;
const REGISTER_VENT_POSITION: f32 = 1.0 / 3.0;
// Tongue-release attack overpressure for the vented register. The register's oscillation
// margin makes the bloom exponential from a tiny seed (~200 ms at the steady pressure); a real
// attack transiently overblows, which raises the reed's gain while the note builds. Measured
// bloom-to-50% vs effective pressure (A4/D5): 0.58 -> 180-205 ms, 0.68 -> 70-75 ms (the
// reference's attack), 0.75 -> 50-60 ms, 0.85 -> the reed chokes silent. The attack drives the
// effective pressure toward ATTACK_PRESSURE_TARGET (never past ATTACK_PRESSURE_MAX, well under
// the choke cliff, including humanize walks), decaying to the steady patch pressure with
// ATTACK_PRESSURE_TAU so the boost covers the bloom.
const ATTACK_PRESSURE_TARGET: f32 = 0.75;
const ATTACK_PRESSURE_MAX: f32 = 0.78;
const ATTACK_PRESSURE_TAU_SECONDS: f32 = 0.07;
/// Scale on the low-register in-loop breath dither. Historically a period-2 stability aid at
/// 1.0, but the current reed (inertial aperture + register-voice rebuild) shows an identical
/// worst-case f/2 ratio (0.278, note 43 at pressure 1.0) at every dither scale including zero —
/// the dither no longer stabilizes anything. Its loop-circulated white noise reads as a loud
/// breath wash against the deliberately clean vented register (glaring on runs over the break),
/// so it is held at a whisper for air character only.
const LOW_REGISTER_BREATH_WASH: f32 = 0.25;
/// A note change waiting at the bottom of the register-break dip.
#[derive(Debug, Clone, Copy)]
struct PendingBreakNote {
    note: u8,
    velocity: f32,
    style: ArticulationStyle,
}
#[derive(Debug)]
pub struct TubeProcessor<'a> {
    sample_rate: f32,
    model_sample_rate: f32,
    patch: TubePatch,
    tube: ReedTube,
    reed: ReedDriver,
    reed_radiation_highpass: Biquad,
    reed_radiation_edge_highpass: Biquad,
    reed_radiation_rolloff_a: Biquad,
    reed_radiation_rolloff_b: Biquad,
    reed_radiation_slot_rolloff: Biquad,
    sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    injector: Injector<'a>,
    selected_slot: usize,
    current_note: Option<u8>,
    /// The most recent played note, persisting through release: the register-key fingering
    /// (vent topology, bore ratio, embouchure tracking) must hold while the bore rings out —
    /// snapping the vented long bore back to the unvented short one at note-off retunes the
    /// delay line mid-ring and truncates the release with a hard waveform step.
    sounding_note: Option<u8>,
    /// Monophonic note stack: keys physically down right now, oldest first, with their played
    /// velocities. Releasing the sounding note returns to the most recent entry (finger-lift
    /// slur) instead of cutting to silence under still-held keys.
    held_notes: [(u8, f32); HELD_NOTE_CAPACITY],
    held_note_count: usize,
    pending_break_note: Option<PendingBreakNote>,
    break_dip_remaining: u32,
    frequency_hz: f32,
    effort: f32,
    drive_gate: f32,
    phrase_engine: PhraseEngine,
    host_expression: HostExpression,
    drive_target: f32,
    gate_coeff: f32,
    attack_envelope: f32,
    /// Transient effort push above the played velocity, riding the attack envelope
    /// (the articulation's air-push accent; see `ArticulationStyle::effort_accent`).
    effort_accent: f32,
    attack_coeff: f32,
    onset_noise_envelope: f32,
    onset_noise_coeff: f32,
    breath_wash_scale: f32,
    steady_variance_source: SteadyVarianceSource,
}

impl<'a> TubeProcessor<'a> {
    pub fn new(
        sample_rate: f32,
        patch: TubePatch,
        sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    ) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let model_sample_rate = sample_rate * OVERSAMPLE_FACTOR as f32;
        let patch = patch.sanitized();
        let reed = ReedDriver::new(reed_params(&patch), model_sample_rate);
        Self {
            sample_rate,
            model_sample_rate,
            tube: ReedTube::new(model_sample_rate),
            reed,
            reed_radiation_highpass: reed_radiation_highpass(model_sample_rate),
            reed_radiation_edge_highpass: reed_radiation_edge_highpass(model_sample_rate),
            reed_radiation_rolloff_a: reed_radiation_rolloff(model_sample_rate),
            reed_radiation_rolloff_b: reed_radiation_rolloff(model_sample_rate),
            reed_radiation_slot_rolloff: reed_radiation_slot_rolloff(model_sample_rate),
            selected_slot: patch.selected_articulation,
            patch,
            sources,
            injector: Injector::default(),
            current_note: None,
            sounding_note: None,
            held_notes: [(0, 0.0); HELD_NOTE_CAPACITY],
            held_note_count: 0,
            pending_break_note: None,
            break_dip_remaining: 0,
            frequency_hz: midi_note_to_hz(60.0),
            effort: 0.0,
            drive_gate: 0.0,
            phrase_engine: PhraseEngine::new(model_sample_rate),
            host_expression: HostExpression::new(model_sample_rate),
            drive_target: 0.0,
            gate_coeff: gate_coeff(model_sample_rate, 1.0),
            attack_envelope: 0.0,
            effort_accent: 0.0,
            attack_coeff: attack_coeff(model_sample_rate, 1.0),
            onset_noise_envelope: 0.0,
            onset_noise_coeff: onset_noise_coeff(model_sample_rate),
            breath_wash_scale: LOW_REGISTER_BREATH_WASH,
            steady_variance_source: SteadyVarianceSource::new(model_sample_rate),
        }
    }

    pub fn reset(&mut self, sample_rate: f32) {
        let sample_rate = sanitize_sample_rate(sample_rate);
        self.sample_rate = sample_rate;
        self.model_sample_rate = sample_rate * OVERSAMPLE_FACTOR as f32;
        self.tube = ReedTube::new(self.model_sample_rate);
        self.reed = ReedDriver::new(reed_params(&self.patch), self.model_sample_rate);
        self.reed_radiation_highpass = reed_radiation_highpass(self.model_sample_rate);
        self.reed_radiation_edge_highpass = reed_radiation_edge_highpass(self.model_sample_rate);
        self.reed_radiation_rolloff_a = reed_radiation_rolloff(self.model_sample_rate);
        self.reed_radiation_rolloff_b = reed_radiation_rolloff(self.model_sample_rate);
        self.reed_radiation_slot_rolloff = reed_radiation_slot_rolloff(self.model_sample_rate);
        self.injector.clear();
        self.current_note = None;
        self.sounding_note = None;
        self.held_notes = [(0, 0.0); HELD_NOTE_CAPACITY];
        self.held_note_count = 0;
        self.pending_break_note = None;
        self.break_dip_remaining = 0;
        self.frequency_hz = midi_note_to_hz(60.0);
        self.effort = 0.0;
        self.drive_gate = 0.0;
        self.phrase_engine = PhraseEngine::new(self.model_sample_rate);
        self.host_expression = HostExpression::new(self.model_sample_rate);
        self.drive_target = 0.0;
        self.gate_coeff = gate_coeff(self.model_sample_rate, 1.0);
        self.attack_envelope = 0.0;
        self.effort_accent = 0.0;
        self.attack_coeff = attack_coeff(self.model_sample_rate, 1.0);
        self.onset_noise_envelope = 0.0;
        self.onset_noise_coeff = onset_noise_coeff(self.model_sample_rate);
        self.breath_wash_scale = LOW_REGISTER_BREATH_WASH;
        self.steady_variance_source.reset(self.model_sample_rate);
    }

    pub fn set_patch(&mut self, patch: TubePatch) {
        self.patch = patch.sanitized();
        self.selected_slot = self.patch.selected_articulation;
        self.reed.set_params(reed_params(&self.patch));
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        left.fill(0.0);
        right.fill(0.0);
        self.handle_events(events);
        let len = left.len().min(right.len());
        let output_gain = db_to_gain(self.patch.output_gain_db);
        for index in 0..len {
            let mut sample = 0.0;
            for _ in 0..OVERSAMPLE_FACTOR {
                sample += self.process_model_sample();
            }
            sample *= output_gain / OVERSAMPLE_FACTOR as f32;
            let sample = soft_limit(sample);
            left[index] = sample;
            right[index] = sample;
        }
    }

    pub fn process_with_taps(
        &mut self,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
        tap_traces: &mut [Vec<f32>; TUBE_RENDER_TAP_COUNT],
    ) {
        left.fill(0.0);
        right.fill(0.0);
        self.handle_events(events);
        let len = left.len().min(right.len());
        let output_gain = db_to_gain(self.patch.output_gain_db);
        for index in 0..len {
            let mut sample = 0.0;
            let mut frame_taps = [0.0; TUBE_RENDER_TAP_COUNT];
            for _ in 0..OVERSAMPLE_FACTOR {
                let mut taps = TubeRenderTaps::default();
                sample += self.process_model_sample_with_taps(Some(&mut taps));
                for (slot, value) in frame_taps.iter_mut().zip(taps.values()) {
                    *slot += value;
                }
            }
            let oversample_scale = 1.0 / OVERSAMPLE_FACTOR as f32;
            for value in &mut frame_taps {
                *value *= oversample_scale;
            }
            let final_pre_gain = sample * oversample_scale;
            let final_post_gain = final_pre_gain * output_gain;
            let final_output = soft_limit(final_post_gain);
            frame_taps[FINAL_PRE_GAIN_TAP_INDEX] = final_pre_gain;
            frame_taps[FINAL_POST_GAIN_TAP_INDEX] = final_post_gain;
            frame_taps[FINAL_OUTPUT_TAP_INDEX] = final_output;
            for (trace, value) in tap_traces.iter_mut().zip(frame_taps) {
                trace.push(value);
            }
            left[index] = final_output;
            right[index] = final_output;
        }
    }

    /// The active articulation slot (editor pick, patch, or the last keyswitch). The plugin
    /// shell mirrors this back into its patch after each block so keyswitch changes survive
    /// later patch pushes (parameter edits clone the shell's patch over the processor's).
    pub fn selected_slot(&self) -> usize {
        self.selected_slot
    }

    fn process_model_sample(&mut self) -> f32 {
        self.process_model_sample_with_taps(None)
    }

    /// Per-sample register-scoped reed staging; returns the coherent-radiation amount (1.0 above
    /// the break, 0.0 below). Above the break, raw noise sources are replaced by coherent
    /// radiation (the in-loop dither is a low-register period-2 stability aid whose
    /// loop-circulated white noise buries the register's weak voiced lines as a breath wash),
    /// the embouchure tracks the played pitch so the reed's pumping gain stops collapsing with
    /// mode frequency, and the tongue-release overpressure drives the effective pressure toward
    /// the attack target while the envelope is hot — never past the choke-cliff ceiling
    /// (humanize walks included) — decaying to the steady patch pressure. Below the break all of
    /// it is inert and the reed plays exactly as auditioned.
    fn apply_register_performance(&mut self, pressure_mod: f32, embouchure_mod: f32) -> f32 {
        let register_active =
            register_key_state(&self.patch, self.sounding_note).vent_admittance > 0.0;
        let coherent_amount = if register_active { 1.0 } else { 0.0 };
        // The overpressure decays on the breath's clock: a swelled (slow-gate) entry keeps its
        // attack boost until the air actually arrives, instead of spending it into silence.
        self.attack_envelope *= 1.0 - self.attack_coeff * self.drive_gate;
        self.onset_noise_envelope *= 1.0 - self.onset_noise_coeff;
        let steady_pressure = math::finite_clamp(
            self.patch.pressure * (1.0 + pressure_mod),
            0.0,
            1.0,
            self.patch.pressure,
        );
        let attack_target = ATTACK_PRESSURE_TARGET
            .max(steady_pressure)
            .min(ATTACK_PRESSURE_MAX);
        let attack_pressure =
            steady_pressure + (attack_target - steady_pressure) * self.attack_envelope;
        let pressure_mod = attack_pressure / self.patch.pressure.max(1.0e-3) - 1.0;
        let tracking_frequency_hz = if register_active {
            self.frequency_hz
        } else {
            0.0
        };
        // Onset turbulence rides on top of the register's steady noise floor
        // (full breath wash below the break, none above it), so a breath
        // articulation reads as air in either register.
        let breath_noise = math::finite_clamp(
            self.breath_wash_scale * (1.0 - coherent_amount) + self.onset_noise_envelope,
            0.0,
            2.0,
            self.breath_wash_scale * (1.0 - coherent_amount),
        );
        self.reed.set_params(reed_params_with_mod(
            &self.patch,
            pressure_mod,
            embouchure_mod,
            breath_noise,
            tracking_frequency_hz,
        ));
        coherent_amount
    }

    /// Phrasing: living breath intensity + breath vibrato on the pressure
    /// rail; the release taper holds the gate open while it sounds. At both
    /// knobs zero the engine is bypassed (bit-identical static behavior).
    /// A reed is a threshold oscillator with starting hysteresis: it cannot
    /// take multiplicative level modulation near the speaking threshold (a
    /// swell dip kills a soft note for good). Effort therefore stays at the
    /// played velocity, gated by the release taper; the swell and the breath
    /// vibrato ride the pressure rail the humanize walks already proved safe.
    fn phrase_drive(&mut self) -> (f32, f32) {
        let phrasing = self.patch.phrasing;
        let vibrato = self.patch.vibrato;
        if phrasing > f32::EPSILON || vibrato > f32::EPSILON {
            let outputs = self
                .phrase_engine
                .process(phrase_params_for_knobs(phrasing, vibrato));
            self.drive_target = if outputs.active { 1.0 } else { 0.0 };
            (
                (self.effort * outputs.release_multiplier).clamp(0.0, 1.0),
                outputs.sustain_swell
                    + outputs.pitch_lean_cents * phrasing::PHRASE_VIBRATO_BREATH_PER_CENT,
            )
        } else {
            (self.effort, 0.0)
        }
    }

    /// The played effort line: host expression scales it, the articulation's air-push accent
    /// rides the attack envelope, [`VENTED_EFFORT_FLOOR`] maps onto the speaking region.
    fn shaped_effort(&self, effort_signal: f32, intensity_factor: f32) -> f32 {
        let effort_signal = (effort_signal * intensity_factor
            + self.effort_accent * self.attack_envelope)
            .clamp(0.0, 1.0);
        let register_active =
            register_key_state(&self.patch, self.sounding_note).vent_admittance > 0.0;
        if register_active {
            VENTED_EFFORT_FLOOR + (1.0 - VENTED_EFFORT_FLOOR) * effort_signal
        } else {
            effort_signal
        }
    }

    fn process_model_sample_with_taps(&mut self, taps: Option<&mut TubeRenderTaps>) -> f32 {
        let transition_loop_scale = self.process_break_transition();
        let (effort_signal, phrase_breath_mod) = self.phrase_drive();
        if self.pending_break_note.is_some() {
            // The break dip outranks the phrase engine's gate.
            self.drive_target = 0.0;
        }
        let expression = self.host_expression.process();
        let effort_signal = self.shaped_effort(effort_signal, expression.intensity_factor);
        let frequency_hz = self.frequency_hz * phrase::cents_ratio(expression.bend_cents);
        self.drive_gate += (self.drive_target - self.drive_gate) * self.gate_coeff;
        let excitation = self.injector.process();
        self.tube.set_brightness_effort(effort_signal);
        let variance = TubeSteadyVariance::from_humanize(self.patch.humanize).sanitized();
        let (humanize_pressure_mod, embouchure_mod, voicing_mod) =
            self.steady_variance_source.process(variance);
        let pressure_mod = humanize_pressure_mod + phrase_breath_mod;
        let mut params =
            tube_params_with_mod(&self.patch, self.sounding_note, frequency_hz, voicing_mod);
        params.loop_gain *= transition_loop_scale;
        let coherent_amount = self.apply_register_performance(pressure_mod, embouchure_mod);
        params.reed_phase_delay_samples = self
            .reed
            .aperture_phase_delay_samples(frequency_hz, effort_signal);
        let feedback = self.tube.driven_feedback();
        let mut reed_taps = ReedProcessTaps::default();
        let mouth_wave = self.reed.process_with_taps(
            excitation,
            effort_signal,
            feedback,
            self.drive_gate,
            &mut reed_taps,
        );
        let radiated_flow = reed_taps.source_flow
            + (self.reed.coherent_source_flow() - reed_taps.source_flow) * coherent_amount;
        if coherent_amount > 0.0 {
            let register_source =
                mouth_wave + (self.reed.coherent_output() - mouth_wave) * coherent_amount;
            self.tube.set_register_radiation_source(register_source);
        }
        let reed_radiated = if self.patch.switches.reed_radiation_enabled {
            self.reed_radiated_sample(radiated_flow)
        } else {
            0.0
        };
        let radiated_makeup = self.radiated_makeup(frequency_hz);
        if let Some(taps) = taps {
            let mut tube_taps = ReedTubeTaps::default();
            let tube_output = self
                .tube
                .process_wind_with_taps(mouth_wave, params, &mut tube_taps);
            let output = (tube_output + reed_radiated) * radiated_makeup;
            *taps = TubeRenderTaps {
                excitation,
                drive_gate: self.drive_gate,
                effort: self.effort,
                pressure_mod,
                embouchure_mod,
                voicing_mod,
                feedback,
                reed_breath: reed_taps.breath,
                reed_delta_p: reed_taps.delta_p,
                reed_aperture: reed_taps.aperture,
                reed_raw_flow: reed_taps.raw_flow,
                reed_source_flow: reed_taps.source_flow,
                reed_output: reed_taps.output,
                reed_radiated,
                mouth_wave,
                mouth_incident: tube_taps.mouth_incident,
                mouth_filtered: tube_taps.mouth_filtered,
                mouth_reflection: tube_taps.mouth_reflection,
                bell_incident: tube_taps.bell_incident,
                bell_reflection: tube_taps.bell_reflection,
                bell_pressure: tube_taps.bell_pressure,
                pickup_left: tube_taps.pickup_left,
                pickup_right: tube_taps.pickup_right,
                pickup_pressure: tube_taps.pickup_pressure,
                pickup_flow: tube_taps.pickup_flow,
                pickup_sample: tube_taps.pickup_sample,
                body_input: tube_taps.body_input,
                body_output: tube_taps.body_output,
                body_reaction_flow: tube_taps.body_reaction_flow,
                bell_radiated: tube_taps.bell_radiated,
                register_vent_flow: tube_taps.register_vent_flow,
                register_vent_output: tube_taps.register_vent_output,
                body_bell_sum: tube_taps.body_bell_sum,
                tube_main_output: tube_taps.main_output,
                tube_final_output: tube_output,
                ..TubeRenderTaps::default()
            };
            output
        } else {
            (self.tube.process_wind(mouth_wave, params) + reed_radiated) * radiated_makeup
        }
    }

    fn reed_radiated_sample(&mut self, source_flow: f32) -> f32 {
        let bright_flow = self.reed_radiation_highpass.process(source_flow);
        let edge_flow = self.reed_radiation_edge_highpass.process(bright_flow);
        let anti_air_flow = self
            .reed_radiation_rolloff_b
            .process(self.reed_radiation_rolloff_a.process(edge_flow));
        let shaped_flow = self.reed_radiation_slot_rolloff.process(anti_air_flow);
        math::finite_clamp(
            shaped_flow * REED_RADIATION_GAIN * self.drive_gate,
            -REED_RADIATION_LIMIT,
            REED_RADIATION_LIMIT,
            0.0,
        )
    }
}

fn gate_coeff(sample_rate: f32, ramp_scale: f32) -> f32 {
    let ramp = GATE_RAMP_SECONDS * ramp_scale.clamp(0.25, 16.0);
    1.0 - (-1.0 / (ramp * sanitize_sample_rate(sample_rate))).exp()
}

fn attack_coeff(model_sample_rate: f32, tau_scale: f32) -> f32 {
    let tau = ATTACK_PRESSURE_TAU_SECONDS * tau_scale.clamp(0.25, 2.5);
    1.0 - (-1.0 / (tau * sanitize_sample_rate(model_sample_rate))).exp()
}

fn onset_noise_coeff(model_sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (ONSET_NOISE_TAU_SECONDS * sanitize_sample_rate(model_sample_rate))).exp()
}

fn reed_radiation_highpass(sample_rate: f32) -> Biquad {
    let sample_rate = sanitize_sample_rate(sample_rate);
    Biquad::new(BiquadCoefficients::highpass(
        sample_rate,
        REED_RADIATION_CUTOFF_HZ,
        REED_RADIATION_Q,
    ))
}

fn reed_radiation_edge_highpass(sample_rate: f32) -> Biquad {
    let sample_rate = sanitize_sample_rate(sample_rate);
    Biquad::new(BiquadCoefficients::highpass(
        sample_rate,
        REED_RADIATION_EDGE_CUTOFF_HZ,
        REED_RADIATION_EDGE_Q,
    ))
}

fn reed_radiation_rolloff(sample_rate: f32) -> Biquad {
    let sample_rate = sanitize_sample_rate(sample_rate);
    Biquad::new(BiquadCoefficients::lowpass(
        sample_rate,
        REED_RADIATION_ROLLOFF_HZ,
        REED_RADIATION_ROLLOFF_Q,
    ))
}

fn reed_radiation_slot_rolloff(sample_rate: f32) -> Biquad {
    let sample_rate = sanitize_sample_rate(sample_rate);
    Biquad::new(BiquadCoefficients::lowpass(
        sample_rate,
        REED_RADIATION_SLOT_ROLLOFF_HZ,
        REED_RADIATION_SLOT_ROLLOFF_Q,
    ))
}

fn sanitize_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        DEFAULT_SAMPLE_RATE
    }
}

fn soft_limit(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.tanh()
    } else {
        0.0
    }
}

#[cfg(test)]
mod mono_stack_tests;
#[cfg(test)]
mod tests;
