use lindelion_dsp_utils::analysis::{rms, spectral_centroid_hz};
use lindelion_phrase_analysis::PhraseAnalysisResult;
use lindelion_pitch_detect::median_voiced_pitch;

use crate::{
    AudioExpressionFeatures, AudioExpressionFrame, AudioExpressionFrameSource,
    AudioExpressionMapping, AudioExpressionSource,
};

pub type AudioAnalysisExpressionSource<'a, const VOICES: usize> =
    AudioExpressionSource<PhraseAnalysisExpressionFrameSource<'a>, VOICES>;

impl<'a, const VOICES: usize> AudioAnalysisExpressionSource<'a, VOICES> {
    pub fn new(
        audio: &'a [f32],
        sample_rate: u32,
        analysis: &'a PhraseAnalysisResult,
        mapping: AudioExpressionMapping,
    ) -> Self {
        Self::from_frame_source(
            PhraseAnalysisExpressionFrameSource::new(audio, sample_rate, analysis),
            mapping,
        )
    }
}

#[derive(Debug, Clone)]
pub struct PhraseAnalysisExpressionFrameSource<'a> {
    audio: &'a [f32],
    sample_rate: u32,
    analysis: &'a PhraseAnalysisResult,
    frame: AudioExpressionFrame,
}

impl<'a> PhraseAnalysisExpressionFrameSource<'a> {
    pub fn new(audio: &'a [f32], sample_rate: u32, analysis: &'a PhraseAnalysisResult) -> Self {
        Self {
            audio,
            sample_rate: sample_rate.max(1),
            analysis,
            frame: AudioExpressionFrame::default(),
        }
    }

    fn expression_frame(
        &self,
        start_sample: usize,
        end_sample: usize,
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame {
        let note_index = self.strongest_note_overlap(start_sample, end_sample);
        let Some(note_index) = note_index else {
            return AudioExpressionFrame {
                start_sample,
                end_sample,
                ..AudioExpressionFrame::default()
            };
        };

        let note = self.analysis.detected_notes[note_index];
        let frames = self
            .analysis
            .pitch_contour
            .frames_in_range(start_sample, end_sample);
        let pitch_hz = median_voiced_pitch(frames).or(Some(note.pitch_hz));
        let loudness = if frames.is_empty() {
            rms(self.audio.get(start_sample..end_sample).unwrap_or_default()).max(note.mean_rms)
        } else {
            frames
                .iter()
                .map(|frame| frame.rms)
                .filter(|value| value.is_finite())
                .sum::<f32>()
                / frames.len() as f32
        };
        let audio = self.audio.get(start_sample..end_sample).unwrap_or_default();
        let centroid = spectral_centroid_hz(audio, self.sample_rate as f32).unwrap_or(0.0);

        mapping.frame_from_features(AudioExpressionFeatures {
            start_sample,
            end_sample,
            pitch_hz,
            loudness_rms: loudness,
            spectral_centroid_hz: centroid,
            gate: true,
        })
    }

    fn strongest_note_overlap(&self, start_sample: usize, end_sample: usize) -> Option<usize> {
        self.analysis
            .detected_notes
            .iter()
            .enumerate()
            .filter_map(|(index, note)| {
                let start = start_sample.max(note.start_sample);
                let end = end_sample.min(note.end_sample);
                if end > start {
                    Some((index, end - start))
                } else {
                    None
                }
            })
            .max_by_key(|(_, overlap)| *overlap)
            .map(|(index, _)| index)
    }
}

impl AudioExpressionFrameSource for PhraseAnalysisExpressionFrameSource<'_> {
    fn current_frame(&self) -> AudioExpressionFrame {
        self.frame
    }

    fn set_block(
        &mut self,
        start_sample: usize,
        len_samples: usize,
        mapping: AudioExpressionMapping,
    ) -> AudioExpressionFrame {
        let start = start_sample.min(self.audio.len());
        let end = start.saturating_add(len_samples).min(self.audio.len());
        self.frame = self.expression_frame(start, end, mapping);
        self.frame
    }
}
