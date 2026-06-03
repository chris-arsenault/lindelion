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
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
        debug_log(format!(
            "vizia-window: attach begin parent=0x{:x}",
            parent as usize
        ));
        let parent = ParentWindow(parent);
        debug_log("vizia-window: open_parented begin");
        let window = application.open_parented(&parent);
        debug_log("vizia-window: open_parented done");
        Self { window }
    }
}

impl Drop for ViziaWindowEditor {
    fn drop(&mut self) {
        if self.window.is_open() {
            self.window.close();
        }
    }
}

/// Append one diagnostic line to Galad's per-user log from plugin/editor crates that cannot depend
/// on Galad directly. This is intentionally best-effort and Windows-only with this module.
pub fn debug_log(message: impl AsRef<str>) {
    let path = default_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {}", timestamp_ms(), message.as_ref());
    }
}

fn default_log_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Galad")
        .join("galad.log")
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
