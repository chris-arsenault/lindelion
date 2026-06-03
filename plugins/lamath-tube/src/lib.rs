mod parameters;
mod patch;
mod patch_io;
mod plugin;
mod processor;
mod vst3_entry;

pub use lindelion_plugin_metadata::LAMATH_TUBE_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use plugin::{DESCRIPTOR, LamathTube};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
