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
use std::sync::atomic::{AtomicU32, Ordering};

use crate::patch::TubePatch;

pub const ARTICULATION_SLOT_COUNT: usize = 8;
pub const TUBE_RENDER_TAP_COUNT: usize = 38;
pub const TUBE_RENDER_TAP_NAMES: [&str; TUBE_RENDER_TAP_COUNT] = [
    "excitation",
    "drive_gate",
    "effort",
    "pressure_mod",
    "embouchure_mod",
    "voicing_mod",
    "feedback",
    "reed_breath",
    "reed_delta_p",
    "reed_aperture",
    "reed_raw_flow",
    "reed_source_flow",
    "reed_output",
    "reed_radiated",
    "mouth_wave",
    "mouth_incident",
    "mouth_filtered",
    "mouth_reflection",
    "bell_incident",
    "bell_reflection",
    "bell_pressure",
    "pickup_left",
    "pickup_right",
    "pickup_pressure",
    "pickup_flow",
    "pickup_sample",
    "body_input",
    "body_output",
    "body_reaction_flow",
    "bell_radiated",
    "register_vent_flow",
    "register_vent_output",
    "body_bell_sum",
    "tube_main_output",
    "tube_final_output",
    "final_pre_gain",
    "final_post_gain",
    "final_output",
];

const FINAL_PRE_GAIN_TAP_INDEX: usize = 35;
const FINAL_POST_GAIN_TAP_INDEX: usize = 36;
const FINAL_OUTPUT_TAP_INDEX: usize = 37;

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
const PRESSURE_VARIANCE_TARGET_SECONDS: f32 = 0.37;
const PRESSURE_VARIANCE_SMOOTH_SECONDS: f32 = 0.26;
const EMBOUCHURE_VARIANCE_TARGET_SECONDS: f32 = 0.53;
const EMBOUCHURE_VARIANCE_SMOOTH_SECONDS: f32 = 0.42;
const VOICING_VARIANCE_TARGET_SECONDS: f32 = 0.71;
const VOICING_VARIANCE_SMOOTH_SECONDS: f32 = 0.55;
const HUMANIZE_PRESSURE_DEPTH: f32 = 0.37;
const HUMANIZE_EMBOUCHURE_DEPTH: f32 = 0.35;
const HUMANIZE_VOICING_DEPTH: f32 = 2.0;
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
const REGISTER_MODE_RATIO: f32 = 2.88;
const REGISTER_VENT_ADMITTANCE: f32 = 1.20;
const REGISTER_VENT_POSITION: f32 = 1.0 / 3.0;
const PRESSURE_WALK_TAG: u32 = 0xA511_E9B3;
const EMBOUCHURE_WALK_TAG: u32 = 0x63D8_35AF;
const VOICING_WALK_TAG: u32 = 0xD1B5_4A32;

static NEXT_VARIANCE_INSTANCE: AtomicU32 = AtomicU32::new(0x6C8E_9CF5);

const TONGUE: &[f32] = &[
    0.00, 0.46, -0.30, 0.18, -0.12, 0.08, -0.055, 0.038, -0.026, 0.018, -0.012, 0.008, -0.005,
    0.003, -0.0018, 0.001, 0.0,
];
const SFORZANDO: &[f32] = &[
    0.00, 0.86, -0.62, 0.44, -0.34, 0.26, -0.20, 0.16, -0.12, 0.095, -0.073, 0.056, -0.043, 0.032,
    -0.024, 0.017, -0.012, 0.008, -0.005, 0.002, 0.0,
];
const LEGATO: &[f32] = &[0.0, 0.035, -0.018, 0.009, -0.004, 0.0];
const STACCATO: &[f32] = &[0.0, 0.62, -0.48, 0.22, -0.08, 0.018, 0.0];
const MARCATO: &[f32] = &[
    0.0, 0.58, -0.36, 0.23, -0.17, 0.13, -0.10, 0.076, -0.055, 0.038, -0.024, 0.012, 0.0,
];
const BREATH: &[f32] = &[
    0.0, 0.08, -0.03, 0.07, -0.02, 0.05, -0.015, 0.035, -0.012, 0.025, -0.01, 0.016, -0.006, 0.0,
];
const ACCENT: &[f32] = &[
    0.0, 0.70, -0.42, 0.18, -0.06, 0.04, -0.03, 0.02, -0.012, 0.006, 0.0,
];
const SLUR: &[f32] = &[0.0, 0.12, -0.04, 0.028, -0.014, 0.006, 0.0];

pub const ARTICULATION_NAMES: [&str; ARTICULATION_SLOT_COUNT] = [
    "Tongue",
    "Sforzando",
    "Legato",
    "Staccato",
    "Marcato",
    "Breath",
    "Accent",
    "Slur",
];

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TubeRenderTaps {
    pub excitation: f32,
    pub drive_gate: f32,
    pub effort: f32,
    pub pressure_mod: f32,
    pub embouchure_mod: f32,
    pub voicing_mod: f32,
    pub feedback: f32,
    pub reed_breath: f32,
    pub reed_delta_p: f32,
    pub reed_aperture: f32,
    pub reed_raw_flow: f32,
    pub reed_source_flow: f32,
    pub reed_output: f32,
    pub reed_radiated: f32,
    pub mouth_wave: f32,
    pub mouth_incident: f32,
    pub mouth_filtered: f32,
    pub mouth_reflection: f32,
    pub bell_incident: f32,
    pub bell_reflection: f32,
    pub bell_pressure: f32,
    pub pickup_left: f32,
    pub pickup_right: f32,
    pub pickup_pressure: f32,
    pub pickup_flow: f32,
    pub pickup_sample: f32,
    pub body_input: f32,
    pub body_output: f32,
    pub body_reaction_flow: f32,
    pub bell_radiated: f32,
    pub register_vent_flow: f32,
    pub register_vent_output: f32,
    pub body_bell_sum: f32,
    pub tube_main_output: f32,
    pub tube_final_output: f32,
    pub final_pre_gain: f32,
    pub final_post_gain: f32,
    pub final_output: f32,
}

impl TubeRenderTaps {
    pub fn values(self) -> [f32; TUBE_RENDER_TAP_COUNT] {
        [
            self.excitation,
            self.drive_gate,
            self.effort,
            self.pressure_mod,
            self.embouchure_mod,
            self.voicing_mod,
            self.feedback,
            self.reed_breath,
            self.reed_delta_p,
            self.reed_aperture,
            self.reed_raw_flow,
            self.reed_source_flow,
            self.reed_output,
            self.reed_radiated,
            self.mouth_wave,
            self.mouth_incident,
            self.mouth_filtered,
            self.mouth_reflection,
            self.bell_incident,
            self.bell_reflection,
            self.bell_pressure,
            self.pickup_left,
            self.pickup_right,
            self.pickup_pressure,
            self.pickup_flow,
            self.pickup_sample,
            self.body_input,
            self.body_output,
            self.body_reaction_flow,
            self.bell_radiated,
            self.register_vent_flow,
            self.register_vent_output,
            self.body_bell_sum,
            self.tube_main_output,
            self.tube_final_output,
            self.final_pre_gain,
            self.final_post_gain,
            self.final_output,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TubeSteadyVariance {
    pressure_depth: f32,
    embouchure_depth: f32,
    voicing_depth: f32,
}

impl TubeSteadyVariance {
    const OFF: Self = Self {
        pressure_depth: 0.0,
        embouchure_depth: 0.0,
        voicing_depth: 0.0,
    };

    const fn with_voicing(pressure_depth: f32, embouchure_depth: f32, voicing_depth: f32) -> Self {
        Self {
            pressure_depth,
            embouchure_depth,
            voicing_depth,
        }
    }

    fn from_humanize(humanize: f32) -> Self {
        let humanize = math::finite_clamp(humanize, 0.0, 1.0, 0.0);
        if humanize <= f32::EPSILON {
            return Self::OFF;
        }
        Self::with_voicing(
            HUMANIZE_PRESSURE_DEPTH * humanize,
            HUMANIZE_EMBOUCHURE_DEPTH * humanize,
            HUMANIZE_VOICING_DEPTH * humanize,
        )
    }

    fn sanitized(self) -> Self {
        Self {
            pressure_depth: self.pressure_depth.clamp(0.0, 0.95),
            embouchure_depth: self.embouchure_depth.clamp(0.0, 0.45),
            voicing_depth: self.voicing_depth.clamp(0.0, 2.0),
        }
    }

    fn is_active(self) -> bool {
        self.pressure_depth > 0.0 || self.embouchure_depth > 0.0 || self.voicing_depth > 0.0
    }
}

impl Default for TubeSteadyVariance {
    fn default() -> Self {
        Self::OFF
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExcitationSource<'a> {
    samples: &'a [f32],
    sample_rate: f32,
}

impl<'a> ExcitationSource<'a> {
    pub const fn builtin(slot: usize) -> Self {
        Self {
            samples: builtin_samples(slot),
            sample_rate: BUILTIN_EXCITATION_SAMPLE_RATE,
        }
    }

    pub fn from_samples(samples: &'a [f32], sample_rate: f32, slot: usize) -> Self {
        if samples.is_empty() {
            return Self::builtin(slot);
        }
        Self {
            samples,
            sample_rate: sanitize_sample_rate(sample_rate),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Injector<'a> {
    source: ExcitationSource<'a>,
    position: f32,
    step: f32,
    gain: f32,
}

#[derive(Debug, Clone, Copy)]
struct SteadyVarianceSource {
    pressure: SmoothNoise,
    embouchure: SmoothNoise,
    voicing: SmoothNoise,
}

impl SteadyVarianceSource {
    fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let instance = NEXT_VARIANCE_INSTANCE.fetch_add(0x9E37_79B9, Ordering::Relaxed);
        Self {
            pressure: SmoothNoise::new(
                sample_rate,
                PRESSURE_VARIANCE_TARGET_SECONDS,
                PRESSURE_VARIANCE_SMOOTH_SECONDS,
                walk_state(instance, PRESSURE_WALK_TAG),
            ),
            embouchure: SmoothNoise::new(
                sample_rate,
                EMBOUCHURE_VARIANCE_TARGET_SECONDS,
                EMBOUCHURE_VARIANCE_SMOOTH_SECONDS,
                walk_state(instance, EMBOUCHURE_WALK_TAG),
            ),
            voicing: SmoothNoise::new(
                sample_rate,
                VOICING_VARIANCE_TARGET_SECONDS,
                VOICING_VARIANCE_SMOOTH_SECONDS,
                walk_state(instance, VOICING_WALK_TAG),
            ),
        }
    }

    fn reset(&mut self, sample_rate: f32) {
        *self = Self::new(sample_rate);
    }

    fn process(&mut self, variance: TubeSteadyVariance) -> (f32, f32, f32) {
        if variance.is_active() {
            (
                self.pressure.process() * variance.pressure_depth,
                self.embouchure.process() * variance.embouchure_depth,
                self.voicing.process() * variance.voicing_depth,
            )
        } else {
            (0.0, 0.0, 0.0)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SmoothNoise {
    state: u32,
    value: f32,
    target: f32,
    coeff: f32,
    target_samples: usize,
    samples_until_target: usize,
}

impl SmoothNoise {
    fn new(sample_rate: f32, target_seconds: f32, smooth_seconds: f32, seed: u32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let target_samples = ((target_seconds.max(0.001) * sample_rate).round() as usize).max(1);
        let coeff = 1.0 - (-1.0 / (smooth_seconds.max(0.001) * sample_rate)).exp();
        let mut noise = Self {
            state: seed,
            value: 0.0,
            target: 0.0,
            coeff,
            target_samples,
            samples_until_target: target_samples,
        };
        let initial = noise.next_noise();
        noise.value = initial;
        noise.target = initial;
        noise
    }

    fn process(&mut self) -> f32 {
        if self.samples_until_target == 0 {
            self.target = self.next_noise();
            self.samples_until_target = self.target_samples;
        }
        self.samples_until_target = self.samples_until_target.saturating_sub(1);
        self.value += (self.target - self.value) * self.coeff;
        self.value
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

impl Default for Injector<'_> {
    fn default() -> Self {
        Self {
            source: ExcitationSource::builtin(0),
            position: 0.0,
            step: 1.0,
            gain: 0.0,
        }
    }
}

impl<'a> Injector<'a> {
    fn trigger(&mut self, source: ExcitationSource<'a>, gain: f32, model_sample_rate: f32) {
        self.source = source;
        self.position = 0.0;
        self.step = source.sample_rate / sanitize_sample_rate(model_sample_rate);
        self.gain = gain.clamp(0.0, 2.0);
    }

    fn clear(&mut self) {
        self.gain = 0.0;
        self.position = 0.0;
    }

    fn process(&mut self) -> f32 {
        if self.gain <= 0.0 {
            return 0.0;
        }
        let index = self.position.floor() as usize;
        if index >= self.source.samples.len() {
            self.clear();
            return 0.0;
        }
        let next = (index + 1).min(self.source.samples.len() - 1);
        let fraction = self.position - index as f32;
        let a = self.source.samples[index];
        let b = self.source.samples[next];
        self.position += self.step.max(0.000_001);
        (a + (b - a) * fraction) * self.gain
    }
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
    frequency_hz: f32,
    effort: f32,
    drive_gate: f32,
    drive_target: f32,
    gate_coeff: f32,
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
            frequency_hz: midi_note_to_hz(60.0),
            effort: 0.0,
            drive_gate: 0.0,
            drive_target: 0.0,
            gate_coeff: gate_coeff(model_sample_rate),
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
        self.reed_radiation_edge_highpass =
            reed_radiation_edge_highpass(self.model_sample_rate);
        self.reed_radiation_rolloff_a = reed_radiation_rolloff(self.model_sample_rate);
        self.reed_radiation_rolloff_b = reed_radiation_rolloff(self.model_sample_rate);
        self.reed_radiation_slot_rolloff = reed_radiation_slot_rolloff(self.model_sample_rate);
        self.injector.clear();
        self.current_note = None;
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
        self.frequency_hz = midi_note_to_hz(note as f32);
        self.effort = velocity.clamp(0.0, 1.0);
        self.drive_target = 1.0;
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

    fn process_model_sample_with_taps(&mut self, taps: Option<&mut TubeRenderTaps>) -> f32 {
        self.drive_gate += (self.drive_target - self.drive_gate) * self.gate_coeff;
        let excitation = self.injector.process();
        self.tube.set_brightness_effort(self.effort);
        let variance = TubeSteadyVariance::from_humanize(self.patch.humanize).sanitized();
        let (pressure_mod, embouchure_mod, voicing_mod) =
            self.steady_variance_source.process(variance);
        let mut params = tube_params_with_mod(
            &self.patch,
            self.current_note,
            self.frequency_hz,
            voicing_mod,
        );
        self.reed.set_params(reed_params_with_mod(
            &self.patch,
            pressure_mod,
            embouchure_mod,
        ));
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
        let reed_radiated = if self.patch.switches.reed_radiation_enabled {
            self.reed_radiated_sample(reed_taps.source_flow)
        } else {
            0.0
        };
        if let Some(taps) = taps {
            let mut tube_taps = ReedTubeTaps::default();
            let tube_output =
                self.tube
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
    reed_params_with_mod(patch, 0.0, 0.0)
}

fn reed_params_with_mod(patch: &TubePatch, pressure_mod: f32, embouchure_mod: f32) -> ReedParams {
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

fn keyswitch_slot(note: u8) -> Option<usize> {
    let slot = note.checked_sub(KEYSWITCH_BASE_NOTE)? as usize;
    (slot < ARTICULATION_SLOT_COUNT).then_some(slot)
}

const fn builtin_samples(slot: usize) -> &'static [f32] {
    match slot {
        0 => TONGUE,
        1 => SFORZANDO,
        2 => LEGATO,
        3 => STACCATO,
        4 => MARCATO,
        5 => BREATH,
        6 => ACCENT,
        _ => SLUR,
    }
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

fn walk_state(instance: u32, tag: u32) -> u32 {
    let mut x = instance ^ tag;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^ (x >> 16)
}

#[cfg(test)]
mod tests;
