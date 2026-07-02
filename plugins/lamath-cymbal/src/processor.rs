use lindelion_dsp_utils::{db_to_gain, math::midi_note_to_hz};
use lindelion_idiophone::{EnergyFollower, MeshResonator, MeshVoiceParams};
use lindelion_plugin_shell::{MidiEvent, NoteEvent};

use crate::patch::CymbalPatch;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const BUILTIN_EXCITATION_SAMPLE_RATE: f32 = 48_000.0;
const INJECTOR_POOL_SIZE: usize = 16;
const CHOKE_RAMP_MS: f32 = 60.0;
/// Mesh energy (raw-body RMS) below which a non-excited voice is treated as silent and the scatter is
/// skipped. ~-100 dBFS after output gain — far below audibility and below every sustain test window.
const MESH_ENERGY_GATE: f32 = 1.0e-5;
/// Excitation magnitude under which the mesh is considered un-driven for gating purposes.
const GATE_EXCITATION: f32 = 1.0e-9;
/// Samples the energy gate stays open after the last nonzero excitation. A short strike
/// pulse ends well before its bending wave reaches the pickup (plate group velocity is a
/// fraction of a cell per sample), so gating on output energy alone would close on the
/// in-flight strike and trap it silently. 100 ms covers the slowest audible crossing.
const GATE_HOLD_SAMPLES: u32 = 4_800;
const STRIKER_KEYSWITCH_BASE_NOTE: u8 = 0;
const DAMP_KEY_LOW: u8 = 4;
const DAMP_KEY_HIGH: u8 = 11;

pub const STRIKER_SLOT_COUNT: usize = 4;
pub const STRIKER_NAMES: [&str; STRIKER_SLOT_COUNT] =
    ["Hard stick", "Soft mallet", "Jazz brush", "Bell stick"];

// Built-in striker contact-force pulses (M1, ADR-0050): unipolar raised-cosine pushes
// whose *duration* carries hardness (hard stick ~0.45 ms, soft mallet ~3.5 ms, bell
// stick ~0.2 ms; contact times per Rossing, *Science of Percussion Instruments* and
// Chaigne's Hertzian contact models). The jazz brush is eight light contacts jittered
// deterministically over ~15 ms.
const HARD_STICK_EXCITATION: [f32; 22] = [
    0.0, 0.022214, 0.086881, 0.18826, 0.31733, 0.46263, 0.61126, 0.75, 0.86653, 0.95048, 0.99442,
    0.99442, 0.95048, 0.86653, 0.75, 0.61126, 0.46263, 0.31733, 0.18826, 0.086881, 0.022214, 0.0,
];

const SOFT_MALLET_EXCITATION: [f32; 170] = [
    0.0, 0.00019004, 0.00075989, 0.0017088, 0.0030353, 0.0047378, 0.0068138, 0.0092605, 0.012074,
    0.015252, 0.018788, 0.022678, 0.026918, 0.0315, 0.036418, 0.041666, 0.047237, 0.053123,
    0.059315, 0.065805, 0.072585, 0.079644, 0.086973, 0.094562, 0.1024, 0.11048, 0.11878, 0.1273,
    0.13603, 0.14494, 0.15404, 0.1633, 0.17272, 0.18228, 0.19197, 0.20177, 0.21167, 0.22166,
    0.23173, 0.24185, 0.25202, 0.26222, 0.27244, 0.28267, 0.29288, 0.30307, 0.31322, 0.32331,
    0.33334, 0.34329, 0.35315, 0.36289, 0.37252, 0.38201, 0.39135, 0.40053, 0.40954, 0.41836,
    0.42698, 0.4354, 0.44359, 0.45155, 0.45926, 0.46672, 0.47392, 0.48084, 0.48748, 0.49382,
    0.49986, 0.50559, 0.511, 0.51608, 0.52083, 0.52525, 0.52931, 0.53302, 0.53638, 0.53938,
    0.54201, 0.54427, 0.54616, 0.54768, 0.54881, 0.54957, 0.54995, 0.54995, 0.54957, 0.54881,
    0.54768, 0.54616, 0.54427, 0.54201, 0.53938, 0.53638, 0.53302, 0.52931, 0.52525, 0.52083,
    0.51608, 0.511, 0.50559, 0.49986, 0.49382, 0.48748, 0.48084, 0.47392, 0.46672, 0.45926,
    0.45155, 0.44359, 0.4354, 0.42698, 0.41836, 0.40954, 0.40053, 0.39135, 0.38201, 0.37252,
    0.36289, 0.35315, 0.34329, 0.33334, 0.32331, 0.31322, 0.30307, 0.29288, 0.28267, 0.27244,
    0.26222, 0.25202, 0.24185, 0.23173, 0.22166, 0.21167, 0.20177, 0.19197, 0.18228, 0.17272,
    0.1633, 0.15404, 0.14494, 0.13603, 0.1273, 0.11878, 0.11048, 0.1024, 0.094562, 0.086973,
    0.079644, 0.072585, 0.065805, 0.059315, 0.053123, 0.047237, 0.041666, 0.036418, 0.0315,
    0.026918, 0.022678, 0.018788, 0.015252, 0.012074, 0.0092605, 0.0068138, 0.0047378, 0.0030353,
    0.0017088, 0.00075989, 0.00019004, 0.0,
];

const JAZZ_BRUSH_EXCITATION: [f32; 740] = [
    0.0, 0.0027107, 0.010745, 0.023812, 0.04144, 0.062991, 0.087688, 0.11464, 0.14286, 0.17135,
    0.19906, 0.225, 0.24823, 0.26791, 0.28333, 0.29392, 0.29932, 0.29932, 0.29392, 0.28333,
    0.26791, 0.24823, 0.225, 0.19906, 0.17135, 0.14286, 0.11464, 0.087688, 0.062991, 0.04144,
    0.023812, 0.010745, 0.0027107, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0029651, 0.0117, 0.025735, 0.044313, 0.066431, 0.090899, 0.1164,
    0.14155, 0.165, 0.18549, 0.2019, 0.21337, 0.21926, 0.21926, 0.21337, 0.2019, 0.18549, 0.165,
    0.14155, 0.1164, 0.090899, 0.066431, 0.044313, 0.025735, 0.0117, 0.0029651, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0016835, 0.0066903, 0.014891,
    0.026072, 0.039946, 0.056152, 0.07427, 0.093832, 0.11433, 0.13523, 0.156, 0.1761, 0.195,
    0.21222, 0.22731, 0.23987, 0.2496, 0.25622, 0.25958, 0.25958, 0.25622, 0.2496, 0.23987,
    0.22731, 0.21222, 0.195, 0.1761, 0.156, 0.13523, 0.11433, 0.093832, 0.07427, 0.056152,
    0.039946, 0.026072, 0.014891, 0.0066903, 0.0016835, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0021041, 0.0083182, 0.018352, 0.031735, 0.047843, 0.065922, 0.085127, 0.10456, 0.12331,
    0.14051, 0.15534, 0.16712, 0.17529, 0.17947, 0.17947, 0.17529, 0.16712, 0.15534, 0.14051,
    0.12331, 0.10456, 0.085127, 0.065922, 0.047843, 0.031735, 0.018352, 0.0083182, 0.0021041, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0011722, 0.0046639,
    0.010401, 0.01826, 0.028075, 0.039636, 0.052697, 0.066978, 0.082177, 0.097969, 0.11402,
    0.12998, 0.14552, 0.1603, 0.174, 0.18635, 0.19706, 0.20593, 0.21274, 0.21737, 0.21971, 0.21971,
    0.21737, 0.21274, 0.20593, 0.19706, 0.18635, 0.174, 0.1603, 0.14552, 0.12998, 0.11402,
    0.097969, 0.082177, 0.066978, 0.052697, 0.039636, 0.028075, 0.01826, 0.010401, 0.0046639,
    0.0011722, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0015353, 0.0060782,
    0.013443, 0.023327, 0.035328, 0.048952, 0.063643, 0.078799, 0.093799, 0.10803, 0.12091,
    0.13191, 0.14058, 0.14656, 0.14962, 0.14962, 0.14656, 0.14058, 0.13191, 0.12091, 0.10803,
    0.093799, 0.078799, 0.063643, 0.048952, 0.035328, 0.023327, 0.013443, 0.0060782, 0.0015353,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0012946, 0.005141, 0.011429, 0.019977, 0.030539, 0.042812, 0.056443, 0.071039,
    0.08618, 0.10143, 0.11635, 0.13052, 0.14352, 0.15498, 0.16457, 0.17201, 0.1771, 0.17968,
    0.17968, 0.1771, 0.17201, 0.16457, 0.15498, 0.14352, 0.13052, 0.11635, 0.10143, 0.08618,
    0.071039, 0.056443, 0.042812, 0.030539, 0.019977, 0.011429, 0.005141, 0.0012946, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0014028, 0.0055455, 0.012234, 0.021157, 0.031895, 0.043948, 0.056752,
    0.069707, 0.082208, 0.093671, 0.10356, 0.11141, 0.11686, 0.11965, 0.11965, 0.11686, 0.11141,
    0.10356, 0.093671, 0.082208, 0.069707, 0.056752, 0.043948, 0.031895, 0.021157, 0.012234,
    0.0055455, 0.0014028, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];

const BELL_STICK_EXCITATION: [f32; 10] = [
    0.0, 0.11698, 0.41318, 0.75, 0.96985, 0.96985, 0.75, 0.41318, 0.11698, 0.0,
];

#[derive(Debug, Clone, Copy)]
pub struct ExcitationSource<'a> {
    samples: &'a [f32],
    sample_rate: f32,
}

impl<'a> ExcitationSource<'a> {
    pub const fn builtin(slot: usize) -> Self {
        Self {
            samples: match slot {
                1 => &SOFT_MALLET_EXCITATION,
                2 => &JAZZ_BRUSH_EXCITATION,
                3 => &BELL_STICK_EXCITATION,
                _ => &HARD_STICK_EXCITATION,
            },
            sample_rate: BUILTIN_EXCITATION_SAMPLE_RATE,
        }
    }

    pub fn from_samples(samples: &'a [f32], sample_rate: f32, fallback_slot: usize) -> Self {
        if samples.is_empty() {
            return Self::builtin(fallback_slot);
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
    fn trigger(&mut self, source: ExcitationSource<'a>, gain: f32, output_sample_rate: f32) {
        self.source = source;
        self.position = 0.0;
        self.step = source.sample_rate / sanitize_sample_rate(output_sample_rate);
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
pub struct CymbalProcessor<'a> {
    sample_rate: f32,
    patch: CymbalPatch,
    mesh: MeshResonator,
    energy: EnergyFollower,
    body_energy: f32,
    sources: [ExcitationSource<'a>; STRIKER_SLOT_COUNT],
    selected_slot: usize,
    injectors: [Injector<'a>; INJECTOR_POOL_SIZE],
    cursor: usize,
    gate_hold: u32,
    choke_gain: f32,
    choking: bool,
    choke_step: f32,
}

impl<'a> CymbalProcessor<'a> {
    pub fn new(
        sample_rate: f32,
        patch: CymbalPatch,
        sources: [ExcitationSource<'a>; STRIKER_SLOT_COUNT],
    ) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let patch = patch.sanitized();
        let selected_slot = patch.selected_striker;
        let mut mesh = MeshResonator::new(sample_rate);
        mesh.configure(mesh_params(&patch, 60));
        Self {
            sample_rate,
            patch,
            mesh,
            energy: EnergyFollower::new(sample_rate),
            body_energy: 0.0,
            sources,
            selected_slot,
            injectors: [Injector::default(); INJECTOR_POOL_SIZE],
            cursor: 0,
            gate_hold: 0,
            choke_gain: 1.0,
            choking: false,
            choke_step: choke_step(sample_rate),
        }
    }

    pub fn reset(&mut self, sample_rate: f32) {
        let sample_rate = sanitize_sample_rate(sample_rate);
        self.sample_rate = sample_rate;
        self.mesh = MeshResonator::new(sample_rate);
        self.mesh.configure(mesh_params(&self.patch, 60));
        self.energy = EnergyFollower::new(sample_rate);
        self.body_energy = 0.0;
        self.injectors.fill(Injector::default());
        self.cursor = 0;
        self.gate_hold = 0;
        self.choke_gain = 1.0;
        self.choking = false;
        self.choke_step = choke_step(sample_rate);
    }

    pub fn set_patch(&mut self, patch: CymbalPatch) {
        self.patch = patch.sanitized();
        self.selected_slot = self.patch.selected_striker;
        self.mesh.configure(mesh_params(&self.patch, 60));
    }

    pub fn set_sources(&mut self, sources: [ExcitationSource<'a>; STRIKER_SLOT_COUNT]) {
        self.sources = sources;
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
        // Put the audio thread's FPU in flush-to-zero mode so the mesh's decaying ring never pays the
        // denormal penalty; the hot loop then needs no per-sample software denormal guard.
        lindelion_dsp_utils::denormal::flush_denormals_on_this_thread();
        left.fill(0.0);
        right.fill(0.0);
        self.handle_events(events);
        let len = left.len().min(right.len());
        let output_gain = db_to_gain(self.patch.output_gain_db);
        for index in 0..len {
            let excitation = self
                .injectors
                .iter_mut()
                .map(Injector::process)
                .sum::<f32>();
            // Energy gate: once the ring has decayed below audibility, nothing is exciting the
            // plate, and the post-strike hold has elapsed, skip the whole O(cells) stencil and
            // emit silence. The hold keeps the gate open while a just-injected strike is still
            // propagating toward the pickup (output energy alone would close on it and trap the
            // strike silently); the follower keeps decaying on the zero output, so it stays
            // gated until the next strike re-enters with excitation.
            if excitation.abs() > GATE_EXCITATION {
                self.gate_hold = GATE_HOLD_SAMPLES;
            }
            let body = if self.gate_hold == 0 && self.body_energy < MESH_ENERGY_GATE {
                0.0
            } else {
                self.gate_hold = self.gate_hold.saturating_sub(1);
                self.mesh.process_sample(excitation)
            };
            self.body_energy = self.energy.observe(body);
            let choke = self.next_choke_gain();
            let sample = soft_limit(body * output_gain * choke);
            left[index] = sample;
            right[index] = sample;
        }
    }

    pub fn active_injectors(&self) -> usize {
        self.injectors
            .iter()
            .filter(|injector| injector.gain > 0.0)
            .count()
    }

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
                    } else if damp_key(note) {
                        self.damp();
                    } else {
                        self.strike(note, velocity);
                    }
                }
                _ => {}
            }
        }
    }

    fn strike(&mut self, note: u8, velocity: f32) {
        self.choking = false;
        self.choke_gain = 1.0;
        self.mesh.configure(mesh_params(&self.patch, note));
        let gain = velocity.clamp(0.0, 1.0).sqrt();
        let index = self.free_injector_index();
        self.injectors[index].trigger(self.sources[self.selected_slot], gain, self.sample_rate);
    }

    fn select_slot(&mut self, slot: usize) {
        self.selected_slot = slot.min(STRIKER_SLOT_COUNT - 1);
        self.patch.selected_striker = self.selected_slot;
    }

    fn damp(&mut self) {
        self.choking = true;
    }

    fn next_choke_gain(&mut self) -> f32 {
        if !self.choking {
            return self.choke_gain;
        }
        self.choke_gain = (self.choke_gain - self.choke_step).max(0.0);
        if self.choke_gain <= 0.0 {
            self.choking = false;
            self.mesh.reset();
            self.energy.reset();
            self.body_energy = 0.0;
            self.injectors.iter_mut().for_each(Injector::clear);
        }
        self.choke_gain
    }

    fn free_injector_index(&mut self) -> usize {
        if let Some(index) = self
            .injectors
            .iter()
            .position(|injector| injector.gain <= 0.0)
        {
            return index;
        }
        let index = self.cursor;
        self.cursor = (self.cursor + 1) % self.injectors.len();
        index
    }
}

fn mesh_params(patch: &CymbalPatch, midi_note: u8) -> MeshVoiceParams {
    MeshVoiceParams {
        frequency_hz: midi_note_to_hz(midi_note as f32),
        material: patch.material,
        size: patch.size,
        damping: patch.damping,
        tension: patch.tension,
        strike_position: patch.strike_position,
        pickup_spread: patch.pickup_spread,
    }
}

fn damp_key(note: u8) -> bool {
    (DAMP_KEY_LOW..=DAMP_KEY_HIGH).contains(&note)
}

fn keyswitch_slot(note: u8) -> Option<usize> {
    let slot = note.checked_sub(STRIKER_KEYSWITCH_BASE_NOTE)? as usize;
    (slot < STRIKER_SLOT_COUNT).then_some(slot)
}

fn choke_step(sample_rate: f32) -> f32 {
    1.0 / (CHOKE_RAMP_MS * 0.001 * sanitize_sample_rate(sample_rate)).max(1.0)
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
mod balance_tests;
#[cfg(test)]
mod striker_tests;
#[cfg(test)]
mod tests;
