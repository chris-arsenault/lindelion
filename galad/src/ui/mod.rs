//! The Galad host UI. The **framework-neutral** state + commands (`state`, `command`) are tested on
//! Linux; the Vizia views + standalone app (Windows) wrap them. The UI framework is **Vizia (winit
//! standalone)** per [ADR-0024](../../docs/adr/0024-galad-ui-vizia.md).
//!
//! `dead_code`: `state`/`command` are the neutral, Linux-tested core, but their setters and the
//! device/chain command variants are consumed by the Windows-only `vizia_app`, so they read as dead
//! on the neutral build.
#![allow(dead_code)]

mod command;
mod layout;
mod state;
#[cfg(windows)]
mod vizia_app;

#[cfg(windows)]
pub use vizia_app::run;
