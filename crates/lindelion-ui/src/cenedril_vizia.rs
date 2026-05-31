//! Cenedril editor surface.
//!
//! The size/host types here are platform-neutral and compile everywhere (including Linux
//! `make ci`). The Vizia view and the Windows `IPlugView`→`HWND` attach live in the
//! `windows`-gated [`platform`] submodule, because `vizia` is a Windows-target dependency for the
//! new VSTs (ADR-0023) — mirroring how the macOS instruments gate their Vizia code on macOS.

pub const CENEDRIL_EDITOR_WIDTH: i32 = 720;
pub const CENEDRIL_EDITOR_HEIGHT: i32 = 480;

#[derive(Debug, Clone, Copy)]
pub struct CenedrilEditorSize {
    pub width: i32,
    pub height: i32,
}

/// Minimal editor host: just the controller pointer. Cenedril has no parameters in M0, and the
/// analysis/level data the views will read arrives in M2 — so the M1 view is a static placeholder.
#[derive(Debug, Clone, Copy)]
pub struct CenedrilEditorHost {
    pub controller: usize,
}

impl CenedrilEditorHost {
    pub fn new(controller: usize) -> Self {
        Self { controller }
    }
}

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::CenedrilViziaEditor;
