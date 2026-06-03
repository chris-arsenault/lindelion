pub mod parameters;
pub mod patch;
pub mod patch_io;
pub mod plugin;
pub mod processor;
mod vst3_entry;

pub use lindelion_plugin_metadata::LAMATH_CYMBAL_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use plugin::{DESCRIPTOR, LamathCymbal};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
