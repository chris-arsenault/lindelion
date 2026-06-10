use lindelion_dsp_utils::{
    db_to_gain,
    filters::{Biquad, BiquadCoefficients},
    math::{self, midi_note_to_hz},
};
use lindelion_plugin_shell::{MidiEvent, NoteEvent};
use lindelion_wind::{
    ReedDriver, ReedParams, ReedProcessTaps, ReedTube, ReedTubeParams, ReedTubeSwitches,
    ReedTubeTaps,
};

use crate::patch::TubePatch;

pub const ARTICULATION_SLOT_COUNT: usize = 8;
use taps::{FINAL_OUTPUT_TAP_INDEX, FINAL_POST_GAIN_TAP_INDEX, FINAL_PRE_GAIN_TAP_INDEX};
pub use taps::{TUBE_RENDER_TAP_COUNT, TUBE_RENDER_TAP_NAMES, TubeRenderTaps};

mod excitation;
mod taps;
mod variance_source;

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
    frequency_hz: f32,
    effort: f32,
    drive_gate: f32,
    drive_target: f32,
    gate_coeff: f32,
    attack_envelope: f32,
    attack_coeff: f32,
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
            frequency_hz: midi_note_to_hz(60.0),
            effort: 0.0,
            drive_gate: 0.0,
            drive_target: 0.0,
            gate_coeff: gate_coeff(model_sample_rate),
            attack_envelope: 0.0,
            attack_coeff: 1.0 - (-1.0 / (ATTACK_PRESSURE_TAU_SECONDS * model_sample_rate)).exp(),
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
        self.frequency_hz = midi_note_to_hz(60.0);
        self.effort = 0.0;
        self.drive_gate = 0.0;
        self.drive_target = 0.0;
        self.gate_coeff = gate_coeff(self.model_sample_rate);
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

    #[cfg(test)]
    pub fn selected_slot(&self) -> usize {
        self.selected_slot
    }

    fn handle_events(&mut self, events: &[MidiEvent]) {
        for event in events {
            let MidiEvent::Note(note) = *event else {
                continue;
            };
            match note {
                NoteEvent::On { note, velocity, .. } if velocity > 0.0 => {
                    if let Some(slot) = keyswitch_slot(note) {
                        self.select_slot(slot);
                    } else {
                        self.note_on(note, velocity);
                    }
                }
                NoteEvent::Off { note, .. } => self.note_off(note),
                NoteEvent::On { note, .. } => self.note_off(note),
            }
        }
    }

    fn select_slot(&mut self, slot: usize) {
        self.selected_slot = slot.min(ARTICULATION_SLOT_COUNT - 1);
        self.patch.selected_articulation = self.selected_slot;
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.current_note = Some(note);
        self.sounding_note = Some(note);
        self.frequency_hz = midi_note_to_hz(note as f32);
        self.effort = velocity.clamp(0.0, 1.0);
        self.drive_target = 1.0;
        // Tongue-release overpressure: every vented-register attack (including legato note
        // changes, which must re-lock the new mode) starts with the boost armed.
        if register_key_state(&self.patch, self.sounding_note).vent_admittance > 0.0 {
            self.attack_envelope = 1.0;
        }
        let gain = self.effort.sqrt();
        let source = self.sources[self.selected_slot];
        self.injector.trigger(source, gain, self.model_sample_rate);
    }

    fn note_off(&mut self, note: u8) {
        if self.current_note == Some(note) {
            self.current_note = None;
            self.drive_target = 0.0;
        }
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
        self.attack_envelope *= 1.0 - self.attack_coeff;
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
        self.reed.set_params(reed_params_with_mod(
            &self.patch,
            pressure_mod,
            embouchure_mod,
            1.0 - coherent_amount,
            tracking_frequency_hz,
        ));
        coherent_amount
    }

    fn process_model_sample_with_taps(&mut self, taps: Option<&mut TubeRenderTaps>) -> f32 {
        self.drive_gate += (self.drive_target - self.drive_gate) * self.gate_coeff;
        let excitation = self.injector.process();
        self.tube.set_brightness_effort(self.effort);
        let variance = TubeSteadyVariance::from_humanize(self.patch.humanize).sanitized();
        let (pressure_mod, embouchure_mod, voicing_mod) =
            self.steady_variance_source.process(variance);
        let mut params = tube_params_with_mod(
            &self.patch,
            self.sounding_note,
            self.frequency_hz,
            voicing_mod,
        );
        let coherent_amount = self.apply_register_performance(pressure_mod, embouchure_mod);
        params.reed_phase_delay_samples = self
            .reed
            .aperture_phase_delay_samples(self.frequency_hz, self.effort);
        let feedback = self.tube.driven_feedback();
        let mut reed_taps = ReedProcessTaps::default();
        let mouth_wave = self.reed.process_with_taps(
            excitation,
            self.effort,
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
        if let Some(taps) = taps {
            let mut tube_taps = ReedTubeTaps::default();
            let tube_output = self
                .tube
                .process_wind_with_taps(mouth_wave, params, &mut tube_taps);
            let output = tube_output + reed_radiated;
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
            self.tube.process_wind(mouth_wave, params) + reed_radiated
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

fn tube_params_with_mod(
    patch: &TubePatch,
    current_note: Option<u8>,
    frequency_hz: f32,
    body_formant_shift: f32,
) -> ReedTubeParams {
    let register = register_key_state(patch, current_note);
    ReedTubeParams {
        frequency_hz,
        loop_filter_cutoff_hz: brightness_hz(patch.brightness),
        loop_filter_resonance: 0.0,
        loop_gain: loop_gain_from_damping(patch.damping),
        loop_nonlinearity: 0.0,
        boundary_reflection: -0.75,
        pickup_position: 0.82,
        bell_radiation: bell_radiation_from_mix(patch.bell),
        bell_radiation_shape: patch.bell_radiation_shape,
        body_formant: patch.body_formant,
        body_formant_shift: math::finite_clamp(body_formant_shift, -2.0, 2.0, 0.0),
        register_mode_ratio: register.mode_ratio,
        register_vent_admittance: register.vent_admittance,
        register_vent_position: REGISTER_VENT_POSITION,
        body_odd_mode_projection: patch.body_odd_mode_projection,
        body_upper_odd_modes: patch.body_upper_odd_modes,
        reed_phase_delay_samples: 0.0,
        switches: ReedTubeSwitches {
            reed_enabled: true,
            bell_enabled: patch.switches.bell_enabled,
            bore_steepening_enabled: patch.switches.bore_steepening_enabled,
            body_enabled: patch.switches.body_enabled,
            clarinet_contour_enabled: patch.switches.clarinet_contour_enabled,
        },
    }
}

fn reed_params(patch: &TubePatch) -> ReedParams {
    reed_params_with_mod(patch, 0.0, 0.0, 1.0, 0.0)
}

fn reed_params_with_mod(
    patch: &TubePatch,
    pressure_mod: f32,
    embouchure_mod: f32,
    breath_noise: f32,
    tracking_frequency_hz: f32,
) -> ReedParams {
    ReedParams {
        pressure_depth: math::finite_clamp(
            patch.pressure * (1.0 + pressure_mod),
            0.0,
            1.0,
            patch.pressure,
        ),
        stiffness: patch.reed_stiffness,
        embouchure: math::finite_clamp(
            patch.embouchure + embouchure_mod,
            0.0,
            1.0,
            patch.embouchure,
        ),
        aperture_inertia: patch.reed_aperture_inertia,
        breath_noise,
        tracking_frequency_hz,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RegisterKeyState {
    mode_ratio: f32,
    vent_admittance: f32,
}

fn register_key_state(patch: &TubePatch, current_note: Option<u8>) -> RegisterKeyState {
    let break_note = math::finite_clamp(patch.register_break_note, 48.0, 96.0, 69.0).round();
    let active = current_note
        .map(|note| note as f32 >= break_note)
        .unwrap_or(false);
    if active {
        RegisterKeyState {
            mode_ratio: REGISTER_MODE_RATIO,
            vent_admittance: REGISTER_VENT_ADMITTANCE,
        }
    } else {
        RegisterKeyState {
            mode_ratio: 1.0,
            vent_admittance: 0.0,
        }
    }
}

fn brightness_hz(brightness: f32) -> f32 {
    let brightness = brightness.clamp(0.0, 1.0);
    MIN_BRIGHTNESS_HZ * (MAX_BRIGHTNESS_HZ / MIN_BRIGHTNESS_HZ).powf(brightness)
}

fn loop_gain_from_damping(damping: f32) -> f32 {
    let damping = damping.clamp(0.0, 1.0);
    MAX_LOOP_GAIN + (MIN_LOOP_GAIN - MAX_LOOP_GAIN) * damping
}

fn bell_radiation_from_mix(mix: f32) -> f32 {
    mix.clamp(0.0, 1.0).powf(BELL_MIX_EXPONENT)
}

fn gate_coeff(sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (GATE_RAMP_SECONDS * sanitize_sample_rate(sample_rate))).exp()
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
mod tests;
