mod analysis;
mod analysis_job;
mod audition;
mod capture;
#[cfg(test)]
mod detection_quality_tests;
mod midi_export;
mod parameters;
mod patch;
mod patch_io;
mod plugin;
mod sample_library;
mod vst3_entry;
mod worker;

pub use analysis::{AnalysisError, AnalysisResult};
pub use analysis_job::{
    AnalysisJob, AnalysisJobResult, AnalysisResultCache, AnalysisSequence, AnalysisStatus,
    RequantizeJob, run_analysis_job,
};
pub use parameters::{
    AUDITION_VOLUME_PARAMETER_ID, CAPTURE_BARS_PARAMETER_ID, CONFIDENCE_PARAMETER_ID,
    COUNT_IN_PARAMETER_ID, EDITOR_PARAMETER_BINDINGS, GRID_PARAMETER_ID, MIN_NOTE_PARAMETER_ID,
    ONSET_SENSITIVITY_PARAMETER_ID, PARAMETER_BINDINGS, PARAMETERS, ParameterApplyKind,
    ParameterBinding, ROOT_PARAMETER_ID, SCALE_PARAMETER_ID, SNAP_PARAMETER_ID,
    SYNC_MODE_PARAMETER_ID, TIMING_STRENGTH_PARAMETER_ID, VELOCITY_AMOUNT_PARAMETER_ID,
    apply_parameter_plain, editor_parameter_bindings, parameter_binding,
};
pub use patch::{
    AnalysisSettings, AuditionSettings, CaptureBars, CaptureSettings, CaptureState, GlirdirPatch,
    ScratchpadAudio, ScratchpadMetadata, SyncMode,
};
pub use patch_io::{
    FORMAT_VERSION, PLUGIN_STATE_FORMAT_VERSION, PatchIoError, from_plugin_state, from_toml_str,
    load_library_patch, load_patch, save_library_patch, save_patch, to_plugin_state,
    to_toml_string,
};
pub use plugin::{DESCRIPTOR, Glirdir};
pub(crate) use worker::{GlirdirWorker, GlirdirWorkerQueue, GlirdirWorkerResult};

#[cfg(test)]
pub(crate) use allocation_tests::assert_no_allocations;

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
