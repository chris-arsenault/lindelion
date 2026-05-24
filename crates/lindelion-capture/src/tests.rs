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

    let event =
        lindelion_test_allocator::assert_no_allocations("capture process completion", || {
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

#[test]
fn capture_settings_accept_legacy_named_bars() {
    let settings: CaptureSettings = toml::from_str(
        r#"
        bars = "Sixteen"
        sync_mode = "PhraseBoundary"
        count_in_bars = 1
        "#,
    )
    .unwrap();

    assert_eq!(settings.bars, 16);
    assert_eq!(settings.sync_mode, SyncMode::PhraseBoundary);
    assert_eq!(settings.count_in_bars, 1);
}

#[test]
fn capture_settings_sanitize_generic_bar_count() {
    assert_eq!(
        CaptureSettings {
            bars: 0,
            count_in_bars: 99,
            ..CaptureSettings::default()
        }
        .sanitized(),
        CaptureSettings {
            bars: 1,
            count_in_bars: MAX_COUNT_IN_BARS,
            ..CaptureSettings::default()
        }
    );
    assert_eq!(
        ScratchpadMetadata::new(f64::NAN, 0, 3, 99),
        ScratchpadMetadata {
            bpm: 120,
            time_signature_numerator: 1,
            time_signature_denominator: 4,
            capture_bars: 16,
        }
    );
}
