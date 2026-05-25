use lindelion_dsp_utils::math::{finite_or, hz_to_midi_note, midi_note_to_hz};
use midly::{
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
    num::{u4, u7, u15, u24, u28},
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PPQ: u16 = 960;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetectedNote {
    pub start_sample: usize,
    pub end_sample: usize,
    pub pitch_hz: f32,
    pub peak_rms: f32,
    pub mean_rms: f32,
}

impl DetectedNote {
    pub fn duration_samples(self) -> usize {
        self.end_sample.saturating_sub(self.start_sample)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootNote {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl RootNote {
    pub const ALL: [Self; 12] = [
        Self::C,
        Self::CSharp,
        Self::D,
        Self::DSharp,
        Self::E,
        Self::F,
        Self::FSharp,
        Self::G,
        Self::GSharp,
        Self::A,
        Self::ASharp,
        Self::B,
    ];

    pub const fn pitch_class(self) -> i16 {
        match self {
            Self::C => 0,
            Self::CSharp => 1,
            Self::D => 2,
            Self::DSharp => 3,
            Self::E => 4,
            Self::F => 5,
            Self::FSharp => 6,
            Self::G => 7,
            Self::GSharp => 8,
            Self::A => 9,
            Self::ASharp => 10,
            Self::B => 11,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scale {
    Chromatic,
    Major,
    NaturalMinor,
    HarmonicMinor,
    MelodicMinor,
    PentatonicMajor,
    PentatonicMinor,
    Blues,
    Dorian,
    Mixolydian,
    Custom(Vec<u8>),
}

impl Scale {
    pub fn intervals(&self) -> Vec<i16> {
        match self {
            Self::Chromatic => (0..12).collect(),
            Self::Major => vec![0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => vec![0, 2, 3, 5, 7, 8, 10],
            Self::HarmonicMinor => vec![0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => vec![0, 2, 3, 5, 7, 9, 11],
            Self::PentatonicMajor => vec![0, 2, 4, 7, 9],
            Self::PentatonicMinor => vec![0, 3, 5, 7, 10],
            Self::Blues => vec![0, 3, 5, 6, 7, 10],
            Self::Dorian => vec![0, 2, 3, 5, 7, 9, 10],
            Self::Mixolydian => vec![0, 2, 4, 5, 7, 9, 10],
            Self::Custom(intervals) => {
                let mut intervals = intervals
                    .iter()
                    .map(|interval| i16::from(*interval % 12))
                    .collect::<Vec<_>>();
                intervals.sort_unstable();
                intervals.dedup();
                if intervals.is_empty() {
                    vec![0]
                } else {
                    intervals
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapMode {
    Hard,
    Soft,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingGrid {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    QuarterTriplet,
    EighthTriplet,
    SixteenthTriplet,
}

impl TimingGrid {
    pub const fn beats(self) -> f32 {
        match self {
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
            Self::QuarterTriplet => 2.0 / 3.0,
            Self::EighthTriplet => 1.0 / 3.0,
            Self::SixteenthTriplet => 1.0 / 6.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantizeSettings {
    pub root: RootNote,
    pub scale: Scale,
    pub snap_mode: SnapMode,
    pub soft_snap_cents: f32,
    pub grid: TimingGrid,
    pub timing_strength: f32,
    pub velocity_amount: f32,
    pub sample_rate: u32,
    pub bpm: f32,
    #[serde(default = "default_time_signature_numerator")]
    pub time_signature_numerator: u8,
    #[serde(default = "default_time_signature_denominator")]
    pub time_signature_denominator: u8,
    pub ppq: u16,
}

impl Default for QuantizeSettings {
    fn default() -> Self {
        Self {
            root: RootNote::C,
            scale: Scale::Chromatic,
            snap_mode: SnapMode::Hard,
            soft_snap_cents: 50.0,
            grid: TimingGrid::Sixteenth,
            timing_strength: 1.0,
            velocity_amount: 0.0,
            sample_rate: 48_000,
            bpm: 120.0,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
            ppq: DEFAULT_PPQ,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantizedNote {
    pub start_tick: u32,
    pub duration_ticks: u32,
    pub midi_note: u8,
    pub velocity: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiClip {
    pub ppq: u16,
    pub bpm: u16,
    pub time_signature_numerator: u8,
    pub time_signature_denominator: u8,
    pub notes: Vec<QuantizedNote>,
}

impl MidiClip {
    pub fn empty(bpm: u16) -> Self {
        Self::empty_with_time_signature(bpm, 4, 4)
    }

    pub fn empty_with_time_signature(
        bpm: u16,
        time_signature_numerator: u8,
        time_signature_denominator: u8,
    ) -> Self {
        Self {
            ppq: DEFAULT_PPQ,
            bpm,
            time_signature_numerator: sanitize_time_signature_numerator(time_signature_numerator),
            time_signature_denominator: sanitize_time_signature_denominator(
                time_signature_denominator,
            ),
            notes: Vec::new(),
        }
    }

    pub fn to_smf_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        let mut events = Vec::new();
        let tempo = 60_000_000u32 / u32::from(self.bpm.max(1));
        events.push(AbsoluteEvent {
            tick: 0,
            order: 0,
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(tempo))),
        });
        events.push(AbsoluteEvent {
            tick: 0,
            order: 1,
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(
                self.time_signature_numerator,
                denominator_power(self.time_signature_denominator),
                24,
                8,
            )),
        });

        for note in &self.notes {
            let note_key = u7::new(note.midi_note.min(127));
            events.push(AbsoluteEvent {
                tick: note.start_tick,
                order: 3,
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOn {
                        key: note_key,
                        vel: u7::new(note.velocity.min(127)),
                    },
                },
            });
            events.push(AbsoluteEvent {
                tick: note.start_tick.saturating_add(note.duration_ticks.max(1)),
                order: 2,
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOff {
                        key: note_key,
                        vel: u7::new(0),
                    },
                },
            });
        }

        events.sort_by_key(|event| (event.tick, event.order));
        let mut track = Vec::with_capacity(events.len() + 1);
        let mut last_tick = 0;
        for event in events {
            track.push(TrackEvent {
                delta: u28::new(event.tick.saturating_sub(last_tick)),
                kind: event.kind,
            });
            last_tick = event.tick;
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header {
                format: Format::SingleTrack,
                timing: Timing::Metrical(u15::new(self.ppq)),
            },
            tracks: vec![track],
        };

        let mut out = Vec::new();
        smf.write_std(&mut out)?;
        Ok(out)
    }
}

#[derive(Debug, Clone)]
struct AbsoluteEvent<'a> {
    tick: u32,
    order: u8,
    kind: TrackEventKind<'a>,
}

pub fn quantize_notes(notes: &[DetectedNote], settings: &QuantizeSettings) -> Vec<QuantizedNote> {
    if notes.is_empty() {
        return Vec::new();
    }

    let settings = sanitize_settings(settings);
    let peak_capture_rms = notes
        .iter()
        .map(|note| finite_or(note.peak_rms, 0.0).max(0.0))
        .fold(0.0, f32::max)
        .max(0.000_001);

    let notes = notes
        .iter()
        .filter_map(|note| quantize_note(*note, &settings, peak_capture_rms))
        .collect();
    make_notes_monophonic(notes, settings.ppq)
}

pub fn clip_from_detected_notes(notes: &[DetectedNote], settings: &QuantizeSettings) -> MidiClip {
    let settings = sanitize_settings(settings);
    MidiClip {
        ppq: settings.ppq,
        bpm: settings.bpm.round().clamp(1.0, u16::MAX as f32) as u16,
        time_signature_numerator: settings.time_signature_numerator,
        time_signature_denominator: settings.time_signature_denominator,
        notes: quantize_notes(notes, &settings),
    }
}

pub fn midi_note_from_hz(hz: f32) -> f32 {
    hz_to_midi_note(hz).unwrap_or(0.0)
}

pub fn hz_from_midi_note(note: f32) -> f32 {
    midi_note_to_hz(note)
}

fn quantize_note(
    note: DetectedNote,
    settings: &QuantizeSettings,
    peak_capture_rms: f32,
) -> Option<QuantizedNote> {
    if note.end_sample <= note.start_sample || note.pitch_hz <= 0.0 || !note.pitch_hz.is_finite() {
        return None;
    }

    let start_beats = samples_to_beats(note.start_sample, settings.sample_rate, settings.bpm);
    let end_beats = samples_to_beats(note.end_sample, settings.sample_rate, settings.bpm);
    let duration_beats = (end_beats - start_beats).max(1.0 / 64.0);
    let grid = settings.grid.beats();
    let target_beats = (start_beats / grid).round() * grid;
    let strength = settings.timing_strength.clamp(0.0, 1.0);
    let emitted_start_beats = start_beats + (target_beats - start_beats) * strength;
    let start_tick = beats_to_ticks(emitted_start_beats, settings.ppq);
    let duration_ticks =
        beats_to_ticks(duration_beats, settings.ppq).max(minimum_note_ticks(settings.ppq));

    Some(QuantizedNote {
        start_tick,
        duration_ticks,
        midi_note: snap_midi_note(midi_note_from_hz(note.pitch_hz), settings),
        velocity: velocity_from_rms(note.peak_rms, peak_capture_rms, settings.velocity_amount),
    })
}

fn make_notes_monophonic(mut notes: Vec<QuantizedNote>, ppq: u16) -> Vec<QuantizedNote> {
    notes.sort_by_key(|note| (note.start_tick, note.midi_note));
    let mut next_available_tick = 0;
    for note in &mut notes {
        note.start_tick = note.start_tick.max(next_available_tick);
        note.duration_ticks = note.duration_ticks.max(minimum_note_ticks(ppq));
        next_available_tick = note.start_tick.saturating_add(note.duration_ticks);
    }
    notes
}

fn snap_midi_note(note: f32, settings: &QuantizeSettings) -> u8 {
    snap_midi_note_to_scale(
        note,
        settings.root,
        &settings.scale,
        settings.snap_mode,
        settings.soft_snap_cents,
    )
}

pub fn snap_midi_note_to_scale(
    note: f32,
    root: RootNote,
    scale: &Scale,
    snap_mode: SnapMode,
    soft_snap_cents: f32,
) -> u8 {
    let chromatic = note.round().clamp(0.0, 127.0) as i16;
    match snap_mode {
        SnapMode::None => chromatic as u8,
        SnapMode::Hard => nearest_scale_midi_note(note, root, scale),
        SnapMode::Soft => {
            let scale_note = nearest_scale_midi_note(note, root, scale);
            let scale_cents = (note - scale_note as f32).abs() * 100.0;
            if scale_cents <= soft_snap_cents {
                scale_note
            } else {
                chromatic as u8
            }
        }
    }
}

pub fn nearest_scale_midi_note(note: f32, root: RootNote, scale: &Scale) -> u8 {
    nearest_scale_degree(note, root, scale) as u8
}

fn nearest_scale_degree(note: f32, root: RootNote, scale: &Scale) -> i16 {
    let root = root.pitch_class();
    let intervals = scale.intervals();
    let rounded = note.round() as i16;
    let mut best_note = rounded;
    let mut best_distance = f32::MAX;

    for candidate in (rounded - 24)..=(rounded + 24) {
        let pitch_class = candidate.rem_euclid(12);
        let in_scale = intervals
            .iter()
            .any(|interval| (root + *interval).rem_euclid(12) == pitch_class);
        if !in_scale {
            continue;
        }

        let distance = (note - candidate as f32).abs();
        if distance < best_distance {
            best_distance = distance;
            best_note = candidate;
        }
    }

    best_note.clamp(0, 127)
}

fn velocity_from_rms(peak_rms: f32, capture_peak_rms: f32, amount: f32) -> u8 {
    let dynamic = rms_to_midi_velocity(peak_rms, capture_peak_rms);
    let velocity = 100.0 + (dynamic as f32 - 100.0) * amount.clamp(0.0, 1.0);
    velocity.round().clamp(1.0, 127.0) as u8
}

pub fn rms_to_midi_velocity(peak_rms: f32, capture_peak_rms: f32) -> u8 {
    let peak = finite_or(peak_rms, 0.0).max(0.000_001);
    let capture_peak = finite_or(capture_peak_rms, 0.0).max(peak).max(0.000_001);
    let db = 20.0 * (peak / capture_peak).log10();
    let normalized = ((db + 40.0) / 40.0).clamp(0.0, 1.0);
    (1.0 + normalized * 126.0).round().clamp(1.0, 127.0) as u8
}

fn samples_to_beats(samples: usize, sample_rate: u32, bpm: f32) -> f32 {
    samples as f32 / sample_rate.max(1) as f32 * bpm / 60.0
}

fn beats_to_ticks(beats: f32, ppq: u16) -> u32 {
    (beats.max(0.0) * ppq.max(1) as f32).round() as u32
}

fn denominator_power(denominator: u8) -> u8 {
    match denominator {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        _ => 2,
    }
}

fn sanitize_settings(settings: &QuantizeSettings) -> QuantizeSettings {
    let mut settings = settings.clone();
    settings.soft_snap_cents = finite_or(settings.soft_snap_cents, 50.0).clamp(0.0, 100.0);
    settings.timing_strength = finite_or(settings.timing_strength, 1.0).clamp(0.0, 1.0);
    settings.velocity_amount = finite_or(settings.velocity_amount, 0.0).clamp(0.0, 1.0);
    settings.sample_rate = settings.sample_rate.max(1);
    settings.bpm = finite_or(settings.bpm, 120.0).clamp(1.0, 999.0);
    settings.time_signature_numerator =
        sanitize_time_signature_numerator(settings.time_signature_numerator);
    settings.time_signature_denominator =
        sanitize_time_signature_denominator(settings.time_signature_denominator);
    settings.ppq = settings.ppq.max(1);
    settings
}

fn minimum_note_ticks(ppq: u16) -> u32 {
    (ppq.max(1) / 16).max(1) as u32
}

fn sanitize_time_signature_numerator(numerator: u8) -> u8 {
    numerator.max(1)
}

fn sanitize_time_signature_denominator(denominator: u8) -> u8 {
    match denominator {
        1 | 2 | 4 | 8 | 16 | 32 => denominator,
        _ => 4,
    }
}

const fn default_time_signature_numerator() -> u8 {
    4
}

const fn default_time_signature_denominator() -> u8 {
    4
}

#[cfg(test)]
mod midi_export_tests;

#[cfg(test)]
mod tests;
