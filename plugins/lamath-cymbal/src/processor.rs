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
const STRIKER_KEYSWITCH_BASE_NOTE: u8 = 0;
const DAMP_KEY_LOW: u8 = 4;
const DAMP_KEY_HIGH: u8 = 11;

pub const STRIKER_SLOT_COUNT: usize = 4;
pub const STRIKER_NAMES: [&str; STRIKER_SLOT_COUNT] =
    ["Hard stick", "Soft mallet", "Jazz brush", "Bell stick"];

const HARD_STICK_EXCITATION: [f32; 48] = [
    0.00, 0.82, -0.58, 0.41, -0.36, 0.27, -0.20, 0.16, -0.13, 0.11, -0.09, 0.08, -0.07, 0.06,
    -0.052, 0.044, -0.038, 0.033, -0.028, 0.023, -0.019, 0.016, -0.013, 0.010, -0.008, 0.006,
    -0.0048, 0.0038, -0.0030, 0.0024, -0.0019, 0.0015, -0.0012, 0.00095, -0.00074, 0.00056,
    -0.00042, 0.00031, -0.00023, 0.00017, -0.00012, 0.000085, -0.00006, 0.00004, -0.000026,
    0.000016, -0.000009, 0.0,
];

const SOFT_MALLET_EXCITATION: [f32; 64] = [
    0.00, 0.18, 0.36, 0.42, 0.34, 0.21, 0.08, -0.03, -0.09, -0.11, -0.10, -0.075, -0.050, -0.026,
    -0.006, 0.011, 0.022, 0.027, 0.026, 0.021, 0.014, 0.007, 0.001, -0.004, -0.007, -0.008, -0.007,
    -0.0055, -0.0035, -0.0015, 0.0004, 0.0018, 0.0026, 0.0028, 0.0024, 0.0017, 0.0010, 0.0003,
    -0.0002, -0.00055, -0.00072, -0.00070, -0.00056, -0.00038, -0.00020, -0.00005, 0.00008,
    0.00016, 0.00019, 0.00018, 0.00014, 0.00009, 0.00005, 0.00001, -0.00002, -0.000035, -0.00004,
    -0.000035, -0.000025, -0.000015, -0.000008, -0.000003, 0.0, 0.0,
];

const JAZZ_BRUSH_EXCITATION: [f32; 72] = [
    0.00, 0.055, -0.020, 0.080, 0.012, -0.034, 0.065, -0.016, 0.043, 0.020, -0.026, 0.046, -0.010,
    0.035, -0.020, 0.030, 0.014, -0.018, 0.026, -0.008, 0.021, -0.012, 0.017, 0.009, -0.012, 0.014,
    -0.006, 0.011, -0.007, 0.009, 0.0045, -0.0065, 0.0078, -0.0038, 0.0058, -0.0035, 0.0045,
    0.0021, -0.0034, 0.0038, -0.0019, 0.0028, -0.0017, 0.0022, 0.0010, -0.0017, 0.0018, -0.0009,
    0.0013, -0.0008, 0.00095, 0.00045, -0.00070, 0.00072, -0.00035, 0.00050, -0.00028, 0.00035,
    0.00016, -0.00024, 0.00022, -0.00010, 0.00014, -0.000075, 0.000085, 0.000035, -0.000046,
    0.000036, -0.000014, 0.000012, 0.000004, 0.0,
];

const BELL_STICK_EXCITATION: [f32; 40] = [
    0.00, 0.96, -0.82, 0.60, -0.50, 0.38, -0.31, 0.24, -0.19, 0.15, -0.12, 0.095, -0.075, 0.058,
    -0.044, 0.034, -0.026, 0.020, -0.015, 0.011, -0.0080, 0.0058, -0.0041, 0.0029, -0.0020, 0.0014,
    -0.00095, 0.00064, -0.00042, 0.00027, -0.00017, 0.00010, -0.00006, 0.000036, -0.000020,
    0.000011, -0.000005, 0.000002, 0.0, 0.0,
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
            // Energy gate: once the ring has decayed below audibility and nothing is exciting the
            // mesh, skip the whole O(cells) scatter and emit silence. The follower keeps decaying on
            // the zero output, so it stays gated until the next strike re-enters with excitation.
            let body = if self.body_energy < MESH_ENERGY_GATE && excitation.abs() <= GATE_EXCITATION
            {
                0.0
            } else {
                self.mesh.set_geometric_drive(self.body_energy);
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
mod tests;
