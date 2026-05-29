use lindelion_plugin_shell::vst3::{PluginMessageDecodeError, PluginMessagePayload};
use lindelion_ui::WaveformPoint;
use serde::{Deserialize, Serialize};

use crate::{
    AutoTuneEdit, ChokeGroupId, EnvelopeConfig, PadEdit, PadId, PlaybackMode, SourceAnalysisStatus,
};

lindelion_plugin_shell::define_vst3_plugin_messages! {
    pub(super) enum LinnodMessageKind;
    pub(super) enum LinnodPluginMessage;
    prefix "lindelion.linnod.";
    messages {
        empty {
            SourceLoadRequest => "source_load_request",
            RedetectSlices => "redetect_slices",
            TuneSelectedSlice => "tune_selected_slice",
            TuneAllSlices => "tune_all_slices",
            SnapAllSlicesToScale => "snap_all_slices_to_scale",
            StatusRequest => "status_request",
            SourceSummaryRequest => "source_summary_request",
            TelemetryRequest => "telemetry_request",
        }
        payload {
            PatchUpdate(Vec<u8>) => "patch_update",
            SourceIngestRequest(Vec<u8>) => "source_ingest_request",
            AnalysisStatusResponse(LinnodStatusPayload) => "analysis_status_response",
            SourceSummaryResponse(Vec<u8>) => "source_summary_response",
            MarkerEdit(Vec<u8>) => "marker_edit",
            PadEdit(Vec<u8>) => "pad_edit",
            PlaybackEdit(Vec<u8>) => "playback_edit",
            AutoTuneEdit(Vec<u8>) => "auto_tune_edit",
            DetectionEdit(Vec<u8>) => "detection_edit",
            SliceEdit(Vec<u8>) => "slice_edit",
            StatusResponse(LinnodStatusPayload) => "status_response",
            TelemetryResponse(Vec<u8>) => "telemetry_response",
        }
    }
}

impl LinnodPluginMessage {
    pub(super) fn patch_update(payload: Vec<u8>) -> Self {
        Self::PatchUpdate(payload)
    }

    pub(super) fn source_ingest_request(path: Vec<u8>) -> Self {
        Self::SourceIngestRequest(path)
    }

    pub(super) fn status_request() -> Self {
        Self::StatusRequest
    }

    pub(super) fn source_summary_request() -> Self {
        Self::SourceSummaryRequest
    }

    pub(super) fn telemetry_request() -> Self {
        Self::TelemetryRequest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LinnodStatusPayload {
    pub(super) source_status: SourceAnalysisStatus,
    pub(super) has_source: bool,
    pub(super) has_analysis: bool,
    pub(super) marker_count: usize,
    pub(super) selected_slice_index: Option<usize>,
    pub(super) active_voices: usize,
}

impl LinnodStatusPayload {
    pub(super) fn from_plugin(plugin: &crate::Linnod) -> Self {
        Self {
            source_status: plugin.source_status(),
            has_source: plugin.patch().source_sample.is_some(),
            has_analysis: plugin.source_analysis().is_some(),
            marker_count: plugin.patch().markers.len(),
            selected_slice_index: plugin.patch().selected_slice_index(),
            active_voices: plugin.active_voice_count(),
        }
    }

    fn encode(self) -> Vec<u8> {
        format!(
            "{},{},{},{},{},{}",
            source_status_id(self.source_status),
            u8::from(self.has_source),
            u8::from(self.has_analysis),
            self.marker_count,
            self.selected_slice_index
                .map(|index| index.to_string())
                .unwrap_or_else(|| "-".to_string()),
            self.active_voices
        )
        .into_bytes()
    }

    fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut parts = text.split(',');
        let source_status = source_status_from_id(parts.next()?)?;
        let has_source = bool_from_id(parts.next()?)?;
        let has_analysis = bool_from_id(parts.next()?)?;
        let marker_count = parts.next()?.parse().ok()?;
        let selected_slice_index = selected_index_from_id(parts.next()?)?;
        let active_voices = parts.next()?.parse().ok()?;
        parts.next().is_none().then_some(Self {
            source_status,
            has_source,
            has_analysis,
            marker_count,
            selected_slice_index,
            active_voices,
        })
    }
}

impl Default for LinnodStatusPayload {
    fn default() -> Self {
        Self {
            source_status: SourceAnalysisStatus::Idle,
            has_source: false,
            has_analysis: false,
            marker_count: 0,
            selected_slice_index: Some(0),
            active_voices: 0,
        }
    }
}

impl PluginMessagePayload for LinnodStatusPayload {
    fn into_payload(self) -> Vec<u8> {
        self.encode()
    }

    fn from_payload(payload: Vec<u8>) -> Result<Self, PluginMessageDecodeError> {
        Self::decode(&payload).ok_or(PluginMessageDecodeError::MalformedPayload)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct LinnodSourceSummaryPayload {
    pub(super) source_label: String,
    pub(super) source_sample_rate: u32,
    pub(super) waveform: Vec<LinnodWaveformPointPayload>,
    pub(super) slices: Vec<LinnodSourceSlicePayload>,
}

impl LinnodSourceSummaryPayload {
    pub(super) fn encode(&self) -> Result<Vec<u8>, toml::ser::Error> {
        toml::to_string(self).map(String::into_bytes)
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        toml::from_str(text).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(super) struct LinnodWaveformPointPayload {
    pub(super) min: f32,
    pub(super) max: f32,
    pub(super) rms: f32,
}

impl From<WaveformPoint> for LinnodWaveformPointPayload {
    fn from(value: WaveformPoint) -> Self {
        Self {
            min: value.min,
            max: value.max,
            rms: value.rms,
        }
    }
}

impl From<LinnodWaveformPointPayload> for WaveformPoint {
    fn from(value: LinnodWaveformPointPayload) -> Self {
        Self {
            min: value.min,
            max: value.max,
            rms: value.rms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(super) struct LinnodSourceSlicePayload {
    pub(super) index: usize,
    pub(super) start_sample: usize,
    pub(super) end_sample: usize,
    pub(super) detected_f0_hz: Option<f32>,
    pub(super) detected_midi_note: Option<f32>,
    pub(super) nearest_midi_note: Option<u8>,
    pub(super) nearest_scale_midi_note: Option<u8>,
    pub(super) nearest_midi_note_hz: Option<f32>,
    pub(super) nearest_scale_midi_note_hz: Option<f32>,
    pub(super) cents_deviation: Option<f32>,
    pub(super) root_target_f0_hz: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct LinnodTelemetryPayload {
    pub(super) left_peak: f32,
    pub(super) right_peak: f32,
    pub(super) active_voices: f32,
}

impl Default for LinnodTelemetryPayload {
    fn default() -> Self {
        Self {
            left_peak: 0.0,
            right_peak: 0.0,
            active_voices: 0.0,
        }
    }
}

impl LinnodTelemetryPayload {
    pub(super) fn encode(self) -> Vec<u8> {
        format!(
            "{:.8},{:.8},{:.8}",
            finite_telemetry(self.left_peak),
            finite_telemetry(self.right_peak),
            finite_telemetry(self.active_voices)
        )
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut parts = text.split(',');
        let left_peak = finite_telemetry(parts.next()?.parse().ok()?);
        let right_peak = finite_telemetry(parts.next()?.parse().ok()?);
        let active_voices = finite_telemetry(parts.next()?.parse().ok()?);
        parts.next().is_none().then_some(Self {
            left_peak,
            right_peak,
            active_voices,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinnodMarkerEditMessage {
    AddUser { position_samples: usize },
    RemoveAt { position_samples: usize },
}

impl LinnodMarkerEditMessage {
    pub(super) fn encode(self) -> Vec<u8> {
        match self {
            Self::AddUser { position_samples } => format!("add_user,{position_samples}"),
            Self::RemoveAt { position_samples } => format!("remove_at,{position_samples}"),
        }
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut parts = text.split(',');
        let kind = parts.next()?;
        let position_samples = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        match kind {
            "add_user" => Some(Self::AddUser { position_samples }),
            "remove_at" => Some(Self::RemoveAt { position_samples }),
            _ => None,
        }
    }
}

include!("messages_slice_edit.rs");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinnodPadEditMessage {
    ChokeGroup {
        pad: PadId,
        group: Option<ChokeGroupId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LinnodPlaybackEditMessage {
    Mode(PlaybackMode),
    Envelope(EnvelopeConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinnodAutoTuneEditMessage {
    Enabled(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LinnodDetectionEditMessage {
    Algorithm(lindelion_onset_detect::DetectionAlgorithm),
    MinSliceMs(f32),
    LookbackFrames(u32),
    MaxFilterRadius(u32),
    GroupDelayWeight(f32),
    SpectralWindowSize(usize),
    PitchStabilityThresholdCents(f32),
    PitchStabilityDurationMs(f32),
    EnergyFrameSize(usize),
    ManualGridDivisions(usize),
    ManualGridOffsetMs(f32),
}

impl LinnodPadEditMessage {
    pub(super) fn edit(self) -> PadEdit {
        match self {
            Self::ChokeGroup { group, .. } => PadEdit::ChokeGroup(group),
        }
    }

    pub(super) fn pad(self) -> PadId {
        match self {
            Self::ChokeGroup { pad, .. } => pad,
        }
    }

    pub(super) fn encode(self) -> Vec<u8> {
        match self {
            Self::ChokeGroup { pad, group } => format!(
                "choke_group\n{}\n{}",
                pad.sanitized().0,
                group
                    .map(|group| group.sanitized().0.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ),
        }
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let mut parts = text.splitn(3, '\n');
        let kind = parts.next()?;
        let pad = PadId::new(parts.next()?.parse().ok()?)?;
        let value = parts.next().unwrap_or_default();
        match kind {
            "choke_group" => Some(Self::ChokeGroup {
                pad,
                group: choke_group_from_id(value)?,
            }),
            _ => None,
        }
    }
}

impl LinnodDetectionEditMessage {
    pub(super) fn encode(self) -> Vec<u8> {
        match self {
            Self::Algorithm(algorithm) => {
                format!("algorithm\n{}", detection_algorithm_id(algorithm))
            }
            Self::MinSliceMs(value) => format!("min_slice_ms\n{value:.8}"),
            Self::LookbackFrames(value) => format!("lookback_frames\n{value}"),
            Self::MaxFilterRadius(value) => format!("max_filter_radius\n{value}"),
            Self::GroupDelayWeight(value) => format!("group_delay_weight\n{value:.8}"),
            Self::SpectralWindowSize(value) => format!("spectral_window_size\n{value}"),
            Self::PitchStabilityThresholdCents(value) => {
                format!("pitch_stability_threshold_cents\n{value:.8}")
            }
            Self::PitchStabilityDurationMs(value) => {
                format!("pitch_stability_duration_ms\n{value:.8}")
            }
            Self::EnergyFrameSize(value) => format!("energy_frame_size\n{value}"),
            Self::ManualGridDivisions(value) => format!("manual_grid_divisions\n{value}"),
            Self::ManualGridOffsetMs(value) => format!("manual_grid_offset_ms\n{value:.8}"),
        }
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let (kind, value) = text.split_once('\n')?;
        match kind {
            "algorithm" => detection_algorithm_from_id(value).map(Self::Algorithm),
            "min_slice_ms" => Some(Self::MinSliceMs(value.parse().ok()?)),
            "lookback_frames" => Some(Self::LookbackFrames(value.parse().ok()?)),
            "max_filter_radius" => Some(Self::MaxFilterRadius(value.parse().ok()?)),
            "group_delay_weight" => Some(Self::GroupDelayWeight(value.parse().ok()?)),
            "spectral_window_size" => Some(Self::SpectralWindowSize(value.parse().ok()?)),
            "pitch_stability_threshold_cents" => {
                Some(Self::PitchStabilityThresholdCents(value.parse().ok()?))
            }
            "pitch_stability_duration_ms" => {
                Some(Self::PitchStabilityDurationMs(value.parse().ok()?))
            }
            "energy_frame_size" => Some(Self::EnergyFrameSize(value.parse().ok()?)),
            "manual_grid_divisions" => Some(Self::ManualGridDivisions(value.parse().ok()?)),
            "manual_grid_offset_ms" => Some(Self::ManualGridOffsetMs(value.parse().ok()?)),
            _ => None,
        }
    }
}

impl LinnodAutoTuneEditMessage {
    pub(super) fn edit(self) -> AutoTuneEdit {
        match self {
            Self::Enabled(enabled) => AutoTuneEdit::Enabled(enabled),
        }
    }

    pub(super) fn encode(self) -> Vec<u8> {
        match self {
            Self::Enabled(enabled) => format!("enabled\n{}", u8::from(enabled)),
        }
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let (kind, value) = text.split_once('\n')?;
        match kind {
            "enabled" => Some(Self::Enabled(bool_from_id(value)?)),
            _ => None,
        }
    }
}

impl LinnodPlaybackEditMessage {
    pub(super) fn encode(self) -> Vec<u8> {
        match self {
            Self::Mode(mode) => format!("mode\n{}", playback_mode_id(mode)),
            Self::Envelope(envelope) => format!("envelope\n{}", encode_envelope(envelope)),
        }
        .into_bytes()
    }

    pub(super) fn decode(payload: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(payload).ok()?;
        let (kind, value) = text.split_once('\n')?;
        match kind {
            "mode" => playback_mode_from_id(value.parse().ok()?).map(Self::Mode),
            "envelope" => decode_envelope(value).map(Self::Envelope),
            _ => None,
        }
    }
}

fn source_status_id(status: SourceAnalysisStatus) -> &'static str {
    match status {
        SourceAnalysisStatus::Idle => "idle",
        SourceAnalysisStatus::PendingLoad => "pending_load",
        SourceAnalysisStatus::Analyzing => "analyzing",
        SourceAnalysisStatus::Ready => "ready",
        SourceAnalysisStatus::MissingSource => "missing_source",
        SourceAnalysisStatus::Error => "error",
    }
}

fn source_status_from_id(id: &str) -> Option<SourceAnalysisStatus> {
    match id {
        "idle" => Some(SourceAnalysisStatus::Idle),
        "pending_load" => Some(SourceAnalysisStatus::PendingLoad),
        "analyzing" => Some(SourceAnalysisStatus::Analyzing),
        "ready" => Some(SourceAnalysisStatus::Ready),
        "missing_source" => Some(SourceAnalysisStatus::MissingSource),
        "error" => Some(SourceAnalysisStatus::Error),
        _ => None,
    }
}

fn selected_index_from_id(id: &str) -> Option<Option<usize>> {
    if id == "-" {
        Some(None)
    } else {
        id.parse().ok().map(Some)
    }
}

fn bool_from_id(id: &str) -> Option<bool> {
    match id {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

pub(super) fn playback_mode_id(mode: PlaybackMode) -> u8 {
    match mode {
        PlaybackMode::OneShot => 0,
        PlaybackMode::Gated => 1,
        PlaybackMode::Looped => 2,
        PlaybackMode::Continue => 3,
    }
}

pub(super) fn playback_mode_from_id(id: u8) -> Option<PlaybackMode> {
    match id {
        0 => Some(PlaybackMode::OneShot),
        1 => Some(PlaybackMode::Gated),
        2 => Some(PlaybackMode::Looped),
        3 => Some(PlaybackMode::Continue),
        _ => None,
    }
}

fn encode_envelope(envelope: EnvelopeConfig) -> String {
    let envelope = envelope.sanitized();
    format!(
        "{:.8},{:.8},{:.8},{:.8}",
        envelope.attack_ms, envelope.decay_ms, envelope.sustain, envelope.release_ms
    )
}

fn decode_envelope(value: &str) -> Option<EnvelopeConfig> {
    let mut parts = value.split(',');
    let envelope = EnvelopeConfig {
        attack_ms: parts.next()?.parse().ok()?,
        decay_ms: parts.next()?.parse().ok()?,
        sustain: parts.next()?.parse().ok()?,
        release_ms: parts.next()?.parse().ok()?,
    }
    .sanitized();
    parts.next().is_none().then_some(envelope)
}

fn detection_algorithm_id(algorithm: lindelion_onset_detect::DetectionAlgorithm) -> &'static str {
    match algorithm {
        lindelion_onset_detect::DetectionAlgorithm::SuperFlux => "super_flux",
        lindelion_onset_detect::DetectionAlgorithm::ComplexFlux => "complex_flux",
        lindelion_onset_detect::DetectionAlgorithm::SpectralSparsity => "spectral_sparsity",
        lindelion_onset_detect::DetectionAlgorithm::PitchStability => "pitch_stability",
        lindelion_onset_detect::DetectionAlgorithm::EnergyTransient => "energy_transient",
        lindelion_onset_detect::DetectionAlgorithm::ManualGrid => "manual_grid",
    }
}

fn detection_algorithm_from_id(id: &str) -> Option<lindelion_onset_detect::DetectionAlgorithm> {
    match id {
        "super_flux" => Some(lindelion_onset_detect::DetectionAlgorithm::SuperFlux),
        "complex_flux" => Some(lindelion_onset_detect::DetectionAlgorithm::ComplexFlux),
        "spectral_sparsity" => Some(lindelion_onset_detect::DetectionAlgorithm::SpectralSparsity),
        "pitch_stability" => Some(lindelion_onset_detect::DetectionAlgorithm::PitchStability),
        "energy_transient" => Some(lindelion_onset_detect::DetectionAlgorithm::EnergyTransient),
        "manual_grid" => Some(lindelion_onset_detect::DetectionAlgorithm::ManualGrid),
        _ => None,
    }
}

fn choke_group_from_id(id: &str) -> Option<Option<ChokeGroupId>> {
    if id == "-" {
        Some(None)
    } else {
        id.parse().ok().and_then(ChokeGroupId::new).map(Some)
    }
}

fn finite_telemetry(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 64.0)
    } else {
        0.0
    }
}
