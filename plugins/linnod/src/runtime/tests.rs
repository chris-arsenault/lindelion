use super::*;
use crate::patch::{ChokeGroupId, PadAssignment, PadId, PitchOffset};
use lindelion_dsp_utils::analysis::{assert_all_finite, estimate_frequency_zero_crossings};
use lindelion_onset_detect::{MarkerKind, SliceMarker};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use lindelion_pitch_shift::PitchShiftAnalyzer;
use lindelion_plugin_shell::{MidiEvent, NoteEvent};
use lindelion_sample_library::{
    OwnedMonoAudioBuffer, RuntimeMonoAudioBuffer, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};
use lindelion_test_allocator::assert_no_allocations;

#[test]
fn runtime_triggers_pad_voice_and_renders_audio() {
    let mut fixture = RuntimeFixture::new();
    let mut left = [0.0; 256];
    let mut right = [0.0; 256];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 1);
    assert!(peak_abs(&left) > 0.000_01);
    assert!(peak_abs(&right) > 0.000_01);
}

#[test]
fn pad_mode_retrigger_chokes_existing_owned_voice() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.slices[0].playback_mode = PlaybackMode::Looped;
    let mut left = [0.0; 64];
    let mut right = [0.0; 64];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0), note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 1);
}

#[test]
fn pad_mode_choke_group_clears_other_pad_voice() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.pad_map = vec![
        PadAssignment {
            pad: PadId(1),
            slice_index: 0,
            midi_note: 36,
            choke_group: Some(ChokeGroupId(1)),
        },
        PadAssignment {
            pad: PadId(2),
            slice_index: 0,
            midi_note: 37,
            choke_group: Some(ChokeGroupId(1)),
        },
    ];
    fixture.patch.normalize_layout();
    fixture.patch.slices[0].playback_mode = PlaybackMode::Looped;
    let mut left = [0.0; 64];
    let mut right = [0.0; 64];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0), note_on(0, 37, 1.0)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 1);
}

#[test]
fn pad_mode_distinct_choke_groups_can_overlap() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.pad_map = vec![
        PadAssignment {
            pad: PadId(1),
            slice_index: 0,
            midi_note: 36,
            choke_group: Some(ChokeGroupId(1)),
        },
        PadAssignment {
            pad: PadId(2),
            slice_index: 0,
            midi_note: 37,
            choke_group: Some(ChokeGroupId(2)),
        },
    ];
    fixture.patch.normalize_layout();
    fixture.patch.slices[0].playback_mode = PlaybackMode::Looped;
    let mut left = [0.0; 64];
    let mut right = [0.0; 64];

    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0), note_on(0, 37, 1.0)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 2);
}

#[test]
fn chromatic_mode_resolves_selected_pad_and_pitch_delta() {
    let mut patch = LinnodPatch {
        trigger_mode: TriggerMode::Chromatic,
        active_chromatic_pad: PadId(2),
        pad_map: vec![PadAssignment {
            pad: PadId(2),
            slice_index: 7,
            midi_note: 64,
            choke_group: None,
        }],
        ..LinnodPatch::default()
    };
    patch.normalize_layout();

    let resolved = resolve_note_trigger(&patch, 76).unwrap();

    assert_eq!(resolved.slice_index, 7);
    assert_eq!(resolved.chromatic_semitones, 12.0);
}

#[test]
fn pad_mode_renders_pitch_offset_sine_at_requested_frequency() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.slices[0].pitch = PitchOffset::from_frequency_ratio(2.0);
    let trigger =
        voice_trigger_from_note(&fixture.patch, &fixture.analysis, 36, 48_000.0, 1.0).unwrap();

    let detected_f0_hz = fixture
        .analysis
        .pitch_shift_cache
        .slice_summary(trigger.slice_index)
        .and_then(|summary| summary.detected_f0_hz)
        .unwrap();
    assert_eq!(trigger.slice_index, 0);
    assert!((detected_f0_hz * trigger.ratios.pitch_ratio - 440.0).abs() < 0.01);
    assert_eq!(
        trigger.ratios.formant_ratio,
        Some(trigger.ratios.pitch_ratio)
    );

    let mut left = [0.0; 4_096];
    let mut right = [0.0; 4_096];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    let estimated_hz = estimate_frequency_zero_crossings(&left[512..], 48_000.0).unwrap();
    assert!(
        (estimated_hz - 440.0).abs() < 3.0,
        "expected 440 Hz pad output, got {estimated_hz:.2} Hz"
    );
}

#[test]
fn identity_pitch_playback_reads_source_samples_directly() {
    let fixture = RuntimeFixture::new();
    let source = fixture.analysis.audio.samples();

    let sample = direct_slice_sample(&fixture.analysis, 0, 37.0);

    assert_eq!(sample, source[37]);
    assert!(is_identity_pitch_request(PitchShiftRatios::identity()));
    assert!(!is_identity_pitch_request(PitchShiftRatios {
        pitch_ratio: 2.0,
        formant_ratio: None,
    }));
}

#[test]
fn note_trigger_does_not_allocate() {
    let mut fixture = RuntimeFixture::new();
    let events = [note_on(0, 36, 1.0)];
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];

    fixture.process_no_alloc("linnod note trigger", &events, &mut left, &mut right);

    assert_eq!(fixture.processor.active_voice_count(), 1);
}

#[test]
fn note_release_does_not_allocate() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.slices[0].playback_mode = PlaybackMode::Gated;
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    fixture.process_no_alloc(
        "linnod note release",
        &[note_off(0, 36)],
        &mut left,
        &mut right,
    );
}

#[test]
fn pad_choke_retrigger_does_not_allocate() {
    let mut fixture = RuntimeFixture::new();
    fixture.patch.slices[0].playback_mode = PlaybackMode::Looped;
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    fixture.process_no_alloc(
        "linnod pad choke retrigger",
        &[note_on(0, 36, 0.75)],
        &mut left,
        &mut right,
    );

    assert_eq!(fixture.processor.active_voice_count(), 1);
}

#[test]
fn block_render_does_not_allocate() {
    let mut fixture = RuntimeFixture::new();
    let mut left = [0.0; 128];
    let mut right = [0.0; 128];
    fixture.processor.process(
        &fixture.patch,
        Some(&fixture.analysis),
        &[note_on(0, 36, 1.0)],
        &mut left,
        &mut right,
    );

    fixture.process_no_alloc("linnod block render", &[], &mut left, &mut right);

    assert!(peak_abs(&left) > 0.000_01);
}

struct RuntimeFixture {
    processor: LinnodProcessor,
    patch: LinnodPatch,
    analysis: SourceAnalysis,
}

impl RuntimeFixture {
    fn new() -> Self {
        Self {
            processor: LinnodProcessor::new(48_000.0),
            patch: LinnodPatch::default(),
            analysis: source_analysis(),
        }
    }

    fn process_no_alloc(
        &mut self,
        label: &str,
        events: &[MidiEvent],
        left: &mut [f32],
        right: &mut [f32],
    ) {
        assert_no_allocations(label, || {
            self.processor
                .process(&self.patch, Some(&self.analysis), events, left, right);
        });
        assert_all_finite(left);
        assert_all_finite(right);
    }
}

fn source_analysis() -> SourceAnalysis {
    let sample_rate = 48_000;
    let samples = sine_wave(220.0, sample_rate, 4_800);
    let owned_audio = OwnedMonoAudioBuffer::new(samples.clone(), sample_rate);
    let pitch_contour = PitchContour {
        source_sample_rate: sample_rate,
        analysis_sample_rate: sample_rate,
        hop_size: 1_200,
        frames: vec![
            pitch_frame(0, 0, Some(220.0)),
            pitch_frame(1, 1_200, Some(220.0)),
            pitch_frame(2, 2_400, Some(220.0)),
            pitch_frame(3, 3_600, Some(220.0)),
        ],
    };
    let markers = vec![SliceMarker {
        position_samples: 0,
        kind: MarkerKind::Auto,
    }];
    let pitch_shift_cache = PitchShiftAnalyzer::default()
        .analyze(&samples, sample_rate, &pitch_contour, &markers)
        .unwrap();

    SourceAnalysis {
        source: SampleMetadata {
            reference: SampleReference::new("hash", "Samples/source.wav"),
            filename: "source.wav".to_string(),
            duration_ms: 100,
            sample_rate,
            channels: 1,
            rms_db: None,
            peak_db: None,
            waveform_preview: SampleWaveformPreview { points: Vec::new() },
        },
        audio: RuntimeMonoAudioBuffer::from_owned(owned_audio),
        pitch_contour,
        markers,
        pitch_shift_cache,
    }
}

fn pitch_frame(
    frame_index: usize,
    source_sample_position: usize,
    f0_hz: Option<f32>,
) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / 48_000.0,
        f0_hz,
        raw_f0_hz: f0_hz.unwrap_or(0.0),
        confidence: 0.95,
        voiced: f0_hz.is_some(),
        rms: 0.2,
    }
}

fn note_on(channel: u8, note: u8, velocity: f32) -> MidiEvent {
    MidiEvent::Note(NoteEvent::On {
        channel,
        note,
        velocity,
    })
}

fn note_off(channel: u8, note: u8) -> MidiEvent {
    MidiEvent::Note(NoteEvent::Off {
        channel,
        note,
        velocity: 0.0,
    })
}

fn sine_wave(frequency_hz: f32, sample_rate: u32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate as f32).sin() * 0.5
        })
        .collect()
}

fn peak_abs(samples: &[f32]) -> f32 {
    samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max)
}
