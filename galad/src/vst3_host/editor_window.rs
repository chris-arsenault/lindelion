//! Win32 editor window — a top-level host window the plugin's `IPlugView` attaches into. Windows-only;
//! cross-compile-verified. The real open/attach/render is the M5 Windows field check.

use std::cell::Cell;
use std::ffi::c_void;

use vst3::Steinberg::*;
use vst3::{ComPtr, ComRef, ComWrapper};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
    GetClientRect, GetWindowLongPtrW, PostQuitMessage, RegisterClassW, SW_SHOW, SWP_NOMOVE,
    SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, ShowWindow, WINDOW_EX_STYLE, WM_CLOSE,
    WM_DESTROY, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, w};

thread_local! {
    /// Number of open editor windows on this Win32 UI thread.
    static OPEN_WINDOWS: Cell<usize> = const { Cell::new(0) };
    /// Whether the thread-local message loop should exit when its last editor window closes.
    static QUIT_ON_LAST_CLOSE: Cell<bool> = const { Cell::new(false) };
}

use super::editor_frame::HostPlugFrame;
use super::editor_view::prepare;
use super::instance::HostError;

const CLASS_NAME: PCWSTR = w!("GaladEditorWindow");

/// A top-level host window hosting a plugin's `IPlugView`.
pub struct EditorWindow {
    hwnd: HWND,
    view: ComPtr<IPlugView>,
    frame: ComWrapper<HostPlugFrame>,
}

impl EditorWindow {
    /// Create a host window sized to the view, set the host frame, and attach the view.
    pub fn open(
        view: ComPtr<IPlugView>,
        title: &str,
        quit_on_last_close: bool,
    ) -> Result<Self, HostError> {
        unsafe {
            crate::diagnostics::log(format!(
                "editor-window: open begin title={title:?} quit_on_last_close={quit_on_last_close}"
            ));
            QUIT_ON_LAST_CLOSE.with(|quit| quit.set(quit_on_last_close));
            let frame = HostPlugFrame::new();
            let frame_ptr = frame.to_com_ptr::<IPlugFrame>().expect("IPlugFrame");
            crate::diagnostics::log("editor-window: prepare begin");
            let rect = prepare(&view, &frame_ptr)?;
            crate::diagnostics::log(format!(
                "editor-window: prepare done rect=({}, {}, {}, {})",
                rect.left, rect.top, rect.right, rect.bottom
            ));
            let width = (rect.right - rect.left).max(1);
            let height = (rect.bottom - rect.top).max(1);

            crate::diagnostics::log("editor-window: GetModuleHandleW begin");
            let hinstance = GetModuleHandleW(None)
                .map_err(|error| HostError::EditorWindow(error.to_string()))?;
            crate::diagnostics::log("editor-window: GetModuleHandleW done");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            crate::diagnostics::log("editor-window: RegisterClassW begin");
            RegisterClassW(&wc); // benign if already registered
            crate::diagnostics::log("editor-window: RegisterClassW done");

            let mut rc = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            let _ = AdjustWindowRect(&mut rc, WS_OVERLAPPEDWINDOW, false);
            let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
            crate::diagnostics::log(format!(
                "editor-window: CreateWindowExW begin size={}x{}",
                rc.right - rc.left,
                rc.bottom - rc.top
            ));
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                CLASS_NAME,
                PCWSTR(title_w.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                rc.right - rc.left,
                rc.bottom - rc.top,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
            .map_err(|error| HostError::EditorWindow(error.to_string()))?;
            crate::diagnostics::log(format!(
                "editor-window: CreateWindowExW done hwnd=0x{:x}",
                hwnd.0 as isize
            ));

            // Realize the wrapper before `IPlugView::attached`. Some plugin UI toolkits create
            // GPU-backed child windows during attach and expect the parent HWND to be visible.
            crate::diagnostics::log("editor-window: pre-attach ShowWindow begin");
            let _ = ShowWindow(hwnd, SW_SHOW);
            crate::diagnostics::log("editor-window: pre-attach ShowWindow done");
            crate::diagnostics::spawn_window_probe("editor-attach");

            crate::diagnostics::log("editor-window: view.attached begin");
            if view.attached(hwnd.0 as *mut c_void, kPlatformTypeHWND) != kResultOk {
                crate::diagnostics::log("editor-window: view.attached failed");
                let _ = DestroyWindow(hwnd);
                return Err(HostError::EditorUnsupported);
            }
            crate::diagnostics::log("editor-window: view.attached done");
            // Stash the view pointer after attach, then push the current client size once. This
            // avoids sending `onSize` to an unattached view if showing the wrapper produces a
            // synchronous `WM_SIZE`.
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, view.as_ptr() as isize);
            forward_current_size(hwnd, &view);
            crate::diagnostics::log("editor-window: ShowWindow begin");
            let _ = ShowWindow(hwnd, SW_SHOW);
            OPEN_WINDOWS.with(|count| count.set(count.get().saturating_add(1)));
            crate::diagnostics::log("editor-window: ShowWindow done");

            Ok(EditorWindow { hwnd, view, frame })
        }
    }

    /// Apply any pending plugin-requested resize (`IPlugFrame::resizeView`) to the window.
    pub fn apply_pending_resize(&self) {
        if let Some((w, h)) = self.frame.take_requested_size() {
            unsafe {
                let mut rc = RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                };
                let _ = AdjustWindowRect(&mut rc, WS_OVERLAPPEDWINDOW, false);
                let _ = SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
                let mut view_rect = ViewRect {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                };
                let _ = self.view.onSize(&mut view_rect);
            }
        }
    }
}

impl Drop for EditorWindow {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = self.view.removed();
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn forward_current_size(hwnd: HWND, view: &ComPtr<IPlugView>) {
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let mut view_rect = ViewRect {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        };
        let _ = view.onSize(&mut view_rect);
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_SIZE => {
                let view_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut IPlugView;
                if let Some(view) = ComRef::from_raw(view_ptr) {
                    let mut rc = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rc);
                    let mut view_rect = ViewRect {
                        left: rc.left,
                        top: rc.top,
                        right: rc.right,
                        bottom: rc.bottom,
                    };
                    let _ = view.onSize(&mut view_rect);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                OPEN_WINDOWS.with(|count| {
                    let current = count.get();
                    let remaining = current.saturating_sub(1);
                    count.set(remaining);
                    crate::diagnostics::log(format!(
                        "editor-window: WM_DESTROY current={current} remaining={remaining}"
                    ));
                    let should_quit =
                        current > 0 && remaining == 0 && QUIT_ON_LAST_CLOSE.with(|quit| quit.get());
                    if should_quit {
                        crate::diagnostics::log("editor-window: WM_DESTROY PostQuitMessage");
                        PostQuitMessage(0);
                    }
                });
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
