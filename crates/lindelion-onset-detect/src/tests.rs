use super::*;
use lindelion_pitch_detect::PitchFrame;

#[test]
fn manual_grid_places_requested_markers() {
    let detector = ManualGridDetector;
    let audio = vec![0.0; 400];
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::ManualGrid,
        sensitivity: 0.5,
        min_slice_ms: 50.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::ManualGrid {
            divisions: 4,
            offset_ms: 0.0,
        },
    };

    let markers = detector.detect(OnsetDetectionInput::new(&audio, 48_000), config);
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
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::EnergyTransient { frame_size: 256 },
    };

    let markers = detector.detect(OnsetDetectionInput::new(&audio, 48_000), config);

    assert!(
        markers
            .iter()
            .any(|marker| marker.position_samples >= 4_608)
    );
}

#[test]
fn streaming_energy_detector_finds_transient_across_blocks() {
    let mut audio = vec![0.0; 4_800];
    audio.extend(vec![0.8; 4_800]);
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::EnergyTransient,
        sensitivity: 0.8,
        min_slice_ms: 50.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::EnergyTransient { frame_size: 256 },
    };
    let mut detector = StreamingEnergyTransientDetector::new(48_000, config);
    let mut markers = Vec::new();

    for block in audio.chunks(333) {
        markers.extend_from_slice(detector.next_block(block));
    }
    markers.extend_from_slice(detector.finish());

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
    let config = DetectionConfig::superflux(0.4, 80.0, OnsetDetectionProfile::default());

    let markers = detector.detect(OnsetDetectionInput::new(&audio, 48_000), config);

    assert!(
        markers
            .iter()
            .any(|marker| marker.position_samples >= 4_000)
    );
}

#[test]
fn streaming_spectral_flux_carries_state_across_blocks() {
    let audio = (0..4_096)
        .map(|index| {
            let phase = 2.0 * std::f32::consts::PI * 440.0 * index as f32 / 48_000.0;
            phase.sin() * 0.5
        })
        .collect::<Vec<_>>();
    let mut flux = StreamingSpectralFlux::new(1024, 256, 3);
    let mut frames = Vec::new();

    for block in audio.chunks(300) {
        frames.extend_from_slice(flux.next_block(block));
    }
    frames.extend_from_slice(flux.finish());

    assert!(frames.len() >= 8);
    assert_eq!(frames[0].position_samples, 0);
    assert!(
        frames
            .windows(2)
            .all(|pair| pair[1].position_samples - pair[0].position_samples == 256)
    );
    assert!(frames.iter().all(|frame| frame.flux.is_finite()));
}

#[test]
fn pitch_stability_detects_legato_jump_from_supplied_track() {
    let frames = synthetic_pitch_jump_track();
    let track = PitchTrack {
        source_sample_rate: 16_000,
        frame_hop_samples: 256,
        frames: &frames,
    };
    let audio = vec![0.0; 16_384];
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::PitchStability,
        sensitivity: 0.5,
        min_slice_ms: 80.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::PitchStability {
            threshold_cents: 120.0,
            min_stable_duration_ms: 48.0,
        },
    };
    let markers = PitchStabilityDetector.detect(
        OnsetDetectionInput::new(&audio, 16_000).with_pitch_track(track),
        config,
    );

    assert!(
        markers
            .iter()
            .any(|marker| { marker.position_samples > 6_400 && marker.position_samples < 9_600 })
    );
}

#[test]
fn configured_detector_uses_pitch_track_for_pitch_stability() {
    let frames = synthetic_pitch_jump_track();
    let track = PitchTrack {
        source_sample_rate: 16_000,
        frame_hop_samples: 256,
        frames: &frames,
    };
    let audio = vec![0.0; 16_384];
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::PitchStability,
        sensitivity: 0.5,
        min_slice_ms: 80.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::PitchStability {
            threshold_cents: 120.0,
            min_stable_duration_ms: 48.0,
        },
    };

    let markers = ConfiguredOnsetDetector.detect(
        OnsetDetectionInput::new(&audio, 16_000).with_pitch_track(track),
        config,
    );

    assert!(markers.iter().any(|marker| marker.position_samples > 0));
}

#[test]
fn pitch_stability_without_pitch_track_is_empty_by_contract() {
    let audio = vec![0.0; 16_384];
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::PitchStability,
        sensitivity: 0.5,
        min_slice_ms: 80.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::PitchStability {
            threshold_cents: 120.0,
            min_stable_duration_ms: 48.0,
        },
    };

    let markers = ConfiguredOnsetDetector.detect(OnsetDetectionInput::new(&audio, 16_000), config);

    assert!(markers.is_empty());
}

#[test]
fn detection_profile_builds_superflux_and_pitch_stability_params() {
    let profile = OnsetDetectionProfile {
        lookback_frames: 9,
        max_filter_radius: 4,
        pitch_stability_threshold_cents: 90.0,
        pitch_stability_duration_ms: 40.0,
    };

    let config = DetectionConfig::superflux(0.7, 60.0, profile);

    assert_eq!(config.profile, profile);
    assert_eq!(
        config.params,
        AlgorithmParams::SuperFlux {
            lookback_frames: 9,
            max_filter_radius: 4
        }
    );
    assert_eq!(
        profile.pitch_stability_params(),
        AlgorithmParams::PitchStability {
            threshold_cents: 90.0,
            min_stable_duration_ms: 40.0
        }
    );
}

#[test]
fn onset_profile_sanitizes_non_finite_thresholds() {
    let profile = OnsetDetectionProfile {
        lookback_frames: 0,
        max_filter_radius: 100,
        pitch_stability_threshold_cents: f32::NAN,
        pitch_stability_duration_ms: f32::INFINITY,
    }
    .sanitized();

    assert_eq!(profile.lookback_frames, 1);
    assert_eq!(profile.max_filter_radius, 32);
    assert_eq!(
        profile.pitch_stability_threshold_cents,
        DEFAULT_PITCH_STABILITY_THRESHOLD_CENTS
    );
    assert_eq!(
        profile.pitch_stability_duration_ms,
        DEFAULT_PITCH_STABILITY_DURATION_MS
    );
}

fn synthetic_pitch_jump_track() -> Vec<PitchFrame> {
    (0..64)
        .map(|index| PitchFrame {
            frame_index: index,
            source_sample_position: index * 256,
            timestamp_seconds: index as f32 * 256.0 / 16_000.0,
            f0_hz: Some(if index < 32 { 440.0 } else { 660.0 }),
            raw_f0_hz: if index < 32 { 440.0 } else { 660.0 },
            confidence: 0.95,
            voiced: true,
            rms: 0.2,
        })
        .collect()
}
