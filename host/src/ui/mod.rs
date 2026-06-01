//! The Galad host UI. The **framework-neutral** state + commands (`state`, `command`) are tested on
//! Linux; the Vizia views + standalone app (Windows) wrap them. The UI framework is **Vizia (winit
//! standalone)** per [ADR-0024](../../docs/adr/0024-galad-ui-vizia.md).
//!
//! Wired into `main` by the Vizia app (M6 Step 7); allow dead code / not-yet-consumed re-exports
//! until then.
#![allow(dead_code)]
#![allow(unused_imports)]

mod command;
mod state;
#[cfg(windows)]
mod vizia_app;

pub use command::{UiCommand, apply};
pub use state::{Dir, HostUiState, UiSlot};
#[cfg(windows)]
pub use vizia_app::{AppData, AppEvent, Signals, build_ui, run};
