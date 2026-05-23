use std::{error::Error, fmt};

use lindelion_dsp_utils::analysis;
use lindelion_midi::{DetectedNote, MidiClip, QuantizeSettings, clip_from_detected_notes};
use lindelion_onset_detect::{HybridOnsetDetector, PitchTrack, PitchTrackFrame, SliceMarker};
use lindelion_pitch_detect::{
    PitchContour, PitchDetectionError, PitchDetector, SWIFTF0_HOP_SIZE, SWIFTF0_TARGET_SAMPLE_RATE,
    SwiftF0Detector,
};
use serde::{Deserialize, Serialize};

use crate::patch::{AnalysisSettings, ScratchpadAudio};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub pitch_contour: PitchContour,
    pub markers: Vec<SliceMarker>,
    pub detected_notes: Vec<DetectedNote>,
    pub midi_clip: MidiClip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    EmptyScratchpad,
    Pitch(PitchDetectionError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyScratchpad => write!(formatter, "scratchpad audio is empty"),
            Self::Pitch(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for AnalysisError {}

impl From<PitchDetectionError> for AnalysisError {
    fn from(value: PitchDetectionError) -> Self {
        Self::Pitch(value)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GlirdirAnalyzer<D = SwiftF0Detector> {
    pitch_detector: D,
}

impl Default for GlirdirAnalyzer<SwiftF0Detector> {
    fn default() -> Self {
        Self {
            pitch_detector: SwiftF0Detector::default(),
        }
    }
}

impl<D> GlirdirAnalyzer<D> {
    #[cfg(test)]
    pub fn new(pitch_detector: D) -> Self {
        Self { pitch_detector }
    }
}

impl<D> GlirdirAnalyzer<D>
where
    D: PitchDetector,
{
    pub fn analyze(
        &self,
        scratchpad: &ScratchpadAudio,
        analysis_settings: AnalysisSettings,
        quantize_settings: &QuantizeSettings,
    ) -> Result<AnalysisResult, AnalysisError> {
        if scratchpad.samples.is_empty() {
            return Err(AnalysisError::EmptyScratchpad);
        }

        let pitch_contour = self.pitch_detector.detect_with_config(
            &scratchpad.samples,
            scratchpad.sample_rate,
            analysis_settings.pitch_detection_config(),
        )?;
        Ok(analyze_with_pitch_contour(
            scratchpad,
            analysis_settings,
            quantize_settings,
            pitch_contour,
        ))
    }
}

pub(crate) fn analyze_with_pitch_contour(
    scratchpad: &ScratchpadAudio,
    analysis_settings: AnalysisSettings,
    quantize_settings: &QuantizeSettings,
    pitch_contour: PitchContour,
) -> AnalysisResult {
    let analysis_settings = analysis_settings.sanitized();
    let pitch_track_frames = pitch_track_frames(&pitch_contour);
    let pitch_track = PitchTrack {
        source_sample_rate: scratchpad.sample_rate,
        frame_hop_samples: source_frame_hop_samples(&pitch_contour),
        frames: &pitch_track_frames,
    };
    let markers = HybridOnsetDetector.detect_with_pitch_track(
        &scratchpad.samples,
        scratchpad.sample_rate,
        analysis_settings.into(),
        pitch_track,
    );
    let detected_notes = segment_notes(
        &scratchpad.samples,
        scratchpad.sample_rate,
        &pitch_contour,
        &markers,
        analysis_settings,
    );
    let mut quantize_settings = quantize_settings.clone();
    quantize_settings.sample_rate = scratchpad.sample_rate;
    let midi_clip = clip_from_detected_notes(&detected_notes, &quantize_settings);

    AnalysisResult {
        pitch_contour,
        markers,
        detected_notes,
        midi_clip,
    }
}

pub(crate) fn requantize_result(result: &mut AnalysisResult, quantize_settings: &QuantizeSettings) {
    result.midi_clip = clip_from_detected_notes(&result.detected_notes, quantize_settings);
}

pub(crate) fn segment_notes(
    audio: &[f32],
    sample_rate: u32,
    pitch_contour: &PitchContour,
    markers: &[SliceMarker],
    settings: AnalysisSettings,
) -> Vec<DetectedNote> {
    if audio.is_empty() || markers.is_empty() {
        return Vec::new();
    }

    let min_samples = ms_to_samples(settings.sanitized().min_note_ms, sample_rate);
    let mut positions = markers
        .iter()
        .map(|marker| marker.position_samples.min(audio.len()))
        .collect::<Vec<_>>();
    positions.push(audio.len());
    positions.sort_unstable();
    positions.dedup();

    let mut notes = Vec::new();
    let mut previous_pitch = None;
    for window in positions.windows(2) {
        let start = window[0];
        let end = window[1].min(audio.len());
        if end.saturating_sub(start) < min_samples.max(1) {
            continue;
        }

        let frames = frames_in_range(pitch_contour, start, end);
        let pitch = median_pitch(frames).or(previous_pitch);
        let Some(pitch_hz) = pitch else {
            continue;
        };
        previous_pitch = Some(pitch_hz);

        let (peak_rms, mean_rms) = note_rms(audio, start, end, frames);
        notes.push(DetectedNote {
            start_sample: start,
            end_sample: end,
            pitch_hz,
            peak_rms,
            mean_rms,
        });
    }

    notes
}

fn pitch_track_frames(contour: &PitchContour) -> Vec<PitchTrackFrame> {
    contour
        .frames
        .iter()
        .map(|frame| PitchTrackFrame {
            source_sample_position: frame.source_sample_position,
            f0_hz: frame.f0_hz,
            confidence: frame.confidence,
        })
        .collect()
}

fn source_frame_hop_samples(contour: &PitchContour) -> usize {
    contour
        .frames
        .windows(2)
        .find_map(|window| {
            window[1]
                .source_sample_position
                .checked_sub(window[0].source_sample_position)
                .filter(|hop| *hop > 0)
        })
        .unwrap_or_else(|| {
            (SWIFTF0_HOP_SIZE as f32 * contour.source_sample_rate as f32
                / SWIFTF0_TARGET_SAMPLE_RATE as f32)
                .round()
                .max(1.0) as usize
        })
}

fn frames_in_range(
    contour: &PitchContour,
    start: usize,
    end: usize,
) -> &[lindelion_pitch_detect::PitchFrame] {
    let first = contour
        .frames
        .partition_point(|frame| frame.source_sample_position < start);
    let last = contour
        .frames
        .partition_point(|frame| frame.source_sample_position < end);
    &contour.frames[first..last]
}

fn median_pitch(frames: &[lindelion_pitch_detect::PitchFrame]) -> Option<f32> {
    let mut pitches = frames
        .iter()
        .filter_map(|frame| frame.f0_hz)
        .filter(|pitch| pitch.is_finite() && *pitch > 0.0)
        .collect::<Vec<_>>();
    if pitches.is_empty() {
        return None;
    }
    pitches.sort_by(f32::total_cmp);
    Some(pitches[pitches.len() / 2])
}

fn note_rms(
    audio: &[f32],
    start: usize,
    end: usize,
    frames: &[lindelion_pitch_detect::PitchFrame],
) -> (f32, f32) {
    if !frames.is_empty() {
        let peak = frames
            .iter()
            .map(|frame| frame.rms)
            .filter(|value| value.is_finite())
            .fold(0.0, f32::max);
        let mean = frames
            .iter()
            .map(|frame| {
                if frame.rms.is_finite() {
                    frame.rms
                } else {
                    0.0
                }
            })
            .sum::<f32>()
            / frames.len() as f32;
        return (peak, mean);
    }

    let rms = analysis::rms(audio.get(start..end).unwrap_or_default());
    (rms, rms)
}

fn ms_to_samples(ms: f32, sample_rate: u32) -> usize {
    ((ms.max(0.0) * 0.001) * sample_rate.max(1) as f32).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_pitch_detect::{
        PitchDetectionConfig, PitchDetectionError, PitchFrame, SWIFTF0_MODEL_FMAX_HZ,
        SWIFTF0_MODEL_FMIN_HZ,
    };
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn segmentation_preserves_previous_pitch_through_low_confidence_region() {
        let audio = vec![0.2; 4_800];
        let contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0), 0.95),
                pitch_frame(1, 1_200, Some(440.0), 0.95),
                pitch_frame(2, 2_400, None, 0.1),
                pitch_frame(3, 3_600, None, 0.1),
            ],
        };
        let markers = vec![
            SliceMarker {
                position_samples: 0,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
            SliceMarker {
                position_samples: 2_400,
                kind: lindelion_onset_detect::MarkerKind::Auto,
            },
        ];

        let notes = segment_notes(
            &audio,
            48_000,
            &contour,
            &markers,
            AnalysisSettings {
                min_note_ms: 10.0,
                ..AnalysisSettings::default()
            },
        );

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].pitch_hz, 440.0);
        assert_eq!(notes[1].pitch_hz, 440.0);
    }

    #[test]
    fn analysis_reuses_shared_midi_quantizer() {
        let scratchpad = ScratchpadAudio::new(48_000, vec![0.2; 4_800]);
        let contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 16_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0, Some(440.0), 0.95),
                pitch_frame(1, 1_200, Some(440.0), 0.95),
                pitch_frame(2, 2_400, Some(493.88), 0.95),
                pitch_frame(3, 3_600, Some(493.88), 0.95),
            ],
        };

        let result = analyze_with_pitch_contour(
            &scratchpad,
            AnalysisSettings {
                min_note_ms: 10.0,
                ..AnalysisSettings::default()
            },
            &QuantizeSettings::default(),
            contour,
        );

        assert!(!result.detected_notes.is_empty());
        assert_eq!(result.midi_clip.ppq, lindelion_midi::DEFAULT_PPQ);
    }

    #[test]
    fn analyzer_passes_analysis_settings_into_pitch_detection_config() {
        let configs = Rc::new(RefCell::new(Vec::new()));
        let analyzer = GlirdirAnalyzer::new(RecordingPitchDetector {
            configs: Rc::clone(&configs),
        });
        let scratchpad = ScratchpadAudio::new(48_000, vec![0.2; 4_800]);

        analyzer
            .analyze(
                &scratchpad,
                AnalysisSettings {
                    confidence_threshold: 0.82,
                    onset_sensitivity: 0.25,
                    min_note_ms: 40.0,
                },
                &QuantizeSettings::default(),
            )
            .unwrap();

        let configs = configs.borrow();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].confidence_threshold, 0.82);
        assert_eq!(configs[0].fmin_hz, SWIFTF0_MODEL_FMIN_HZ);
        assert_eq!(configs[0].fmax_hz, SWIFTF0_MODEL_FMAX_HZ);
    }

    #[derive(Debug, Clone)]
    struct RecordingPitchDetector {
        configs: Rc<RefCell<Vec<PitchDetectionConfig>>>,
    }

    impl PitchDetector for RecordingPitchDetector {
        fn detect(
            &self,
            audio: &[f32],
            sample_rate: u32,
        ) -> Result<PitchContour, PitchDetectionError> {
            self.detect_with_config(audio, sample_rate, PitchDetectionConfig::default())
        }

        fn detect_with_config(
            &self,
            audio: &[f32],
            sample_rate: u32,
            config: PitchDetectionConfig,
        ) -> Result<PitchContour, PitchDetectionError> {
            self.configs.borrow_mut().push(config);
            Ok(PitchContour {
                source_sample_rate: sample_rate,
                analysis_sample_rate: sample_rate,
                hop_size: 256,
                frames: vec![
                    pitch_frame(0, 0, Some(440.0), 0.95),
                    pitch_frame(1, audio.len().saturating_div(2), Some(440.0), 0.95),
                ],
            })
        }
    }

    fn pitch_frame(
        frame_index: usize,
        source_sample_position: usize,
        f0_hz: Option<f32>,
        confidence: f32,
    ) -> PitchFrame {
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds: source_sample_position as f32 / 48_000.0,
            f0_hz,
            raw_f0_hz: f0_hz.unwrap_or(0.0),
            confidence,
            voiced: f0_hz.is_some(),
            rms: 0.2,
        }
    }
}
