//! Host-side VST3 protocol — the **host** (not guest) side of the `vst3` COM bindings.
//!
//! Galad drives third-party and Lindelion VST3 plugins: it loads a module, gets the
//! `IPluginFactory`, enumerates classes, instantiates `IComponent`/`IAudioProcessor`, provides the
//! `IHostApplication`/`IComponentHandler` context, sets up processing, and calls `process()`. This
//! is written from scratch against the raw `vst3` crate (ADR-0002, no host framework). The COM model
//! is platform-neutral, so the protocol is exercised in-process on Linux against a test fixture; the
//! real Windows `.vst3` module load is in `module`/`spike`.
//!
//! COM trait methods are camelCase and the bindings are FFI, so the lint allows below mirror the
//! plugins' `vst3_entry` modules. Dead code is allowed until the spike entry point (M1 Step 6) wires
//! every piece into `main`.
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(unsafe_op_in_unsafe_fn)]
// This host-side VST3 layer is a neutral COM core consumed by the Windows-only engine/UI and by
// tests; its re-exports and helpers — plus error-variant payloads kept for `{:?}` and `PoolSlot`'s
// DLL-lifetime guard field — read as unused on the neutral, non-test build.
#![allow(dead_code)]
#![allow(unused_imports)]

mod bstream;
mod chain;
#[cfg(windows)]
mod editor;
mod editor_controller;
mod editor_frame;
mod editor_view;
#[cfg(windows)]
mod editor_window;
mod events;
#[cfg(test)]
mod fixture;
mod handoff;
mod host_context;
mod host_objects;
mod instance;
mod module;
mod parameters;
mod processing;
mod session_runtime;
mod spike;
mod state;
mod validate;

pub use chain::{ChainProcessor, PoolSlot};
#[cfg(windows)]
pub use editor::EditorHost;
pub use editor_controller::EditorController;
pub use editor_frame::HostPlugFrame;
pub use handoff::Handoff;
pub use host_context::HostContext;
pub use instance::{HostError, PluginInstance};
pub use module::{LoadedModule, load_module};
pub use processing::ProcessDriver;
pub use session_runtime::{SessionSlot, capture_session, restore_pool};
pub use spike::{SpikeReport, run_spike};
pub use state::{capture_state, restore_state};
pub use validate::{PluginProbe, probe_plugin, validate_plugin};
