use std::{error::Error, fmt};

use lindelion_dsp_utils::{
    analysis,
    math::{cents_between, finite_clamp, finite_or, ms_to_samples},
};
use lindelion_midi::DetectedNote;
use lindelion_onset_detect::{
    DetectionConfig, HybridOnsetDetector, OnsetDetectionInput, OnsetDetector, SliceMarker,
    slice_regions_from_markers,
};
use lindelion_pitch_detect::{
    PitchContour, PitchDetectionConfig, PitchDetectionError, PitchDetector, PitchFrame,
    SwiftF0Detector, median_voiced_pitch,
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_MIN_NOTE_MS: f32 = 80.0;
pub const DEFAULT_MIN_INHERITED_PITCH_RMS: f32 = 0.04;
pub const DEFAULT_SAME_PITCH_MERGE_CENTS: f32 = 35.0;
pub const DEFAULT_ARTICULATION_SEARCH_MS: f32 = 45.0;
pub const DEFAULT_ARTICULATION_GAP_RATIO: f32 = 0.35;
pub const DEFAULT_RMS_CHUNK_SAMPLES: usize = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PhraseAnalysisConfig {
    pub pitch_detection: PitchDetectionConfig,
    pub onset_detection: DetectionConfig,
    pub note_segmentation: NoteSegmentationConfig,
}

impl PhraseAnalysisConfig {
    pub fn sanitized(self) -> Self {
        Self {
            pitch_detection: self.pitch_detection.sanitized(),
            onset_detection: self.onset_detection,
            note_segmentation: self.note_segmentation.sanitized(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhraseAnalysisResult {
    pub pitch_contour: PitchContour,
    pub markers: Vec<SliceMarker>,
    pub segmented_notes: Vec<SegmentedNote>,
    pub detected_notes: Vec<DetectedNote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhraseAnalysisError {
    EmptyAudio,
    Pitch(PitchDetectionError),
}

impl From<PitchDetectionError> for PhraseAnalysisError {
    fn from(value: PitchDetectionError) -> Self {
        Self::Pitch(value)
    }
}

impl fmt::Display for PhraseAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyAudio => write!(formatter, "phrase analysis input is empty"),
            Self::Pitch(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for PhraseAnalysisError {}

#[derive(Debug, Clone)]
pub struct PhraseAnalyzer<P = SwiftF0Detector, O = HybridOnsetDetector> {
    pitch_detector: P,
    onset_detector: O,
}

impl Default for PhraseAnalyzer<SwiftF0Detector, HybridOnsetDetector> {
    fn default() -> Self {
        Self::new(SwiftF0Detector::default(), HybridOnsetDetector)
    }
}

impl<P, O> PhraseAnalyzer<P, O> {
    pub fn new(pitch_detector: P, onset_detector: O) -> Self {
        Self {
            pitch_detector,
            onset_detector,
        }
    }
}

impl<P, O> PhraseAnalyzer<P, O>
where
    P: PitchDetector,
    O: OnsetDetector,
{
    pub fn analyze(
        &self,
        audio: &[f32],
        sample_rate: u32,
        config: PhraseAnalysisConfig,
    ) -> Result<PhraseAnalysisResult, PhraseAnalysisError> {
        if audio.is_empty() {
            return Err(PhraseAnalysisError::EmptyAudio);
        }

        let config = config.sanitized();
        let pitch_contour =
            self.pitch_detector
                .detect_with_config(audio, sample_rate, config.pitch_detection)?;
        Ok(self.analyze_with_pitch_contour(audio, sample_rate, config, pitch_contour))
    }

    pub fn analyze_with_pitch_contour(
        &self,
        audio: &[f32],
        sample_rate: u32,
        config: PhraseAnalysisConfig,
        pitch_contour: PitchContour,
    ) -> PhraseAnalysisResult {
        analyze_with_pitch_contour(
            audio,
            sample_rate,
            config,
            pitch_contour,
            &self.onset_detector,
        )
    }
}

pub fn analyze_with_pitch_contour(
    audio: &[f32],
    sample_rate: u32,
    config: PhraseAnalysisConfig,
    pitch_contour: PitchContour,
    onset_detector: &impl OnsetDetector,
) -> PhraseAnalysisResult {
    let config = config.sanitized();
    let markers = {
        let input = OnsetDetectionInput::new(audio, sample_rate).with_pitch_contour(&pitch_contour);
        onset_detector.detect(input, config.onset_detection)
    };
    analyze_with_markers(
        audio,
        sample_rate,
        config.note_segmentation,
        pitch_contour,
        markers,
    )
}

pub fn analyze_with_markers(
    audio: &[f32],
    sample_rate: u32,
    config: NoteSegmentationConfig,
    pitch_contour: PitchContour,
    markers: Vec<SliceMarker>,
) -> PhraseAnalysisResult {
    let segmented_notes =
        NoteSegmenter::new(config).segment_notes(audio, sample_rate, &pitch_contour, &markers);
    let detected_notes = detected_notes(&segmented_notes);

    PhraseAnalysisResult {
        pitch_contour,
        markers,
        segmented_notes,
        detected_notes,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NoteSegmentationConfig {
    pub min_note_ms: f32,
    pub min_inherited_pitch_rms: f32,
    pub same_pitch_merge_cents: f32,
    pub articulation_search_ms: f32,
    pub articulation_gap_ratio: f32,
    pub rms_chunk_samples: usize,
}

impl Default for NoteSegmentationConfig {
    fn default() -> Self {
        Self {
            min_note_ms: DEFAULT_MIN_NOTE_MS,
            min_inherited_pitch_rms: DEFAULT_MIN_INHERITED_PITCH_RMS,
            same_pitch_merge_cents: DEFAULT_SAME_PITCH_MERGE_CENTS,
            articulation_search_ms: DEFAULT_ARTICULATION_SEARCH_MS,
            articulation_gap_ratio: DEFAULT_ARTICULATION_GAP_RATIO,
            rms_chunk_samples: DEFAULT_RMS_CHUNK_SAMPLES,
        }
    }
}

impl NoteSegmentationConfig {
    pub fn relaxed() -> Self {
        Self {
            min_note_ms: 60.0,
            min_inherited_pitch_rms: 0.025,
            same_pitch_merge_cents: 50.0,
            articulation_search_ms: 55.0,
            articulation_gap_ratio: 0.25,
            rms_chunk_samples: DEFAULT_RMS_CHUNK_SAMPLES,
        }
    }

    pub fn aggressive() -> Self {
        Self {
            min_note_ms: 45.0,
            min_inherited_pitch_rms: 0.06,
            same_pitch_merge_cents: 20.0,
            articulation_search_ms: 35.0,
            articulation_gap_ratio: 0.5,
            rms_chunk_samples: 128,
        }
    }

    pub fn sanitized(self) -> Self {
        Self {
            min_note_ms: finite_clamp(self.min_note_ms, 0.0, 5_000.0, DEFAULT_MIN_NOTE_MS),
            min_inherited_pitch_rms: finite_clamp(
                self.min_inherited_pitch_rms,
                0.0,
                1.0,
                DEFAULT_MIN_INHERITED_PITCH_RMS,
            ),
            same_pitch_merge_cents: finite_clamp(
                self.same_pitch_merge_cents,
                0.0,
                1_200.0,
                DEFAULT_SAME_PITCH_MERGE_CENTS,
            ),
            articulation_search_ms: finite_clamp(
                self.articulation_search_ms,
                0.0,
                1_000.0,
                DEFAULT_ARTICULATION_SEARCH_MS,
            ),
            articulation_gap_ratio: finite_clamp(
                self.articulation_gap_ratio,
                0.0,
                1.0,
                DEFAULT_ARTICULATION_GAP_RATIO,
            ),
            rms_chunk_samples: self.rms_chunk_samples.max(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SegmentedNote {
    pub note: DetectedNote,
    pub inherited_pitch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteSegmenter {
    config: NoteSegmentationConfig,
}

impl Default for NoteSegmenter {
    fn default() -> Self {
        Self::new(NoteSegmentationConfig::default())
    }
}

impl NoteSegmenter {
    pub fn new(config: NoteSegmentationConfig) -> Self {
        Self {
            config: config.sanitized(),
        }
    }

    pub const fn config(&self) -> NoteSegmentationConfig {
        self.config
    }

    pub fn segment_notes(
        &self,
        audio: &[f32],
        sample_rate: u32,
        pitch_contour: &PitchContour,
        markers: &[SliceMarker],
    ) -> Vec<SegmentedNote> {
        if audio.is_empty() || markers.is_empty() {
            return Vec::new();
        }

        let min_samples = ms_to_samples(self.config.min_note_ms, sample_rate);
        let mut notes = Vec::new();
        let mut previous_pitch = None;
        for region in slice_regions_from_markers(markers, audio.len()) {
            let start = region.start_sample;
            let end = region.end_sample;
            if end.saturating_sub(start) < min_samples.max(1) {
                continue;
            }

            let frames = pitch_contour.frames_in_range(start, end);
            let pitch = median_voiced_pitch(frames);
            let inherited_pitch = pitch.is_none();
            let pitch = pitch.or(previous_pitch);
            let Some(pitch_hz) = pitch else {
                continue;
            };

            let (peak_rms, mean_rms) = note_rms(audio, start, end, frames);
            let audio_region = audio.get(start..end).unwrap_or_default();
            if inherited_pitch
                && (peak_rms < self.config.min_inherited_pitch_rms
                    || minimum_chunk_rms(audio_region, self.config.rms_chunk_samples)
                        < self.config.min_inherited_pitch_rms)
            {
                continue;
            }

            previous_pitch = Some(pitch_hz);
            notes.push(SegmentedNote {
                note: DetectedNote {
                    start_sample: start,
                    end_sample: end,
                    pitch_hz,
                    peak_rms,
                    mean_rms,
                },
                inherited_pitch,
            });
        }

        self.merge_unarticulated_same_pitch_notes(notes, audio, sample_rate)
    }

    fn merge_unarticulated_same_pitch_notes(
        &self,
        notes: Vec<SegmentedNote>,
        audio: &[f32],
        sample_rate: u32,
    ) -> Vec<SegmentedNote> {
        let mut merged: Vec<SegmentedNote> = Vec::new();
        for note in notes {
            if let Some(previous) = merged.last_mut()
                && self.should_merge_same_pitch_split(previous, &note, audio, sample_rate)
            {
                previous.note.end_sample = note.note.end_sample;
                previous.note.peak_rms = previous.note.peak_rms.max(note.note.peak_rms);
                previous.note.mean_rms = previous.note.mean_rms.max(note.note.mean_rms);
                continue;
            }
            merged.push(note);
        }
        merged
    }

    fn should_merge_same_pitch_split(
        &self,
        previous: &SegmentedNote,
        next: &SegmentedNote,
        audio: &[f32],
        sample_rate: u32,
    ) -> bool {
        !previous.inherited_pitch
            && !next.inherited_pitch
            && cents_between(previous.note.pitch_hz, next.note.pitch_hz)
                <= self.config.same_pitch_merge_cents
            && !self.has_articulation_gap(previous.note, next.note, audio, sample_rate)
    }

    fn has_articulation_gap(
        &self,
        previous: DetectedNote,
        next: DetectedNote,
        audio: &[f32],
        sample_rate: u32,
    ) -> bool {
        if audio.is_empty() {
            return false;
        }
        let search_radius = ms_to_samples(self.config.articulation_search_ms, sample_rate).max(1);
        let boundary = next.start_sample.min(audio.len() - 1);
        let start = boundary.saturating_sub(search_radius);
        let end = boundary;
        let window = audio.get(start..end).unwrap_or_default();
        let reference = previous
            .peak_rms
            .min(next.peak_rms)
            .max(self.config.min_inherited_pitch_rms);
        minimum_chunk_rms(window, self.config.rms_chunk_samples)
            < reference * self.config.articulation_gap_ratio
    }
}

pub fn segment_notes(
    audio: &[f32],
    sample_rate: u32,
    pitch_contour: &PitchContour,
    markers: &[SliceMarker],
    config: NoteSegmentationConfig,
) -> Vec<SegmentedNote> {
    NoteSegmenter::new(config).segment_notes(audio, sample_rate, pitch_contour, markers)
}

pub fn detected_notes(notes: &[SegmentedNote]) -> Vec<DetectedNote> {
    notes.iter().map(|note| note.note).collect()
}

pub fn note_rms(audio: &[f32], start: usize, end: usize, frames: &[PitchFrame]) -> (f32, f32) {
    if !frames.is_empty() {
        let peak = frames
            .iter()
            .map(|frame| frame.rms)
            .filter(|value| value.is_finite())
            .fold(0.0, f32::max);
        let mean = frames
            .iter()
            .map(|frame| finite_or(frame.rms, 0.0))
            .sum::<f32>()
            / frames.len() as f32;
        return (peak, mean);
    }

    let rms = analysis::rms(audio.get(start..end).unwrap_or_default());
    (rms, rms)
}

pub fn minimum_chunk_rms(audio: &[f32], chunk_samples: usize) -> f32 {
    audio
        .chunks(chunk_samples.max(1))
        .filter(|chunk| !chunk.is_empty())
        .map(analysis::rms)
        .fold(f32::MAX, f32::min)
}

#[cfg(test)]
mod tests;
