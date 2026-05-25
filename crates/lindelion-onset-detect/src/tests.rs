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
fn configured_complex_flux_detects_phase_discontinuous_articulation() {
    let audio = phase_reset_sine(48_000, 440.0, 5_000);
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::ComplexFlux,
        sensitivity: 0.35,
        min_slice_ms: 40.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::ComplexFlux {
            lookback_frames: 2,
            group_delay_weight: 1.0,
        },
    };

    let markers = ConfiguredOnsetDetector.detect(OnsetDetectionInput::new(&audio, 48_000), config);

    assert!(
        markers
            .iter()
            .any(|marker| marker.position_samples > 3_600 && marker.position_samples < 6_000)
    );
}

#[test]
fn configured_spectral_sparsity_detects_noise_to_tone_change() {
    let mut audio = (0..4_800)
        .map(|index| if index % 2 == 0 { 0.35 } else { -0.35 })
        .collect::<Vec<_>>();
    audio.extend((0..4_800).map(|index| {
        let phase = 2.0 * std::f32::consts::PI * 880.0 * index as f32 / 48_000.0;
        phase.sin() * 0.5
    }));
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::SpectralSparsity,
        sensitivity: 0.4,
        min_slice_ms: 40.0,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::SpectralSparsity { window_size: 512 },
    };

    let markers = ConfiguredOnsetDetector.detect(OnsetDetectionInput::new(&audio, 48_000), config);

    assert!(
        markers
            .iter()
            .any(|marker| marker.position_samples > 3_600 && marker.position_samples < 6_000)
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
fn marker_reconciliation_supports_merge_replace_and_cancel() {
    let auto = [SliceMarker {
        position_samples: 2_400,
        kind: MarkerKind::Auto,
    }];
    let existing = [SliceMarker {
        position_samples: 2_450,
        kind: MarkerKind::User,
    }];

    assert_eq!(
        reconcile_markers(
            auto,
            &existing,
            MarkerReconcilePolicy::CancelIfUserMarkers,
            256,
            4_800,
        ),
        MarkerReconcileOutcome::Cancelled
    );
    assert_eq!(
        reconcile_markers(
            auto,
            &existing,
            MarkerReconcilePolicy::ReplaceUserMarkers,
            256,
            4_800,
        ),
        MarkerReconcileOutcome::Applied(vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 2_400,
                kind: MarkerKind::Auto,
            }
        ])
    );
    assert_eq!(
        reconcile_markers(
            auto,
            &existing,
            MarkerReconcilePolicy::MergeUserMarkers,
            256,
            4_800,
        ),
        MarkerReconcileOutcome::Applied(vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 2_450,
                kind: MarkerKind::User,
            }
        ])
    );
}

#[test]
fn slice_regions_and_selection_are_source_bounded() {
    let markers = normalize_markers(
        [
            SliceMarker {
                position_samples: 900,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 100,
                kind: MarkerKind::User,
            },
            SliceMarker {
                position_samples: 1_200,
                kind: MarkerKind::Auto,
            },
        ],
        50,
        1_000,
    );

    assert_eq!(
        markers,
        vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 100,
                kind: MarkerKind::User,
            },
            SliceMarker {
                position_samples: 900,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 999,
                kind: MarkerKind::Auto,
            }
        ]
    );
    let regions = slice_regions_from_markers(&markers, 1_000);
    assert_eq!(regions[1].duration_samples(), 800);
    assert_eq!(
        slice_region_at_sample(&markers, 1_000, 950).unwrap(),
        SliceRegion {
            index: 2,
            start_sample: 900,
            end_sample: 999
        }
    );
}

#[test]
fn marker_positions_snap_to_nearest_zero_crossing() {
    let audio = [-0.5, -0.25, 0.1, 0.5, 0.25, -0.1, -0.4];

    assert_eq!(snap_position_to_nearest_zero_crossing(&audio, 3, 2), 2);
    assert_eq!(snap_position_to_nearest_zero_crossing(&audio, 6, 1), 5);
}

#[test]
fn strongest_marker_selection_keeps_start_and_loudest_candidates() {
    let mut audio = vec![0.0; 1_000];
    audio[120] = 0.25;
    audio[360] = 0.9;
    audio[720] = 0.6;
    let markers = [
        SliceMarker {
            position_samples: 120,
            kind: MarkerKind::Auto,
        },
        SliceMarker {
            position_samples: 360,
            kind: MarkerKind::Auto,
        },
        SliceMarker {
            position_samples: 720,
            kind: MarkerKind::Auto,
        },
    ];

    let selected = select_strongest_markers(markers, &audio, 3, 32);

    assert_eq!(
        selected,
        vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 360,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 720,
                kind: MarkerKind::Auto,
            },
        ]
    );
}

#[test]
fn strongest_marker_selection_preserves_user_markers_before_auto_markers() {
    let mut audio = vec![0.0; 1_000];
    audio[200] = 0.1;
    audio[500] = 1.0;
    audio[800] = 0.9;
    let markers = [
        SliceMarker {
            position_samples: 200,
            kind: MarkerKind::User,
        },
        SliceMarker {
            position_samples: 500,
            kind: MarkerKind::Auto,
        },
        SliceMarker {
            position_samples: 800,
            kind: MarkerKind::Auto,
        },
    ];

    let selected = select_strongest_markers(markers, &audio, 3, 32);

    assert_eq!(
        selected,
        vec![
            SliceMarker {
                position_samples: 0,
                kind: MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 200,
                kind: MarkerKind::User,
            },
            SliceMarker {
                position_samples: 500,
                kind: MarkerKind::Auto,
            },
        ]
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

#[test]
fn detection_config_switches_algorithms_with_matching_default_params() {
    let config = DetectionConfig::default().with_algorithm(DetectionAlgorithm::ComplexFlux);

    assert_eq!(config.algorithm, DetectionAlgorithm::ComplexFlux);
    assert_eq!(
        config.params,
        AlgorithmParams::ComplexFlux {
            lookback_frames: DEFAULT_SUPERFLUX_LOOKBACK_FRAMES,
            group_delay_weight: DEFAULT_COMPLEX_FLUX_GROUP_DELAY_WEIGHT,
        }
    );

    let config = config.with_algorithm(DetectionAlgorithm::ManualGrid);

    assert_eq!(
        config.params,
        AlgorithmParams::ManualGrid {
            divisions: DEFAULT_MANUAL_GRID_DIVISIONS,
            offset_ms: DEFAULT_MANUAL_GRID_OFFSET_MS,
        }
    );
}

#[test]
fn detection_config_sanitizes_algorithm_specific_params() {
    let config = DetectionConfig {
        algorithm: DetectionAlgorithm::SpectralSparsity,
        sensitivity: f32::INFINITY,
        min_slice_ms: f32::NAN,
        profile: OnsetDetectionProfile::default(),
        params: AlgorithmParams::SpectralSparsity { window_size: 1 },
    }
    .sanitized();

    assert_eq!(config.sensitivity, DEFAULT_ONSET_SENSITIVITY);
    assert_eq!(config.min_slice_ms, DEFAULT_MIN_SLICE_MS);
    assert_eq!(
        config.params,
        AlgorithmParams::SpectralSparsity { window_size: 64 }
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

fn phase_reset_sine(sample_rate: u32, frequency: f32, reset_position: usize) -> Vec<f32> {
    (0..9_600)
        .map(|index| {
            let phase_index = if index < reset_position {
                index
            } else {
                index - reset_position
            };
            let phase =
                2.0 * std::f32::consts::PI * frequency * phase_index as f32 / sample_rate as f32;
            phase.sin() * 0.5
        })
        .collect()
}
