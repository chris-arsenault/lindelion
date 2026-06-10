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
    audio_file_slot::{AudioFileSlotId, AudioFileSlotListView, AudioFileSlotView, AudioFileSource},
    lamath_cymbal_vizia::{LamathCymbalKnob, LamathCymbalPreset},
    waveform_points_from_samples,
};

use crate::{
    parameters,
    patch::CymbalPatch,
    patch_io,
    presets::{self, CYMBAL_PRESETS},
    processor::{CymbalProcessor, ExcitationSource, STRIKER_NAMES, STRIKER_SLOT_COUNT},
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
    loaded_buffers: [Option<RuntimeMonoAudioBuffer>; STRIKER_SLOT_COUNT],
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
                builtin_sources(),
            ),
            patch,
            loaded_buffers: std::array::from_fn(|_| None),
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

    pub fn select_striker_slot(&mut self, slot: usize) {
        self.patch.selected_striker = slot.min(STRIKER_SLOT_COUNT - 1);
        self.processor.set_patch(self.patch.clone());
    }

    pub fn load_excitation_from_path(
        &mut self,
        slot: usize,
        path: &Path,
    ) -> Result<(), SampleLibraryError> {
        if slot >= STRIKER_SLOT_COUNT {
            return Err(SampleLibraryError::InvalidPath(path.to_path_buf()));
        }
        let mut library = FileSampleLibrary::open(
            lindelion_sample_library::LibraryPaths::from_root(self.library_root.clone()),
        )?;
        let metadata = library.ingest(path.to_path_buf())?;
        let Some(decoded) = library.decode(&metadata.reference)? else {
            return Err(SampleLibraryError::InvalidPath(path.to_path_buf()));
        };
        self.patch.strikers[slot].sample = Some(metadata.reference);
        self.loaded_buffers[slot] = Some(RuntimeMonoAudioBuffer::from_owned(
            OwnedMonoAudioBuffer::from(decoded),
        ));
        self.patch.selected_striker = slot;
        self.rebuild_processor();
        Ok(())
    }

    pub fn clear_excitation(&mut self, slot: usize) {
        if slot >= STRIKER_SLOT_COUNT {
            return;
        }
        self.patch.strikers[slot].sample = None;
        self.loaded_buffers[slot] = None;
        self.patch.selected_striker = slot;
        self.rebuild_processor();
    }

    pub fn presets(&self) -> Vec<LamathCymbalPreset> {
        CYMBAL_PRESETS
            .iter()
            .map(|preset| LamathCymbalPreset {
                name: preset.name,
                description: preset.description,
            })
            .collect()
    }

    /// Apply the preset at `index` to the tonal parameters, preserving the striker slots, loaded
    /// samples, and selected striker. No-op for an out-of-range index.
    pub fn apply_preset(&mut self, index: usize) {
        let Some(preset) = CYMBAL_PRESETS.get(index) else {
            return;
        };
        preset.apply_to(&mut self.patch);
        self.patch = self.patch.clone().sanitized();
        self.rebuild_processor();
    }

    pub fn active_preset(&self) -> Option<usize> {
        presets::active_preset_index(&self.patch)
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

    pub fn striker_slot_list_view(&self) -> AudioFileSlotListView {
        AudioFileSlotListView {
            selected: AudioFileSlotId(self.patch.selected_striker),
            slots: (0..STRIKER_SLOT_COUNT)
                .map(|index| AudioFileSlotView {
                    label: self.striker_label(index),
                    source: if self.patch.strikers[index].sample.is_some() {
                        AudioFileSource::Loaded
                    } else {
                        AudioFileSource::BuiltIn
                    },
                    waveform: self.striker_waveform(index),
                })
                .collect(),
        }
    }

    fn rebuild_processor(&mut self) {
        self.processor = CymbalProcessor::new(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            self.runtime_sources(),
        );
    }

    fn runtime_sources(&self) -> [ExcitationSource<'static>; STRIKER_SLOT_COUNT] {
        std::array::from_fn(|slot| {
            let Some(buffer) = &self.loaded_buffers[slot] else {
                return ExcitationSource::builtin(slot);
            };
            let samples = unsafe { buffer.samples_with_static_lifetime() };
            ExcitationSource::from_samples(samples, buffer.sample_rate(), slot)
        })
    }

    fn load_patch_from_sample_paths(&mut self, patch: CymbalPatch) {
        self.patch = patch.sanitized();
        let references = self
            .patch
            .strikers
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.sample.as_ref().map(|reference| (index, reference)));
        let (buffers, _report) =
            load_referenced_mono_audio_from_paths::<STRIKER_SLOT_COUNT, _>(references);
        self.loaded_buffers = buffers.map(|slot| slot.map(RuntimeMonoAudioBuffer::from_owned));
        self.rebuild_processor();
    }

    fn striker_label(&self, index: usize) -> String {
        self.patch
            .strikers
            .get(index)
            .and_then(|slot| slot.sample.as_ref())
            .and_then(|reference| reference.last_known_path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| STRIKER_NAMES.get(index).copied().unwrap_or("Striker"))
            .to_string()
    }

    fn striker_waveform(&self, index: usize) -> Vec<WaveformPoint> {
        self.loaded_buffers
            .get(index)
            .and_then(Option::as_ref)
            .map(|buffer| waveform_points_from_samples(buffer.samples(), WAVEFORM_PREVIEW_POINTS))
            .unwrap_or_default()
    }
}

fn builtin_sources() -> [ExcitationSource<'static>; STRIKER_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
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
        self.patch.selected_striker = self.processor.selected_slot();
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
    fn exposes_four_builtin_striker_slots() {
        let plugin = LamathCymbal::default();
        let view = plugin.striker_slot_list_view();

        assert_eq!(view.selected, AudioFileSlotId(0));
        assert_eq!(view.slots.len(), STRIKER_SLOT_COUNT);
        assert_eq!(view.slots[0].label, "Hard stick");
        assert_eq!(view.slots[1].label, "Soft mallet");
        assert_eq!(view.slots[2].label, "Jazz brush");
        assert_eq!(view.slots[3].label, "Bell stick");
        assert!(
            view.slots
                .iter()
                .all(|slot| slot.source == AudioFileSource::BuiltIn)
        );
    }

    #[test]
    fn midi_keyswitch_updates_exposed_selected_striker() {
        let mut plugin = LamathCymbal::default();
        let mut left = [0.0; 128];
        let mut right = [0.0; 128];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 2,
            velocity: 1.0,
        })];

        plugin.process(ProcessContext::new(
            ProcessSetup::default(),
            AudioBuffer {
                left: &mut left,
                right: &mut right,
            },
            &events,
        ));

        assert_eq!(plugin.striker_slot_list_view().selected, AudioFileSlotId(2));
        assert!(left.iter().all(|sample| sample.abs() == 0.0));
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
