use std::{
    cell::{Cell, RefCell},
    ffi::c_char,
    sync::Arc,
};

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

use crate::{Cenedril, analysis::FrameRing};

pub(super) const CENEDRIL_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_input(2, "Input"),
    Vst3BusInfo::audio_output(2, "Output"),
];

/// Cenedril is a **single-component** plugin: one COM object implements `IComponent` +
/// `IAudioProcessor` + `IEditController`, so the editor (`createView`) reads the audio thread's
/// lock-free `FrameRing`/meter **directly** — no message marshaling, no shared-state handshake.
/// (`vst3` allows the shared `IPluginBase` because both `IComponentTrait` and
/// `IEditControllerTrait` extend `IPluginBaseTrait`.)
pub(super) struct CenedrilVst3Processor {
    plugin: RefCell<Cenedril>,
    setup: Cell<ShellProcessSetup>,
}

impl Class for CenedrilVst3Processor {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl CenedrilVst3Processor {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.processor_cid[0],
        crate::VST3_BUNDLE_METADATA.processor_cid[1],
        crate::VST3_BUNDLE_METADATA.processor_cid[2],
        crate::VST3_BUNDLE_METADATA.processor_cid[3],
    );

    pub(super) fn new() -> Self {
        let setup = ShellProcessSetup::default();
        let mut plugin = Cenedril::default();
        plugin.reset(setup);
        Self {
            plugin: RefCell::new(plugin),
            setup: Cell::new(setup),
        }
    }

    /// A clone of the audio→editor frame ring, for the editor view to drain.
    pub(super) fn frame_ring(&self) -> Arc<FrameRing> {
        self.plugin.borrow().frame_ring()
    }

    /// The current audio sample rate, for the editor's frequency axis.
    pub(super) fn sample_rate(&self) -> f32 {
        self.setup.get().sample_rate as f32
    }
}

impl IPluginBaseTrait for CenedrilVst3Processor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for CenedrilVst3Processor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        // Single-component: this object is its own controller, so there is no separate controller
        // class. The host detects this and queries `IEditController` on the component.
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        vst3_bus_count(&CENEDRIL_BUSES, media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        fill_vst3_bus_info(&CENEDRIL_BUSES, media_type, dir, index, bus)
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

impl IAudioProcessorTrait for CenedrilVst3Processor {
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
        plugin.start_analysis_worker(shell_setup.sample_rate as f32);
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

impl IProcessContextRequirementsTrait for CenedrilVst3Processor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

// Single-component edit controller: Cenedril exposes no parameters; `createView` returns the
// editor view, which (on Windows) reads this component's `FrameRing` directly.
impl IEditControllerTrait for CenedrilVst3Processor {
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
                &CENEDRIL_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kInput as BusDirection,
            ),
            1
        );
        assert_eq!(
            vst3_bus_count(
                &CENEDRIL_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kOutput as BusDirection,
            ),
            1
        );
    }

    #[test]
    fn processor_reports_zero_latency() {
        let processor = CenedrilVst3Processor::new();
        assert_eq!(unsafe { processor.getLatencySamples() }, 0);
    }
}
