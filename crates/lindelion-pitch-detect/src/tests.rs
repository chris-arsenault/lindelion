use super::*;
use lindelion_dsp_utils::analysis::windowed_dft_magnitude_at;
use lindelion_dsp_utils::math::cents_between;

#[test]
fn swiftf0_model_bytes_are_embedded() {
    assert!(SWIFTF0_MODEL_BYTES.len() > 300_000);
}

#[test]
fn resampling_preserves_duration() {
    let audio = vec![0.0; 48_000];
    let resampled = resample_to_swiftf0_rate(&audio, 48_000);

    assert_eq!(resampled.len(), 16_000);
}

#[test]
fn downsampling_rejects_above_nyquist_alias() {
    // A 10 kHz tone at 48 kHz sits above the 8 kHz Nyquist of the 16 kHz analysis
    // rate; pure decimation folds it to a 6 kHz alias. The band-limited resampler
    // must reject it instead of aliasing it down.
    let source_rate = 48_000u32;
    let tone_hz = 10_000.0;
    let alias_hz = 6_000.0;
    let input: Vec<f32> = (0..source_rate)
        .map(|index| (std::f32::consts::TAU * tone_hz * index as f32 / source_rate as f32).sin())
        .collect();

    let resampled = resample_to_swiftf0_rate(&input, source_rate);

    let trim = 128;
    let body = &resampled[trim..resampled.len() - trim];
    let alias = windowed_dft_magnitude_at(body, SWIFTF0_TARGET_SAMPLE_RATE as f32, alias_hz);
    let source_level = windowed_dft_magnitude_at(&input, source_rate as f32, tone_hz);

    assert!(
        alias < source_level * 0.05,
        "alias at {alias_hz} Hz should be >= 26 dB below the source tone; alias={alias}, source_level={source_level}"
    );
}

#[test]
fn silence_is_unvoiced_and_finite() {
    let contour = SwiftF0Detector::default()
        .detect(&vec![0.0; 16_000], 16_000)
        .unwrap();

    assert!(!contour.is_empty());
    assert!(contour.frames.iter().all(|frame| !frame.voiced));
    assert!(
        contour
            .frames
            .iter()
            .all(|frame| frame.confidence.is_finite())
    );
}

#[test]
fn non_finite_input_is_sanitized() {
    let mut audio = vec![0.0; 16_000];
    audio[100] = f32::NAN;
    audio[200] = f32::INFINITY;

    let contour = SwiftF0Detector::default().detect(&audio, 16_000).unwrap();

    assert!(
        contour
            .frames
            .iter()
            .all(|frame| frame.raw_f0_hz.is_finite())
    );
    assert!(contour.frames.iter().all(|frame| frame.rms.is_finite()));
}

#[test]
fn swiftf0_tracks_synthetic_sine_pitch() {
    let audio = sine_wave(440.0, 16_000);

    let contour = SwiftF0Detector::default().detect(&audio, 16_000).unwrap();
    let voiced = contour
        .frames
        .iter()
        .filter_map(|frame| frame.f0_hz)
        .collect::<Vec<_>>();

    assert!(
        voiced.len() >= 4,
        "expected voiced SwiftF0 frames, got {}",
        voiced.len()
    );
    assert_close_cents(median(voiced), 440.0, 80.0);
}

#[test]
fn contour_reports_source_hop_from_frame_positions() {
    let contour = PitchContour {
        source_sample_rate: 48_000,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames: vec![
            pitch_frame(0, 1_000, Some(220.0)),
            pitch_frame(1, 1_768, Some(220.0)),
            pitch_frame(2, 2_536, Some(220.0)),
        ],
    };

    assert_eq!(contour.source_frame_hop_samples(), 768);
}

#[test]
fn contour_hop_fallback_uses_contour_sample_rates() {
    let contour = PitchContour {
        source_sample_rate: 44_100,
        analysis_sample_rate: 22_050,
        hop_size: 128,
        frames: vec![pitch_frame(0, 0, Some(220.0))],
    };

    assert_eq!(contour.source_frame_hop_samples(), 256);
}

#[test]
fn contour_frames_in_range_and_median_use_shared_pitch_frames() {
    let contour = PitchContour {
        source_sample_rate: 48_000,
        analysis_sample_rate: 16_000,
        hop_size: 256,
        frames: vec![
            pitch_frame(0, 0, Some(440.0)),
            pitch_frame(1, 768, None),
            pitch_frame(2, 1_536, Some(660.0)),
            pitch_frame(3, 2_304, Some(550.0)),
        ],
    };

    let frames = contour.frames_in_range(700, 2_400);

    assert_eq!(frames.len(), 3);
    assert_eq!(median_voiced_pitch(frames), Some(660.0));
}

#[test]
fn streaming_pitch_tracker_emits_monotonic_frames_across_blocks() {
    let audio = sine_wave(440.0, 16_000);
    let mut tracker = SwiftF0StreamingPitchTracker::new(16_000, PitchDetectionConfig::default());
    let mut frames = Vec::new();

    for block in audio.chunks(4_096) {
        frames.extend_from_slice(tracker.next_block(block).unwrap());
    }
    frames.extend_from_slice(tracker.finish().unwrap());

    assert!(frames.len() >= 4);
    assert!(
        frames
            .windows(2)
            .all(|pair| pair[0].source_sample_position < pair[1].source_sample_position)
    );
    let voiced = frames
        .iter()
        .filter_map(|frame| frame.f0_hz)
        .collect::<Vec<_>>();
    assert_close_cents(median(voiced), 440.0, 80.0);
}

#[test]
fn zero_crossing_streaming_pitch_tracker_detects_block_pitch() {
    let audio = sine_wave(440.0, 8_192);
    let mut tracker =
        ZeroCrossingStreamingPitchTracker::new(16_000, PitchDetectionConfig::default());

    let frames = tracker.next_block(&audio).unwrap();

    assert_eq!(frames.len(), 1);
    assert_close_cents(frames[0].f0_hz.unwrap(), 440.0, 5.0);
    assert_eq!(frames[0].source_sample_position, 0);
}

#[test]
fn zero_crossing_streaming_pitch_tracker_advances_and_resets_position() {
    let audio = sine_wave(220.0, 4_096);
    let mut tracker =
        ZeroCrossingStreamingPitchTracker::new(16_000, PitchDetectionConfig::default());

    tracker.next_block(&audio).unwrap();
    let frames = tracker.next_block(&audio).unwrap();
    assert_eq!(frames[0].source_sample_position, audio.len());

    tracker.reset();
    let frames = tracker.next_block(&audio).unwrap();
    assert_eq!(frames[0].source_sample_position, 0);
}

fn sine_wave(frequency_hz: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|index| {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / 16_000.0;
            phase.sin() * 0.5
        })
        .collect()
}

fn median(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

fn assert_close_cents(actual_hz: f32, expected_hz: f32, tolerance_cents: f32) {
    let cents = cents_between(expected_hz, actual_hz);
    assert!(
        cents <= tolerance_cents,
        "expected {actual_hz} Hz within {tolerance_cents} cents of {expected_hz} Hz, got {cents}"
    );
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
        confidence: if f0_hz.is_some() { 0.95 } else { 0.1 },
        voiced: f0_hz.is_some(),
        rms: if f0_hz.is_some() { 0.2 } else { 0.0 },
    }
}
