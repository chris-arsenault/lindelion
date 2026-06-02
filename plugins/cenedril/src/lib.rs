// Cenedril — Windows-only passthrough Visualizer VST3 (M0 scaffold).

pub mod analysis;
pub mod plugin;
pub mod settings;
mod vst3_entry;

pub use lindelion_plugin_metadata::CENEDRIL_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use plugin::{Cenedril, DESCRIPTOR};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
