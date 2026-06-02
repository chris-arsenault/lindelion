use std::cell::{Cell, RefCell};
use std::ffi::c_char;

use lindelion_plugin_shell::{
    AudioPlugin, ProcessContext as ShellProcessContext, ProcessSetup as ShellProcessSetup,
    vst3::{
        Vst3BusInfo, audio_input_buffer_from_vst_process_data, can_process_32_bit_sample_size,
        clear_vst_outputs, fill_vst3_bus_info, process_setup_from_vst,
        read_plugin_state_from_stream, stereo_output_buffers_from_vst_process_data, vst3_bus_count,
        write_plugin_state_to_stream,
    },
};
use vst3::{Class, Steinberg::Vst::*, Steinberg::*, uid};

use crate::{DeliveryReader, Lumedir};

pub(super) const LUMEDIR_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_input(2, "Input"),
    Vst3BusInfo::audio_output(2, "Output"),
];

/// Lúmedir is a **single-component** plugin: one COM object implements `IComponent` +
/// `IAudioProcessor` + `IEditController`, so the editor (`createView`) reads the off-thread delivery
/// worker's snapshots **directly** through a cloned `DeliveryReader` — no message marshaling, no
/// shared-state handshake (matches Cenedril/Calóma, which also expose no host parameters). (`vst3`
/// allows the shared `IPluginBase` because both `IComponentTrait` and `IEditControllerTrait` extend
/// `IPluginBaseTrait`.)
pub(super) struct LumedirVst3Processor {
    plugin: RefCell<Lumedir>,
    setup: Cell<ShellProcessSetup>,
}

impl Class for LumedirVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl LumedirVst3Processor {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.processor_cid[0],
        crate::VST3_BUNDLE_METADATA.processor_cid[1],
        crate::VST3_BUNDLE_METADATA.processor_cid[2],
        crate::VST3_BUNDLE_METADATA.processor_cid[3],
    );

    pub(super) fn new() -> Self {
        let setup = ShellProcessSetup::default();
        let mut plugin = Lumedir::default();
        plugin.reset(setup);
        Self {
            plugin: RefCell::new(plugin),
            setup: Cell::new(setup),
        }
    }

    /// A cloneable read handle onto the delivery worker's snapshots, for the editor view to poll.
    pub(super) fn delivery_reader(&self) -> Option<DeliveryReader> {
        self.plugin.borrow().delivery_reader()
    }

    /// The shared coaching config, for the editor to read and edit (factor + target bands).
    pub(super) fn shared_config(&self) -> std::sync::Arc<crate::SharedConfig> {
        self.plugin.borrow().shared_config()
    }
}

impl IPluginBaseTrait for LumedirVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for LumedirVst3Processor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        // Single-component: this object is its own controller, so there is no separate controller
        // class. The host detects this and queries `IEditController` on the component.
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        vst3_bus_count(&LUMEDIR_BUSES, media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        fill_vst3_bus_info(&LUMEDIR_BUSES, media_type, dir, index, bus)
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

impl IAudioProcessorTrait for LumedirVst3Processor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if inputs.is_null() || outputs.is_null() || num_ins != 1 || num_outs != 1 {
            return kResultFalse;
        }
        if *inputs == SpeakerArr::kStereo && *outputs == SpeakerArr::kStereo {
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
            BusDirections_::kInput | BusDirections_::kOutput => {
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

        if data.symbolicSampleSize as SymbolicSampleSizes != SymbolicSampleSizes_::kSample32 {
            clear_vst_outputs(data);
            return kResultOk;
        }

        let data_ptr = data as *mut ProcessData;
        let input = audio_input_buffer_from_vst_process_data(&*data_ptr);
        let Some(buffer) = stereo_output_buffers_from_vst_process_data(&mut *data_ptr) else {
            clear_vst_outputs(&mut *data_ptr);
            return kResultOk;
        };

        let Ok(mut plugin) = self.plugin.try_borrow_mut() else {
            let mut buffer = buffer;
            buffer.clear();
            return kResultFalse;
        };
        plugin.process(ShellProcessContext::new(self.setup.get(), buffer, &[]).with_input(input));
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for LumedirVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

// Single-component edit controller: Lúmedir exposes no parameters; `createView` returns the editor
// view, which (on Windows) reads this component's delivery snapshots directly.
impl IEditControllerTrait for LumedirVst3Processor {
    unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        0
    }

    unsafe fn getParameterInfo(&self, _param_index: i32, _info: *mut ParameterInfo) -> tresult {
        kInvalidArgument
    }

    unsafe fn getParamStringByValue(
        &self,
        _id: u32,
        _value_normalized: f64,
        _string: *mut String128,
    ) -> tresult {
        kInvalidArgument
    }

    unsafe fn getParamValueByString(
        &self,
        _id: u32,
        _string: *mut TChar,
        _value_normalized: *mut f64,
    ) -> tresult {
        kInvalidArgument
    }

    unsafe fn normalizedParamToPlain(&self, _id: u32, value_normalized: f64) -> f64 {
        value_normalized
    }

    unsafe fn plainParamToNormalized(&self, _id: u32, plain_value: f64) -> f64 {
        plain_value
    }

    unsafe fn getParamNormalized(&self, _id: u32) -> f64 {
        0.0
    }

    unsafe fn setParamNormalized(&self, _id: u32, _value: f64) -> tresult {
        kResultOk
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        super::editor::create_editor_view(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_buses_are_one_stereo_in_and_one_stereo_out() {
        assert_eq!(
            vst3_bus_count(
                &LUMEDIR_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kInput as BusDirection,
            ),
            1
        );
        assert_eq!(
            vst3_bus_count(
                &LUMEDIR_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kOutput as BusDirection,
            ),
            1
        );
    }

    #[test]
    fn processor_reports_zero_latency() {
        let processor = LumedirVst3Processor::new();
        assert_eq!(unsafe { processor.getLatencySamples() }, 0);
    }

    #[test]
    fn loads_and_runs_bit_exact_passthrough_through_the_vst3_boundary() {
        use std::ptr;

        // Exercise the actual VST3 host entry path (the COM `IAudioProcessor`/`IComponent`
        // methods), not just the `AudioPlugin` trait: a host calls `setupProcessing` →
        // `setActive(true)` → `process` with a `ProcessData`. This is the "loads/runs correctly"
        // validation, driven on Linux through the same COM surface a real host uses.
        let processor = LumedirVst3Processor::new();

        let mut setup: ProcessSetup = unsafe { std::mem::zeroed() };
        setup.processMode = ProcessModes_::kRealtime as i32;
        setup.symbolicSampleSize = SymbolicSampleSizes_::kSample32 as i32;
        setup.sampleRate = 48_000.0;
        setup.maxSamplesPerBlock = 1024;
        assert_eq!(unsafe { processor.setupProcessing(&mut setup) }, kResultOk);
        assert_eq!(unsafe { processor.setActive(1) }, kResultOk);

        let left_in = [0.0_f32, 0.5, -0.25, 1.0];
        let right_in = [-1.0_f32, 0.123, 0.0, -0.5];
        let mut left_out = [9.0_f32; 4];
        let mut right_out = [9.0_f32; 4];

        let mut in_channels = [
            left_in.as_ptr() as *mut Sample32,
            right_in.as_ptr() as *mut Sample32,
        ];
        let mut out_channels = [left_out.as_mut_ptr(), right_out.as_mut_ptr()];
        let mut input_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: in_channels.as_mut_ptr(),
            },
        };
        let mut output_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: out_channels.as_mut_ptr(),
            },
        };
        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: 4,
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut input_bus,
            outputs: &mut output_bus,
            inputParameterChanges: ptr::null_mut(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: ptr::null_mut(),
        };

        assert_eq!(unsafe { processor.process(&mut data) }, kResultOk);

        // The host's output buffers are filled bit-exact from the input (0-latency passthrough).
        assert_eq!(left_out, left_in);
        assert_eq!(right_out, right_in);
    }
}
