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
    lamath_tube_vizia::{LamathTubeKnob, LamathTubeModelSwitch, LamathTubeSwitchId},
    waveform_points_from_samples,
};

use crate::{
    parameters,
    patch::TubePatch,
    patch_io,
    processor::{ARTICULATION_NAMES, ARTICULATION_SLOT_COUNT, ExcitationSource, TubeProcessor},
};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Lamath Tube", *b"lindelion_lamtub");

const DEFAULT_LIBRARY_DIR: &str = "Ahara";
const WAVEFORM_PREVIEW_POINTS: usize = 384;

#[derive(Debug)]
pub struct LamathTube {
    setup: ProcessSetup,
    patch: TubePatch,
    processor: TubeProcessor<'static>,
    loaded_buffers: [Option<RuntimeMonoAudioBuffer>; ARTICULATION_SLOT_COUNT],
    library_root: PathBuf,
}

impl Default for LamathTube {
    fn default() -> Self {
        let setup = ProcessSetup::default();
        let patch = TubePatch::default();
        Self {
            setup,
            processor: TubeProcessor::new(
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

impl LamathTube {
    pub fn patch(&self) -> &TubePatch {
        &self.patch
    }

    pub fn set_parameter_normalized(&mut self, id: ParameterId, normalized: f32) -> bool {
        let applied = parameters::apply_normalized(&mut self.patch, id, normalized);
        if applied {
            self.processor.set_patch(self.patch.clone());
        }
        applied
    }

    pub fn set_model_switch(&mut self, id: LamathTubeSwitchId, enabled: bool) {
        match id {
            LamathTubeSwitchId::Reed => {}
            LamathTubeSwitchId::Bell => self.patch.switches.bell_enabled = enabled,
            LamathTubeSwitchId::BoreSteepening => {
                self.patch.switches.bore_steepening_enabled = enabled;
            }
            LamathTubeSwitchId::Body => self.patch.switches.body_enabled = enabled,
        }
        self.processor.set_patch(self.patch.clone());
    }

    pub fn select_articulation_slot(&mut self, slot: usize) {
        self.patch.selected_articulation = slot.min(ARTICULATION_SLOT_COUNT - 1);
        self.processor.set_patch(self.patch.clone());
    }

    pub fn load_excitation_from_path(
        &mut self,
        slot: usize,
        path: &Path,
    ) -> Result<(), SampleLibraryError> {
        if slot >= ARTICULATION_SLOT_COUNT {
            return Err(SampleLibraryError::InvalidPath(path.to_path_buf()));
        }
        let mut library = FileSampleLibrary::open(
            lindelion_sample_library::LibraryPaths::from_root(self.library_root.clone()),
        )?;
        let metadata = library.ingest(path.to_path_buf())?;
        let Some(decoded) = library.decode(&metadata.reference)? else {
            return Err(SampleLibraryError::InvalidPath(path.to_path_buf()));
        };
        self.patch.articulations[slot].sample = Some(metadata.reference);
        self.loaded_buffers[slot] = Some(RuntimeMonoAudioBuffer::from_owned(
            OwnedMonoAudioBuffer::from(decoded),
        ));
        self.patch.selected_articulation = slot;
        self.rebuild_processor();
        Ok(())
    }

    pub fn clear_excitation(&mut self, slot: usize) {
        if slot >= ARTICULATION_SLOT_COUNT {
            return;
        }
        self.patch.articulations[slot].sample = None;
        self.loaded_buffers[slot] = None;
        self.patch.selected_articulation = slot;
        self.rebuild_processor();
    }

    pub fn editor_knobs(&self) -> Vec<LamathTubeKnob> {
        parameters::PARAMETERS
            .iter()
            .map(|parameter| LamathTubeKnob {
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

    pub fn model_switches(&self) -> Vec<LamathTubeModelSwitch> {
        vec![
            LamathTubeModelSwitch {
                id: LamathTubeSwitchId::Reed,
                label: "Reed",
                enabled: true,
                editable: false,
            },
            LamathTubeModelSwitch {
                id: LamathTubeSwitchId::Bell,
                label: "Bell",
                enabled: self.patch.switches.bell_enabled,
                editable: true,
            },
            LamathTubeModelSwitch {
                id: LamathTubeSwitchId::BoreSteepening,
                label: "Bore color",
                enabled: self.patch.switches.bore_steepening_enabled,
                editable: true,
            },
            LamathTubeModelSwitch {
                id: LamathTubeSwitchId::Body,
                label: "Body",
                enabled: self.patch.switches.body_enabled,
                editable: true,
            },
        ]
    }

    pub fn articulation_slot_list_view(&self) -> AudioFileSlotListView {
        AudioFileSlotListView {
            selected: AudioFileSlotId(self.patch.selected_articulation),
            slots: (0..ARTICULATION_SLOT_COUNT)
                .map(|index| AudioFileSlotView {
                    label: self.articulation_label(index),
                    source: if self.patch.articulations[index].sample.is_some() {
                        AudioFileSource::Loaded
                    } else {
                        AudioFileSource::BuiltIn
                    },
                    waveform: self.articulation_waveform(index),
                })
                .collect(),
        }
    }

    fn rebuild_processor(&mut self) {
        self.processor = TubeProcessor::new(
            self.setup.sample_rate as f32,
            self.patch.clone(),
            self.runtime_sources(),
        );
    }

    fn runtime_sources(&self) -> [ExcitationSource<'static>; ARTICULATION_SLOT_COUNT] {
        std::array::from_fn(|slot| {
            let Some(buffer) = &self.loaded_buffers[slot] else {
                return ExcitationSource::builtin(slot);
            };
            let samples = unsafe { buffer.samples_with_static_lifetime() };
            ExcitationSource::from_samples(samples, buffer.sample_rate(), slot)
        })
    }

    fn load_patch_from_sample_paths(&mut self, patch: TubePatch) {
        self.patch = patch.sanitized();
        let references = self
            .patch
            .articulations
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.sample.as_ref().map(|reference| (index, reference)));
        let (buffers, _report) =
            load_referenced_mono_audio_from_paths::<ARTICULATION_SLOT_COUNT, _>(references);
        self.loaded_buffers = buffers.map(|slot| slot.map(RuntimeMonoAudioBuffer::from_owned));
        self.rebuild_processor();
    }

    fn articulation_label(&self, index: usize) -> String {
        self.patch
            .articulations
            .get(index)
            .and_then(|slot| slot.sample.as_ref())
            .and_then(|reference| reference.last_known_path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| ARTICULATION_NAMES.get(index).copied().unwrap_or("Slot"))
            .to_string()
    }

    fn articulation_waveform(&self, index: usize) -> Vec<WaveformPoint> {
        self.loaded_buffers
            .get(index)
            .and_then(Option::as_ref)
            .map(|buffer| waveform_points_from_samples(buffer.samples(), WAVEFORM_PREVIEW_POINTS))
            .unwrap_or_default()
    }
}

impl AudioPlugin for LamathTube {
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

fn builtin_sources() -> [ExcitationSource<'static>; ARTICULATION_SLOT_COUNT] {
    std::array::from_fn(ExcitationSource::builtin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_plugin_shell::{AudioBuffer, MidiEvent, NoteEvent, ProcessContext};

    #[test]
    fn state_round_trips_sparse_patch_and_switches() {
        let mut plugin = LamathTube::default();
        plugin.set_parameter_normalized(ParameterId(parameters::DAMPING_ID), 0.8);
        plugin.set_model_switch(LamathTubeSwitchId::Bell, false);

        let state = plugin.state();
        let mut restored = LamathTube::default();
        restored.load_state(state);

        assert!((restored.patch.damping - plugin.patch.damping).abs() < 0.000_001);
        assert!(!restored.patch.switches.bell_enabled);
    }

    #[test]
    fn midi_note_produces_audio_without_allocation() {
        let mut plugin = LamathTube::default();
        let mut left = [0.0; 1024];
        let mut right = [0.0; 1024];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        crate::assert_no_allocations("lamath_tube_plugin_process", || {
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
