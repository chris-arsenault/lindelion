use std::cell::{Cell, RefCell};

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

use crate::Lumedir;

pub(super) const LUMEDIR_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_input(2, "Input"),
    Vst3BusInfo::audio_output(2, "Output"),
];

pub(super) struct LumedirVst3Processor {
    plugin: RefCell<Lumedir>,
    setup: Cell<ShellProcessSetup>,
}

impl Class for LumedirVst3Processor {
    type Interfaces = (IComponent, IAudioProcessor, IProcessContextRequirements);
}

impl LumedirVst3Processor {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.processor_cid[0],
        crate::VST3_BUNDLE_METADATA.processor_cid[1],
        crate::VST3_BUNDLE_METADATA.processor_cid[2],
        crate::VST3_BUNDLE_METADATA.processor_cid[3],
    );

    const CONTROLLER_CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.controller_cid[0],
        crate::VST3_BUNDLE_METADATA.controller_cid[1],
        crate::VST3_BUNDLE_METADATA.controller_cid[2],
        crate::VST3_BUNDLE_METADATA.controller_cid[3],
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
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        *class_id = Self::CONTROLLER_CID;
        kResultOk
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
}
