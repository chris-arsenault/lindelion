use lindelion_plugin_shell::vst3::{
    PluginMessage, PluginMessageDecodeError, PluginMessageType, TypedPluginMessage,
    decode_typed_message,
};
use vst3::{ComWrapper, Steinberg::Vst::IMessage};

use crate::{AnalysisStatus, CaptureState, Glirdir};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GlirdirMessageKind {
    ArmCapture,
    ClearScratchpad,
    FinalizeCaptureRequest,
    PlayAudition,
    StopAudition,
    ToggleAuditionLoop,
    ToggleAuditionLiveEdit,
    SampleLibrarySaveRequest,
    SampleLibrarySaveResponse,
    AnalysisStatusResponse,
    PatchUpdate,
    MidiExportRequest,
    MidiExportResponse,
    StatusRequest,
    StatusResponse,
    TelemetryRequest,
    TelemetryResponse,
}

impl PluginMessageType for GlirdirMessageKind {
    fn id(self) -> &'static str {
        match self {
            Self::ArmCapture => "lindelion.glirdir.arm_capture",
            Self::ClearScratchpad => "lindelion.glirdir.clear_scratchpad",
            Self::FinalizeCaptureRequest => "lindelion.glirdir.finalize_capture_request",
            Self::PlayAudition => "lindelion.glirdir.play_audition",
            Self::StopAudition => "lindelion.glirdir.stop_audition",
            Self::ToggleAuditionLoop => "lindelion.glirdir.toggle_audition_loop",
            Self::ToggleAuditionLiveEdit => "lindelion.glirdir.toggle_audition_live_edit",
            Self::SampleLibrarySaveRequest => "lindelion.glirdir.sample_library_save_request",
            Self::SampleLibrarySaveResponse => "lindelion.glirdir.sample_library_save_response",
            Self::AnalysisStatusResponse => "lindelion.glirdir.analysis_status_response",
            Self::PatchUpdate => "lindelion.glirdir.patch_update",
            Self::MidiExportRequest => "lindelion.glirdir.midi_export_request",
            Self::MidiExportResponse => "lindelion.glirdir.midi_export_response",
            Self::StatusRequest => "lindelion.glirdir.status_request",
            Self::StatusResponse => "lindelion.glirdir.status_response",
            Self::TelemetryRequest => "lindelion.glirdir.telemetry_request",
            Self::TelemetryResponse => "lindelion.glirdir.telemetry_response",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "lindelion.glirdir.arm_capture" => Some(Self::ArmCapture),
            "lindelion.glirdir.clear_scratchpad" => Some(Self::ClearScratchpad),
            "lindelion.glirdir.finalize_capture_request" => Some(Self::FinalizeCaptureRequest),
            "lindelion.glirdir.play_audition" => Some(Self::PlayAudition),
            "lindelion.glirdir.stop_audition" => Some(Self::StopAudition),
            "lindelion.glirdir.toggle_audition_loop" => Some(Self::ToggleAuditionLoop),
            "lindelion.glirdir.toggle_audition_live_edit" => Some(Self::ToggleAuditionLiveEdit),
            "lindelion.glirdir.sample_library_save_request" => Some(Self::SampleLibrarySaveRequest),
            "lindelion.glirdir.sample_library_save_response" => {
                Some(Self::SampleLibrarySaveResponse)
            }
            "lindelion.glirdir.analysis_status_response" => Some(Self::AnalysisStatusResponse),
            "lindelion.glirdir.patch_update" => Some(Self::PatchUpdate),
            "lindelion.glirdir.midi_export_request" => Some(Self::MidiExportRequest),
            "lindelion.glirdir.midi_export_response" => Some(Self::MidiExportResponse),
            "lindelion.glirdir.status_request" => Some(Self::StatusRequest),
            "lindelion.glirdir.status_response" => Some(Self::StatusResponse),
            "lindelion.glirdir.telemetry_request" => Some(Self::TelemetryRequest),
            "lindelion.glirdir.telemetry_response" => Some(Self::TelemetryResponse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GlirdirPluginMessage {
    ArmCapture,
    ClearScratchpad,
    FinalizeCaptureRequest,
    PlayAudition,
    StopAudition,
    ToggleAuditionLoop,
    ToggleAuditionLiveEdit,
    SampleLibrarySaveRequest,
    SampleLibrarySaveResponse(Vec<u8>),
    AnalysisStatusResponse(GlirdirStatusPayload),
    PatchUpdate(Vec<u8>),
    MidiExportRequest,
    MidiExportResponse(Vec<u8>),
    StatusRequest,
    StatusResponse(GlirdirStatusPayload),
    TelemetryRequest,
    TelemetryResponse(GlirdirStatusPayload),
}

impl GlirdirPluginMessage {
    pub(super) fn arm_capture() -> Self {
        Self::ArmCapture
    }

    pub(super) fn clear_scratchpad() -> Self {
        Self::ClearScratchpad
    }

    pub(super) fn finalize_capture_request() -> Self {
        Self::FinalizeCaptureRequest
    }

    pub(super) fn play_audition() -> Self {
        Self::PlayAudition
    }

    pub(super) fn stop_audition() -> Self {
        Self::StopAudition
    }

    pub(super) fn toggle_audition_loop() -> Self {
        Self::ToggleAuditionLoop
    }

    pub(super) fn toggle_audition_live_edit() -> Self {
        Self::ToggleAuditionLiveEdit
    }

    pub(super) fn sample_library_save_request() -> Self {
        Self::SampleLibrarySaveRequest
    }

    pub(super) fn patch_update(payload: Vec<u8>) -> Self {
        Self::PatchUpdate(payload)
    }

    pub(super) fn midi_export_request() -> Self {
        Self::MidiExportRequest
    }

    pub(super) fn status_request() -> Self {
        Self::StatusRequest
    }

    pub(super) fn telemetry_request() -> Self {
        Self::TelemetryRequest
    }

    pub(super) fn into_com_message(self) -> ComWrapper<PluginMessage> {
        PluginMessage::from_typed(match self {
            Self::ArmCapture => TypedPluginMessage::empty(GlirdirMessageKind::ArmCapture),
            Self::ClearScratchpad => TypedPluginMessage::empty(GlirdirMessageKind::ClearScratchpad),
            Self::FinalizeCaptureRequest => {
                TypedPluginMessage::empty(GlirdirMessageKind::FinalizeCaptureRequest)
            }
            Self::PlayAudition => TypedPluginMessage::empty(GlirdirMessageKind::PlayAudition),
            Self::StopAudition => TypedPluginMessage::empty(GlirdirMessageKind::StopAudition),
            Self::ToggleAuditionLoop => {
                TypedPluginMessage::empty(GlirdirMessageKind::ToggleAuditionLoop)
            }
            Self::ToggleAuditionLiveEdit => {
                TypedPluginMessage::empty(GlirdirMessageKind::ToggleAuditionLiveEdit)
            }
            Self::SampleLibrarySaveRequest => {
                TypedPluginMessage::empty(GlirdirMessageKind::SampleLibrarySaveRequest)
            }
            Self::SampleLibrarySaveResponse(payload) => {
                TypedPluginMessage::new(GlirdirMessageKind::SampleLibrarySaveResponse, payload)
            }
            Self::AnalysisStatusResponse(status) => {
                TypedPluginMessage::new(GlirdirMessageKind::AnalysisStatusResponse, status.encode())
            }
            Self::PatchUpdate(payload) => {
                TypedPluginMessage::new(GlirdirMessageKind::PatchUpdate, payload)
            }
            Self::MidiExportRequest => {
                TypedPluginMessage::empty(GlirdirMessageKind::MidiExportRequest)
            }
            Self::MidiExportResponse(payload) => {
                TypedPluginMessage::new(GlirdirMessageKind::MidiExportResponse, payload)
            }
            Self::StatusRequest => TypedPluginMessage::empty(GlirdirMessageKind::StatusRequest),
            Self::StatusResponse(status) => {
                TypedPluginMessage::new(GlirdirMessageKind::StatusResponse, status.encode())
            }
            Self::TelemetryRequest => {
                TypedPluginMessage::empty(GlirdirMessageKind::TelemetryRequest)
            }
            Self::TelemetryResponse(status) => {
                TypedPluginMessage::new(GlirdirMessageKind::TelemetryResponse, status.encode())
            }
        })
    }

    pub(super) unsafe fn decode(
        message: *mut IMessage,
    ) -> Result<Option<Self>, PluginMessageDecodeError> {
        let Some(message) = decode_typed_message::<GlirdirMessageKind>(message)? else {
            return Ok(None);
        };
        match message.kind {
            GlirdirMessageKind::ArmCapture => empty_request(message.payload, Self::ArmCapture),
            GlirdirMessageKind::ClearScratchpad => {
                empty_request(message.payload, Self::ClearScratchpad)
            }
            GlirdirMessageKind::FinalizeCaptureRequest => {
                empty_request(message.payload, Self::FinalizeCaptureRequest)
            }
            GlirdirMessageKind::PlayAudition => empty_request(message.payload, Self::PlayAudition),
            GlirdirMessageKind::StopAudition => empty_request(message.payload, Self::StopAudition),
            GlirdirMessageKind::ToggleAuditionLoop => {
                empty_request(message.payload, Self::ToggleAuditionLoop)
            }
            GlirdirMessageKind::ToggleAuditionLiveEdit => {
                empty_request(message.payload, Self::ToggleAuditionLiveEdit)
            }
            GlirdirMessageKind::SampleLibrarySaveRequest => {
                empty_request(message.payload, Self::SampleLibrarySaveRequest)
            }
            GlirdirMessageKind::SampleLibrarySaveResponse => {
                Ok(Some(Self::SampleLibrarySaveResponse(message.payload)))
            }
            GlirdirMessageKind::PatchUpdate => Ok(Some(Self::PatchUpdate(message.payload))),
            GlirdirMessageKind::MidiExportRequest => {
                empty_request(message.payload, Self::MidiExportRequest)
            }
            GlirdirMessageKind::MidiExportResponse => {
                Ok(Some(Self::MidiExportResponse(message.payload)))
            }
            GlirdirMessageKind::StatusRequest => {
                empty_request(message.payload, Self::StatusRequest)
            }
            GlirdirMessageKind::TelemetryRequest => {
                empty_request(message.payload, Self::TelemetryRequest)
            }
            GlirdirMessageKind::AnalysisStatusResponse => decode_status(message.payload)
                .map(|status| Some(Self::AnalysisStatusResponse(status))),
            GlirdirMessageKind::StatusResponse => {
                decode_status(message.payload).map(|status| Some(Self::StatusResponse(status)))
            }
            GlirdirMessageKind::TelemetryResponse => {
                decode_status(message.payload).map(|status| Some(Self::TelemetryResponse(status)))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GlirdirStatusPayload {
    pub(super) capture_state: CaptureState,
    pub(super) analysis_status: AnalysisStatus,
    pub(super) has_scratchpad: bool,
    pub(super) has_analysis: bool,
}

impl Default for GlirdirStatusPayload {
    fn default() -> Self {
        Self {
            capture_state: CaptureState::Idle,
            analysis_status: AnalysisStatus::Idle,
            has_scratchpad: false,
            has_analysis: false,
        }
    }
}

impl GlirdirStatusPayload {
    pub(super) fn from_plugin(plugin: &Glirdir) -> Self {
        Self {
            capture_state: plugin.capture_state(),
            analysis_status: plugin.analysis_status(),
            has_scratchpad: plugin.patch().scratchpad.is_some(),
            has_analysis: plugin.analysis().is_some(),
        }
    }

    fn encode(self) -> Vec<u8> {
        format!(
            "{},{},{},{}",
            capture_state_id(self.capture_state),
            analysis_status_id(self.analysis_status),
            u8::from(self.has_scratchpad),
            u8::from(self.has_analysis)
        )
        .into_bytes()
    }

    fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut parts = text.split(',');
        let capture_state = capture_state_from_id(parts.next()?)?;
        let analysis_status = analysis_status_from_id(parts.next()?)?;
        let has_scratchpad = bool_from_id(parts.next()?)?;
        let has_analysis = bool_from_id(parts.next()?)?;
        parts.next().is_none().then_some(Self {
            capture_state,
            analysis_status,
            has_scratchpad,
            has_analysis,
        })
    }
}

fn empty_request(
    payload: Vec<u8>,
    message: GlirdirPluginMessage,
) -> Result<Option<GlirdirPluginMessage>, PluginMessageDecodeError> {
    if payload.is_empty() {
        Ok(Some(message))
    } else {
        Err(PluginMessageDecodeError::MalformedPayload)
    }
}

fn decode_status(payload: Vec<u8>) -> Result<GlirdirStatusPayload, PluginMessageDecodeError> {
    GlirdirStatusPayload::decode(&payload).ok_or(PluginMessageDecodeError::MalformedPayload)
}

fn capture_state_id(state: CaptureState) -> &'static str {
    match state {
        CaptureState::Idle => "idle",
        CaptureState::Armed => "armed",
        CaptureState::CountIn => "count_in",
        CaptureState::Capturing => "capturing",
        CaptureState::Captured => "captured",
    }
}

fn capture_state_from_id(id: &str) -> Option<CaptureState> {
    match id {
        "idle" => Some(CaptureState::Idle),
        "armed" => Some(CaptureState::Armed),
        "count_in" => Some(CaptureState::CountIn),
        "capturing" => Some(CaptureState::Capturing),
        "captured" => Some(CaptureState::Captured),
        _ => None,
    }
}

fn analysis_status_id(status: AnalysisStatus) -> &'static str {
    match status {
        AnalysisStatus::Idle => "idle",
        AnalysisStatus::Capturing => "capturing",
        AnalysisStatus::CapturedPendingAnalysis => "captured_pending_analysis",
        AnalysisStatus::Analyzing => "analyzing",
        AnalysisStatus::Ready => "ready",
        AnalysisStatus::Error => "error",
    }
}

fn analysis_status_from_id(id: &str) -> Option<AnalysisStatus> {
    match id {
        "idle" => Some(AnalysisStatus::Idle),
        "capturing" => Some(AnalysisStatus::Capturing),
        "captured_pending_analysis" => Some(AnalysisStatus::CapturedPendingAnalysis),
        "analyzing" => Some(AnalysisStatus::Analyzing),
        "ready" => Some(AnalysisStatus::Ready),
        "error" => Some(AnalysisStatus::Error),
        _ => None,
    }
}

fn bool_from_id(id: &str) -> Option<bool> {
    match id {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}
