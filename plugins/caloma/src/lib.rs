// Calóma — Windows-only single VST3 packaging the ported speech effects (`speech/`) as one serial
// chain. M0 scaffold: the signal-order parameter + the patch model. Chain runtime (M2), the three
// preset orders (M3), the VST3 adapter + Vizia editor (M4), and end-to-end default tuning (M5)
// land in later milestones.

pub mod analysis;
pub mod chain_effect;
pub mod controls;
pub mod order;
pub mod patch;
pub mod patch_io;
pub mod plugin;
pub mod runtime;
pub mod slot;
pub mod slot_params;
pub mod topology;
pub mod tuning;
mod vst3_entry;

pub use lindelion_plugin_metadata::CALOMA_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use plugin::{Caloma, DESCRIPTOR};

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;
