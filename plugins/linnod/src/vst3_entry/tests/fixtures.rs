use crate::{SourceAnalysis, SourceAnalysisStatus};
use lindelion_pitch_detect::{PitchContour, PitchFrame};
use lindelion_pitch_shift::PitchShiftAnalyzer;
use lindelion_sample_library::{
    OwnedMonoAudioBuffer, RuntimeMonoAudioBuffer, SampleMetadata, SampleReference,
    SampleWaveformPreview,
};

use super::super::{
    LinnodStatusPayload,
    messages::{LinnodSourceSlicePayload, LinnodSourceSummaryPayload, LinnodWaveformPointPayload},
};

pub(super) fn status_payload() -> LinnodStatusPayload {
    LinnodStatusPayload {
        source_status: SourceAnalysisStatus::Ready,
        has_source: true,
        has_analysis: true,
        marker_count: 3,
        selected_slice_index: Some(1),
        active_voices: 2,
    }
}

pub(super) fn source_summary_payload() -> LinnodSourceSummaryPayload {
    LinnodSourceSummaryPayload {
        source_label: "source.wav".to_string(),
        source_sample_rate: 48_000,
        waveform: vec![
            LinnodWaveformPointPayload {
                min: -0.5,
                max: 0.1,
                rms: 0.25,
            },
            LinnodWaveformPointPayload {
                min: -0.2,
                max: 0.75,
                rms: 0.35,
            },
        ],
        slices: vec![LinnodSourceSlicePayload {
            index: 0,
            start_sample: 0,
            end_sample: 4_800,
            detected_f0_hz: Some(220.0),
            detected_midi_note: Some(57.0),
            nearest_midi_note: Some(57),
            nearest_scale_midi_note: Some(57),
            nearest_midi_note_hz: Some(220.0),
            nearest_scale_midi_note_hz: Some(220.0),
            cents_deviation: Some(0.0),
            root_target_f0_hz: Some(220.0),
        }],
    }
}

pub(super) fn source_analysis() -> SourceAnalysis {
    let samples = sine_wave(220.0, 48_000, 4_800);
    let owned_audio = OwnedMonoAudioBuffer::new(samples.clone(), 48_000);
    let pitch_contour = PitchContour {
        source_sample_rate: 48_000,
        analysis_sample_rate: 48_000,
        hop_size: 256,
        frames: vec![
            pitch_frame(0, 0),
            pitch_frame(1, 1_200),
            pitch_frame(2, 2_400),
            pitch_frame(3, 3_600),
        ],
    };
    let markers = vec![lindelion_onset_detect::SliceMarker {
        position_samples: 0,
        kind: lindelion_onset_detect::MarkerKind::Auto,
    }];
    let pitch_shift_cache = PitchShiftAnalyzer::default()
        .analyze(&samples, owned_audio.sample_rate, &pitch_contour, &markers)
        .unwrap();

    SourceAnalysis {
        source: SampleMetadata {
            reference: SampleReference::new("hash", "Samples/source.wav"),
            filename: "source.wav".to_string(),
            duration_ms: 1,
            sample_rate: 48_000,
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

fn pitch_frame(frame_index: usize, source_sample_position: usize) -> PitchFrame {
    PitchFrame {
        frame_index,
        source_sample_position,
        timestamp_seconds: source_sample_position as f32 / 48_000.0,
        f0_hz: Some(220.0),
        raw_f0_hz: 220.0,
        confidence: 0.95,
        voiced: true,
        rms: 0.1,
    }
}

fn sine_wave(frequency_hz: f32, sample_rate: u32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate as f32).sin() * 0.5
        })
        .collect()
}
