#[test]
fn telemetry_payload_roundtrips() {
    let telemetry = ResonatorTelemetry {
        left_peak: 0.25,
        right_peak: 0.5,
        left_rms: 0.125,
        right_rms: 0.375,
        active_voices: 3,
        sidechain: ResonatorSidechainTelemetry {
            required: true,
            input_detected: true,
            signal_active: true,
            note_detected: true,
            pitch_confidence: 0.875,
        },
    };

    let decoded = decode_telemetry(encode_telemetry(telemetry).as_bytes()).unwrap();

    assert_eq!(decoded.left_peak, 0.25);
    assert_eq!(decoded.right_peak, 0.5);
    assert_eq!(decoded.left_rms, 0.125);
    assert_eq!(decoded.right_rms, 0.375);
    assert_eq!(decoded.active_voices, 3.0);
    assert!(decoded.sidechain_required);
    assert!(decoded.sidechain_input_detected);
    assert!(decoded.sidechain_signal_active);
    assert!(decoded.audio_note_detected);
    assert_eq!(decoded.audio_note_pitch_confidence, 0.875);
}

#[test]
fn plugin_message_roundtrips_payload() {
    let message = ResonatorPluginMessage::patch_update(b"patch".to_vec())
        .into_com_message()
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { ResonatorPluginMessage::decode(message.as_ptr()) };

    assert_eq!(
        decoded,
        Ok(Some(ResonatorPluginMessage::PatchUpdate(b"patch".to_vec())))
    );
}

#[test]
fn unknown_plugin_messages_are_ignored_safely() {
    let processor = ResonatorVst3Processor::new();
    let message = PluginMessage::with_payload("lindelion.lamath.future", Vec::new())
        .to_com_ptr::<IMessage>()
        .unwrap();

    let decoded = unsafe { ResonatorPluginMessage::decode(message.as_ptr()) };
    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(decoded, Ok(None));
    assert_eq!(result, kNotImplemented);
}

#[test]
fn malformed_plugin_message_payloads_do_not_panic() {
    let processor = ResonatorVst3Processor::new();
    let message = PluginMessage::with_payload(
        ResonatorMessageKind::TelemetryRequest.id(),
        b"unexpected".to_vec(),
    )
    .to_com_ptr::<IMessage>()
    .unwrap();

    let decoded = unsafe { ResonatorPluginMessage::decode(message.as_ptr()) };
    let result = unsafe { processor.notify(message.as_ptr()) };

    assert_eq!(decoded, Err(PluginMessageDecodeError::MalformedPayload));
    assert_eq!(result, kResultFalse);
}
