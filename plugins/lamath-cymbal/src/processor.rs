use lindelion_dsp_utils::{db_to_gain, math::midi_note_to_hz};
use lindelion_idiophone::{EnergyFollower, MeshResonator, MeshVoiceParams};
use lindelion_plugin_shell::{MidiEvent, NoteEvent};

use crate::patch::CymbalPatch;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const BUILTIN_EXCITATION_SAMPLE_RATE: f32 = 48_000.0;
const INJECTOR_POOL_SIZE: usize = 16;
const CHOKE_RAMP_MS: f32 = 60.0;
const DAMP_KEY_LOW: u8 = 0;
const DAMP_KEY_HIGH: u8 = 11;

const BUILTIN_EXCITATION: [f32; 48] = [
    0.00, 0.82, -0.58, 0.41, -0.36, 0.27, -0.20, 0.16, -0.13, 0.11, -0.09, 0.08, -0.07, 0.06,
    -0.052, 0.044, -0.038, 0.033, -0.028, 0.023, -0.019, 0.016, -0.013, 0.010, -0.008, 0.006,
    -0.0048, 0.0038, -0.0030, 0.0024, -0.0019, 0.0015, -0.0012, 0.00095, -0.00074, 0.00056,
    -0.00042, 0.00031, -0.00023, 0.00017, -0.00012, 0.000085, -0.00006, 0.00004, -0.000026,
    0.000016, -0.000009, 0.0,
];

#[derive(Debug, Clone, Copy)]
pub struct ExcitationSource<'a> {
    samples: &'a [f32],
    sample_rate: f32,
}

impl<'a> ExcitationSource<'a> {
    pub const fn builtin() -> Self {
        Self {
            samples: &BUILTIN_EXCITATION,
            sample_rate: BUILTIN_EXCITATION_SAMPLE_RATE,
        }
    }

    pub fn from_samples(samples: &'a [f32], sample_rate: f32) -> Self {
        if samples.is_empty() {
            return Self::builtin();
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
            source: ExcitationSource::builtin(),
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
    excitation: ExcitationSource<'a>,
    injectors: [Injector<'a>; INJECTOR_POOL_SIZE],
    cursor: usize,
    choke_gain: f32,
    choking: bool,
    choke_step: f32,
}

impl<'a> CymbalProcessor<'a> {
    pub fn new(sample_rate: f32, patch: CymbalPatch, excitation: ExcitationSource<'a>) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let patch = patch.sanitized();
        let mut mesh = MeshResonator::new(sample_rate);
        mesh.configure(mesh_params(&patch, 60));
        Self {
            sample_rate,
            patch,
            mesh,
            energy: EnergyFollower::new(sample_rate),
            body_energy: 0.0,
            excitation,
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
        self.mesh.configure(mesh_params(&self.patch, 60));
    }

    pub fn set_excitation(&mut self, excitation: ExcitationSource<'a>) {
        self.excitation = excitation;
    }

    pub fn process(&mut self, events: &[MidiEvent], left: &mut [f32], right: &mut [f32]) {
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
            self.mesh.set_geometric_drive(self.body_energy);
            let body = self.mesh.process_sample(excitation);
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

    fn handle_events(&mut self, events: &[MidiEvent]) {
        for event in events {
            let MidiEvent::Note(note) = *event else {
                continue;
            };
            match note {
                NoteEvent::On { note, velocity, .. } if velocity > 0.0 && damp_key(note) => {
                    self.damp()
                }
                NoteEvent::On { note, velocity, .. } if velocity > 0.0 => {
                    self.strike(note, velocity)
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
        self.injectors[index].trigger(self.excitation, gain, self.sample_rate);
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
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{assert_all_finite, peak_abs, rms};

    fn note_on(note: u8, velocity: f32) -> MidiEvent {
        MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note,
            velocity,
        })
    }

    #[test]
    fn strike_produces_sustained_finite_ring() {
        let mut processor = CymbalProcessor::new(
            48_000.0,
            CymbalPatch::default(),
            ExcitationSource::builtin(),
        );
        let mut left = vec![0.0; 24_000];
        let mut right = vec![0.0; 24_000];

        processor.process(&[note_on(60, 1.0)], &mut left, &mut right);

        assert_all_finite(&left);
        assert_all_finite(&right);
        assert!(peak_abs(&left) > 0.001);
        assert!(rms(&left[12_000..]) > 0.000_001);
    }

    #[test]
    fn strike_processing_does_not_allocate() {
        let mut processor = CymbalProcessor::new(
            48_000.0,
            CymbalPatch::default(),
            ExcitationSource::builtin(),
        );
        let mut left = [0.0; 512];
        let mut right = [0.0; 512];

        crate::assert_no_allocations("lamath_cymbal_process", || {
            processor.process(&[note_on(64, 0.8)], &mut left, &mut right);
            processor.process(&[], &mut left, &mut right);
        });
    }

    #[test]
    fn damp_key_chokes_the_body() {
        let mut processor = CymbalProcessor::new(
            48_000.0,
            CymbalPatch::default(),
            ExcitationSource::builtin(),
        );
        let mut left = vec![0.0; 4_096];
        let mut right = vec![0.0; 4_096];
        processor.process(&[note_on(60, 1.0)], &mut left, &mut right);
        let before = rms(&left);

        let damp_events = [note_on(1, 1.0)];
        for block in 0..8 {
            let events = if block == 0 {
                damp_events.as_slice()
            } else {
                [].as_slice()
            };
            processor.process(events, &mut left, &mut right);
        }

        assert!(rms(&left) < before * 0.2);
    }
}
