#[cfg(feature = "vst3-entry")]
pub mod parameters;
pub mod patch;
#[cfg(feature = "vst3-entry")]
pub mod patch_io;
#[cfg(feature = "vst3-entry")]
pub mod plugin;
pub mod processor;
#[cfg(feature = "vst3-entry")]
mod vst3_entry;

#[cfg(feature = "vst3-entry")]
pub use lindelion_plugin_metadata::LAMATH_CYMBAL_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use patch::CymbalPatch;
#[cfg(feature = "vst3-entry")]
pub use plugin::{DESCRIPTOR, LamathCymbal};
pub use processor::{CymbalProcessor, ExcitationSource as CymbalExcitationSource};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
