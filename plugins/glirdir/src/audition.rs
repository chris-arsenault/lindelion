use lindelion_dsp_utils::{
    math::midi_note_to_hz,
    smoothing::{SmoothedParam, SmoothedParamSpec},
};
use lindelion_midi::MidiClip;
use lindelion_plugin_shell::{AudioBuffer, ProcessSetup, TransportContext};

use crate::patch::AuditionSettings;

const AUDITION_VOLUME_SMOOTH_MS: f32 = 10.0;
const TWO_PI: f32 = std::f32::consts::PI * 2.0;

#[derive(Debug, Clone)]
pub struct AuditionEngine {
    playing: bool,
    position_samples: usize,
    loop_enabled: bool,
    volume: SmoothedParam,
}

impl Default for AuditionEngine {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl AuditionEngine {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            playing: false,
            position_samples: 0,
            loop_enabled: true,
            volume: SmoothedParam::with_initial(volume_spec(), sample_rate, 0.35),
        }
    }

    pub fn reset(&mut self, setup: ProcessSetup) {
        self.volume.set_sample_rate(setup.sample_rate as f32);
        self.position_samples = 0;
        self.playing = false;
    }

    pub fn play(&mut self) {
        self.playing = true;
        self.position_samples = 0;
    }

    pub fn resume(&mut self) {
        self.playing = true;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.position_samples = 0;
    }

    pub fn set_settings(&mut self, settings: AuditionSettings) {
        let settings = settings.sanitized();
        self.loop_enabled = settings.loop_enabled;
        self.volume.set_target(settings.volume);
    }

    pub fn render(
        &mut self,
        clip: Option<&MidiClip>,
        setup: ProcessSetup,
        transport: TransportContext,
        buffer: &mut AudioBuffer<'_>,
    ) {
        if !self.playing {
            return;
        }
        let Some(clip) = clip else {
            return;
        };
        if clip.notes.is_empty() {
            return;
        }

        let phrase_samples = clip_end_samples(clip, setup).max(1);
        let len = buffer.len();
        let synced_position = synced_position_samples(clip, setup, transport);
        let start_position = synced_position.unwrap_or(self.position_samples);
        for index in 0..len {
            let position = start_position + index;
            let phrase_position = if self.loop_enabled {
                position % phrase_samples
            } else {
                position
            };
            let sample =
                self.render_sample(clip, setup, phrase_position) * self.volume.next_sample();
            buffer.left[index] += sample;
            buffer.right[index] += sample;
        }

        self.position_samples = start_position.saturating_add(len);
        if !self.loop_enabled && self.position_samples >= phrase_samples {
            self.stop();
        }
    }

    fn render_sample(&self, clip: &MidiClip, setup: ProcessSetup, position_samples: usize) -> f32 {
        let mut sample = 0.0;
        for note in &clip.notes {
            let start = ticks_to_samples(note.start_tick, clip, setup);
            let end = start.saturating_add(ticks_to_samples(note.duration_ticks, clip, setup));
            if position_samples < start || position_samples >= end {
                continue;
            }

            let age = position_samples - start;
            let remaining = end - position_samples;
            let envelope = envelope(age, remaining, setup.sample_rate as f32);
            let phase = TWO_PI * midi_note_to_hz(note.midi_note as f32) * age as f32
                / setup.sample_rate.max(1.0) as f32;
            sample += phase.sin() * envelope * (note.velocity as f32 / 127.0);
        }
        sample.clamp(-1.0, 1.0)
    }

    #[cfg(test)]
    const fn is_playing(&self) -> bool {
        self.playing
    }

    #[cfg(test)]
    const fn position_samples(&self) -> usize {
        self.position_samples
    }

    #[cfg(test)]
    const fn current_volume(&self) -> f32 {
        self.volume.current()
    }
}

fn volume_spec() -> SmoothedParamSpec {
    SmoothedParamSpec::new(0.0, 1.0, 0.35, AUDITION_VOLUME_SMOOTH_MS, 0.000_1)
}

fn clip_end_samples(clip: &MidiClip, setup: ProcessSetup) -> usize {
    clip.notes
        .iter()
        .map(|note| note.start_tick.saturating_add(note.duration_ticks))
        .map(|tick| ticks_to_samples(tick, clip, setup))
        .max()
        .unwrap_or(0)
}

fn synced_position_samples(
    clip: &MidiClip,
    setup: ProcessSetup,
    transport: TransportContext,
) -> Option<usize> {
    if !transport.playing {
        return None;
    }
    if let Some(project_quarter_note) = transport.project_quarter_note
        && project_quarter_note.is_finite()
        && project_quarter_note >= 0.0
    {
        let seconds = project_quarter_note * 60.0 / f64::from(clip.bpm.max(1));
        return Some((seconds * setup.sample_rate.max(1.0)).round() as usize);
    }
    transport
        .sample_position
        .filter(|position| *position >= 0)
        .map(|position| position as usize)
}

fn ticks_to_samples(ticks: u32, clip: &MidiClip, setup: ProcessSetup) -> usize {
    let beats = ticks as f64 / clip.ppq.max(1) as f64;
    let seconds = beats * 60.0 / f64::from(clip.bpm.max(1));
    (seconds * setup.sample_rate.max(1.0)).round() as usize
}

fn envelope(age_samples: usize, remaining_samples: usize, sample_rate: f32) -> f32 {
    let attack_samples = (sample_rate * 0.010).round().max(1.0) as usize;
    let release_samples = (sample_rate * 0.200).round().max(1.0) as usize;
    let attack = age_samples as f32 / attack_samples as f32;
    let release = remaining_samples as f32 / release_samples as f32;
    attack.min(release).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_midi::QuantizedNote;
    use lindelion_plugin_shell::{AudioBuffer, ProcessMode};

    #[test]
    fn audition_renders_finite_audio_from_shared_midi_clip() {
        let setup = ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        };
        let clip = MidiClip {
            ppq: 960,
            bpm: 120,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            notes: vec![QuantizedNote {
                start_tick: 0,
                duration_ticks: 960,
                midi_note: 69,
                velocity: 100,
            }],
        };
        let mut audition = AuditionEngine::new(setup.sample_rate as f32);
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        audition.play();
        render_block(
            &mut audition,
            &clip,
            setup,
            TransportContext::default(),
            &mut left,
            &mut right,
        );

        assert!(left.iter().all(|sample| sample.is_finite()));
        assert!(right.iter().all(|sample| sample.is_finite()));
        assert!(left.iter().any(|sample| sample.abs() > 0.000_001));
    }

    #[test]
    fn render_does_not_allocate() {
        let setup = setup();
        let clip = test_clip();
        let mut audition = AuditionEngine::new(setup.sample_rate as f32);
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        audition.play();
        crate::assert_no_allocations("glirdir audition render", || {
            render_block(
                &mut audition,
                &clip,
                setup,
                TransportContext::default(),
                &mut left,
                &mut right,
            );
        });
    }

    #[test]
    fn loop_wraps_playhead_inside_clip() {
        let setup = setup();
        let clip = test_clip();
        let mut audition = AuditionEngine::new(setup.sample_rate as f32);
        let mut left = vec![0.0; 50_000];
        let mut right = vec![0.0; 50_000];

        audition.play();
        render_block(
            &mut audition,
            &clip,
            setup,
            TransportContext::default(),
            &mut left,
            &mut right,
        );

        assert!(audition.is_playing());
        assert!(
            left[48_010..48_200]
                .iter()
                .any(|sample| sample.abs() > 0.000_001)
        );
    }

    #[test]
    fn stop_clears_playhead() {
        let setup = setup();
        let clip = test_clip();
        let mut audition = AuditionEngine::new(setup.sample_rate as f32);
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        audition.play();
        render_block(
            &mut audition,
            &clip,
            setup,
            TransportContext::default(),
            &mut left,
            &mut right,
        );
        assert!(audition.position_samples() > 0);

        audition.stop();

        assert!(!audition.is_playing());
        assert_eq!(audition.position_samples(), 0);
    }

    #[test]
    fn resume_keeps_current_playhead() {
        let setup = setup();
        let clip = test_clip();
        let mut audition = AuditionEngine::new(setup.sample_rate as f32);
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        audition.play();
        render_block(
            &mut audition,
            &clip,
            setup,
            TransportContext::default(),
            &mut left,
            &mut right,
        );
        let position = audition.position_samples();

        audition.resume();

        assert!(audition.is_playing());
        assert_eq!(audition.position_samples(), position);
    }

    #[test]
    fn volume_changes_are_smoothed() {
        let mut audition = AuditionEngine::new(1_000.0);

        audition.set_settings(AuditionSettings {
            volume: 0.0,
            ..AuditionSettings::default()
        });

        assert_eq!(audition.volume.target(), 0.0);
        assert!(audition.volume.is_smoothing());
        assert!(audition.current_volume() > 0.0);
    }

    #[test]
    fn host_transport_syncs_audition_position_to_project_beats() {
        let setup = setup();
        let clip = test_clip();
        let mut audition = AuditionEngine::new(setup.sample_rate as f32);
        let mut left = vec![0.0; 512];
        let mut right = vec![0.0; 512];

        audition.play();
        render_block(
            &mut audition,
            &clip,
            setup,
            TransportContext {
                playing: true,
                project_quarter_note: Some(1.0),
                ..TransportContext::default()
            },
            &mut left,
            &mut right,
        );

        assert_eq!(audition.position_samples(), 24_512);
    }

    fn setup() -> ProcessSetup {
        ProcessSetup {
            sample_rate: 48_000.0,
            max_block_size: 512,
            mode: ProcessMode::Realtime,
        }
    }

    fn test_clip() -> MidiClip {
        MidiClip {
            ppq: 960,
            bpm: 120,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            notes: vec![QuantizedNote {
                start_tick: 0,
                duration_ticks: 960,
                midi_note: 69,
                velocity: 100,
            }],
        }
    }

    fn render_block(
        audition: &mut AuditionEngine,
        clip: &MidiClip,
        setup: ProcessSetup,
        transport: TransportContext,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        let mut buffer = AudioBuffer { left, right };
        audition.render(Some(clip), setup, transport, &mut buffer);
    }
}
