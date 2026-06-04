#[cfg(feature = "vst3-entry")]
mod parameters;
mod patch;
#[cfg(feature = "vst3-entry")]
mod patch_io;
#[cfg(feature = "vst3-entry")]
mod plugin;
mod processor;
#[cfg(feature = "vst3-entry")]
mod vst3_entry;

#[cfg(feature = "vst3-entry")]
pub use lindelion_plugin_metadata::LAMATH_STRINGED_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use patch::{ArticulationSlot, BodySelection, DriverSelection, ModelSwitches, StringPatch};
#[cfg(feature = "vst3-entry")]
pub use plugin::{DESCRIPTOR, LamathStringed};
pub use processor::{
    ARTICULATION_NAMES, ARTICULATION_SLOT_COUNT, ExcitationSource as StringExcitationSource,
    StringProcessor,
};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
