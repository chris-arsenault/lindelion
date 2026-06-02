//! Shared Windows `IPlugView`→`HWND` attach for the Vizia editors of the Windows-only VSTs
//! (Calóma, Cenedril, Lúmedir; ADR-0023).
//!
//! Each plugin builds its own `vizia::Application` — the view content, styles, and timers, which are
//! the genuinely plugin-specific part. This type owns the identical baseview attach + teardown those
//! editors previously copy-pasted: wrap the host child window as a `ParentWindow`, `open_parented`
//! the application into it, and close the window on drop. A plugin's editor becomes a thin newtype
//! over this one.
//!
//! Windows-only: `vizia` is a Windows-target dependency for these VSTs (the macOS instruments gate an
//! equivalent — but drop-target-augmented — attach on macOS), so this module compiles under the
//! Windows build (cargo-xwin), not Linux `make ci`.

use std::ffi::c_void;

use vizia::prelude::Context;
use vizia::{Application, ParentWindow, WindowHandle};

/// A Vizia editor embedded in a host-provided child window (an `HWND` on Windows). Owns the
/// `WindowHandle` and closes it on drop.
pub struct ViziaWindowEditor {
    window: WindowHandle,
}

impl ViziaWindowEditor {
    /// Open the built `application` parented into the host child window `parent`.
    ///
    /// # Safety
    /// `parent` must be a valid host child-window handle (an `HWND` on Windows).
    pub unsafe fn attach<F>(parent: *mut c_void, application: Application<F>) -> Self
    where
        F: Fn(&mut Context) + Send + 'static,
    {
        let parent = ParentWindow(parent);
        Self {
            window: application.open_parented(&parent),
        }
    }
}

impl Drop for ViziaWindowEditor {
    fn drop(&mut self) {
        if self.window.is_open() {
            self.window.close();
        }
    }
}
