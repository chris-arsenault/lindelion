#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::Path;
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
use std::sync::RwLock;
use std::{
    cell::{Cell, RefCell},
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
};

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
use lindelion_plugin_shell::vst3::restart_vst3_parameter_values_changed;
use lindelion_plugin_shell::{
    AudioPlugin, MidiEvent, MidiEventNormalizer, ParameterId,
    ProcessContext as ShellProcessContext, ProcessSetup as ShellProcessSetup,
    vst3::{
        Vst3BusInfo, Vst3ParameterMirror, can_process_32_bit_sample_size, clear_vst_outputs,
        fill_vst3_bus_info, for_each_vst3_parameter_change, process_setup_from_vst,
        read_plugin_state_from_stream, stereo_output_buffers_from_vst_process_data,
        vst_event_to_midi, vst3_bus_count, write_plugin_state_to_stream,
    },
};
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
use lindelion_ui::lamath_tube_vizia::LamathTubeSwitchId;
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{LamathTube, parameters, plugin};

use super::MAX_BLOCK_EVENTS;

const TUBE_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_output(2, "Output"),
    Vst3BusInfo::event_input(1, "MIDI Input"),
];

/// Retry budget for editor file edits (load/clear excitation): the audio
/// thread holds the plugin borrow only while rendering one block, so a short
/// UI-thread retry window makes these rare, filesystem-touching edits
/// effectively lossless without queueing their work onto the audio thread.
#[cfg(any(target_os = "macos", target_os = "windows"))]
const EDITOR_EDIT_BORROW_RETRIES: usize = 400;
#[cfg(any(target_os = "macos", target_os = "windows"))]
const EDITOR_EDIT_RETRY_SLEEP_MICROS: u64 = 500;

pub(crate) struct LamathTubeVst3Processor {
    plugin: RefCell<LamathTube>,
    setup: Cell<ShellProcessSetup>,
    pub(super) values: Vst3ParameterMirror<{ parameters::PARAMETER_COUNT }>,
    pending_values: [AtomicU32; parameters::PARAMETER_COUNT],
    pending_dirty: [AtomicBool; parameters::PARAMETER_COUNT],
    /// Editor model-switch toggles and articulation-slot picks queued past a
    /// busy audio-thread borrow, drained at the head of the next process
    /// block; without this, a click landing mid-block was silently dropped.
    pending_switch_values: [AtomicBool; plugin::MODEL_SWITCH_COUNT],
    pending_switch_dirty: [AtomicBool; plugin::MODEL_SWITCH_COUNT],
    pending_articulation_slot: AtomicUsize,
    pending_articulation_dirty: AtomicBool,
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    editor_switches: RwLock<Vec<lindelion_ui::lamath_tube_vizia::LamathTubeModelSwitch>>,
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    editor_slots: RwLock<lindelion_ui::audio_file_slot::AudioFileSlotListView>,
    pub(super) handler: Cell<*mut IComponentHandler>,
}

impl Class for LamathTubeVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl LamathTubeVst3Processor {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.processor_cid[0],
        crate::VST3_BUNDLE_METADATA.processor_cid[1],
        crate::VST3_BUNDLE_METADATA.processor_cid[2],
        crate::VST3_BUNDLE_METADATA.processor_cid[3],
    );

    pub(super) fn new() -> Self {
        let setup = ShellProcessSetup::default();
        let mut plugin = LamathTube::default();
        plugin.reset(setup);
        let default_values = parameters::default_normalized_values();
        #[cfg(any(test, target_os = "macos", target_os = "windows"))]
        let editor_switches = plugin.model_switches();
        #[cfg(any(test, target_os = "macos", target_os = "windows"))]
        let editor_slots = plugin.articulation_slot_list_view();
        Self {
            values: Vst3ParameterMirror::new(default_values),
            pending_values: std::array::from_fn(|index| {
                AtomicU32::new((default_values[index] as f32).to_bits())
            }),
            pending_dirty: std::array::from_fn(|_| AtomicBool::new(false)),
            pending_switch_values: std::array::from_fn(|_| AtomicBool::new(false)),
            pending_switch_dirty: std::array::from_fn(|_| AtomicBool::new(false)),
            pending_articulation_slot: AtomicUsize::new(0),
            pending_articulation_dirty: AtomicBool::new(false),
            #[cfg(any(test, target_os = "macos", target_os = "windows"))]
            editor_switches: RwLock::new(editor_switches),
            #[cfg(any(test, target_os = "macos", target_os = "windows"))]
            editor_slots: RwLock::new(editor_slots),
            plugin: RefCell::new(plugin),
            setup: Cell::new(setup),
            handler: Cell::new(ptr::null_mut()),
        }
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(super) fn editor_knobs(&self) -> Vec<lindelion_ui::lamath_tube_vizia::LamathTubeKnob> {
        self.knobs_from_values()
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(super) fn model_switches(
        &self,
    ) -> Vec<lindelion_ui::lamath_tube_vizia::LamathTubeModelSwitch> {
        if let Ok(plugin) = self.plugin.try_borrow() {
            let switches = plugin.model_switches();
            self.replace_editor_switches(switches.clone());
            return switches;
        }
        self.cached_editor_switches()
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(super) fn articulation_slot_list_view(
        &self,
    ) -> lindelion_ui::audio_file_slot::AudioFileSlotListView {
        if let Ok(plugin) = self.plugin.try_borrow() {
            let slots = plugin.articulation_slot_list_view();
            self.replace_editor_slots(slots.clone());
            return slots;
        }
        self.cached_editor_slots()
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(super) fn set_editor_parameter(&self, id: u32, normalized: f32) {
        let _ = self.set_value(id, f64::from(normalized));
        if let Some(handler) = unsafe { ComRef::from_raw(self.handler.get()) } {
            unsafe {
                handler.beginEdit(id);
                handler.performEdit(id, f64::from(normalized.clamp(0.0, 1.0)));
                handler.endEdit(id);
            }
        }
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(super) fn set_model_switch(&self, id: LamathTubeSwitchId, enabled: bool) {
        if let Ok(mut plugin) = self.plugin.try_borrow_mut() {
            // Older queued toggles apply first so this click stays the newest.
            self.apply_pending_model_switches(&mut plugin);
            plugin.set_model_switch(id, enabled);
            self.replace_editor_switches(plugin.model_switches());
        } else {
            let index = plugin::model_switch_index(id);
            self.pending_switch_values[index].store(enabled, Ordering::Release);
            self.pending_switch_dirty[index].store(true, Ordering::Release);
            let mut switches = self.cached_editor_switches();
            if let Some(switch) = switches.iter_mut().find(|switch| switch.id == id) {
                switch.enabled = enabled;
            }
            self.replace_editor_switches(switches);
        }
        unsafe { restart_vst3_parameter_values_changed(self.handler.get()) };
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    pub(super) fn select_articulation_slot(&self, slot: usize) {
        if let Ok(mut plugin) = self.plugin.try_borrow_mut() {
            // A stale queued pick must not override this newer one in process.
            self.pending_articulation_dirty
                .store(false, Ordering::Release);
            plugin.select_articulation_slot(slot);
            self.replace_editor_slots(plugin.articulation_slot_list_view());
        } else {
            let slot = slot.min(crate::processor::ARTICULATION_SLOT_COUNT - 1);
            self.pending_articulation_slot
                .store(slot, Ordering::Release);
            self.pending_articulation_dirty
                .store(true, Ordering::Release);
            let mut slots = self.cached_editor_slots();
            slots.selected = lindelion_ui::audio_file_slot::AudioFileSlotId(slot);
            self.replace_editor_slots(slots);
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn borrow_plugin_for_editor_edit(&self) -> Option<std::cell::RefMut<'_, LamathTube>> {
        for _ in 0..EDITOR_EDIT_BORROW_RETRIES {
            if let Ok(plugin) = self.plugin.try_borrow_mut() {
                return Some(plugin);
            }
            std::thread::sleep(std::time::Duration::from_micros(
                EDITOR_EDIT_RETRY_SLEEP_MICROS,
            ));
        }
        None
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn load_excitation_from_path(&self, slot: usize, path: &Path) {
        let Some(mut plugin) = self.borrow_plugin_for_editor_edit() else {
            return;
        };
        let _ = plugin.load_excitation_from_path(slot, path);
        self.values
            .replace(parameters::normalized_values_from_patch(plugin.patch()));
        self.replace_editor_slots(plugin.articulation_slot_list_view());
        unsafe { restart_vst3_parameter_values_changed(self.handler.get()) };
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn clear_excitation(&self, slot: usize) {
        let Some(mut plugin) = self.borrow_plugin_for_editor_edit() else {
            return;
        };
        plugin.clear_excitation(slot);
        self.replace_editor_slots(plugin.articulation_slot_list_view());
        unsafe { restart_vst3_parameter_values_changed(self.handler.get()) };
    }

    fn process_events(&self, input_events: *mut IEventList, events: &mut [MidiEvent]) -> usize {
        let Some(input_events) = (unsafe { ComRef::from_raw(input_events) }) else {
            return 0;
        };
        let normalizer = MidiEventNormalizer::new(&[], 2.0);
        let mut used = 0;
        for index in 0..(unsafe { input_events.getEventCount() }.max(0) as usize).min(events.len())
        {
            let mut event = MaybeUninit::<Event>::uninit();
            let result = unsafe { input_events.getEvent(index as i32, event.as_mut_ptr()) };
            if result == kResultOk
                && let Some(midi_event) =
                    unsafe { vst_event_to_midi(event.assume_init(), normalizer) }
            {
                events[used] = midi_event;
                used += 1;
            }
        }
        used
    }

    fn apply_parameter_changes(&self, changes: *mut IParameterChanges) {
        unsafe {
            for_each_vst3_parameter_change(changes, |change| {
                let _ = self.set_value(change.id, change.normalized_value);
            });
        }
    }

    pub(super) fn set_value(&self, id: u32, normalized: f64) -> tresult {
        let Some(index) = parameters::parameter_index(id) else {
            return kInvalidArgument;
        };
        let defaults = parameters::default_normalized_values();
        let Some(value) = self
            .values
            .set_normalized(index, normalized, defaults[index])
        else {
            return kInvalidArgument;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            self.queue_pending_value(index, value as f32);
            return kResultOk;
        };
        if plugin.set_parameter_normalized(ParameterId(id), value as f32) {
            self.clear_pending_value(index, value as f32);
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    fn update_mirror_from_plugin(&self) {
        if let Ok(plugin) = self.plugin.try_borrow() {
            self.values
                .replace(parameters::normalized_values_from_patch(plugin.patch()));
            #[cfg(any(test, target_os = "macos", target_os = "windows"))]
            self.replace_editor_switches(plugin.model_switches());
            #[cfg(any(test, target_os = "macos", target_os = "windows"))]
            self.replace_editor_slots(plugin.articulation_slot_list_view());
        }
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    fn knobs_from_values(&self) -> Vec<lindelion_ui::lamath_tube_vizia::LamathTubeKnob> {
        let values = self.values.values();
        parameters::PARAMETERS
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let normalized = values[index] as f32;
                lindelion_ui::lamath_tube_vizia::LamathTubeKnob {
                    id: parameter.id.0,
                    label: parameter.name,
                    units: parameter.units,
                    group: crate::plugin::knob_group(parameter.id.0),
                    normalized,
                    plain: parameter.range.denormalize(normalized),
                }
            })
            .collect()
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    fn cached_editor_switches(
        &self,
    ) -> Vec<lindelion_ui::lamath_tube_vizia::LamathTubeModelSwitch> {
        self.editor_switches
            .read()
            .map(|switches| switches.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    fn replace_editor_switches(
        &self,
        switches: Vec<lindelion_ui::lamath_tube_vizia::LamathTubeModelSwitch>,
    ) {
        match self.editor_switches.write() {
            Ok(mut cached) => *cached = switches,
            Err(poisoned) => *poisoned.into_inner() = switches,
        }
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    fn cached_editor_slots(&self) -> lindelion_ui::audio_file_slot::AudioFileSlotListView {
        self.editor_slots
            .read()
            .map(|slots| slots.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    fn replace_editor_slots(&self, slots: lindelion_ui::audio_file_slot::AudioFileSlotListView) {
        match self.editor_slots.write() {
            Ok(mut cached) => *cached = slots,
            Err(poisoned) => *poisoned.into_inner() = slots,
        }
    }

    fn queue_pending_value(&self, index: usize, normalized: f32) {
        self.pending_values[index].store(normalized.to_bits(), Ordering::Release);
        self.pending_dirty[index].store(true, Ordering::Release);
    }

    fn clear_pending_value(&self, index: usize, normalized: f32) {
        self.pending_values[index].store(normalized.to_bits(), Ordering::Release);
        self.pending_dirty[index].store(false, Ordering::Release);
    }

    fn apply_pending_values(&self, plugin: &mut LamathTube) {
        for (index, parameter) in parameters::PARAMETERS.iter().enumerate() {
            if self.pending_dirty[index].swap(false, Ordering::AcqRel) {
                let normalized = f32::from_bits(self.pending_values[index].load(Ordering::Acquire));
                let _ = plugin.set_parameter_normalized(parameter.id, normalized);
            }
        }
    }

    fn apply_pending_model_switches(&self, plugin: &mut LamathTube) {
        for index in 0..crate::plugin::MODEL_SWITCH_COUNT {
            if self.pending_switch_dirty[index].swap(false, Ordering::AcqRel) {
                let enabled = self.pending_switch_values[index].load(Ordering::Acquire);
                plugin.set_model_switch(crate::plugin::model_switch_id(index), enabled);
            }
        }
    }

    fn apply_pending_articulation_slot(&self, plugin: &mut LamathTube) {
        if self
            .pending_articulation_dirty
            .swap(false, Ordering::AcqRel)
        {
            plugin.select_articulation_slot(self.pending_articulation_slot.load(Ordering::Acquire));
        }
    }
}

impl IPluginBaseTrait for LamathTubeVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for LamathTubeVst3Processor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        vst3_bus_count(&TUBE_BUSES, media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        fill_vst3_bus_info(&TUBE_BUSES, media_type, dir, index, bus)
    }

    unsafe fn getRoutingInfo(
        &self,
        _in_info: *mut RoutingInfo,
        _out_info: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        _media_type: MediaType,
        _dir: BusDirection,
        _index: i32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultFalse;
        };
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.load_state(plugin_state);
        drop(plugin);
        self.update_mirror_from_plugin();
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Ok(plugin) = self.plugin.try_borrow() else {
            return kResultFalse;
        };
        if write_plugin_state_to_stream(state, plugin.state()) {
            kResultOk
        } else {
            kResultFalse
        }
    }
}

impl IAudioProcessorTrait for LamathTubeVst3Processor {
    unsafe fn setBusArrangements(
        &self,
        _inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins == 0 && num_outs == 1 && !outputs.is_null() && *outputs == SpeakerArr::kStereo {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arrangement: *mut SpeakerArrangement,
    ) -> tresult {
        if arrangement.is_null() || index != 0 {
            return kInvalidArgument;
        }
        match dir as BusDirections {
            BusDirections_::kOutput => {
                *arrangement = SpeakerArr::kStereo;
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        can_process_32_bit_sample_size(symbolic_sample_size)
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        if setup.is_null() {
            return kInvalidArgument;
        }
        let shell_setup = process_setup_from_vst(&*setup);
        self.setup.set(shell_setup);
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return kResultFalse;
        };
        plugin.reset(shell_setup);
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        let data = &mut *data;
        self.apply_parameter_changes(data.inputParameterChanges);
        if data.symbolicSampleSize as SymbolicSampleSizes != SymbolicSampleSizes_::kSample32 {
            clear_vst_outputs(data);
            return kResultOk;
        }
        let data_ptr = data as *mut ProcessData;
        let Some(buffer) = stereo_output_buffers_from_vst_process_data(&mut *data_ptr) else {
            clear_vst_outputs(&mut *data_ptr);
            return kResultOk;
        };
        let mut events = [empty_midi_event(); MAX_BLOCK_EVENTS];
        let event_count = self.process_events((*data_ptr).inputEvents, &mut events);
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            let mut buffer = buffer;
            buffer.clear();
            return kResultFalse;
        };
        self.apply_pending_values(&mut plugin);
        self.apply_pending_model_switches(&mut plugin);
        self.apply_pending_articulation_slot(&mut plugin);
        plugin.process(ShellProcessContext::new(
            self.setup.get(),
            buffer,
            &events[..event_count],
        ));
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for LamathTubeVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

fn empty_midi_event() -> MidiEvent {
    MidiEvent::Note(lindelion_plugin_shell::NoteEvent::Off {
        channel: 0,
        note: 0,
        velocity: 0.0,
    })
}

#[cfg(test)]
mod tests;
