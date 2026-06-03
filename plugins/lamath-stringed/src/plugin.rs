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
    lamath_stringed_vizia::{
        LamathStringedBodyId, LamathStringedDriverId, LamathStringedKnob,
        LamathStringedModelSwitch, LamathStringedSwitchId,
    },
    waveform_points_from_samples,
};

use crate::{
    parameters,
    patch::{BodySelection, DriverSelection, StringPatch},
    patch_io,
    processor::{ARTICULATION_NAMES, ARTICULATION_SLOT_COUNT, ExcitationSource, StringProcessor},
};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Lamath Stringed", *b"lindelion_lamstr");

const DEFAULT_LIBRARY_DIR: &str = "Ahara";
const WAVEFORM_PREVIEW_POINTS: usize = 384;

#[derive(Debug)]
pub struct LamathStringed {
    setup: ProcessSetup,
    patch: StringPatch,
    processor: StringProcessor<'static>,
    loaded_buffers: [Option<RuntimeMonoAudioBuffer>; ARTICULATION_SLOT_COUNT],
    library_root: PathBuf,
}

impl Default for LamathStringed {
    fn default() -> Self {
        let setup = ProcessSetup::default();
        let patch = StringPatch::default();
        Self {
            setup,
            processor: StringProcessor::new(
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

impl LamathStringed {
    pub fn patch(&self) -> &StringPatch {
        &self.patch
    }

    pub fn set_parameter_normalized(&mut self, id: ParameterId, normalized: f32) -> bool {
        let applied = parameters::apply_normalized(&mut self.patch, id, normalized);
        if applied {
            self.processor.set_patch(self.patch.clone());
        }
        applied
    }

    pub fn set_driver(&mut self, driver: LamathStringedDriverId) {
        self.patch.driver = match driver {
            LamathStringedDriverId::None => DriverSelection::None,
            LamathStringedDriverId::Pick => DriverSelection::Pick,
            LamathStringedDriverId::Bow => DriverSelection::Bow,
        };
        self.processor.set_patch(self.patch.clone());
    }

    pub fn set_body(&mut self, body: LamathStringedBodyId) {
        self.patch.body = match body {
            LamathStringedBodyId::Disabled => BodySelection::Disabled,
            LamathStringedBodyId::Guitar => BodySelection::Guitar,
            LamathStringedBodyId::Violin => BodySelection::Violin,
        };
        self.processor.set_patch(self.patch.clone());
    }

    pub fn set_model_switch(&mut self, id: LamathStringedSwitchId, enabled: bool) {
        match id {
            LamathStringedSwitchId::BodyContact => self.patch.switches.body_contact = enabled,
            LamathStringedSwitchId::BowDrive => self.patch.switches.bow_drive = enabled,
            LamathStringedSwitchId::Tension => self.patch.switches.tension = enabled,
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

    pub fn editor_knobs(&self) -> Vec<LamathStringedKnob> {
        parameters::PARAMETERS
            .iter()
            .map(|parameter| LamathStringedKnob {
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

    pub fn selected_driver(&self) -> LamathStringedDriverId {
        match self.patch.driver {
            DriverSelection::None => LamathStringedDriverId::None,
            DriverSelection::Pick => LamathStringedDriverId::Pick,
            DriverSelection::Bow => LamathStringedDriverId::Bow,
        }
    }

    pub fn selected_body(&self) -> LamathStringedBodyId {
        match self.patch.body {
            BodySelection::Disabled => LamathStringedBodyId::Disabled,
            BodySelection::Guitar => LamathStringedBodyId::Guitar,
            BodySelection::Violin => LamathStringedBodyId::Violin,
        }
    }

    pub fn model_switches(&self) -> Vec<LamathStringedModelSwitch> {
        vec![
            LamathStringedModelSwitch {
                id: LamathStringedSwitchId::BodyContact,
                label: "Body contact",
                enabled: self.patch.switches.body_contact,
                editable: true,
            },
            LamathStringedModelSwitch {
                id: LamathStringedSwitchId::BowDrive,
                label: "Bow drive",
                enabled: self.patch.switches.bow_drive,
                editable: true,
            },
            LamathStringedModelSwitch {
                id: LamathStringedSwitchId::Tension,
                label: "Tension",
                enabled: self.patch.switches.tension,
                editable: true,
            },
        ]
    }

    pub fn articulation_slot_list_view(&self) -> AudioFileSlotListView {
        AudioFileSlotListView {
            selected: AudioFileSlotId(self.patch.selected_articulation),
            slots: (0..ARTICULATION_SLOT_COUNT)
                .map(|slot| AudioFileSlotView {
                    label: self.articulation_label(slot),
                    source: if self.patch.articulations[slot].sample.is_some() {
                        AudioFileSource::Loaded
                    } else {
                        AudioFileSource::BuiltIn
                    },
                    waveform: self.articulation_waveform(slot),
                })
                .collect(),
        }
    }

    fn rebuild_processor(&mut self) {
        self.processor = StringProcessor::new(
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

    fn load_patch_from_sample_paths(&mut self, patch: StringPatch) {
        self.patch = patch.sanitized();
        let references = self
            .patch
            .articulations
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| entry.sample.as_ref().map(|reference| (slot, reference)));
        let (buffers, _report) =
            load_referenced_mono_audio_from_paths::<ARTICULATION_SLOT_COUNT, _>(references);
        self.loaded_buffers = buffers.map(|slot| slot.map(RuntimeMonoAudioBuffer::from_owned));
        self.rebuild_processor();
    }

    fn articulation_label(&self, slot: usize) -> String {
        self.patch
            .articulations
            .get(slot)
            .and_then(|entry| entry.sample.as_ref())
            .and_then(|reference| reference.last_known_path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| ARTICULATION_NAMES.get(slot).copied().unwrap_or("Slot"))
            .to_string()
    }

    fn articulation_waveform(&self, slot: usize) -> Vec<WaveformPoint> {
        self.loaded_buffers
            .get(slot)
            .and_then(Option::as_ref)
            .map(|buffer| waveform_points_from_samples(buffer.samples(), WAVEFORM_PREVIEW_POINTS))
            .unwrap_or_default()
    }
}

impl AudioPlugin for LamathStringed {
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
    fn state_round_trips_sparse_patch_selectors_and_switches() {
        let mut plugin = LamathStringed::default();
        plugin.set_parameter_normalized(ParameterId(parameters::DAMPING_ID), 0.8);
        plugin.set_driver(LamathStringedDriverId::Bow);
        plugin.set_body(LamathStringedBodyId::Violin);
        plugin.set_model_switch(LamathStringedSwitchId::BodyContact, false);

        let state = plugin.state();
        let mut restored = LamathStringed::default();
        restored.load_state(state);

        assert!((restored.patch.damping - plugin.patch.damping).abs() < 0.000_001);
        assert_eq!(restored.patch.driver, DriverSelection::Bow);
        assert_eq!(restored.patch.body, BodySelection::Violin);
        assert!(!restored.patch.switches.body_contact);
    }

    #[test]
    fn midi_note_produces_audio_without_allocation() {
        let mut plugin = LamathStringed::default();
        let mut left = [0.0; 1024];
        let mut right = [0.0; 1024];
        let events = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 60,
            velocity: 1.0,
        })];

        crate::assert_no_allocations("lamath_stringed_plugin_process", || {
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
