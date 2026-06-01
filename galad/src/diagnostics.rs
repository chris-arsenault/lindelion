//! File-based diagnostics for Windows GUI launch failures.
//!
//! The release executable has no console, so startup failures need a log path that works for both
//! double-click and command-line launches.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static INIT: Once = Once::new();
static LOG_LOCK: Mutex<()> = Mutex::new(());
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Initialize startup diagnostics and the panic hook.
pub fn init() {
    INIT.call_once(|| {
        let path = log_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        append_line(&path, "----- galad startup -----");
        log(format!(
            "diagnostics: path={} pid={}",
            path.display(),
            std::process::id()
        ));
        std::panic::set_hook(Box::new(|panic| {
            log(format!("panic: {panic}"));
        }));
    });
}

/// Append one diagnostic line.
pub fn log(message: impl AsRef<str>) {
    append_line(&log_path(), message.as_ref());
}

/// The per-user Galad diagnostic log file.
pub fn log_path() -> PathBuf {
    LOG_PATH.get_or_init(default_log_path).clone()
}

/// Probe this process's top-level windows shortly after the event loop starts.
#[cfg(windows)]
pub fn spawn_window_probe(label: &'static str) {
    std::thread::spawn(move || {
        for delay_ms in [250u64, 1000, 3000] {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            probe_windows(label, delay_ms);
        }
    });
}

/// Non-Windows builds have no HWNDs to inspect.
#[cfg(not(windows))]
pub fn spawn_window_probe(_label: &'static str) {}

fn append_line(path: &PathBuf, message: &str) {
    let _guard = LOG_LOCK.lock().ok();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {message}", timestamp_ms());
    }
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[cfg(windows)]
#[derive(Debug)]
struct WindowProbe {
    hwnd: isize,
    title: String,
    visible: bool,
    iconic: bool,
    rect: Option<(i32, i32, i32, i32)>,
}

#[cfg(windows)]
fn probe_windows(label: &str, delay_ms: u64) {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    use windows::core::BOOL;

    struct ProbeState {
        pid: u32,
        windows: Vec<WindowProbe>,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        };

        let state = unsafe { &mut *(lparam.0 as *mut ProbeState) };
        let mut owner_pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
        if owner_pid == state.pid {
            let mut title = [0u16; 256];
            let title_len = unsafe { GetWindowTextW(hwnd, &mut title) }.max(0) as usize;
            let title = String::from_utf16_lossy(&title[..title_len]);

            let mut rect = RECT::default();
            let rect = unsafe { GetWindowRect(hwnd, &mut rect) }
                .ok()
                .map(|()| (rect.left, rect.top, rect.right, rect.bottom));

            state.windows.push(WindowProbe {
                hwnd: hwnd.0 as isize,
                title,
                visible: unsafe { IsWindowVisible(hwnd).as_bool() },
                iconic: unsafe { IsIconic(hwnd).as_bool() },
                rect,
            });
        }
        true.into()
    }

    let mut state = ProbeState {
        pid: std::process::id(),
        windows: Vec::new(),
    };
    let enum_result = unsafe {
        EnumWindows(
            Some(enum_window),
            LPARAM((&mut state as *mut ProbeState) as isize),
        )
    };

    log(format!(
        "diagnostics: window-probe label={label} delay_ms={delay_ms} enum={enum_result:?} count={}",
        state.windows.len()
    ));
    for window in &state.windows {
        log(format!(
            "diagnostics: window hwnd=0x{:x} visible={} iconic={} rect={:?} title={:?}",
            window.hwnd, window.visible, window.iconic, window.rect, window.title
        ));
    }
}

#[cfg(windows)]
fn default_log_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Galad")
        .join("galad.log")
}

#[cfg(not(windows))]
fn default_log_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join("galad")
        .join("galad.log")
}
