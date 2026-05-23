use lindelion_plugin_shell::vst3::{
    PluginMessage, PluginMessageDecodeError, PluginMessageType, TypedPluginMessage,
    decode_typed_message,
};
use vst3::{ComWrapper, Steinberg::Vst::IMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResonatorMessageKind {
    PatchUpdate,
    TelemetryRequest,
    TelemetryResponse,
}

impl PluginMessageType for ResonatorMessageKind {
    fn id(self) -> &'static str {
        match self {
            Self::PatchUpdate => "lindelion.lamath.patch_update",
            Self::TelemetryRequest => "lindelion.lamath.telemetry_request",
            Self::TelemetryResponse => "lindelion.lamath.telemetry_response",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "lindelion.lamath.patch_update" => Some(Self::PatchUpdate),
            "lindelion.lamath.telemetry_request" => Some(Self::TelemetryRequest),
            "lindelion.lamath.telemetry_response" => Some(Self::TelemetryResponse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResonatorPluginMessage {
    PatchUpdate(Vec<u8>),
    TelemetryRequest,
    TelemetryResponse(Vec<u8>),
}

impl ResonatorPluginMessage {
    pub(super) fn patch_update(payload: Vec<u8>) -> Self {
        Self::PatchUpdate(payload)
    }

    pub(super) fn telemetry_request() -> Self {
        Self::TelemetryRequest
    }

    pub(super) fn telemetry_response(payload: Vec<u8>) -> Self {
        Self::TelemetryResponse(payload)
    }

    pub(super) fn into_com_message(self) -> ComWrapper<PluginMessage> {
        let message = match self {
            Self::PatchUpdate(payload) => {
                TypedPluginMessage::new(ResonatorMessageKind::PatchUpdate, payload)
            }
            Self::TelemetryRequest => {
                TypedPluginMessage::empty(ResonatorMessageKind::TelemetryRequest)
            }
            Self::TelemetryResponse(payload) => {
                TypedPluginMessage::new(ResonatorMessageKind::TelemetryResponse, payload)
            }
        };
        PluginMessage::from_typed(message)
    }

    pub(super) unsafe fn decode(
        message: *mut IMessage,
    ) -> Result<Option<Self>, PluginMessageDecodeError> {
        let Some(message) = decode_typed_message::<ResonatorMessageKind>(message)? else {
            return Ok(None);
        };
        match message.kind {
            ResonatorMessageKind::PatchUpdate => Ok(Some(Self::PatchUpdate(message.payload))),
            ResonatorMessageKind::TelemetryRequest => {
                if message.payload.is_empty() {
                    Ok(Some(Self::TelemetryRequest))
                } else {
                    Err(PluginMessageDecodeError::MalformedPayload)
                }
            }
            ResonatorMessageKind::TelemetryResponse => {
                Ok(Some(Self::TelemetryResponse(message.payload)))
            }
        }
    }
}
