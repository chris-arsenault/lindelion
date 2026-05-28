lindelion_plugin_shell::define_vst3_plugin_messages! {
    pub(super) enum ResonatorMessageKind;
    pub(super) enum ResonatorPluginMessage;
    prefix "lindelion.lamath.";
    messages {
        empty {
            TelemetryRequest => "telemetry_request",
            ResetAudioEngine => "reset_audio_engine",
        }
        payload {
            PatchUpdate(Vec<u8>) => "patch_update",
            TelemetryResponse(Vec<u8>) => "telemetry_response",
        }
    }
}

impl ResonatorPluginMessage {
    pub(super) fn patch_update(payload: Vec<u8>) -> Self {
        Self::PatchUpdate(payload)
    }

    pub(super) fn telemetry_request() -> Self {
        Self::TelemetryRequest
    }

    pub(super) fn reset_audio_engine() -> Self {
        Self::ResetAudioEngine
    }

    pub(super) fn telemetry_response(payload: Vec<u8>) -> Self {
        Self::TelemetryResponse(payload)
    }
}
