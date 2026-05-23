use lindelion_plugin_shell::{AudioInputBuffer, ProcessSetup, TransportContext};

use crate::patch::{CaptureSettings, CaptureState, ScratchpadAudio, ScratchpadMetadata, SyncMode};

const MAX_CAPTURE_BARS: u8 = 16;
const MIN_CAPTURE_BPM: f64 = 60.0;
const DEFAULT_BEATS_PER_BAR: f64 = 4.0;
const DOWNBEAT_EPSILON_BEATS: f64 = 0.02;

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureEvent {
    None,
    Completed,
}

#[derive(Debug, Clone)]
pub struct CaptureEngine {
    state: CaptureState,
    sample_rate: u32,
    buffer: Vec<f32>,
    write_len: usize,
    target_samples: usize,
    count_in_remaining_samples: usize,
    capture_metadata: ScratchpadMetadata,
}

impl Default for CaptureEngine {
    fn default() -> Self {
        let sample_rate = 48_000;
        Self {
            state: CaptureState::Idle,
            sample_rate,
            buffer: vec![0.0; max_capture_samples(sample_rate)],
            write_len: 0,
            target_samples: 0,
            count_in_remaining_samples: 0,
            capture_metadata: ScratchpadMetadata::default(),
        }
    }
}

impl CaptureEngine {
    pub fn reset(&mut self, setup: ProcessSetup) {
        self.sample_rate = setup.sample_rate.round().clamp(1.0, u32::MAX as f64) as u32;
        let max_samples = max_capture_samples(self.sample_rate);
        if self.buffer.len() != max_samples {
            self.buffer = vec![0.0; max_samples];
        }
        self.write_len = 0;
        self.target_samples = 0;
        self.count_in_remaining_samples = 0;
        self.capture_metadata = ScratchpadMetadata::default();
        self.state = CaptureState::Idle;
    }

    pub const fn state(&self) -> CaptureState {
        self.state
    }

    pub fn arm(&mut self) {
        self.write_len = 0;
        self.target_samples = 0;
        self.count_in_remaining_samples = 0;
        self.capture_metadata = ScratchpadMetadata::default();
        self.state = CaptureState::Armed;
    }

    pub fn clear(&mut self) {
        self.write_len = 0;
        self.target_samples = 0;
        self.count_in_remaining_samples = 0;
        self.capture_metadata = ScratchpadMetadata::default();
        self.state = CaptureState::Idle;
    }

    pub fn take_completed_scratchpad(&mut self) -> Option<ScratchpadAudio> {
        if self.state != CaptureState::Captured || self.write_len == 0 {
            return None;
        }
        let scratchpad = ScratchpadAudio::with_metadata(
            self.sample_rate,
            self.capture_metadata,
            self.buffer[..self.write_len].to_vec(),
        );
        self.write_len = 0;
        Some(scratchpad)
    }

    pub fn process(
        &mut self,
        input: AudioInputBuffer<'_>,
        setup: ProcessSetup,
        transport: TransportContext,
        settings: CaptureSettings,
    ) -> CaptureEvent {
        let settings = settings.sanitized();
        self.sample_rate = setup.sample_rate.round().clamp(1.0, u32::MAX as f64) as u32;

        match self.state {
            CaptureState::Idle | CaptureState::Captured => CaptureEvent::None,
            CaptureState::Armed => {
                if trigger_met(settings, transport, setup) {
                    self.start_count_in_or_capture(settings, transport);
                    self.process(input, setup, transport, settings)
                } else {
                    CaptureEvent::None
                }
            }
            CaptureState::CountIn => {
                if capture_paused(settings, transport) {
                    return CaptureEvent::None;
                }
                let consumed = input.len().max(setup.max_block_size);
                self.count_in_remaining_samples =
                    self.count_in_remaining_samples.saturating_sub(consumed);
                if self.count_in_remaining_samples == 0 {
                    self.start_capture(settings, transport);
                    self.process(input, setup, transport, settings)
                } else {
                    CaptureEvent::None
                }
            }
            CaptureState::Capturing => {
                if capture_paused(settings, transport) {
                    return CaptureEvent::None;
                }
                self.write_input(input);
                if self.write_len >= self.target_samples {
                    self.finish_capture()
                } else {
                    CaptureEvent::None
                }
            }
        }
    }

    fn start_count_in_or_capture(
        &mut self,
        settings: CaptureSettings,
        transport: TransportContext,
    ) {
        if settings.count_in_bars == 0 {
            self.start_capture(settings, transport);
            return;
        }

        let beats = settings.count_in_bars as f64 * transport.beats_per_bar();
        let seconds = beats * 60.0 / transport.tempo_bpm_or(120.0);
        self.count_in_remaining_samples = seconds_to_samples(seconds, self.sample_rate).max(1);
        self.state = CaptureState::CountIn;
    }

    fn start_capture(&mut self, settings: CaptureSettings, transport: TransportContext) {
        self.write_len = 0;
        self.target_samples = capture_samples(settings, transport, self.sample_rate)
            .min(self.buffer.len())
            .max(1);
        self.capture_metadata = capture_metadata(settings, transport);
        self.state = CaptureState::Capturing;
    }

    fn write_input(&mut self, input: AudioInputBuffer<'_>) {
        if input.is_empty() || self.write_len >= self.target_samples {
            return;
        }

        let writable = self
            .target_samples
            .saturating_sub(self.write_len)
            .min(input.len());
        let end = self.write_len + writable;
        input.write_mono_to(&mut self.buffer[self.write_len..end]);
        self.write_len = end;
    }

    fn finish_capture(&mut self) -> CaptureEvent {
        self.state = CaptureState::Captured;
        CaptureEvent::Completed
    }
}

pub(crate) fn capture_samples(
    settings: CaptureSettings,
    transport: TransportContext,
    sample_rate: u32,
) -> usize {
    let settings = settings.sanitized();
    let beats = settings.bars.bars() as f64 * transport.beats_per_bar();
    let seconds = beats * 60.0 / transport.tempo_bpm_or(120.0);
    seconds_to_samples(seconds, sample_rate)
}

fn max_capture_samples(sample_rate: u32) -> usize {
    let beats = MAX_CAPTURE_BARS as f64 * DEFAULT_BEATS_PER_BAR;
    seconds_to_samples(beats * 60.0 / MIN_CAPTURE_BPM, sample_rate)
}

fn trigger_met(
    settings: CaptureSettings,
    transport: TransportContext,
    setup: ProcessSetup,
) -> bool {
    match settings.sync_mode {
        SyncMode::Immediate => true,
        SyncMode::NextDownbeat | SyncMode::PhraseBoundary => {
            if !transport.playing {
                return false;
            }
            let Some(bar_position) = transport.bar_position_quarter_note else {
                return false;
            };
            let beats_per_bar = transport.beats_per_bar().max(1.0);
            let bars = bar_position / beats_per_bar;
            let block_beats = setup.max_block_size as f64 / setup.sample_rate.max(1.0)
                * transport.tempo_bpm_or(120.0)
                / 60.0;
            let epsilon = DOWNBEAT_EPSILON_BEATS.max(block_beats);
            let downbeat_distance = bar_position.rem_euclid(beats_per_bar);
            let at_downbeat =
                downbeat_distance <= epsilon || beats_per_bar - downbeat_distance <= epsilon;

            if settings.sync_mode == SyncMode::NextDownbeat {
                return at_downbeat;
            }

            let phrase = settings.bars.bars().max(1) as f64;
            let phrase_distance = bars.rem_euclid(phrase);
            at_downbeat && (phrase_distance <= epsilon || phrase - phrase_distance <= epsilon)
        }
    }
}

fn capture_paused(settings: CaptureSettings, transport: TransportContext) -> bool {
    settings.sync_mode != SyncMode::Immediate && !transport.playing
}

fn capture_metadata(settings: CaptureSettings, transport: TransportContext) -> ScratchpadMetadata {
    let signature = transport.time_signature_or_default();
    ScratchpadMetadata::new(
        transport.tempo_bpm_or(120.0),
        signature.numerator,
        signature.denominator,
        settings.sanitized().bars.bars(),
    )
}

fn seconds_to_samples(seconds: f64, sample_rate: u32) -> usize {
    (seconds.max(0.0) * sample_rate.max(1) as f64).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_plugin_shell::{AudioInputBuffer, ProcessMode, TimeSignature};

    #[test]
    fn immediate_capture_writes_summed_mono_until_target_length() {
        let setup = ProcessSetup {
            sample_rate: 10.0,
            max_block_size: 80,
            mode: ProcessMode::Realtime,
        };
        let mut engine = CaptureEngine::default();
        engine.reset(setup);
        engine.arm();
        let input = vec![0.5; 80];

        let event = engine.process(
            AudioInputBuffer::mono(&input),
            setup,
            TransportContext::default(),
            CaptureSettings::default(),
        );

        let CaptureEvent::Completed = event else {
            panic!("expected capture completion");
        };
        let scratchpad = engine.take_completed_scratchpad().unwrap();
        assert_eq!(engine.state(), CaptureState::Captured);
        assert_eq!(scratchpad.sample_rate, 10);
        assert_eq!(scratchpad.metadata.bpm, 120);
        assert_eq!(scratchpad.metadata.time_signature_numerator, 4);
        assert_eq!(scratchpad.metadata.time_signature_denominator, 4);
        assert_eq!(scratchpad.samples.len(), 80);
        assert_eq!(scratchpad.samples[0], 0.5);
        assert!(engine.take_completed_scratchpad().is_none());
    }

    #[test]
    fn capture_stores_host_musical_context() {
        let setup = ProcessSetup {
            sample_rate: 10.0,
            max_block_size: 280,
            mode: ProcessMode::Realtime,
        };
        let transport = TransportContext {
            tempo_bpm: Some(135.0),
            time_signature: Some(TimeSignature::new(7, 8)),
            ..TransportContext::default()
        };
        let input = vec![0.5; 280];
        let mut engine = CaptureEngine::default();

        engine.reset(setup);
        engine.arm();
        assert_eq!(
            engine.process(
                AudioInputBuffer::mono(&input),
                setup,
                transport,
                CaptureSettings::default(),
            ),
            CaptureEvent::Completed
        );

        let scratchpad = engine.take_completed_scratchpad().unwrap();
        assert_eq!(scratchpad.metadata.bpm, 135);
        assert_eq!(scratchpad.metadata.time_signature_numerator, 7);
        assert_eq!(scratchpad.metadata.time_signature_denominator, 8);
        assert_eq!(scratchpad.metadata.capture_bars, 4);
    }

    #[test]
    fn capture_state_transitions_through_count_in_and_clear() {
        let setup = ProcessSetup {
            sample_rate: 10.0,
            max_block_size: 1,
            mode: ProcessMode::Realtime,
        };
        let settings = CaptureSettings {
            count_in_bars: 1,
            ..CaptureSettings::default()
        };
        let transport = TransportContext {
            tempo_bpm: Some(600.0),
            time_signature: Some(TimeSignature::default()),
            ..TransportContext::default()
        };
        let input = [0.5];
        let mut engine = CaptureEngine::default();

        engine.reset(setup);
        engine.arm();
        assert_eq!(engine.state(), CaptureState::Armed);

        let mut observed_states = vec![engine.state()];
        let mut completed = false;
        for _ in 0..32 {
            let event = engine.process(AudioInputBuffer::mono(&input), setup, transport, settings);
            observed_states.push(engine.state());
            if event == CaptureEvent::Completed {
                completed = true;
                break;
            }
        }

        assert!(completed);
        assert!(observed_states.contains(&CaptureState::CountIn));
        assert!(observed_states.contains(&CaptureState::Capturing));
        assert!(observed_states.contains(&CaptureState::Captured));

        engine.clear();
        assert_eq!(engine.state(), CaptureState::Idle);
    }

    #[test]
    fn audio_path_capture_completion_does_not_allocate_or_finalize_scratchpad() {
        let setup = ProcessSetup {
            sample_rate: 10.0,
            max_block_size: 80,
            mode: ProcessMode::Realtime,
        };
        let mut engine = CaptureEngine::default();
        let input = vec![0.25; 80];

        engine.reset(setup);
        engine.arm();

        let event = crate::assert_no_allocations("capture process completion", || {
            engine.process(
                AudioInputBuffer::mono(&input),
                setup,
                TransportContext::default(),
                CaptureSettings::default(),
            )
        });

        assert_eq!(event, CaptureEvent::Completed);
        assert_eq!(engine.state(), CaptureState::Captured);

        let scratchpad = engine
            .take_completed_scratchpad()
            .expect("scratchpad is only materialized by the explicit off-thread call");
        assert_eq!(scratchpad.samples.len(), 80);
    }

    #[test]
    fn phrase_boundary_waits_for_transport_alignment() {
        let setup = ProcessSetup::default();
        let settings = CaptureSettings {
            sync_mode: SyncMode::PhraseBoundary,
            ..CaptureSettings::default()
        };
        let off_boundary = TransportContext {
            playing: true,
            bar_position_quarter_note: Some(2.0),
            tempo_bpm: Some(120.0),
            time_signature: Some(TimeSignature::default()),
            ..TransportContext::default()
        };
        let on_boundary = TransportContext {
            bar_position_quarter_note: Some(16.0),
            ..off_boundary
        };

        assert!(!trigger_met(settings, off_boundary, setup));
        assert!(trigger_met(settings, on_boundary, setup));
    }
}
