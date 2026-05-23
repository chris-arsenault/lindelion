use super::*;

#[test]
fn manual_grid_places_requested_markers() {
    let detector = ManualGridDetector;
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::ManualGrid,
        sensitivity: 0.5,
        min_slice_ms: 50.0,
        params: AlgorithmParams::ManualGrid {
            divisions: 4,
            offset_ms: 0.0,
        },
    };

    let markers = detector.detect(&vec![0.0; 400], 48_000, config);
    assert_eq!(markers.len(), 4);
    assert_eq!(markers[2].position_samples, 200);
}

#[test]
fn energy_detector_finds_transient_after_silence() {
    let mut audio = vec![0.0; 4_800];
    audio.extend(vec![0.8; 4_800]);
    let detector = EnergyTransientDetector;
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::EnergyTransient,
        sensitivity: 0.8,
        min_slice_ms: 50.0,
        params: AlgorithmParams::EnergyTransient { frame_size: 256 },
    };

    let markers = detector.detect(&audio, 48_000, config);

    assert!(
        markers
            .iter()
            .any(|marker| marker.position_samples >= 4_608)
    );
}

#[test]
fn superflux_detects_soft_sine_entry() {
    let mut audio = vec![0.0; 4_800];
    for index in 0..9_600 {
        let ramp = (index as f32 / 2_400.0).min(1.0);
        let phase = 2.0 * std::f32::consts::PI * 440.0 * index as f32 / 48_000.0;
        audio.push(phase.sin() * ramp * 0.7);
    }
    let detector = SuperFluxDetector;
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::SuperFlux,
        sensitivity: 0.4,
        min_slice_ms: 80.0,
        params: AlgorithmParams::SuperFlux {
            lookback_frames: 3,
            max_filter_radius: 3,
        },
    };

    let markers = detector.detect(&audio, 48_000, config);

    assert!(
        markers
            .iter()
            .any(|marker| marker.position_samples >= 4_000)
    );
}

#[test]
fn pitch_stability_detects_legato_jump_from_supplied_track() {
    let frames = synthetic_pitch_jump_track();
    let track = PitchTrack {
        source_sample_rate: 16_000,
        frame_hop_samples: 256,
        frames: &frames,
    };
    let markers = pitch_stability_markers_from_track(track, 120.0, 48.0, 80.0);

    assert!(
        markers
            .iter()
            .any(|marker| { marker.position_samples > 6_400 && marker.position_samples < 9_600 })
    );
}

fn synthetic_pitch_jump_track() -> Vec<PitchTrackFrame> {
    (0..64)
        .map(|index| PitchTrackFrame {
            source_sample_position: index * 256,
            f0_hz: Some(if index < 32 { 440.0 } else { 660.0 }),
            confidence: 0.95,
        })
        .collect()
}
