// Lúmedir — Windows-only passthrough Speech-Coach VST3 (M0 scaffold).

pub mod clarity;
pub mod config;
pub mod config_io;
pub mod delivery;
mod delivery_worker;
pub mod pause_structure;
pub mod pitch_dynamism;
pub mod plugin;
mod shared_config;
pub mod speaking_rate;
mod vst3_entry;

pub use delivery_worker::{DeliveryReader, DeliveryWorker};
pub use shared_config::SharedConfig;

pub use lindelion_plugin_metadata::LUMEDIR_VST3_BUNDLE_METADATA as VST3_BUNDLE_METADATA;
pub use plugin::{DESCRIPTOR, Lumedir};

#[cfg(test)]
pub(crate) use lindelion_test_allocator::assert_no_allocations;

#[cfg(test)]
lindelion_test_allocator::install_test_allocator!();
