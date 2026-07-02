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
            LamathTubeSwitchId::Reed => self.patch.switches.reed_radiation_enabled = enabled,
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
                group: knob_group(parameter.id.0),
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
                label: "Reed noise",
                enabled: self.patch.switches.reed_radiation_enabled,
                editable: true,
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
        // Keyswitches change the slot inside the processor; mirror it back so the next
        // patch push (any parameter edit clones this patch over the processor) and the
        // editor's slot list both see the keyswitch instead of silently reverting it.
        self.patch.selected_articulation = self.processor.selected_slot();
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

/// Stable dense index for each editor model switch, for the lock-free pending
/// queue that carries editor toggles past the audio thread's plugin borrow.
pub(crate) const MODEL_SWITCH_COUNT: usize = 4;

#[cfg_attr(
    not(any(test, target_os = "macos", target_os = "windows")),
    allow(dead_code)
)]
pub(crate) fn model_switch_index(id: LamathTubeSwitchId) -> usize {
    match id {
        LamathTubeSwitchId::Reed => 0,
        LamathTubeSwitchId::Bell => 1,
        LamathTubeSwitchId::BoreSteepening => 2,
        LamathTubeSwitchId::Body => 3,
    }
}

pub(crate) fn model_switch_id(index: usize) -> LamathTubeSwitchId {
    match index {
        0 => LamathTubeSwitchId::Reed,
        1 => LamathTubeSwitchId::Bell,
        2 => LamathTubeSwitchId::BoreSteepening,
        _ => LamathTubeSwitchId::Body,
    }
}

/// Editor card for each host parameter (reed → bore, played by a player).
pub(crate) fn knob_group(id: u32) -> lindelion_ui::lamath_tube_vizia::LamathTubeKnobGroup {
    use lindelion_ui::lamath_tube_vizia::LamathTubeKnobGroup as Group;
    match id {
        parameters::HUMANIZE_ID | parameters::PHRASING_ID | parameters::VIBRATO_ID => Group::Player,
        parameters::BRIGHTNESS_ID | parameters::DAMPING_ID | parameters::BELL_ID => Group::Bore,
        parameters::OUTPUT_GAIN_ID => Group::Output,
        _ => Group::Reed,
    }
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
    fn every_model_switch_is_editable_and_reaches_the_patch() {
        let mut plugin = LamathTube::default();
        for switch in plugin.model_switches() {
            assert!(switch.editable, "{} switch must be editable", switch.label);
        }

        plugin.set_model_switch(LamathTubeSwitchId::Reed, false);
        assert!(!plugin.patch.switches.reed_radiation_enabled);
        let reed = &plugin.model_switches()[model_switch_index(LamathTubeSwitchId::Reed)];
        assert!(!reed.enabled);

        for index in 0..MODEL_SWITCH_COUNT {
            assert_eq!(model_switch_index(model_switch_id(index)), index);
        }
    }

    #[test]
    fn editor_slot_pick_changes_the_attack() {
        let attack_db = |plugin: &mut LamathTube, note: u8| {
            let mut left = vec![0.0; 4_800];
            let mut right = vec![0.0; 4_800];
            let events = [MidiEvent::Note(NoteEvent::On {
                channel: 0,
                note,
                velocity: 0.9,
            })];
            plugin.process(ProcessContext::new(
                ProcessSetup::default(),
                AudioBuffer {
                    left: &mut left,
                    right: &mut right,
                },
                &events,
            ));
            let window = &left[..2_400];
            let rms = (window.iter().map(|x| x * x).sum::<f32>() / window.len() as f32).sqrt();
            20.0 * rms.max(1.0e-9).log10()
        };
        let render = |slot: usize| {
            let mut plugin = LamathTube::default();
            plugin.select_articulation_slot(slot);
            attack_db(&mut plugin, 74)
        };
        let tongue = render(0);
        let legato = render(2);
        assert!(
            legato < tongue - 6.0,
            "editor slot pick must reach the voice: tongue {tongue:.1} legato {legato:.1} dB"
        );
    }

    #[test]
    fn keyswitch_articulation_survives_parameter_changes() {
        let mut plugin = LamathTube::default();
        let mut left = [0.0; 64];
        let mut right = [0.0; 64];
        // Keyswitch to the Legato slot (note 2 in the keyswitch octave).
        let keyswitch = [MidiEvent::Note(NoteEvent::On {
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
            &keyswitch,
        ));
        assert_eq!(plugin.patch.selected_articulation, 2);

        // A host parameter edit clones the shell patch over the processor; the keyswitch
        // slot must survive it.
        plugin.set_parameter_normalized(ParameterId(parameters::BRIGHTNESS_ID), 0.7);

        let mut note_left = vec![0.0; 4_800];
        let mut note_right = vec![0.0; 4_800];
        let note = [MidiEvent::Note(NoteEvent::On {
            channel: 0,
            note: 74,
            velocity: 0.9,
        })];
        plugin.process(ProcessContext::new(
            ProcessSetup::default(),
            AudioBuffer {
                left: &mut note_left,
                right: &mut note_right,
            },
            &note,
        ));
        let window = &note_left[..2_400];
        let rms = (window.iter().map(|x| x * x).sum::<f32>() / window.len() as f32).sqrt();
        let attack = 20.0 * rms.max(1.0e-9).log10();
        assert!(
            attack < -45.0,
            "the legato keyswitch should still shape the next attack: {attack:.1} dB"
        );
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
