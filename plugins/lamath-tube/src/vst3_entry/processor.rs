#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::Path;
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
use std::sync::RwLock;
use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_plugin_shell::vst3::restart_vst3_parameter_values_changed;
use lindelion_plugin_shell::{
    AudioPlugin, MidiEvent, MidiEventNormalizer, ParameterId,
    ProcessContext as ShellProcessContext, ProcessSetup as ShellProcessSetup,
    vst3::{
        Vst3BusInfo, Vst3ParameterInfo, Vst3ParameterMirror, can_process_32_bit_sample_size,
        clear_vst_outputs, fill_vst3_bus_info, fill_vst3_parameter_info,
        for_each_vst3_parameter_change, parse_vst3_plain_value_string, process_setup_from_vst,
        read_plugin_state_from_stream, stereo_output_buffers_from_vst_process_data,
        vst_event_to_midi, vst3_bus_count, write_plugin_state_to_stream,
        write_vst3_parameter_string,
    },
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lindelion_ui::lamath_tube_vizia::LamathTubeSwitchId;
use vst3::{Class, ComRef, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{LamathTube, parameters};

use super::{MAX_BLOCK_EVENTS, editor};

const TUBE_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_output(2, "Output"),
    Vst3BusInfo::event_input(1, "MIDI Input"),
];

pub(crate) struct LamathTubeVst3Processor {
    plugin: RefCell<LamathTube>,
    setup: Cell<ShellProcessSetup>,
    values: Vst3ParameterMirror<{ parameters::PARAMETER_COUNT }>,
    pending_values: [AtomicU32; parameters::PARAMETER_COUNT],
    pending_dirty: [AtomicBool; parameters::PARAMETER_COUNT],
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    editor_switches: RwLock<Vec<lindelion_ui::lamath_tube_vizia::LamathTubeModelSwitch>>,
    #[cfg(any(test, target_os = "macos", target_os = "windows"))]
    editor_slots: RwLock<lindelion_ui::audio_file_slot::AudioFileSlotListView>,
    handler: Cell<*mut IComponentHandler>,
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

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn set_model_switch(&self, id: LamathTubeSwitchId, enabled: bool) {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return;
        };
        plugin.set_model_switch(id, enabled);
        self.replace_editor_switches(plugin.model_switches());
        unsafe { restart_vst3_parameter_values_changed(self.handler.get()) };
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn select_articulation_slot(&self, slot: usize) {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            return;
        };
        plugin.select_articulation_slot(slot);
        self.replace_editor_slots(plugin.articulation_slot_list_view());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn load_excitation_from_path(&self, slot: usize, path: &Path) {
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
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
        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
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

    fn set_value(&self, id: u32, normalized: f64) -> tresult {
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

impl IEditControllerTrait for LamathTubeVst3Processor {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::setState(self, state)
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::setState(self, state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::getState(self, state)
    }

    unsafe fn getParameterCount(&self) -> i32 {
        parameters::PARAMETER_COUNT as i32
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() || param_index < 0 {
            return kInvalidArgument;
        }
        let Some(parameter) = parameters::parameter_by_index(param_index as usize) else {
            return kInvalidArgument;
        };
        fill_vst3_parameter_info(Vst3ParameterInfo::from_parameter(parameter), info)
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value_normalized: f64,
        string: *mut String128,
    ) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let Some(parameter) = parameters::parameter_by_id(id) else {
            return kInvalidArgument;
        };
        let plain = parameter.range.denormalize(value_normalized as f32);
        write_vst3_parameter_string(&parameters::format_plain_value(id, plain), string)
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value_normalized: *mut f64,
    ) -> tresult {
        if string.is_null() || value_normalized.is_null() {
            return kInvalidArgument;
        }
        let Some(plain) = parse_vst3_plain_value_string(string) else {
            return kInvalidArgument;
        };
        let Some(parameter) = parameters::parameter_by_id(id) else {
            return kInvalidArgument;
        };
        *value_normalized = f64::from(parameter.range.normalize(plain));
        kResultOk
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        parameters::parameter_by_id(id)
            .map(|parameter| f64::from(parameter.range.denormalize(value_normalized as f32)))
            .unwrap_or(0.0)
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        parameters::parameter_by_id(id)
            .map(|parameter| f64::from(parameter.range.normalize(plain_value as f32)))
            .unwrap_or(0.0)
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        parameters::parameter_index(id)
            .and_then(|index| self.values.value(index))
            .unwrap_or(0.0)
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        self.set_value(id, value)
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        self.handler.set(handler);
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        editor::create_editor_view(self)
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
mod tests {
    use super::*;

    #[test]
    fn reports_output_and_midi_buses() {
        assert_eq!(
            vst3_bus_count(
                &TUBE_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kOutput as BusDirection,
            ),
            1
        );
        assert_eq!(
            vst3_bus_count(
                &TUBE_BUSES,
                MediaTypes_::kEvent as MediaType,
                BusDirections_::kInput as BusDirection,
            ),
            1
        );
    }

    #[test]
    fn exposes_sparse_parameter_count() {
        let processor = LamathTubeVst3Processor::new();
        assert_eq!(
            unsafe { processor.getParameterCount() },
            parameters::PARAMETER_COUNT as i32
        );
    }

    #[test]
    fn editor_state_does_not_collapse_while_plugin_is_borrowed() {
        let processor = LamathTubeVst3Processor::new();
        let _busy = processor.plugin.borrow_mut();

        assert_eq!(processor.editor_knobs().len(), parameters::PARAMETER_COUNT);
        assert!(!processor.model_switches().is_empty());
        assert_eq!(
            processor.articulation_slot_list_view().slots.len(),
            crate::processor::ARTICULATION_SLOT_COUNT
        );
    }

    #[test]
    fn editor_parameter_change_queues_while_plugin_is_borrowed() {
        let processor = LamathTubeVst3Processor::new();
        let parameter = parameters::PARAMETERS[0];
        let _busy = processor.plugin.borrow_mut();

        processor.set_editor_parameter(parameter.id.0, 0.83);
        let knob = processor
            .editor_knobs()
            .into_iter()
            .find(|knob| knob.id == parameter.id.0)
            .expect("queued parameter should remain visible in editor knobs");
        assert!((knob.normalized - 0.83).abs() < 0.000_001);

        drop(_busy);
        let mut plugin = processor.plugin.borrow_mut();
        crate::assert_no_allocations("lamath_tube_pending_editor_parameter", || {
            processor.apply_pending_values(&mut plugin);
        });
        let normalized =
            parameters::normalized_value(plugin.patch(), parameter.id.0).expect("known parameter");
        assert!((normalized - 0.83).abs() < 0.000_001);
    }
}
