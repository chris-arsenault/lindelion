use lindelion_plugin_shell::vst3::{PluginMessageDecodeError, PluginMessagePayload};

use crate::{AnalysisStatus, CaptureState, Glirdir};

lindelion_plugin_shell::define_vst3_plugin_messages! {
    pub(super) enum GlirdirMessageKind;
    pub(super) enum GlirdirPluginMessage;
    prefix "lindelion.glirdir.";
    messages {
        empty {
            ArmCapture => "arm_capture",
            ClearScratchpad => "clear_scratchpad",
            FinalizeCaptureRequest => "finalize_capture_request",
            PlayAudition => "play_audition",
            StopAudition => "stop_audition",
            ToggleAuditionLoop => "toggle_audition_loop",
            ToggleAuditionLiveEdit => "toggle_audition_live_edit",
            SampleLibrarySaveRequest => "sample_library_save_request",
            MidiExportRequest => "midi_export_request",
            StatusRequest => "status_request",
            TelemetryRequest => "telemetry_request",
        }
        payload {
            SampleLibrarySaveResponse(Vec<u8>) => "sample_library_save_response",
            AnalysisStatusResponse(GlirdirStatusPayload) => "analysis_status_response",
            PatchUpdate(Vec<u8>) => "patch_update",
            MidiExportResponse(Vec<u8>) => "midi_export_response",
            StatusResponse(GlirdirStatusPayload) => "status_response",
            TelemetryResponse(GlirdirStatusPayload) => "telemetry_response",
        }
    }
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

    pub(super) fn midi_export_request() -> Self {
        Self::MidiExportRequest
    }

    pub(super) fn status_request() -> Self {
        Self::StatusRequest
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

impl PluginMessagePayload for GlirdirStatusPayload {
    fn into_payload(self) -> Vec<u8> {
        self.encode()
    }

    fn from_payload(payload: Vec<u8>) -> Result<Self, PluginMessageDecodeError> {
        GlirdirStatusPayload::decode(&payload).ok_or(PluginMessageDecodeError::MalformedPayload)
    }
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
