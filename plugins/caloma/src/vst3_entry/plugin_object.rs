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
use std::sync::Arc;

use lindelion_ui::caloma_vizia::CalomaControlSurface;
use vst3::{Class, Steinberg::Vst::*, Steinberg::*, uid};

use crate::Caloma;
use crate::controls::SharedControls;

pub(super) const CALOMA_BUSES: [Vst3BusInfo; 2] = [
    Vst3BusInfo::audio_input(2, "Input"),
    Vst3BusInfo::audio_output(2, "Output"),
];

/// Calóma's **single-component** VST3 object: one COM object implements `IComponent`,
/// `IAudioProcessor`, `IEditController`, and `IProcessContextRequirements`. The host
/// `queryInterface`s the controller on this same object, so the Vizia editor and the DSP share one
/// `Caloma` (and its `SharedControls`) directly — no host-relayed parameter channel (ADR-0023).
/// `getControllerClassId` returns `kNotImplemented` to signal that there is no separate controller
/// class; the factory registers exactly one class.
pub(super) struct CalomaVst3Plugin {
    plugin: RefCell<Caloma>,
    setup: Cell<ShellProcessSetup>,
    /// A second handle on the *same* live controls the inner `Caloma` owns. Held outside the
    /// `RefCell` so `createView` can hand the editor the control surface without borrowing the
    /// audio-thread plugin. The order/state operations only ever mutate this object's atomics (never
    /// replace it), so it stays the live surface for the plugin's whole life.
    controls: Arc<SharedControls>,
}

impl Class for CalomaVst3Plugin {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IEditController,
        IProcessContextRequirements,
    );
}

impl CalomaVst3Plugin {
    pub(super) const CID: TUID = uid(
        crate::VST3_BUNDLE_METADATA.processor_cid[0],
        crate::VST3_BUNDLE_METADATA.processor_cid[1],
        crate::VST3_BUNDLE_METADATA.processor_cid[2],
        crate::VST3_BUNDLE_METADATA.processor_cid[3],
    );

    pub(super) fn new() -> Self {
        // Unlike a passthrough plugin, Calóma does **not** `reset` here: `reset` pre-builds all three
        // orders' real (NN) chains, which is heavy and host-driven. The chains are built in
        // `setupProcessing`; before that the object reports zero latency and processes nothing.
        let plugin = Caloma::default();
        let controls = plugin.controls();
        Self {
            plugin: RefCell::new(plugin),
            setup: Cell::new(ShellProcessSetup::default()),
            controls,
        }
    }

    /// The live control surface handed to the Vizia editor (the same atomics the DSP reads).
    pub(super) fn control_surface(&self) -> Arc<dyn CalomaControlSurface> {
        let controls: Arc<dyn CalomaControlSurface> = self.controls.clone();
        controls
    }
}

impl IPluginBaseTrait for CalomaVst3Plugin {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for CalomaVst3Plugin {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        // Single-component: this object is its own controller, so there is no separate controller
        // class. Hosts query `IEditController` on the component itself.
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        vst3_bus_count(&CALOMA_BUSES, media_type, dir)
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        fill_vst3_bus_info(&CALOMA_BUSES, media_type, dir, index, bus)
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

impl IAudioProcessorTrait for CalomaVst3Plugin {
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
        self.plugin
            .try_borrow()
            .map_or(0, |plugin| plugin.latency_samples() as u32)
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

impl IEditControllerTrait for CalomaVst3Plugin {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        // Same object as the component, but a host may still push the component state to the
        // controller side: seed the shared controls from it.
        let Some(plugin_state) = read_plugin_state_from_stream(state) else {
            return kResultOk;
        };
        if let Ok(mut plugin) = self.plugin.try_borrow_mut() {
            plugin.load_state(plugin_state);
        }
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getParameterCount(&self) -> i32 {
        // No host-automatable parameters: the Vizia editor is the control surface (ADR-0023).
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
        // The Vizia control editor, wired to this object's live `SharedControls`. On Windows it
        // attaches to the host child window (ADR-0023); off-Windows the view is created but its
        // attach is a no-op.
        super::editor::create_editor_view(self)
    }
}

impl IProcessContextRequirementsTrait for CalomaVst3Plugin {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::ComWrapper;

    #[test]
    fn buses_are_one_stereo_in_and_one_stereo_out() {
        assert_eq!(
            vst3_bus_count(
                &CALOMA_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kInput as BusDirection,
            ),
            1
        );
        assert_eq!(
            vst3_bus_count(
                &CALOMA_BUSES,
                MediaTypes_::kAudio as MediaType,
                BusDirections_::kOutput as BusDirection,
            ),
            1
        );
    }

    #[test]
    fn exposes_no_parameters_and_zero_latency_before_setup() {
        let plugin = CalomaVst3Plugin::new();
        assert_eq!(unsafe { plugin.getParameterCount() }, 0);
        assert_eq!(unsafe { plugin.getLatencySamples() }, 0);
    }

    #[test]
    fn one_object_exposes_component_processor_and_controller() {
        // The single-component host-load sequence: the host creates one object and queries the
        // component, the audio processor, and the edit controller from it.
        let wrapper = ComWrapper::new(CalomaVst3Plugin::new());
        assert!(
            wrapper.to_com_ptr::<IComponent>().is_some(),
            "must expose IComponent"
        );
        assert!(
            wrapper.to_com_ptr::<IAudioProcessor>().is_some(),
            "must expose IAudioProcessor"
        );
        assert!(
            wrapper.to_com_ptr::<IEditController>().is_some(),
            "must expose IEditController on the same object"
        );
    }

    #[test]
    fn controller_class_id_reports_no_separate_controller_class() {
        let plugin = CalomaVst3Plugin::new();
        let mut cid: TUID = [0; 16];
        assert_eq!(
            unsafe { plugin.getControllerClassId(&mut cid) },
            kNotImplemented
        );
        assert_eq!(cid, [0; 16]);
    }
}
