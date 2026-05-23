use lindelion_midi::{QuantizeSettings, RootNote, Scale, SnapMode, TimingGrid};
use lindelion_onset_detect::{AlgorithmParams, DetectionConfig};
use lindelion_pitch_detect::{PitchDetectionConfig, SWIFTF0_MODEL_FMAX_HZ, SWIFTF0_MODEL_FMIN_HZ};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlirdirPatch {
    pub name: String,
    pub capture: CaptureSettings,
    pub analysis: AnalysisSettings,
    pub quantize: QuantizeSettings,
    pub audition: AuditionSettings,
    pub scratchpad: Option<ScratchpadAudio>,
}

impl Default for GlirdirPatch {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            capture: CaptureSettings::default(),
            analysis: AnalysisSettings::default(),
            quantize: QuantizeSettings {
                root: RootNote::C,
                scale: Scale::Chromatic,
                snap_mode: SnapMode::Hard,
                grid: TimingGrid::Sixteenth,
                ..QuantizeSettings::default()
            },
            audition: AuditionSettings::default(),
            scratchpad: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureBars {
    Four,
    Eight,
    Sixteen,
}

impl CaptureBars {
    pub const ALL: [Self; 3] = [Self::Four, Self::Eight, Self::Sixteen];

    pub const fn bars(self) -> u8 {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }

    pub fn from_plain(value: f32) -> Self {
        match value.round() as i32 {
            value if value <= 4 => Self::Four,
            value if value <= 8 => Self::Eight,
            _ => Self::Sixteen,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    Immediate,
    PhraseBoundary,
    NextDownbeat,
}

impl SyncMode {
    pub const ALL: [Self; 3] = [Self::Immediate, Self::PhraseBoundary, Self::NextDownbeat];

    pub fn from_plain(value: f32) -> Self {
        match value.round() as i32 {
            value if value <= 0 => Self::Immediate,
            1 => Self::PhraseBoundary,
            _ => Self::NextDownbeat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureState {
    Idle,
    Armed,
    CountIn,
    Capturing,
    Captured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureSettings {
    pub bars: CaptureBars,
    pub sync_mode: SyncMode,
    pub count_in_bars: u8,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            bars: CaptureBars::Four,
            sync_mode: SyncMode::Immediate,
            count_in_bars: 0,
        }
    }
}

impl CaptureSettings {
    pub fn sanitized(self) -> Self {
        Self {
            bars: self.bars,
            sync_mode: self.sync_mode,
            count_in_bars: self.count_in_bars.min(2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSettings {
    pub confidence_threshold: f32,
    pub onset_sensitivity: f32,
    pub min_note_ms: f32,
}

impl Default for AnalysisSettings {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.5,
            onset_sensitivity: 0.5,
            min_note_ms: 80.0,
        }
    }
}

impl AnalysisSettings {
    pub fn sanitized(self) -> Self {
        Self {
            confidence_threshold: sanitize_range(self.confidence_threshold, 0.0, 1.0, 0.5),
            onset_sensitivity: sanitize_range(self.onset_sensitivity, 0.0, 1.0, 0.5),
            min_note_ms: sanitize_range(self.min_note_ms, 30.0, 300.0, 80.0),
        }
    }

    pub fn pitch_detection_config(self) -> PitchDetectionConfig {
        PitchDetectionConfig {
            confidence_threshold: self.sanitized().confidence_threshold,
            fmin_hz: SWIFTF0_MODEL_FMIN_HZ,
            fmax_hz: SWIFTF0_MODEL_FMAX_HZ,
        }
        .sanitized()
    }
}

impl From<AnalysisSettings> for DetectionConfig {
    fn from(value: AnalysisSettings) -> Self {
        let value = value.sanitized();
        Self {
            sensitivity: value.onset_sensitivity,
            min_slice_ms: value.min_note_ms,
            params: AlgorithmParams::SuperFlux {
                lookback_frames: 3,
                max_filter_radius: 3,
            },
            ..DetectionConfig::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AuditionSettings {
    pub volume: f32,
    pub loop_enabled: bool,
    pub live_edit: bool,
}

impl Default for AuditionSettings {
    fn default() -> Self {
        Self {
            volume: 0.35,
            loop_enabled: true,
            live_edit: true,
        }
    }
}

impl AuditionSettings {
    pub fn sanitized(self) -> Self {
        Self {
            volume: sanitize_range(self.volume, 0.0, 1.0, 0.35),
            loop_enabled: self.loop_enabled,
            live_edit: self.live_edit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScratchpadAudio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl ScratchpadAudio {
    pub fn new(sample_rate: u32, mut samples: Vec<f32>) -> Self {
        sanitize_samples(&mut samples);
        Self {
            sample_rate: sample_rate.max(1),
            samples,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

fn sanitize_range(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn sanitize_samples(samples: &mut [f32]) {
    for sample in samples {
        if !sample.is_finite() {
            *sample = 0.0;
        }
    }
}
