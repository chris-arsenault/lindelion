use std::ops::Deref;

use lindelion_dsp_utils::math::seconds_to_samples;
use lindelion_plugin_shell::{AudioInputBuffer, ProcessSetup, TransportContext};
use lindelion_sample_library::{IntoAudioSampleRateHz, OwnedMonoAudioBuffer};
use serde::{Deserialize, Serialize};

pub const MAX_CAPTURE_BARS: u8 = 16;
pub const DEFAULT_CAPTURE_BARS: u8 = 4;
pub const MAX_COUNT_IN_BARS: u8 = 2;
pub const MIN_CAPTURE_BPM: f64 = 60.0;
pub const DEFAULT_CAPTURE_BPM: f64 = 120.0;
pub const DEFAULT_BEATS_PER_BAR: f64 = 4.0;
pub const DOWNBEAT_EPSILON_BEATS: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    Immediate,
    PhraseBoundary,
    NextDownbeat,
}

impl SyncMode {
    pub const ALL: [Self; 3] = [Self::Immediate, Self::PhraseBoundary, Self::NextDownbeat];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureState {
    Idle,
    Armed,
    CountIn,
    Capturing,
    Captured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSettings {
    #[serde(
        default = "default_capture_bars",
        deserialize_with = "deserialize_capture_bars"
    )]
    pub bars: u8,
    pub sync_mode: SyncMode,
    pub count_in_bars: u8,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            bars: DEFAULT_CAPTURE_BARS,
            sync_mode: SyncMode::Immediate,
            count_in_bars: 0,
        }
    }
}

impl CaptureSettings {
    pub fn sanitized(self) -> Self {
        Self {
            bars: sanitize_capture_bars(self.bars),
            sync_mode: self.sync_mode,
            count_in_bars: self.count_in_bars.min(MAX_COUNT_IN_BARS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScratchpadAudio {
    #[serde(flatten)]
    pub audio: OwnedMonoAudioBuffer,
    #[serde(default)]
    pub metadata: ScratchpadMetadata,
}

impl ScratchpadAudio {
    pub fn new(sample_rate: u32, samples: Vec<f32>) -> Self {
        Self::with_metadata(sample_rate, ScratchpadMetadata::default(), samples)
    }

    pub fn with_metadata(
        sample_rate: u32,
        metadata: ScratchpadMetadata,
        samples: Vec<f32>,
    ) -> Self {
        Self {
            audio: OwnedMonoAudioBuffer::new(samples, sample_rate),
            metadata,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.audio.is_empty()
    }
}

impl Deref for ScratchpadAudio {
    type Target = OwnedMonoAudioBuffer;

    fn deref(&self) -> &Self::Target {
        &self.audio
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScratchpadMetadata {
    pub bpm: u16,
    pub time_signature_numerator: u8,
    pub time_signature_denominator: u8,
    pub capture_bars: u8,
}

impl Default for ScratchpadMetadata {
    fn default() -> Self {
        Self {
            bpm: DEFAULT_CAPTURE_BPM as u16,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            capture_bars: DEFAULT_CAPTURE_BARS,
        }
    }
}

impl ScratchpadMetadata {
    pub fn new(
        bpm: f64,
        time_signature_numerator: u16,
        time_signature_denominator: u16,
        capture_bars: u8,
    ) -> Self {
        Self {
            bpm: sanitize_bpm(bpm),
            time_signature_numerator: sanitize_u8(time_signature_numerator, 1),
            time_signature_denominator: sanitize_denominator(time_signature_denominator),
            capture_bars: sanitize_capture_bars(capture_bars),
        }
    }
}

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
        self.sample_rate = setup.sample_rate.into_audio_sample_rate_hz();
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
        self.sample_rate = setup.sample_rate.into_audio_sample_rate_hz();

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
        let seconds = beats * 60.0 / transport.tempo_bpm_or(DEFAULT_CAPTURE_BPM);
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

pub fn capture_samples(
    settings: CaptureSettings,
    transport: TransportContext,
    sample_rate: u32,
) -> usize {
    let settings = settings.sanitized();
    let beats = settings.bars as f64 * transport.beats_per_bar();
    let seconds = beats * 60.0 / transport.tempo_bpm_or(DEFAULT_CAPTURE_BPM);
    seconds_to_samples(seconds, sample_rate)
}

pub fn max_capture_samples(sample_rate: u32) -> usize {
    let beats = MAX_CAPTURE_BARS as f64 * DEFAULT_BEATS_PER_BAR;
    seconds_to_samples(beats * 60.0 / MIN_CAPTURE_BPM, sample_rate)
}

pub fn trigger_met(
    settings: CaptureSettings,
    transport: TransportContext,
    setup: ProcessSetup,
) -> bool {
    let settings = settings.sanitized();
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
                * transport.tempo_bpm_or(DEFAULT_CAPTURE_BPM)
                / 60.0;
            let epsilon = DOWNBEAT_EPSILON_BEATS.max(block_beats);
            let downbeat_distance = bar_position.rem_euclid(beats_per_bar);
            let at_downbeat =
                downbeat_distance <= epsilon || beats_per_bar - downbeat_distance <= epsilon;

            if settings.sync_mode == SyncMode::NextDownbeat {
                return at_downbeat;
            }

            let phrase = settings.bars.max(1) as f64;
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
        transport.tempo_bpm_or(DEFAULT_CAPTURE_BPM),
        signature.numerator,
        signature.denominator,
        settings.sanitized().bars,
    )
}

fn default_capture_bars() -> u8 {
    DEFAULT_CAPTURE_BARS
}

fn sanitize_capture_bars(bars: u8) -> u8 {
    bars.clamp(1, MAX_CAPTURE_BARS)
}

fn sanitize_bpm(value: f64) -> u16 {
    if value.is_finite() {
        value.round().clamp(1.0, 999.0) as u16
    } else {
        DEFAULT_CAPTURE_BPM as u16
    }
}

fn sanitize_u8(value: u16, fallback: u8) -> u8 {
    u8::try_from(value).unwrap_or(fallback).max(1)
}

fn sanitize_denominator(value: u16) -> u8 {
    match value {
        1 | 2 | 4 | 8 | 16 | 32 => value as u8,
        _ => 4,
    }
}

fn deserialize_capture_bars<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum CaptureBarsField {
        Count(u8),
        LegacyName(String),
    }

    match CaptureBarsField::deserialize(deserializer)? {
        CaptureBarsField::Count(count) => Ok(sanitize_capture_bars(count)),
        CaptureBarsField::LegacyName(name) => match name.as_str() {
            "Four" | "four" | "4" => Ok(4),
            "Eight" | "eight" | "8" => Ok(8),
            "Sixteen" | "sixteen" | "16" => Ok(16),
            _ => Err(serde::de::Error::custom(format!(
                "unknown capture bars value {name:?}"
            ))),
        },
    }
}

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

#[cfg(test)]
mod tests;
