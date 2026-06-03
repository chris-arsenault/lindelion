use std::path::{Path, PathBuf};

use lindelion_plugin_shell::{
    AudioPlugin, ParameterId, ParameterInfo, PluginDescriptor, PluginState, ProcessContext,
    ProcessSetup,
};
use lindelion_sample_library::{
    FileSampleLibrary, OwnedMonoAudioBuffer, RuntimeMonoAudioBuffer, SampleLibrary,
    SampleLibraryError, load_referenced_mono_audio_from_paths,
};
use lindelion_ui::{
    WaveformPoint,
    audio_file_slot::{AudioFileSlotView, AudioFileSource},
    lamath_cymbal_vizia::LamathCymbalKnob,
    waveform_points_from_samples,
};

use crate::{
    parameters,
    patch::CymbalPatch,
    patch_io,
    processor::{CymbalProcessor, ExcitationSource},
};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Lamath Cymbal", *b"lindelion_lamcym");

const DEFAULT_LIBRARY_DIR: &str = "Ahara";
const WAVEFORM_PREVIEW_POINTS: usize = 512;

#[derive(Debug)]
pub struct LamathCymbal {
    setup: ProcessSetup,
    patch: CymbalPatch,
    processor: CymbalProcessor<'static>,
    loaded_buffer: Option<RuntimeMonoAudioBuffer>,
    library_root: PathBuf,
}

impl Default for LamathCymbal {
    fn default() -> Self {
        let setup = ProcessSetup::default();
        let patch = CymbalPatch::default();
        Self {
            setup,
            processor: CymbalProcessor::new(
                setup.sample_rate as f32,
                patch.clone(),
                ExcitationSource::builtin(),
            ),
            patch,
            loaded_buffer: None,
            library_root: lindelion_sample_library::music_library_root(DEFAULT_LIBRARY_DIR),
        }
    }
}

impl LamathCymbal {
    pub fn patch(&self) -> &CymbalPatch {
        &self.patch
    }

    pub fn set_parameter_normalized(&mut self, id: ParameterId, normalized: f32) -> bool {
        let applied = parameters::apply_normalized(&mut self.patch, id, normalized);
        if applied {
            self.processor.set_patch(self.patch.clone());
        }
        applied
    }

    pub fn set_patch(&mut self, patch: CymbalPatch) {
        self.patch = patch.sanitized();
        self.rebuild_processor();
    }

    pub fn load_excitation_from_path(&mut self, path: &Path) -> Result<(), SampleLibraryError> {
        let mut library = FileSampleLibrary::open(
            lindelion_sample_library::LibraryPaths::from_root(self.library_root.clone()),
        )?;
        let metadata = library.ingest(path.to_path_buf())?;
        let Some(decoded) = library.decode(&metadata.reference)? else {
            return Err(SampleLibraryError::InvalidPath(path.to_path_buf()));
        };
        self.patch.excitation_sample = Some(metadata.reference);
        self.loaded_buffer = Some(RuntimeMonoAudioBuffer::from_owned(
            OwnedMonoAudioBuffer::from(decoded),
        ));
        self.rebuild_processor();
        Ok(())
    }

    pub fn clear_excitation(&mut self) {
        self.patch.excitation_sample = None;
        self.loaded_buffer = None;
        self.rebuild_processor();
    }

    pub fn editor_knobs(&self) -> Vec<LamathCymbalKnob> {
        parameters::PARAMETERS
            .iter()
            .map(|parameter| LamathCymbalKnob {
                id: parameter.id.0,
                label: parameter.name,
                units: parameter.units,
                normalized: parameters::normalized_value(&self.patch, parameter.id.0)
                    .unwrap_or_else(|| parameter.range.normalize(parameter.range.default)),
                plain: parameters::plain_value(&self.patch, parameter.id.0)
                    .unwrap_or(parameter.range.default),
            })
            .collect()
    }

    pub fn excitation_slot_view(&self) -> AudioFileSlotView {
        AudioFileSlotView {
            label: self.excitation_label(),
            source: if self.patch.excitation_sample.is_some() {
                AudioFileSource::Loaded
            } else {
                AudioFileSource::BuiltIn
            },
            waveform: self.excitation_waveform(),
        }
    }

    fn rebuild_processor(&mut self) {
        self.processor = CymbalProcessor::new(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            self.runtime_excitation(),
        );
    }

    fn runtime_excitation(&self) -> ExcitationSource<'static> {
        let Some(buffer) = &self.loaded_buffer else {
            return ExcitationSource::builtin();
        };
        let samples = unsafe { buffer.samples_with_static_lifetime() };
        ExcitationSource::from_samples(samples, buffer.sample_rate())
    }

    fn load_patch_from_sample_paths(&mut self, patch: CymbalPatch) {
        self.patch = patch.sanitized();
        let references = self
            .patch
            .excitation_sample
            .as_ref()
            .map(|reference| (0usize, reference));
        let Some(reference) = references else {
            self.loaded_buffer = None;
            self.rebuild_processor();
            return;
        };
        let (buffers, _report) = load_referenced_mono_audio_from_paths::<1, _>([reference]);
        self.loaded_buffer = buffers
            .into_iter()
            .next()
            .flatten()
            .map(RuntimeMonoAudioBuffer::from_owned);
        self.rebuild_processor();
    }

    fn excitation_label(&self) -> String {
        self.patch
            .excitation_sample
            .as_ref()
            .and_then(|reference| reference.last_known_path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Built-in strike")
            .to_string()
    }

    fn excitation_waveform(&self) -> Vec<WaveformPoint> {
        self.loaded_buffer
            .as_ref()
            .map(|buffer| waveform_points_from_samples(buffer.samples(), WAVEFORM_PREVIEW_POINTS))
            .unwrap_or_default()
    }
}

impl AudioPlugin for LamathCymbal {
    fn descriptor(&self) -> &'static PluginDescriptor {
        &DESCRIPTOR
    }

    fn parameters(&self) -> &'static [ParameterInfo] {
        parameters::PARAMETERS
    }

    fn reset(&mut self, setup: ProcessSetup) {
        self.setup = setup;
        self.processor.reset(setup.sample_rate as f32);
    }

    fn process(&mut self, context: ProcessContext<'_>) {
        self.setup = context.setup;
        self.processor
            .process(context.events, context.buffer.left, context.buffer.right);
    }

    fn state(&self) -> PluginState {
        patch_io::to_plugin_state(&self.patch)
            .unwrap_or_else(|_| PluginState::empty(patch_io::FORMAT_VERSION))
    }

    fn load_state(&mut self, state: PluginState) {
        if let Ok(patch) = patch_io::from_plugin_state(state) {
            self.load_patch_from_sample_paths(patch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_plugin_shell::{AudioBuffer, MidiEvent, NoteEvent, ProcessContext};

    #[test]
    fn state_round_trips_sparse_patch() {
        let mut plugin = LamathCymbal::default();
        plugin.set_parameter_normalized(ParameterId(parameters::DAMPING_ID), 0.8);

        let state = plugin.state();
        let mut restored = LamathCymbal::default();
        restored.load_state(state);

        assert!((restored.patch.damping - plugin.patch.damping).abs() < 0.000_001);
    }

    #[test]
    fn midi_strike_produces_audio_without_allocation() {
        let mut plugin = LamathCymbal::default();
        let mut left = [0.0; 512];
        let mut right = [0.0; 512];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        crate::assert_no_allocations("lamath_cymbal_plugin_process", || {
            plugin.process(ProcessContext::new(
                ProcessSetup::default(),
                AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                &events,
            ));
        });

        assert!(left.iter().any(|sample| sample.abs() > 0.0));
        assert_eq!(left, right);
    }
}
