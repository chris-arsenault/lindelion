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

/// Relaunch Explorer-started UI processes with a clean Win32 startup context.
///
/// Explorer passes `STARTF_USESHOWWINDOW` plus the undocumented `STARTF_MONITOR` flag through
/// `STARTUPINFO`. The winit/Vizia path that creates a hidden DWM-cloaked window is the broken
/// combination, so do this before Vizia creates any HWNDs.
#[cfg(windows)]
pub fn relaunch_without_explorer_startup(already_relaunched: bool) -> bool {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateProcessW, GetStartupInfoW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
        STARTF_USESHOWWINDOW, STARTUPINFOW,
    };
    use windows::core::{PCWSTR, PWSTR};

    const STARTF_MONITOR: u32 = 0x0000_0400;

    if already_relaunched {
        log("launch: explorer startup relaunch skipped already_relaunched");
        return false;
    }

    let mut startup = STARTUPINFOW::default();
    unsafe { GetStartupInfoW(&mut startup) };
    let flags = startup.dwFlags.0;
    let explorer_startup = flags & (STARTF_USESHOWWINDOW.0 | STARTF_MONITOR)
        == STARTF_USESHOWWINDOW.0 | STARTF_MONITOR;
    if !explorer_startup {
        log(format!(
            "launch: explorer startup relaunch skipped flags=0x{flags:x}"
        ));
        return false;
    }

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            log(format!(
                "launch: explorer startup relaunch current_exe error {error:?}"
            ));
            return false;
        }
    };
    let exe_arg = quote_windows_arg(&exe.to_string_lossy());
    let command_line = format!("{exe_arg} {}", crate::RELAUNCH_ARG);
    let mut command_line_w: Vec<u16> = command_line.encode_utf16().chain([0]).collect();
    let exe_w: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let cwd_w: Option<Vec<u16>> = std::env::current_dir()
        .ok()
        .map(|cwd| cwd.as_os_str().encode_wide().chain([0]).collect());
    let cwd_ptr = cwd_w
        .as_ref()
        .map(|cwd| PCWSTR(cwd.as_ptr()))
        .unwrap_or_else(|| PCWSTR(std::ptr::null()));

    let mut child_startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process_info = PROCESS_INFORMATION::default();

    log(format!(
        "launch: explorer startup relaunch start command={command_line:?}"
    ));
    let result = unsafe {
        CreateProcessW(
            PCWSTR(exe_w.as_ptr()),
            Some(PWSTR(command_line_w.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            cwd_ptr,
            &mut child_startup,
            &mut process_info,
        )
    };

    match result {
        Ok(()) => {
            log(format!(
                "launch: explorer startup relaunch ok pid={} process=0x{:x} thread=0x{:x}",
                process_info.dwProcessId,
                process_info.hProcess.0 as isize,
                process_info.hThread.0 as isize
            ));
            let _ = unsafe { CloseHandle(process_info.hThread) };
            let _ = unsafe { CloseHandle(process_info.hProcess) };
            true
        }
        Err(error) => {
            log(format!(
                "launch: explorer startup relaunch error {error:?}; continuing current process"
            ));
            false
        }
    }
}

#[cfg(windows)]
fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty()
        && !arg
            .chars()
            .any(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | '"'))
    {
        return arg.to_owned();
    }

    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                for _ in 0..(backslashes * 2 + 1) {
                    quoted.push('\\');
                }
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                for _ in 0..backslashes {
                    quoted.push('\\');
                }
                quoted.push(ch);
                backslashes = 0;
            }
        }
    }
    for _ in 0..(backslashes * 2) {
        quoted.push('\\');
    }
    quoted.push('"');
    quoted
}

/// Non-Windows builds have no Win32 startup context.
#[cfg(not(windows))]
pub fn relaunch_without_explorer_startup(_already_relaunched: bool) -> bool {
    false
}

/// Probe this process's top-level windows immediately on the current thread.
#[cfg(windows)]
pub fn log_window_probe(label: &str) {
    probe_windows(label, 0);
}

/// Non-Windows builds have no HWNDs to inspect.
#[cfg(not(windows))]
pub fn log_window_probe(_label: &str) {}

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
            Ok(handle) => log(format!(
                "launch: std_handle {name}=0x{:x}",
                handle.0 as isize
            )),
            Err(error) => log(format!("launch: std_handle {name}=error {error:?}")),
        }
    }
}

#[cfg(windows)]
fn utf16_array_to_string(buffer: &[u16]) -> String {
    let len = buffer
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(buffer.len());
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
        use windows::Win32::Foundation::RECT;
        use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
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
