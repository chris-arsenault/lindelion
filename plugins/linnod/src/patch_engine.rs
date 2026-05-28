use serde::{Deserialize, Serialize};

use lindelion_pitch_shift::PitchShiftSynthesisAlgorithm;

use crate::patch::TriggerMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default)]
    pub pitch_shift_algorithm: PitchShiftAlgorithm,
}

impl EngineConfig {
    pub const fn sanitized(self) -> Self {
        self
    }

    pub fn apply_edit(&mut self, edit: EngineEdit) {
        match edit {
            EngineEdit::PitchShiftAlgorithm(algorithm) => {
                self.pitch_shift_algorithm = algorithm;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PitchShiftAlgorithm {
    #[default]
    SpectralPeak,
    Varispeed,
    TimeStretch,
    ResampleStretch,
}

impl PitchShiftAlgorithm {
    pub fn formant_ratio(self, trigger_mode: TriggerMode, pitch_ratio: f32) -> Option<f32> {
        match self {
            Self::SpectralPeak => {
                (!matches!(trigger_mode, TriggerMode::Pad)).then_some(pitch_ratio)
            }
            Self::Varispeed => None,
            Self::TimeStretch => Some(pitch_ratio),
            Self::ResampleStretch => None,
        }
    }

    pub fn render_pitch_ratio(self, pitch_ratio: f32) -> f32 {
        match self {
            Self::Varispeed => 1.0,
            Self::SpectralPeak | Self::TimeStretch | Self::ResampleStretch => pitch_ratio,
        }
    }

    pub fn playback_pitch_ratio(self, pitch_ratio: f32) -> f32 {
        match self {
            Self::Varispeed => pitch_ratio,
            Self::SpectralPeak | Self::TimeStretch | Self::ResampleStretch => 1.0,
        }
    }

    pub const fn synthesis_algorithm(self) -> PitchShiftSynthesisAlgorithm {
        match self {
            Self::SpectralPeak => PitchShiftSynthesisAlgorithm::SpectralPeak,
            Self::Varispeed => PitchShiftSynthesisAlgorithm::Varispeed,
            Self::TimeStretch => PitchShiftSynthesisAlgorithm::PitchSynchronous,
            Self::ResampleStretch => PitchShiftSynthesisAlgorithm::ResampleStretch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineEdit {
    PitchShiftAlgorithm(PitchShiftAlgorithm),
}
