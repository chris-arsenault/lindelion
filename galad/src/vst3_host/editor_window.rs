//! Win32 editor window — a top-level host window the plugin's `IPlugView` attaches into. Windows-only;
//! cross-compile-verified. The real open/attach/render is the M5 Windows field check.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Number of open editor windows; the standalone `galad editor` message loop exits when it hits 0.
static OPEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);

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
    pub fn open(view: ComPtr<IPlugView>, title: &str) -> Result<Self, HostError> {
        unsafe {
            let frame = HostPlugFrame::new();
            let frame_ptr = frame.to_com_ptr::<IPlugFrame>().expect("IPlugFrame");
            let rect = prepare(&view, &frame_ptr)?;
            let width = (rect.right - rect.left).max(1);
            let height = (rect.bottom - rect.top).max(1);

            let hinstance = GetModuleHandleW(None)
                .map_err(|error| HostError::EditorWindow(error.to_string()))?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc); // benign if already registered

            let mut rc = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            let _ = AdjustWindowRect(&mut rc, WS_OVERLAPPEDWINDOW, false);
            let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
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

            // Stash the view pointer so `wndproc` can forward `WM_SIZE` → `onSize`.
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, view.as_ptr() as isize);

            if view.attached(hwnd.0 as *mut c_void, kPlatformTypeHWND) != kResultOk {
                let _ = DestroyWindow(hwnd);
                return Err(HostError::EditorUnsupported);
            }
            let _ = ShowWindow(hwnd, SW_SHOW);
            OPEN_WINDOWS.fetch_add(1, Ordering::Relaxed);

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
                if OPEN_WINDOWS.fetch_sub(1, Ordering::Relaxed) == 1 {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
