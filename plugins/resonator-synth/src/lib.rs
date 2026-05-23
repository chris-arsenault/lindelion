#![cfg_attr(target_os = "macos", allow(unexpected_cfgs))]

pub mod dsp;
mod parameters;
mod patch;
pub mod patch_io;
mod plugin;
pub mod runtime;
mod vst3_entry;

pub use dsp::WaveguideStyle;
pub use parameters::PARAMETERS;
pub use patch::{
    EnvelopeConfig, ExcitationSlot, FilterMode, LfoConfig, LfoShape, ModalConfig, ModalPreset,
    ModulationConfig, ModulationDestination, ModulationSlot, ModulationSource, OutputConfig,
    ResonatorConfig, ResonatorRouting, ResonatorSynthPatch, WaveguideConfig,
};
pub use plugin::{
    LoadedExcitationBuffer, ResonatorSynth, ResonatorTelemetry, SampleLoadError, SampleLoadReport,
};

#[cfg(test)]
mod allocation_tests {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    thread_local! {
        static ALLOCATION_COUNT: Cell<Option<usize>> = const { Cell::new(None) };
    }

    pub struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record_allocation();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) };
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record_allocation();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    fn record_allocation() {
        ALLOCATION_COUNT.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    pub fn assert_no_allocations<R>(label: &str, run: impl FnOnce() -> R) -> R {
        ALLOCATION_COUNT.with(|count| count.set(Some(0)));
        let result = run();
        let allocations = ALLOCATION_COUNT.with(|count| {
            let allocations = count.get().unwrap_or(0);
            count.set(None);
            allocations
        });

        assert_eq!(allocations, 0, "{label} allocated {allocations} time(s)");
        result
    }
}

#[cfg(test)]
pub(crate) use allocation_tests::assert_no_allocations;

use ahara_plugin_shell::PluginDescriptor;

#[cfg(test)]
pub(crate) use parameters::ParameterCodec;
pub(crate) use parameters::{
    FILTER_CUTOFF_PARAMETER_ID, FILTER_RESONANCE_PARAMETER_ID, MASTER_GAIN_PARAMETER_ID,
    MASTER_PAN_PARAMETER_ID, PARALLEL_MIX_A_PARAMETER_ID, PARALLEL_MIX_B_PARAMETER_ID,
    RuntimeParameterTarget, SATURATION_PARAMETER_ID, apply_parameter_plain_for_controller,
    parameter_binding, parameter_binding_by_index, parameter_binding_index,
    patch_parameter_plain_value, smoothed_runtime_parameter,
};
#[cfg(test)]
pub(crate) use parameters::{ParameterApplyKind, apply_parameter_plain};

pub const DESCRIPTOR: PluginDescriptor =
    PluginDescriptor::instrument("Ahara Resonator Synth", *b"ahara_resonator!");

pub(crate) const RESONATOR_MOD_WHEEL_CONTROLLER: u8 = 1;
pub(crate) const RESONATOR_BRIGHTNESS_CONTROLLER: u8 = 74;

#[cfg(test)]
mod plugin_tests;
