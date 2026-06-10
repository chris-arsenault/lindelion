//! File-based diagnostics for Windows GUI launch failures.
//!
//! The release executable has no console, so startup failures need a log path that works for both
//! double-click and command-line launches.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static INIT: Once = Once::new();
static LOG_LOCK: Mutex<()> = Mutex::new(());
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
#[cfg(windows)]
static NATIVE_CRASH_LOGGED: AtomicBool = AtomicBool::new(false);

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
        install_crash_handler();
        std::panic::set_hook(Box::new(|panic| {
            log_panic(panic);
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

#[cfg(windows)]
fn install_crash_handler() {
    use windows::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter;

    let previous = unsafe { SetUnhandledExceptionFilter(Some(unhandled_exception_filter)) };
    log(format!(
        "crash-handler: installed unhandled-exception-filter previous={}",
        if previous.is_some() { "some" } else { "none" }
    ));
}

#[cfg(not(windows))]
fn install_crash_handler() {}

fn log_panic(panic: &std::panic::PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>");
    let location = panic
        .location()
        .map(|location| {
            format!(
                "{}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )
        })
        .unwrap_or_else(|| "<unknown>".to_string());
    append_crash_line(&format!(
        "crash: rust panic thread_id={:?} thread_name={thread_name:?} location={location} payload={}",
        thread.id(),
        panic_payload(panic)
    ));
}

fn panic_payload(panic: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = panic.payload().downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

#[cfg(windows)]
unsafe extern "system" fn unhandled_exception_filter(
    exception_info: *const windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) -> i32 {
    use windows::Win32::System::Diagnostics::Debug::EXCEPTION_EXECUTE_HANDLER;

    log_native_crash(exception_info);
    EXCEPTION_EXECUTE_HANDLER
}

#[cfg(windows)]
fn log_native_crash(
    exception_info: *const windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) {
    if NATIVE_CRASH_LOGGED.swap(true, Ordering::SeqCst) {
        append_crash_line("crash: duplicate native exception while terminating");
        return;
    }

    let Some(exception_info) = (unsafe { exception_info.as_ref() }) else {
        append_crash_line("crash: unhandled native exception exception_info=null");
        return;
    };
    let Some(record) = (unsafe { exception_info.ExceptionRecord.as_ref() }) else {
        append_crash_line("crash: unhandled native exception record=null");
        return;
    };

    let code = record.ExceptionCode.0 as u32;
    let address = record.ExceptionAddress as usize;
    let flags = record.ExceptionFlags;
    let parameter_count = (record.NumberParameters as usize).min(record.ExceptionInformation.len());
    let parameters = &record.ExceptionInformation[..parameter_count];
    let reason = native_exception_reason(code);
    let access = native_exception_access_detail(code, parameters);

    append_crash_line(&format!(
        "crash: unhandled native exception code=0x{code:08x} reason={reason} flags=0x{flags:08x} address=0x{address:x} pid={} thread_id={:?}{access}",
        std::process::id(),
        std::thread::current().id()
    ));
    append_crash_line(&format!(
        "crash: native exception parameters={}",
        exception_parameters(parameters)
    ));
}

#[cfg(windows)]
fn native_exception_access_detail(code: u32, parameters: &[usize]) -> String {
    if code != 0xc000_0005 || parameters.len() < 2 {
        return String::new();
    }

    let operation = match parameters[0] {
        0 => "read",
        1 => "write",
        8 => "execute",
        _ => "unknown-access",
    };
    format!(" access={operation} target=0x{:x}", parameters[1])
}

#[cfg(windows)]
fn exception_parameters(parameters: &[usize]) -> String {
    if parameters.is_empty() {
        return "[]".to_string();
    }

    let mut rendered = String::from("[");
    for (index, parameter) in parameters.iter().enumerate() {
        if index > 0 {
            rendered.push_str(", ");
        }
        rendered.push_str(&format!("0x{parameter:x}"));
    }
    rendered.push(']');
    rendered
}

#[cfg(windows)]
fn native_exception_reason(code: u32) -> &'static str {
    match code {
        0x8000_0003 => "breakpoint",
        0xc000_0005 => "access-violation",
        0xc000_0008 => "invalid-handle",
        0xc000_001d => "illegal-instruction",
        0xc000_0025 => "noncontinuable-exception",
        0xc000_008c => "array-bounds-exceeded",
        0xc000_008d => "floating-point-denormal",
        0xc000_008e => "floating-point-divide-by-zero",
        0xc000_008f => "floating-point-inexact-result",
        0xc000_0090 => "floating-point-invalid-operation",
        0xc000_0091 => "floating-point-overflow",
        0xc000_0092 => "floating-point-stack-check",
        0xc000_0093 => "floating-point-underflow",
        0xc000_0094 => "integer-divide-by-zero",
        0xc000_0095 => "integer-overflow",
        0xc000_0096 => "privileged-instruction",
        0xc000_00fd => "stack-overflow",
        0xc000_0135 => "dll-not-found",
        0xc000_0139 => "entry-point-not-found",
        0xc000_0374 => "heap-corruption",
        0xe06d_7363 => "c-plus-plus-exception",
        _ => "unknown",
    }
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
#[allow(dead_code)] // call sites are Windows-gated; the stub keeps the API total
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
#[allow(dead_code)] // call sites are Windows-gated; the stub keeps the API total
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
#[allow(dead_code)] // call sites are Windows-gated; the stub keeps the API total
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
    append_line_unlocked(path, message);
}

fn append_crash_line(message: &str) {
    let path = LOG_PATH.get().cloned().unwrap_or_else(default_log_path);
    append_line_unlocked(&path, message);
}

fn append_line_unlocked(path: &PathBuf, message: &str) {
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
