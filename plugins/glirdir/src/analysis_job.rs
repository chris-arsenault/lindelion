use lindelion_midi::QuantizeSettings;

use crate::{
    analysis::{AnalysisError, AnalysisResult, GlirdirAnalyzer, requantize_result},
    patch::{AnalysisSettings, ScratchpadAudio},
};

pub type AnalysisSequence = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisStatus {
    Idle,
    Capturing,
    CapturedPendingAnalysis,
    Analyzing,
    Ready,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisJob {
    pub sequence: AnalysisSequence,
    pub scratchpad: ScratchpadAudio,
    pub sample_rate: u32,
    pub analysis_settings: AnalysisSettings,
    pub quantize_settings: QuantizeSettings,
}

impl AnalysisJob {
    pub fn new(
        sequence: AnalysisSequence,
        scratchpad: ScratchpadAudio,
        analysis_settings: AnalysisSettings,
        mut quantize_settings: QuantizeSettings,
    ) -> Self {
        let sample_rate = scratchpad.sample_rate;
        quantize_settings.sample_rate = sample_rate;
        Self {
            sequence,
            scratchpad,
            sample_rate,
            analysis_settings,
            quantize_settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequantizeJob {
    pub sequence: AnalysisSequence,
    pub result: AnalysisResult,
    pub quantize_settings: QuantizeSettings,
}

impl RequantizeJob {
    pub fn new(
        sequence: AnalysisSequence,
        result: AnalysisResult,
        mut quantize_settings: QuantizeSettings,
        sample_rate: u32,
    ) -> Self {
        quantize_settings.sample_rate = sample_rate;
        Self {
            sequence,
            result,
            quantize_settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisJobResult {
    pub sequence: AnalysisSequence,
    pub result: Result<AnalysisResult, AnalysisError>,
}

impl AnalysisJobResult {
    pub fn ready(sequence: AnalysisSequence, result: AnalysisResult) -> Self {
        Self {
            sequence,
            result: Ok(result),
        }
    }

    pub fn error(sequence: AnalysisSequence, error: AnalysisError) -> Self {
        Self {
            sequence,
            result: Err(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisResultCache {
    sequence: AnalysisSequence,
    status: AnalysisStatus,
    result: Option<AnalysisResult>,
    error: Option<AnalysisError>,
}

impl Default for AnalysisResultCache {
    fn default() -> Self {
        Self {
            sequence: 0,
            status: AnalysisStatus::Idle,
            result: None,
            error: None,
        }
    }
}

impl AnalysisResultCache {
    pub const fn sequence(&self) -> AnalysisSequence {
        self.sequence
    }

    pub const fn status(&self) -> AnalysisStatus {
        self.status
    }

    pub const fn result(&self) -> Option<&AnalysisResult> {
        self.result.as_ref()
    }

    pub const fn error(&self) -> Option<&AnalysisError> {
        self.error.as_ref()
    }

    pub fn mark_idle(&mut self, sequence: AnalysisSequence) {
        self.sequence = sequence;
        self.status = AnalysisStatus::Idle;
        self.result = None;
        self.error = None;
    }

    pub fn mark_capturing(&mut self, sequence: AnalysisSequence) {
        self.sequence = sequence;
        self.status = AnalysisStatus::Capturing;
        self.result = None;
        self.error = None;
    }

    pub fn mark_captured_pending_analysis(&mut self, sequence: AnalysisSequence) {
        self.sequence = sequence;
        self.status = AnalysisStatus::CapturedPendingAnalysis;
        self.result = None;
        self.error = None;
    }

    pub fn mark_analyzing(&mut self, sequence: AnalysisSequence) {
        self.sequence = sequence;
        self.status = AnalysisStatus::Analyzing;
        self.result = None;
        self.error = None;
    }

    pub fn mark_requantizing(&mut self, sequence: AnalysisSequence) {
        self.sequence = sequence;
        self.status = AnalysisStatus::Analyzing;
        self.error = None;
    }

    pub fn publish_result(&mut self, job_result: AnalysisJobResult) -> bool {
        if job_result.sequence != self.sequence {
            return false;
        }

        match job_result.result {
            Ok(result) => {
                self.result = Some(result);
                self.error = None;
                self.status = AnalysisStatus::Ready;
            }
            Err(error) => {
                self.result = None;
                self.error = Some(error);
                self.status = AnalysisStatus::Error;
            }
        }

        true
    }

    pub fn requantize_current(&mut self, quantize_settings: &QuantizeSettings) -> bool {
        let Some(result) = self.result.as_mut() else {
            return false;
        };
        requantize_result(result, quantize_settings);
        true
    }
}

pub fn run_analysis_job(job: &AnalysisJob) -> AnalysisJobResult {
    GlirdirAnalyzer::default().analyze_job(job)
}

pub fn run_requantize_job(mut job: RequantizeJob) -> AnalysisJobResult {
    requantize_result(&mut job.result, &job.quantize_settings);
    AnalysisJobResult::ready(job.sequence, job.result)
}

impl<D> GlirdirAnalyzer<D>
where
    D: lindelion_pitch_detect::PitchDetector,
{
    pub(crate) fn analyze_job(&self, job: &AnalysisJob) -> AnalysisJobResult {
        AnalysisJobResult {
            sequence: job.sequence,
            result: self.analyze(
                &job.scratchpad,
                job.analysis_settings,
                &job.quantize_settings,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_midi::{
        DetectedNote, MidiClip, QuantizeSettings, QuantizedNote, RootNote, Scale, SnapMode,
        TimingGrid,
    };
    use lindelion_pitch_detect::{PitchContour, PitchFrame};

    #[test]
    fn stale_job_result_does_not_overwrite_newer_cache_sequence() {
        let mut cache = AnalysisResultCache::default();
        cache.mark_analyzing(2);

        let stale = AnalysisJobResult::ready(1, result_with_note(60));

        assert!(!cache.publish_result(stale));
        assert_eq!(cache.status(), AnalysisStatus::Analyzing);
        assert!(cache.result().is_none());
    }

    #[test]
    fn cache_requantizes_current_result_without_replacing_detection_outputs() {
        let mut cache = AnalysisResultCache::default();
        cache.mark_analyzing(1);
        assert!(cache.publish_result(AnalysisJobResult::ready(1, result_with_note(61))));
        let detected_notes = cache.result().unwrap().detected_notes.clone();
        let pitch_contour = cache.result().unwrap().pitch_contour.clone();
        let markers = cache.result().unwrap().markers.clone();

        assert!(cache.requantize_current(&QuantizeSettings {
            root: RootNote::C,
            scale: Scale::Major,
            snap_mode: SnapMode::Hard,
            grid: TimingGrid::Quarter,
            timing_strength: 1.0,
            sample_rate: 48_000,
            bpm: 120.0,
            ..QuantizeSettings::default()
        }));

        let result = cache.result().unwrap();
        assert_eq!(result.detected_notes, detected_notes);
        assert_eq!(result.pitch_contour, pitch_contour);
        assert_eq!(result.markers, markers);
        assert_eq!(result.midi_clip.notes[0].midi_note, 60);
    }

    fn result_with_note(midi_note: u8) -> AnalysisResult {
        AnalysisResult {
            pitch_contour: PitchContour {
                source_sample_rate: 48_000,
                analysis_sample_rate: 16_000,
                hop_size: 256,
                frames: vec![PitchFrame {
                    frame_index: 0,
                    source_sample_position: 0,
                    timestamp_seconds: 0.0,
                    f0_hz: Some(440.0),
                    raw_f0_hz: 440.0,
                    confidence: 0.95,
                    voiced: true,
                    rms: 0.2,
                }],
            },
            markers: Vec::new(),
            detected_notes: vec![DetectedNote {
                start_sample: 0,
                end_sample: 24_000,
                pitch_hz: 277.18,
                peak_rms: 0.5,
                mean_rms: 0.3,
            }],
            midi_clip: MidiClip {
                ppq: 960,
                bpm: 120,
                time_signature_numerator: 4,
                time_signature_denominator: 4,
                notes: vec![QuantizedNote {
                    start_tick: 0,
                    duration_ticks: 960,
                    midi_note,
                    velocity: 100,
                }],
            },
        }
    }
}
