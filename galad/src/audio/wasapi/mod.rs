//! Windows WASAPI COM shell — device enumeration (`devices`), `IAudioClient` stream setup
//! (`stream`), and the realtime duplex engine (`engine`). Windows-only; cross-compile-verified by
//! `make host-windows-check`. The platform-neutral transport it drives lives in the parent `audio`
//! module.
//!
//! COM signatures are FFI camelCase and unsafe; the lint allows mirror `vst3_host`.
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(unsafe_op_in_unsafe_fn)]
// COM FFI with camelCase/unsafe signatures. The re-export surface, some helpers, and the
// `MeasuredLatency` per-stage breakdown are consumed across the engine; a few read as unused here.
#![allow(dead_code)]
#![allow(unused_imports)]

mod devices;
mod engine;
mod stream;

pub use devices::{AudioDirection, AudioError, default_device, enumerate};
pub use engine::{AudioEngine, MeasuredLatency};
pub use stream::{WasapiStream, device_sample_rate};
