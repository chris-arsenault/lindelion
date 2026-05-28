use std::path::{Path, PathBuf};

use lindelion_plugin_shell::{
    AudioPlugin, ParameterId, ParameterInfo, PluginDescriptor, PluginState, ProcessContext,
    ProcessSetup, advance_async_job_sequence,
};
use lindelion_sample_library::RuntimeMonoAudioBuffer;

use crate::{
    SourceAnalysis, SourceAnalysisCache, SourceAnalysisJob, SourceAnalysisJobResult,
    SourceAnalysisSequence, SourceAnalysisStatus, SourceLoadError,
    parameters::{self, ParameterApplyKind},
    patch::LinnodPatch,
    patch_io,
    runtime::LinnodProcessor,
};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Linnod", *b"lindelion_linnod");
const DEFAULT_LIBRARY_DIR: &str = "Ahara";

#[derive(Debug)]
pub struct Linnod {
    setup: ProcessSetup,
    patch: LinnodPatch,
    source_cache: SourceAnalysisCache,
    next_source_sequence: SourceAnalysisSequence,
    library_root: PathBuf,
    processor: LinnodProcessor,
}

impl Default for Linnod {
    fn default() -> Self {
        Self {
            setup: ProcessSetup::default(),
            patch: LinnodPatch::default(),
            source_cache: SourceAnalysisCache::default(),
            next_source_sequence: 0,
            library_root: lindelion_sample_library::music_library_root(DEFAULT_LIBRARY_DIR),
            processor: LinnodProcessor::new(ProcessSetup::default().sample_rate as f32),
        }
    }
}

impl Linnod {
    #[cfg(test)]
    fn with_library_root(library_root: PathBuf) -> Self {
        Self {
            library_root,
            ..Self::default()
        }
    }

    pub fn patch(&self) -> &LinnodPatch {
        &self.patch
    }

    pub fn source_analysis(&self) -> Option<&SourceAnalysis> {
        self.source_cache.analysis()
    }

    pub fn source_audio(&self) -> Option<&RuntimeMonoAudioBuffer> {
        self.source_analysis().map(|analysis| &analysis.audio)
    }

    pub fn source_status(&self) -> SourceAnalysisStatus {
        self.source_cache.status()
    }

    pub fn source_error(&self) -> Option<&SourceLoadError> {
        self.source_cache.error()
    }

    pub fn active_voice_count(&self) -> usize {
        self.processor.active_voice_count()
    }

    pub fn request_source_load_job(&mut self) -> Option<SourceAnalysisJob> {
        self.patch.source_sample.as_ref()?;
        let sequence = self.advance_source_sequence();
        let job = SourceAnalysisJob::load(sequence, &self.patch, self.library_root.clone())?;
        self.source_cache.mark_analyzing(sequence);
        Some(job)
    }

    pub fn request_source_ingest_job(&mut self, path: impl AsRef<Path>) -> SourceAnalysisJob {
        let sequence = self.advance_source_sequence();
        let job = SourceAnalysisJob::ingest(
            sequence,
            path.as_ref(),
            &self.patch,
            self.library_root.clone(),
        );
        self.source_cache.mark_analyzing(sequence);
        job
    }

    pub fn publish_source_analysis_result(&mut self, result: SourceAnalysisJobResult) -> bool {
        let accepted = self.source_cache.publish_result(result);
        if accepted && self.source_cache.status() == SourceAnalysisStatus::Ready {
            let Some(analysis) = self.source_cache.analysis() else {
                return accepted;
            };
            self.patch.source_sample = Some(analysis.source.reference.clone());
            self.patch.markers = analysis.markers.clone();
            self.processor
                .prepare_source_analysis(&self.patch, analysis);
        }
        accepted
    }

    pub fn set_parameter_normalized(
        &mut self,
        id: ParameterId,
        normalized: f32,
    ) -> Option<ParameterApplyKind> {
        let apply = parameters::apply_parameter_normalized(&mut self.patch, id.0, normalized);
        if matches!(apply, Some(ParameterApplyKind::Analysis)) {
            self.mark_source_pending_or_idle();
        }
        apply
    }

    pub(crate) fn set_patch(&mut self, patch: LinnodPatch) {
        self.patch = patch;
        self.processor.clear_voices();
        self.mark_source_pending_or_idle();
    }

    pub(crate) fn set_patch_preserving_source_analysis(&mut self, patch: LinnodPatch) {
        self.patch = patch;
        self.processor.clear_voices();
        if let Some(analysis) = self.source_cache.analysis() {
            self.processor.prepare_patch(&self.patch, analysis);
        }
    }

    fn mark_source_pending_or_idle(&mut self) {
        let sequence = self.advance_source_sequence();
        if self.patch.source_sample.is_some() {
            self.source_cache.mark_pending_load(sequence);
        } else {
            self.source_cache.mark_idle(sequence);
        }
    }

    fn advance_source_sequence(&mut self) -> SourceAnalysisSequence {
        advance_async_job_sequence(&mut self.next_source_sequence)
    }
}

impl AudioPlugin for Linnod {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        parameters::PARAMETERS
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        self.processor.reset(setup.sample_rate as f32);
        if let Some(analysis) = self.source_cache.analysis() {
            self.processor
                .prepare_source_analysis(&self.patch, analysis);
        }
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        self.setup = context.setup;
        self.processor.process(
            &self.patch,
            self.source_cache.analysis(),
            context.events,
            context.buffer.left,
            context.buffer.right,
        );
    }

    fn state(&self) -> PluginState {
        patch_io::to_plugin_state(&self.patch)
            .unwrap_or_else(|_| PluginState::empty(patch_io::FORMAT_VERSION))
    }

    fn load_state(&mut self, state: PluginState) {
        if let Ok(patch) = patch_io::from_plugin_state(state) {
            self.set_patch(patch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::{SliceEdit, TriggerMode};
    use lindelion_pitch_detect::{PitchContour, PitchFrame};
    use lindelion_pitch_shift::PitchShiftAnalyzer;
    use lindelion_plugin_shell::{AudioBuffer, ProcessContext};
    use lindelion_sample_library::{
        OwnedMonoAudioBuffer, SampleMetadata, SampleReference, SampleWaveformPreview,
    };

    #[test]
    fn plugin_state_preserves_patch_and_slice_edits() {
        let mut plugin = Linnod::default();
        plugin.patch.trigger_mode = TriggerMode::Chromatic;
        plugin
            .patch
            .apply_slice_edit(0, SliceEdit::Name("Lead".to_string()));

        let state = plugin.state();
        let mut restored = Linnod::default();
        restored.load_state(state);

        assert_eq!(restored.patch.trigger_mode, TriggerMode::Chromatic);
        assert_eq!(restored.patch.slices[0].name, "Lead");
    }

    #[test]
    fn plugin_parameter_update_uses_registry_and_persists_in_state() {
        let mut plugin = Linnod::default();

        assert_eq!(
            plugin.set_parameter_normalized(ParameterId(parameters::MASTER_GAIN_PARAMETER_ID), 1.0),
            Some(ParameterApplyKind::Output)
        );

        let state = plugin.state();
        let mut restored = Linnod::default();
        restored.load_state(state);

        assert_eq!(restored.patch.output.master_gain_db, 12.0);
    }

    #[test]
    fn plugin_source_load_job_is_sequence_checked_and_updates_patch_on_publish() {
        let mut plugin = Linnod::with_library_root(PathBuf::from("Library"));
        plugin.patch.source_sample = Some(SampleReference::new("hash", "source.wav"));

        let job = plugin.request_source_load_job().unwrap();
        assert_eq!(plugin.source_status(), SourceAnalysisStatus::Analyzing);
        assert_eq!(job.sequence, 1);

        assert!(
            !plugin.publish_source_analysis_result(SourceAnalysisJobResult::error(
                0,
                SourceLoadError::Analysis(crate::SourceAnalysisError::EmptySource)
            ))
        );
        assert_eq!(plugin.source_status(), SourceAnalysisStatus::Analyzing);

        assert!(
            plugin.publish_source_analysis_result(SourceAnalysisJobResult::ready(
                1,
                source_analysis()
            ))
        );
        assert_eq!(plugin.source_status(), SourceAnalysisStatus::Ready);
        assert_eq!(
            plugin.patch.source_sample.as_ref().unwrap().last_known_path,
            PathBuf::from("Samples/source.wav")
        );
        assert_eq!(plugin.patch.markers.len(), 1);
        assert!(plugin.source_audio().is_some());
    }

    #[test]
    fn load_state_marks_source_for_background_reload() {
        let patch = LinnodPatch {
            source_sample: Some(SampleReference::new("hash", "source.wav")),
            ..LinnodPatch::default()
        };
        let state = patch_io::to_plugin_state(&patch).unwrap();
        let mut plugin = Linnod::default();

        plugin.load_state(state);

        assert_eq!(plugin.source_status(), SourceAnalysisStatus::PendingLoad);
    }

    #[test]
    fn process_stays_silent_for_foundation_stage() {
        let mut plugin = Linnod::default();
        let mut left = [1.0, -0.5];
        let mut right = [0.25, -1.0];
        let buffer = AudioBuffer {
            left: &mut left,
            right: &mut right,
        };

        plugin.process(ProcessContext::new(ProcessSetup::default(), buffer, &[]));

        assert_eq!(left, [0.0, 0.0]);
        assert_eq!(right, [0.0, 0.0]);
    }

    #[test]
    fn process_renders_loaded_source_on_midi_trigger() {
        let mut plugin = Linnod::default();
        let job = plugin.request_source_ingest_job("source.wav");
        assert!(
            plugin.publish_source_analysis_result(SourceAnalysisJobResult::ready(
                job.sequence,
                source_analysis()
            ))
        );
        let mut left = [0.0; 256];
        let mut right = [0.0; 256];

        plugin.process(ProcessContext::new(
            ProcessSetup::default(),
            AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            &[lindelion_plugin_shell::MidiEvent::Note(
                lindelion_plugin_shell::NoteEvent::On {
                    channel: 0,
                    note: 36,
                    velocity: 1.0,
                },
            )],
        ));

        assert_eq!(plugin.active_voice_count(), 1);
        assert!(peak_abs(&left) > 0.000_01);
        assert!(peak_abs(&right) > 0.000_01);
    }

    fn source_analysis() -> SourceAnalysis {
        let samples = sine_wave(220.0, 48_000, 4_800);
        let owned_audio = OwnedMonoAudioBuffer::new(samples.clone(), 48_000);
        let pitch_contour = PitchContour {
            source_sample_rate: 48_000,
            analysis_sample_rate: 48_000,
            hop_size: 256,
            frames: vec![
                pitch_frame(0, 0),
                pitch_frame(1, 1_200),
                pitch_frame(2, 2_400),
                pitch_frame(3, 3_600),
            ],
        };
        let markers = vec![lindelion_onset_detect::SliceMarker {
            position_samples: 0,
            kind: lindelion_onset_detect::MarkerKind::Auto,
        }];
        let pitch_shift_cache = PitchShiftAnalyzer::default()
            .analyze(&samples, owned_audio.sample_rate, &pitch_contour, &markers)
            .unwrap();

        SourceAnalysis {
            source: SampleMetadata {
                reference: SampleReference::new("hash", "Samples/source.wav"),
                filename: "source.wav".to_string(),
                duration_ms: 1,
                sample_rate: 48_000,
                channels: 1,
                rms_db: None,
                peak_db: None,
                waveform_preview: SampleWaveformPreview { points: Vec::new() },
            },
            audio: RuntimeMonoAudioBuffer::from_owned(owned_audio),
            pitch_contour,
            markers,
            pitch_shift_cache,
        }
    }

    fn pitch_frame(frame_index: usize, source_sample_position: usize) -> PitchFrame {
        PitchFrame {
            frame_index,
            source_sample_position,
            timestamp_seconds: source_sample_position as f32 / 48_000.0,
            f0_hz: Some(220.0),
            raw_f0_hz: 220.0,
            confidence: 0.95,
            voiced: true,
            rms: 0.1,
        }
    }

    fn sine_wave(frequency_hz: f32, sample_rate: u32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|index| {
                (std::f32::consts::TAU * frequency_hz * index as f32 / sample_rate as f32).sin()
                    * 0.5
            })
            .collect()
    }

    fn peak_abs(samples: &[f32]) -> f32 {
        samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0, f32::max)
    }
}
