//! Native WASAPI audio engine — live input device → (passthrough; the VST3 chain joins in M3) →
//! output device, with device management. A **platform-neutral core** (the lock-free ring, sample
//! format conversion, channel adaptation, mode/format negotiation, and the per-callback transport)
//! compiles and is tested on all platforms; the WASAPI COM shell (`wasapi`) is Windows-only and is
//! cross-compile-verified. The realtime callback is allocation-free and lock-free (ADR-0001); native
//! WASAPI with exclusive-mode primary + shared fallback per ADR-0022.
//!
//! `dead_code`: neutral helpers (e.g. `format::bytes_per_sample`) are consumed by the Windows-only
//! engine, and the passthrough `Transport::render` only by tests, so they read as unused here.
#![allow(dead_code)]

mod channels;
mod engine_status;
mod format;
mod meter;
mod negotiation;
mod ring;
mod transport;
#[cfg(windows)]
mod wasapi;

pub use engine_status::EngineStatus;
pub use meter::MeterSnapshot;

#[cfg(windows)]
pub use wasapi::{AudioDirection, AudioEngine, default_device, device_sample_rate, enumerate};
