use lindelion_plugin_shell::{
    AudioPlugin, ParameterId, ParameterInfo, PluginDescriptor, PluginState, ProcessContext,
    ProcessSetup,
};

use crate::{
    analysis::AnalysisResult,
    analysis_job::{
        AnalysisJob, AnalysisJobResult, AnalysisResultCache, AnalysisSequence, AnalysisStatus,
        RequantizeJob,
    },
    audition::AuditionEngine,
    capture::{CaptureEngine, CaptureEvent},
    parameters::{PARAMETERS, ParameterApplyKind, apply_parameter_plain, parameter_binding},
    patch::{CaptureState, GlirdirPatch},
    patch_io,
};

pub const DESCRIPTOR: PluginDescriptor = PluginDescriptor::effect("Glirdir", *b"glirdir_capture!");

#[derive(Debug)]
pub struct Glirdir {
    setup: ProcessSetup,
    patch: GlirdirPatch,
    capture: CaptureEngine,
    analysis_cache: AnalysisResultCache,
    next_analysis_sequence: AnalysisSequence,
    audition: AuditionEngine,
}

impl Default for Glirdir {
    fn default() -> Self {
        let setup = ProcessSetup::default();
        Self {
            setup,
            patch: GlirdirPatch::default(),
            capture: CaptureEngine::default(),
            analysis_cache: AnalysisResultCache::default(),
            next_analysis_sequence: 0,
            audition: AuditionEngine::new(setup.sample_rate as f32),
        }
    }
}

impl Glirdir {
    pub fn patch(&self) -> &GlirdirPatch {
        &self.patch
    }

    pub fn analysis(&self) -> Option<&AnalysisResult> {
        self.analysis_cache.result()
    }

    pub fn analysis_cache(&self) -> &AnalysisResultCache {
        &self.analysis_cache
    }

    pub fn analysis_status(&self) -> AnalysisStatus {
        self.analysis_cache.status()
    }

    pub fn capture_state(&self) -> CaptureState {
        self.capture.state()
    }

    pub fn arm_capture(&mut self) {
        self.capture.arm();
        let sequence = self.advance_analysis_sequence();
        self.analysis_cache.mark_capturing(sequence);
    }

    pub fn clear_capture(&mut self) {
        self.capture.clear();
        self.patch.scratchpad = None;
        let sequence = self.advance_analysis_sequence();
        self.analysis_cache.mark_idle(sequence);
    }

    pub fn play_audition(&mut self) {
        self.audition.play();
    }

    pub fn stop_audition(&mut self) {
        self.audition.stop();
    }

    pub fn finalize_completed_capture(&mut self) -> Option<AnalysisJob> {
        let scratchpad = self.capture.take_completed_scratchpad()?;
        self.patch.scratchpad = Some(scratchpad);
        self.request_analysis_job()
    }

    pub fn request_analysis_job(&mut self) -> Option<AnalysisJob> {
        let scratchpad = self.patch.scratchpad.as_ref()?.clone();
        self.patch.quantize.sample_rate = scratchpad.sample_rate;
        let sequence = self.advance_analysis_sequence();
        let job = AnalysisJob::new(
            sequence,
            scratchpad,
            self.patch.analysis,
            self.patch.quantize.clone(),
        );
        self.analysis_cache.mark_analyzing(sequence);
        Some(job)
    }

    pub fn publish_analysis_result(&mut self, result: AnalysisJobResult) -> bool {
        let accepted = self.analysis_cache.publish_result(result);
        if accepted
            && self.analysis_cache.status() == AnalysisStatus::Ready
            && self.patch.audition.live_edit
        {
            self.audition.play();
        }
        accepted
    }

    pub fn set_parameter_normalized(&mut self, id: ParameterId, normalized: f32) {
        let apply = self.apply_parameter_normalized(id, normalized);
        self.handle_parameter_apply(apply);
    }

    pub(crate) fn set_parameter_normalized_deferred(
        &mut self,
        id: ParameterId,
        normalized: f32,
    ) -> ParameterApplyKind {
        let apply = self.apply_parameter_normalized(id, normalized);
        self.handle_deferred_parameter_apply(apply);
        apply
    }

    pub(crate) fn request_requantize_job(&mut self) -> Option<RequantizeJob> {
        let result = self.analysis_cache.result()?.clone();
        let sample_rate = self
            .patch
            .scratchpad
            .as_ref()
            .map(|scratchpad| scratchpad.sample_rate)
            .unwrap_or(result.pitch_contour.source_sample_rate);
        let sequence = self.advance_analysis_sequence();
        self.patch.quantize.sample_rate = sample_rate;
        let job = RequantizeJob::new(sequence, result, self.patch.quantize.clone(), sample_rate);
        self.analysis_cache.mark_requantizing(sequence);
        Some(job)
    }

    pub(crate) fn set_patch(&mut self, patch: GlirdirPatch) {
        self.patch = patch;
        self.capture.clear();
        self.audition.set_settings(self.patch.audition);
        self.mark_scratchpad_pending_or_idle();
    }

    fn apply_parameter_normalized(
        &mut self,
        id: ParameterId,
        normalized: f32,
    ) -> ParameterApplyKind {
        let Some(binding) = parameter_binding(id.0) else {
            return ParameterApplyKind::Ignored;
        };
        let plain = binding.info().range.denormalize(normalized);
        apply_parameter_plain(&mut self.patch, id.0, plain)
    }

    fn handle_parameter_apply(&mut self, apply: ParameterApplyKind) {
        match apply {
            ParameterApplyKind::Analysis => self.mark_scratchpad_pending_or_idle(),
            ParameterApplyKind::Quantize => self.requantize(),
            ParameterApplyKind::Audition => self.audition.set_settings(self.patch.audition),
            ParameterApplyKind::Capture | ParameterApplyKind::Ignored => {}
        }
    }

    fn handle_deferred_parameter_apply(&mut self, apply: ParameterApplyKind) {
        match apply {
            ParameterApplyKind::Analysis => self.mark_scratchpad_pending_or_idle(),
            ParameterApplyKind::Quantize => self.refresh_quantize_sample_rate(),
            ParameterApplyKind::Audition => self.audition.set_settings(self.patch.audition),
            ParameterApplyKind::Capture | ParameterApplyKind::Ignored => {}
        }
    }

    fn mark_scratchpad_pending_or_idle(&mut self) {
        let sequence = self.advance_analysis_sequence();
        if let Some(scratchpad) = self.patch.scratchpad.as_ref() {
            self.patch.quantize.sample_rate = scratchpad.sample_rate;
            self.analysis_cache.mark_captured_pending_analysis(sequence);
        } else {
            self.analysis_cache.mark_idle(sequence);
        }
    }

    fn requantize(&mut self) {
        self.refresh_quantize_sample_rate();
        self.analysis_cache.requantize_current(&self.patch.quantize);
    }

    fn refresh_quantize_sample_rate(&mut self) {
        if let Some(scratchpad) = self.patch.scratchpad.as_ref() {
            self.patch.quantize.sample_rate = scratchpad.sample_rate;
        }
    }

    fn advance_analysis_sequence(&mut self) -> AnalysisSequence {
        self.next_analysis_sequence = self.next_analysis_sequence.saturating_add(1);
        self.next_analysis_sequence
    }
}

impl AudioPlugin for Glirdir {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        PARAMETERS
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        self.capture.reset(setup);
        self.audition.reset(setup);
    }

    fn process(&mut self, mut context: ProcessContext<'_>) {
        let setup = context.setup;
        self.setup = setup;
        context.buffer.clear();

        let capture_event =
            self.capture
                .process(context.input, setup, context.transport, self.patch.capture);
        if let CaptureEvent::Completed = capture_event {
            self.analysis_cache
                .mark_captured_pending_analysis(self.analysis_cache.sequence());
        }

        self.audition.set_settings(self.patch.audition);
        self.audition.render(
            self.analysis_cache.result().map(|result| &result.midi_clip),
            setup,
            &mut context.buffer,
        );
    }

    fn state(&self) -> PluginState {
        patch_io::to_plugin_state(&self.patch).unwrap_or_else(|_| PluginState::empty(1))
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
    use crate::analysis::GlirdirAnalyzer;
    use crate::analysis_job::AnalysisStatus;
    use crate::parameters::TIMING_STRENGTH_PARAMETER_ID;
    use lindelion_pitch_detect::{
        PitchContour, PitchDetectionConfig, PitchDetectionError, PitchDetector, PitchFrame,
    };
    use lindelion_plugin_shell::{AudioBuffer, AudioInputBuffer, ProcessMode};
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn plugin_exposes_effect_descriptor_and_shared_parameters() {
        let plugin = Glirdir::default();

        assert_eq!(plugin.descriptor().name, "Glirdir");
        assert_eq!(
            plugin.descriptor().category,
            lindelion_plugin_shell::PluginCategory::Effect
        );
        assert_eq!(plugin.parameters().len(), PARAMETERS.len());
    }

    #[test]
    fn process_uses_shell_audio_input_for_capture() {
        let setup = ProcessSetup {
            sample_rate: 10.0,
            max_block_size: 80,
            mode: ProcessMode::Realtime,
        };
        let mut plugin = Glirdir::default();
        let input = vec![0.25; 80];
        let mut left = vec![0.0; 80];
        let mut right = vec![0.0; 80];

        plugin.reset(setup);
        plugin.arm_capture();
        plugin.process(
            ProcessContext::new(
                setup,
                AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                &[],
            )
            .with_input(AudioInputBuffer::mono(&input)),
        );

        assert_eq!(plugin.capture_state(), CaptureState::Captured);
        assert!(plugin.patch().scratchpad.is_none());
        assert_eq!(
            plugin.analysis_status(),
            AnalysisStatus::CapturedPendingAnalysis
        );
    }

    #[test]
    fn process_capture_completion_does_not_finalize_or_analyze_on_audio_path() {
        let setup = ProcessSetup {
            sample_rate: 10.0,
            max_block_size: 80,
            mode: ProcessMode::Realtime,
        };
        let calls = Rc::new(Cell::new(0));
        let detector = CountingPitchDetector {
            calls: Rc::clone(&calls),
        };
        let analyzer = GlirdirAnalyzer::new(detector);
        let mut plugin = Glirdir::default();
        let input = vec![0.25; 80];
        let mut left = vec![0.0; 80];
        let mut right = vec![0.0; 80];

        plugin.reset(setup);
        plugin.arm_capture();
        crate::assert_no_allocations("glirdir process capture completion", || {
            plugin.process(
                ProcessContext::new(
                    setup,
                    AudioBuffer {
                        left: &mut left,
                        right: &mut right,
                    },
                    &[],
                )
                .with_input(AudioInputBuffer::mono(&input)),
            );
        });

        assert_capture_pending_without_analysis(&plugin, &calls);

        let job = plugin
            .finalize_completed_capture()
            .expect("completed capture should create an analysis job");
        assert_analysis_job_snapshot(&plugin, &calls, &job);

        let result = analyzer.analyze_job(&job);
        assert_eq!(calls.get(), 1);
        assert_ready_after_publish(&mut plugin, result);
    }

    #[test]
    fn quantize_only_parameter_reuses_detected_notes_without_pitch_detection() {
        let calls = Rc::new(Cell::new(0));
        let detector = CountingPitchDetector {
            calls: Rc::clone(&calls),
        };
        let analyzer = GlirdirAnalyzer::new(detector);
        let mut plugin = Glirdir::default();
        let patch = GlirdirPatch {
            scratchpad: Some(crate::patch::ScratchpadAudio::new(48_000, vec![0.2; 4_800])),
            ..GlirdirPatch::default()
        };

        AudioPlugin::load_state(&mut plugin, patch_io::to_plugin_state(&patch).unwrap());

        assert_eq!(
            plugin.analysis_status(),
            AnalysisStatus::CapturedPendingAnalysis
        );
        assert_eq!(calls.get(), 0);
        let job = plugin
            .request_analysis_job()
            .expect("scratchpad should create an analysis job");
        let result = analyzer.analyze_job(&job);
        assert!(plugin.publish_analysis_result(result));
        assert_eq!(calls.get(), 1);
        let detected_notes = plugin
            .analysis()
            .expect("analysis result")
            .detected_notes
            .clone();

        plugin.set_parameter_normalized(ParameterId(TIMING_STRENGTH_PARAMETER_ID), 0.25);

        assert_eq!(calls.get(), 1);
        assert_eq!(
            plugin.analysis().expect("analysis result").detected_notes,
            detected_notes
        );
    }

    #[test]
    fn state_roundtrip_uses_shared_toml_patch_format() {
        let mut plugin = Glirdir::default();
        plugin.patch.name = "Saved".to_string();

        let state = AudioPlugin::state(&plugin);
        let mut restored = Glirdir::default();
        AudioPlugin::load_state(&mut restored, state);

        assert_eq!(restored.patch().name, "Saved");
    }

    #[derive(Debug, Clone)]
    struct CountingPitchDetector {
        calls: Rc<Cell<usize>>,
    }

    impl PitchDetector for CountingPitchDetector {
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
            _config: PitchDetectionConfig,
        ) -> Result<PitchContour, PitchDetectionError> {
            self.calls.set(self.calls.get() + 1);
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

    fn assert_ready_after_publish(plugin: &mut Glirdir, result: AnalysisJobResult) {
        assert!(plugin.publish_analysis_result(result));
        assert_eq!(plugin.analysis_status(), AnalysisStatus::Ready);
        assert!(plugin.analysis().is_some());
    }

    fn assert_capture_pending_without_analysis(plugin: &Glirdir, calls: &Cell<usize>) {
        assert_eq!(plugin.capture_state(), CaptureState::Captured);
        assert!(plugin.patch().scratchpad.is_none());
        assert!(plugin.analysis().is_none());
        assert_eq!(
            plugin.analysis_status(),
            AnalysisStatus::CapturedPendingAnalysis
        );
        assert_eq!(calls.get(), 0);
    }

    fn assert_analysis_job_snapshot(plugin: &Glirdir, calls: &Cell<usize>, job: &AnalysisJob) {
        assert!(plugin.patch().scratchpad.is_some());
        assert_eq!(plugin.analysis_status(), AnalysisStatus::Analyzing);
        assert_eq!(job.sample_rate, 10);
        assert_eq!(job.scratchpad.samples.len(), 80);
        assert_eq!(calls.get(), 0);
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
