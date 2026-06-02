//! Lúmedir editor surface.
//!
//! The size/host types here are platform-neutral and compile everywhere (including Linux
//! `make ci`). The Vizia view and the Windows `IPlugView`→`HWND` attach live in the
//! `windows`-gated [`platform`] submodule, because `vizia` is a Windows-target dependency for the
//! new VSTs (ADR-0023) — mirroring Cenedril's editor stack.

pub const LUMEDIR_EDITOR_WIDTH: i32 = 720;
pub const LUMEDIR_EDITOR_HEIGHT: i32 = 480;

#[derive(Debug, Clone, Copy)]
pub struct LumedirEditorSize {
    pub width: i32,
    pub height: i32,
}

/// Minimal editor host: just the controller pointer. Lúmedir has no parameters in M0, and the
/// delivery-metric snapshots the views will read arrive in M4 — so the M0 view is a static
/// placeholder.
#[derive(Debug, Clone, Copy)]
pub struct LumedirEditorHost {
    pub controller: usize,
}

impl LumedirEditorHost {
    pub fn new(controller: usize) -> Self {
        Self { controller }
    }
}

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::LumedirViziaEditor;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_host_round_trips_controller_pointer() {
        let host = LumedirEditorHost::new(0xDEAD_BEEF);
        assert_eq!(host.controller, 0xDEAD_BEEF);
    }

    #[test]
    fn editor_size_holds_its_dimensions() {
        let size = LumedirEditorSize {
            width: LUMEDIR_EDITOR_WIDTH,
            height: LUMEDIR_EDITOR_HEIGHT,
        };
        assert_eq!(size.width, LUMEDIR_EDITOR_WIDTH);
        assert_eq!(size.height, LUMEDIR_EDITOR_HEIGHT);
    }
}
