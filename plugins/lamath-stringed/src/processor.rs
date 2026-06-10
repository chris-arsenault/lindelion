use lindelion_dsp_utils::{
    db_to_gain,
    energy::EnergyFollower,
    math::midi_note_to_hz,
    phrase::{HostExpression, PhraseEngine},
    variance::{self, SmoothNoise},
};
use lindelion_plugin_shell::{ControlEvent, MidiEvent, NoteEvent};
use lindelion_string::{
    BowParams, PickParams, StringBodyMode, StringDriver, StringDriverMode, StringModel,
    StringModelParams, StringModelProbe, StringModelSwitches,
};

use crate::patch::{BodySelection, DriverSelection, StringPatch};

pub const ARTICULATION_SLOT_COUNT: usize = 8;

mod excitation;
mod phrasing;
mod variance_source;

pub use excitation::ExcitationSource;
use excitation::Injector;
use phrasing::{PHRASE_DEVELOPMENT_START, phrase_params_for_knobs};
use variance_source::*;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const BUILTIN_SAMPLE_RATE: f32 = 48_000.0;
const OVERSAMPLE_FACTOR: usize = 2;
const KEYSWITCH_BASE_NOTE: u8 = 0;
const GATE_RAMP_SECONDS: f32 = 0.006;
const MIN_BRIGHTNESS_HZ: f32 = 900.0;
const MAX_BRIGHTNESS_HZ: f32 = 12_500.0;
const MIN_LOOP_GAIN: f32 = 0.90;
const MAX_LOOP_GAIN: f32 = 0.996;
const STRING_OUTPUT_LIMIT: f32 = 1.0;

// Humanization: the player's inputs drift, the physics stays untouched. Each
// axis is a slow random walk (`lindelion-dsp-utils::variance::SmoothNoise`)
// with its own individually-tuned full-depth range; the single `humanize`
// patch knob scales all of them linked (the Tube's humanize pattern).
//
// Left hand (both drivers): intonation wander — the finger never lands or
// holds perfectly still; it offsets the played target frequency, which the
// bowed intonation servo then *follows* (it pulls the sounding pitch to the
// wandered target rather than fighting it). Contact-position wander — where
// the bow rides or the pick strikes drifts along the string; the bow reads
// the walk continuously, the pluck samples it at each note-on (per-note
// scatter from the same source).
//
// Right hand (bow only): bow-speed and bow-pressure drift — the arm never
// holds constant velocity or weight. Depths keep the default smooth bow well
// inside the Schelleng cone at full humanize (guarded by test).
// Knob law: 50% is the nominal, intended humanization; 100% approaches the
// unmusical. The NOMINAL_* depths below are the 50% sound (audition-approved).
// Above 50% the three Schelleng axes (pressure/speed/position) grow to
// 1.5x nominal, putting the *worst-case* simultaneous walk corner right at
// the crush boundary (N/N_max ~ 1.0; the effort scale cancels out of the
// ratio) — rare brushes of crunch, not residence in it. Intonation grows
// linearly to 2x nominal (+/-8 cents) at full.
pub const ARTICULATION_NAMES: [&str; ARTICULATION_SLOT_COUNT] = [
    "Pick",
    "Sforzando",
    "Legato",
    "Staccato",
    "Marcato",
    "Tremolo",
    "Harmonic",
    "Soft",
];

#[derive(Debug)]
pub struct StringProcessor<'a> {
    sample_rate: f32,
    model_sample_rate: f32,
    patch: StringPatch,
    string: StringModel,
    driver: StringDriver,
    driver_mode: DriverSelection,
    sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    injector: Injector<'a>,
    selected_slot: usize,
    current_note: Option<u8>,
    frequency_hz: f32,
    effort: f32,
    drive_gate: f32,
    drive_target: f32,
    gate_coeff: f32,
    energy: EnergyFollower,
    energy_value: f32,
    variance_source: StringVarianceSource,
    variance_offsets: StringVarianceOffsets,
    phrase_engine: PhraseEngine,
    host_expression: HostExpression,
    // Contact-position wander sampled at note-on: the pluck's per-note strike
    // scatter, held for the duration of the note (the bow reads the walk
    // continuously instead).
    note_strike_offset: f32,
}

impl<'a> StringProcessor<'a> {
    pub fn new(
        sample_rate: f32,
        patch: StringPatch,
        sources: [ExcitationSource<'a>; ARTICULATION_SLOT_COUNT],
    ) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let model_sample_rate = sample_rate * OVERSAMPLE_FACTOR as f32;
        let patch = patch.sanitized();
        let driver_mode = active_driver_mode(&patch);
        Self {
            sample_rate,
            model_sample_rate,
            string: StringModel::new(model_sample_rate),
            driver: string_driver(driver_mode, &patch, model_sample_rate),
            driver_mode,
            selected_slot: patch.selected_articulation,
            patch,
            sources,
            injector: Injector::default(),
            current_note: None,
            frequency_hz: midi_note_to_hz(60.0),
            effort: 0.0,
            drive_gate: 0.0,
            drive_target: 0.0,
            gate_coeff: gate_coeff(model_sample_rate),
            energy: EnergyFollower::new(model_sample_rate),
            energy_value: 0.0,
            variance_source: StringVarianceSource::new(model_sample_rate),
            phrase_engine: PhraseEngine::new(model_sample_rate),
            host_expression: HostExpression::new(model_sample_rate),
            variance_offsets: StringVarianceOffsets::default(),
            note_strike_offset: 0.0,
        }
    }

    pub fn reset(&mut self, sample_rate: f32) {
        self.sample_rate = sanitize_sample_rate(sample_rate);
        self.model_sample_rate = self.sample_rate * OVERSAMPLE_FACTOR as f32;
        self.string = StringModel::new(self.model_sample_rate);
        self.driver_mode = active_driver_mode(&self.patch);
        self.driver = string_driver(self.driver_mode, &self.patch, self.model_sample_rate);
        self.injector.clear();
        self.current_note = None;
        self.frequency_hz = midi_note_to_hz(60.0);
        self.effort = 0.0;
        self.drive_gate = 0.0;
        self.drive_target = 0.0;
        self.gate_coeff = gate_coeff(self.model_sample_rate);
        self.energy = EnergyFollower::new(self.model_sample_rate);
        self.energy_value = 0.0;
        self.variance_source = StringVarianceSource::new(self.model_sample_rate);
        self.phrase_engine = PhraseEngine::new(self.model_sample_rate);
        self.host_expression = HostExpression::new(self.model_sample_rate);
        self.variance_offsets = StringVarianceOffsets::default();
        self.note_strike_offset = 0.0;
    }

    pub fn set_patch(&mut self, patch: StringPatch) {
        self.patch = patch.sanitized();
        self.selected_slot = self.patch.selected_articulation;
        let next_mode = active_driver_mode(&self.patch);
        if next_mode == self.driver_mode {
            self.driver
                .set_params(pick_params(&self.patch), bow_params(&self.patch));
        } else {
            self.driver = string_driver(next_mode, &self.patch, self.model_sample_rate);
            self.driver_mode = next_mode;
        }
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        self.process_internal(events, left, right, None);
    }

    pub fn process_with_probe(
        &mut self,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
        probes: &mut [StringModelProbe],
    ) {
        probes.fill(StringModelProbe::default());
        self.process_internal(events, left, right, Some(probes));
    }

    fn process_internal(
        &mut self,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
        mut probes: Option<&mut [StringModelProbe]>,
    ) {
        left.fill(0.0);
        right.fill(0.0);
        self.handle_events(events);
        let output_gain = db_to_gain(self.patch.output_gain_db);
        let len = left.len().min(right.len());
        for frame in 0..len {
            let mut sample = 0.0;
            let mut probe_sum = StringModelProbe::default();
            for _ in 0..OVERSAMPLE_FACTOR {
                let (model_sample, model_probe) =
                    self.process_model_sample_with_probe(probes.is_some());
                sample += model_sample;
                add_probe(&mut probe_sum, model_probe);
            }
            sample = soft_limit(sample * output_gain / OVERSAMPLE_FACTOR as f32);
            left[frame] = sample;
            right[frame] = sample;
            if let Some(probe_buffer) = probes.as_deref_mut()
                && let Some(probe) = probe_buffer.get_mut(frame)
            {
                scale_probe(&mut probe_sum, 1.0 / OVERSAMPLE_FACTOR as f32);
                *probe = probe_sum;
            }
        }
    }

    #[cfg(test)]
    pub fn selected_slot(&self) -> usize {
        self.selected_slot
    }

    fn handle_events(&mut self, events: &[MidiEvent]) {
        for event in events {
            let note = match *event {
                MidiEvent::Note(note) => note,
                MidiEvent::Control(control) => {
                    self.handle_control(control);
                    continue;
                }
            };
            match note {
                NoteEvent::On { note, velocity, .. } if velocity > 0.0 => {
                    if let Some(slot) = keyswitch_slot(note) {
                        self.select_slot(slot);
                    } else {
                        self.note_on(note, velocity);
                    }
                }
                NoteEvent::On { note, .. } | NoteEvent::Off { note, .. } => self.note_off(note),
            }
        }
    }

    /// Host performance layer: the CC dynamics line (CC1/CC11) multiplies the
    /// phrase intensity, channel pressure swells above it, pitch bend joins
    /// the played-frequency rail. Inert until the host sends something.
    fn handle_control(&mut self, control: ControlEvent) {
        match control {
            ControlEvent::ContinuousController {
                controller: 1 | 11,
                value,
                ..
            } => self.host_expression.set_expression(value),
            ControlEvent::ChannelPressure { value, .. } => {
                self.host_expression.set_aftertouch(value)
            }
            ControlEvent::PitchBend { semitones, .. } => {
                self.host_expression.set_bend_semitones(semitones)
            }
            _ => {}
        }
    }

    fn select_slot(&mut self, slot: usize) {
        self.selected_slot = slot.min(ARTICULATION_SLOT_COUNT - 1);
        self.patch.selected_articulation = self.selected_slot;
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        // Note overlap is the legato/rearticulation seam: a note arriving while
        // another is held continues the current bow stroke (legato — only the
        // stopped length moves); a note from silence starts a fresh stroke in
        // the opposite direction.
        let legato = self.current_note.is_some();
        if !legato {
            self.driver.flip_bow_stroke();
        }
        self.phrase_engine
            .note_on(velocity.clamp(0.0, 1.0), legato, PHRASE_DEVELOPMENT_START);
        // Sample the contact-position walk at the strike: per-note scatter of
        // where the pick lands, held for the note's duration.
        self.note_strike_offset = self.variance_offsets.position;
        self.current_note = Some(note);
        self.frequency_hz = midi_note_to_hz(note as f32);
        self.effort = velocity.clamp(0.0, 1.0);
        self.drive_target = 1.0;
        let source = self.sources[self.selected_slot];
        self.injector.trigger(
            source,
            articulation_gain(self.selected_slot, self.effort),
            self.model_sample_rate,
        );
    }

    fn note_off(&mut self, note: u8) {
        if self.current_note == Some(note) {
            self.current_note = None;
            self.drive_target = 0.0;
            self.phrase_engine.note_off();
        }
    }

    fn process_model_sample_with_probe(&mut self, collect_probe: bool) -> (f32, StringModelProbe) {
        self.drive_gate += (self.drive_target - self.drive_gate) * self.gate_coeff;
        // Tension modulation reads the string's own stored energy inside the
        // model; only the source↔body balance still rides the output-energy bus.
        self.string.set_balance_drive(self.energy_value);
        self.variance_offsets = self
            .variance_source
            .process(StringSteadyVariance::from_humanize(self.patch.humanize));

        // Phrasing: the living intensity signal replaces the static note-on
        // effort, vibrato joins the played-frequency rail, and the reactive
        // release holds the gate open while the taper sounds. At knob zero the
        // engine is bypassed entirely (bit-identical static behavior).
        let phrasing = self.patch.phrasing;
        let vibrato = self.patch.vibrato;
        let (effort_signal, phrase_pitch_cents) =
            if phrasing > f32::EPSILON || vibrato > f32::EPSILON {
                let outputs = self
                    .phrase_engine
                    .process(phrase_params_for_knobs(phrasing, vibrato));
                self.drive_target = if outputs.active { 1.0 } else { 0.0 };
                (outputs.intensity, outputs.pitch_lean_cents)
            } else {
                (self.effort, 0.0)
            };

        let expression = self.host_expression.process();
        let effort_signal = (effort_signal * expression.intensity_factor).clamp(0.0, 1.0);

        let excitation = self.injector.process();
        let drive = self.driver.process_excitation(excitation, effort_signal);
        let bow_contact = self.driver.bow_contact(
            effort_signal,
            self.drive_gate,
            self.variance_offsets.position,
            self.variance_offsets.speed,
            self.variance_offsets.pressure,
        );
        // Deliberate (vibrato) and involuntary (wander) pitch motion share the
        // played-frequency rail; the bowed intonation servo follows the moving
        // target instead of fighting it.
        let frequency_hz = self.frequency_hz
            * (1.0
                + CENTS_TO_RATIO
                    * (self.variance_offsets.intonation_cents
                        + phrase_pitch_cents
                        + expression.bend_cents));
        let params = string_params(&self.patch, frequency_hz, self.note_strike_offset);
        let (output, probe) = if collect_probe {
            self.string
                .process_with_bow_contact_probe(drive, params, bow_contact)
        } else {
            (
                self.string
                    .process_with_bow_contact(drive, params, bow_contact),
                StringModelProbe::default(),
            )
        };
        self.energy_value = self.energy.observe(output);
        (output, probe)
    }
}

fn add_probe(accumulator: &mut StringModelProbe, probe: StringModelProbe) {
    accumulator.pickup_tap += probe.pickup_tap;
    accumulator.body_radiated += probe.body_radiated;
    accumulator.weighted_pickup += probe.weighted_pickup;
    accumulator.weighted_body += probe.weighted_body;
    accumulator.pickup_weight += probe.pickup_weight;
    accumulator.body_weight += probe.body_weight;
    accumulator.output += probe.output;
    accumulator.bow_force += probe.bow_force;
    accumulator.bow_wave_correction += probe.bow_wave_correction;
    accumulator.current_frequency_hz += probe.current_frequency_hz;
    accumulator.one_way_delay_samples += probe.one_way_delay_samples;
}

fn scale_probe(probe: &mut StringModelProbe, scale: f32) {
    probe.pickup_tap *= scale;
    probe.body_radiated *= scale;
    probe.weighted_pickup *= scale;
    probe.weighted_body *= scale;
    probe.pickup_weight *= scale;
    probe.body_weight *= scale;
    probe.output *= scale;
    probe.bow_force *= scale;
    probe.bow_wave_correction *= scale;
    probe.current_frequency_hz *= scale;
    probe.one_way_delay_samples *= scale;
}

fn string_params(patch: &StringPatch, frequency_hz: f32, strike_offset: f32) -> StringModelParams {
    StringModelParams {
        frequency_hz,
        loop_filter_cutoff_hz: brightness_hz(patch.brightness),
        loop_filter_resonance: 0.0,
        loop_gain: loop_gain_from_damping(patch.damping),
        loop_nonlinearity: 0.0,
        dispersion: patch.stiffness,
        // Held constant per note (sampled at note-on), so the prepared-model
        // cache re-derives at most once per strike.
        strike_position: (patch.strike_position + strike_offset).clamp(0.001, 0.999),
        pickup_position: patch.pickup_position,
        excitation_spread: 0.0,
        source_body_balance: patch.body_balance,
        body_mode: body_mode(patch.body),
        switches: StringModelSwitches {
            body_contact_enabled: patch.switches.body_contact,
            tension_modulation_enabled: patch.switches.tension,
            source_body_balance_enabled: true,
        },
    }
}

fn string_driver(mode: DriverSelection, patch: &StringPatch, sample_rate: f32) -> StringDriver {
    StringDriver::from_mode(
        match mode {
            DriverSelection::None => StringDriverMode::None,
            DriverSelection::Pick => StringDriverMode::Pick,
            DriverSelection::Bow => StringDriverMode::Bow,
        },
        pick_params(patch),
        bow_params(patch),
        sample_rate,
    )
}

fn pick_params(patch: &StringPatch) -> PickParams {
    PickParams {
        hardness: 0.25 + 0.75 * patch.brightness,
        contact_time: 0.15 + 0.55 * (1.0 - patch.stiffness),
    }
}

fn bow_params(patch: &StringPatch) -> BowParams {
    BowParams {
        position: patch.bow_position,
        pressure: patch.bow_pressure,
        speed: patch.bow_speed,
        friction: patch.bow_friction,
    }
}

fn active_driver_mode(patch: &StringPatch) -> DriverSelection {
    if patch.driver == DriverSelection::Bow && !patch.switches.bow_drive {
        DriverSelection::None
    } else {
        patch.driver
    }
}

fn body_mode(body: BodySelection) -> StringBodyMode {
    match body {
        BodySelection::Disabled => StringBodyMode::Disabled,
        BodySelection::Guitar => StringBodyMode::Guitar,
        BodySelection::Violin => StringBodyMode::Violin,
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

fn keyswitch_slot(note: u8) -> Option<usize> {
    let slot = note.checked_sub(KEYSWITCH_BASE_NOTE)? as usize;
    (slot < ARTICULATION_SLOT_COUNT).then_some(slot)
}

fn articulation_gain(slot: usize, effort: f32) -> f32 {
    let accent = match slot {
        1 | 4 => 1.3,
        3 | 6 => 0.85,
        7 => 0.65,
        _ => 1.0,
    };
    accent * effort.clamp(0.0, 1.0).sqrt()
}

fn gate_coeff(sample_rate: f32) -> f32 {
    1.0 - (-1.0 / (GATE_RAMP_SECONDS * sanitize_sample_rate(sample_rate))).exp()
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
        (sample / STRING_OUTPUT_LIMIT).tanh() * STRING_OUTPUT_LIMIT
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
