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
        log_launch_context();
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

/// Probe this process's top-level windows immediately on the current thread.
#[cfg(windows)]
pub fn log_window_probe(label: &str) {
    probe_windows(label, 0);
}

/// Non-Windows builds have no HWNDs to inspect.
#[cfg(not(windows))]
pub fn log_window_probe(_label: &str) {}

/// Ask Windows to deliver a paint for visible top-level Galad windows while preserving Vizia's cloak.
#[cfg(windows)]
pub fn request_visible_window_paint(label: &str) {
    use windows::Win32::Graphics::Gdi::{
        RDW_ALLCHILDREN, RDW_INVALIDATE, RDW_NOERASE, RedrawWindow, UpdateWindow,
    };

    let (_enum_result, windows) = collect_windows();
    let mut changed = 0usize;
    for window in windows {
        let Some((left, top, right, bottom)) = window.rect else {
            continue;
        };
        let has_area = right > left && bottom > top;
        if !window.visible || window.iconic || !has_area {
            continue;
        }

        let redraw = unsafe {
            RedrawWindow(
                Some(window.hwnd),
                None,
                None,
                RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_NOERASE,
            )
            .as_bool()
        };
        let update = unsafe { UpdateWindow(window.hwnd).as_bool() };
        log(format!(
            "diagnostics: paint-nudge label={label} hwnd=0x{:x} cloaked={:?} rect={:?} redraw={} update={}",
            window.hwnd.0 as isize, window.cloaked, window.rect, redraw, update
        ));
        changed += usize::from(redraw || update);
    }
    log(format!(
        "diagnostics: paint-nudge done label={label} changed={changed}"
    ));
}

/// Non-Windows builds have no HWNDs to paint.
#[cfg(not(windows))]
pub fn request_visible_window_paint(_label: &str) {}

#[cfg(windows)]
fn log_launch_context() {
    log_parent_process();
    log_startup_info();
    log_console_state();
    for key in [
        "WT_SESSION",
        "SESSIONNAME",
        "ConEmuANSI",
        "TERM",
        "PROMPT",
        "ComSpec",
    ] {
        log(format!(
            "launch: env {key}={:?}",
            std::env::var_os(key).map(|value| value.to_string_lossy().to_string())
        ));
    }
}

#[cfg(not(windows))]
fn log_launch_context() {}

#[cfg(windows)]
fn log_parent_process() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        log("launch: parent snapshot error");
        return;
    };

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut processes = Vec::new();
    let current_pid = std::process::id();

    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            processes.push(entry);
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }
    let _ = unsafe { CloseHandle(snapshot) };

    let self_entry = processes
        .iter()
        .copied()
        .find(|process| process.th32ProcessID == current_pid);
    let parent_entry = self_entry.and_then(|self_process| {
        processes
            .iter()
            .copied()
            .find(|process| process.th32ProcessID == self_process.th32ParentProcessID)
    });

    match self_entry {
        Some(process) => log(format!(
            "launch: self pid={} parent_pid={} exe={}",
            process.th32ProcessID,
            process.th32ParentProcessID,
            utf16_array_to_string(&process.szExeFile)
        )),
        None => log(format!("launch: self pid={current_pid} parent=unknown")),
    }
    if let Some(process) = parent_entry {
        log(format!(
            "launch: parent pid={} exe={} threads={}",
            process.th32ProcessID,
            utf16_array_to_string(&process.szExeFile),
            process.cntThreads
        ));
    }
}

#[cfg(windows)]
fn log_startup_info() {
    use windows::Win32::System::Threading::GetStartupInfoW;
    use windows::Win32::System::Threading::STARTUPINFOW;

    let mut startup = STARTUPINFOW::default();
    unsafe { GetStartupInfoW(&mut startup) };
    log(format!(
        "launch: startup flags=0x{:x} show={} std_in=0x{:x} std_out=0x{:x} std_err=0x{:x}",
        startup.dwFlags.0,
        startup.wShowWindow,
        startup.hStdInput.0 as isize,
        startup.hStdOutput.0 as isize,
        startup.hStdError.0 as isize
    ));
}

#[cfg(windows)]
fn log_console_state() {
    use windows::Win32::System::Console::{
        GetConsoleWindow, GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    let console_window = unsafe { GetConsoleWindow() };
    log(format!(
        "launch: console_window=0x{:x}",
        console_window.0 as isize
    ));
    for (name, handle_id) in [
        ("stdin", STD_INPUT_HANDLE),
        ("stdout", STD_OUTPUT_HANDLE),
        ("stderr", STD_ERROR_HANDLE),
    ] {
        match unsafe { GetStdHandle(handle_id) } {
            Ok(handle) => log(format!("launch: std_handle {name}=0x{:x}", handle.0 as isize)),
            Err(error) => log(format!("launch: std_handle {name}=error {error:?}")),
        }
    }
}

#[cfg(windows)]
fn utf16_array_to_string(buffer: &[u16]) -> String {
    let len = buffer.iter().position(|ch| *ch == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}

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
    hwnd: windows::Win32::Foundation::HWND,
    visible: bool,
    iconic: bool,
    cloaked: Option<u32>,
    rect: Option<(i32, i32, i32, i32)>,
}

#[cfg(windows)]
fn collect_windows() -> (windows::core::Result<()>, Vec<WindowProbe>) {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
    use windows::core::BOOL;

    struct ProbeState {
        pid: u32,
        windows: Vec<WindowProbe>,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        };

        let state = unsafe { &mut *(lparam.0 as *mut ProbeState) };
        let mut owner_pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
        if owner_pid == state.pid {
            let mut rect = RECT::default();
            let rect = unsafe { GetWindowRect(hwnd, &mut rect) }
                .ok()
                .map(|()| (rect.left, rect.top, rect.right, rect.bottom));

            let mut cloaked = 0u32;
            let cloaked = unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED,
                    std::ptr::addr_of_mut!(cloaked).cast(),
                    std::mem::size_of::<u32>() as u32,
                )
            }
            .ok()
            .map(|()| cloaked);

            state.windows.push(WindowProbe {
                hwnd,
                visible: unsafe { IsWindowVisible(hwnd).as_bool() },
                iconic: unsafe { IsIconic(hwnd).as_bool() },
                cloaked,
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
    (enum_result, state.windows)
}

#[cfg(windows)]
fn probe_windows(label: &str, delay_ms: u64) {
    log(format!(
        "diagnostics: window-probe start label={label} delay_ms={delay_ms}"
    ));
    let (enum_result, windows) = collect_windows();

    log(format!(
        "diagnostics: window-probe label={label} delay_ms={delay_ms} enum={enum_result:?} count={}",
        windows.len()
    ));
    for window in &windows {
        log(format!(
            "diagnostics: window hwnd=0x{:x} visible={} iconic={} cloaked={:?} rect={:?}",
            window.hwnd.0 as isize, window.visible, window.iconic, window.cloaked, window.rect
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
