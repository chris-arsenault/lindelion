use std::{cell::Cell, ffi::c_void, ptr};

#[cfg(any(target_os = "macos", target_os = "windows", test))]
use std::ffi::{CStr, c_char};

use vst3::{Class, Steinberg::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedSizePlugViewSize {
    pub width: i32,
    pub height: i32,
}

impl FixedSizePlugViewSize {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    pub fn view_rect(self) -> ViewRect {
        ViewRect {
            left: 0,
            top: 0,
            right: self.width.max(0),
            bottom: self.height.max(0),
        }
    }

    fn clamp_rect(self, rect: ViewRect) -> ViewRect {
        ViewRect {
            left: rect.left,
            top: rect.top,
            right: rect.left + self.width.max(0),
            bottom: rect.top + self.height.max(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlugViewKeyEvent {
    pub key: char16,
    pub key_code: int16,
    pub modifiers: int16,
}

impl PlugViewKeyEvent {
    pub const fn new(key: char16, key_code: int16, modifiers: int16) -> Self {
        Self {
            key,
            key_code,
            modifiers,
        }
    }

    pub fn is_plain_paste_shortcut(self) -> bool {
        self.is_v_key()
            && self.has_primary_paste_modifier()
            && !self.has_modifier(KeyModifier_::kAlternateKey)
            && !self.has_modifier(KeyModifier_::kShiftKey)
    }

    fn is_v_key(self) -> bool {
        matches!(self.key, KEY_V_LOWER | KEY_V_UPPER)
            || matches!(self.key_code, MACOS_KEY_CODE_V | WINDOWS_VIRTUAL_KEY_V)
    }

    fn has_primary_paste_modifier(self) -> bool {
        self.has_modifier(KeyModifier_::kCommandKey) || self.has_modifier(KeyModifier_::kControlKey)
    }

    // `KeyModifier` is `u32` on Linux/macOS but `i32` on the Windows MSVC target; normalize with
    // `as u32` (identity for these modifier flags). The allow keeps Linux clippy green.
    #[allow(clippy::unnecessary_cast)]
    fn has_modifier(self, modifier: KeyModifier) -> bool {
        (u32::from(self.modifiers as u16) & modifier as u32) != 0
    }
}

const KEY_V_LOWER: char16 = b'v' as u16;
const KEY_V_UPPER: char16 = b'V' as u16;
const MACOS_KEY_CODE_V: int16 = 0x09;
const WINDOWS_VIRTUAL_KEY_V: int16 = 0x56;

pub trait FixedSizePlugViewDelegate {
    /// # Safety
    /// `parent` must be a valid platform view pointer for the host platform and `size` must
    /// describe the attached view bounds.
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult;

    /// # Safety
    /// The host must call this only while the delegated platform view is still owned by the plug
    /// view.
    unsafe fn removed(&self) -> tresult {
        kResultOk
    }

    /// # Safety
    /// The host must call this only for key events targeted at this plug view. Implementations
    /// must return `kResultTrue` only when the key was actually handled.
    unsafe fn key_down(&self, _event: PlugViewKeyEvent) -> tresult {
        kResultFalse
    }
}

pub struct FixedSizePlugView<D> {
    delegate: D,
    frame: Cell<*mut IPlugFrame>,
    size: Cell<ViewRect>,
    fixed_size: FixedSizePlugViewSize,
}

impl<D> FixedSizePlugView<D> {
    pub fn new(delegate: D, fixed_size: FixedSizePlugViewSize) -> Self {
        Self {
            delegate,
            frame: Cell::new(ptr::null_mut()),
            size: Cell::new(fixed_size.view_rect()),
            fixed_size,
        }
    }
}

impl<D: 'static> Class for FixedSizePlugView<D> {
    type Interfaces = (IPlugView,);
}

impl<D: FixedSizePlugViewDelegate> IPlugViewTrait for FixedSizePlugView<D> {
    unsafe fn isPlatformTypeSupported(&self, r#type: FIDString) -> tresult {
        #[cfg(target_os = "macos")]
        {
            if platform_type_is(r#type, b"NSView") {
                kResultTrue
            } else {
                kResultFalse
            }
        }

        #[cfg(target_os = "windows")]
        {
            if platform_type_is(r#type, b"HWND") {
                kResultTrue
            } else {
                kResultFalse
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = r#type;
            kResultFalse
        }
    }

    unsafe fn attached(&self, parent: *mut c_void, r#type: FIDString) -> tresult {
        if parent.is_null() {
            return kInvalidArgument;
        }
        if self.isPlatformTypeSupported(r#type) != kResultTrue {
            return kResultFalse;
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.delegate.attached(parent, self.size.get())
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = parent;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        self.delegate.removed()
    }

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kNotImplemented
    }

    unsafe fn onKeyDown(&self, key: char16, keyCode: int16, modifiers: int16) -> tresult {
        self.delegate
            .key_down(PlugViewKeyEvent::new(key, keyCode, modifiers))
    }

    unsafe fn onKeyUp(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        *size = self.size.get();
        kResultOk
    }

    unsafe fn onSize(&self, newSize: *mut ViewRect) -> tresult {
        if newSize.is_null() {
            return kInvalidArgument;
        }
        let size = self.fixed_size.clamp_rect(*newSize);
        self.size.set(size);
        kResultOk
    }

    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setFrame(&self, frame: *mut IPlugFrame) -> tresult {
        self.frame.set(frame);
        kResultOk
    }

    unsafe fn canResize(&self) -> tresult {
        kResultFalse
    }

    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        if rect.is_null() {
            return kInvalidArgument;
        }
        *rect = self.fixed_size.view_rect();
        kResultOk
    }
}

/// True when the host-supplied platform-type C string equals `expected` (e.g. `b"NSView"` on
/// macOS, `b"HWND"` on Windows). Compiled wherever a platform branch or a test needs it.
///
/// # Safety
/// `platform`, when non-null, must be a valid NUL-terminated C string.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
unsafe fn platform_type_is(platform: FIDString, expected: &[u8]) -> bool {
    !platform.is_null() && CStr::from_ptr(platform as *const c_char).to_bytes() == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_type_is_matches_nul_terminated_c_string() {
        let hwnd = b"HWND\0";
        let nsview = b"NSView\0";
        unsafe {
            assert!(platform_type_is(hwnd.as_ptr() as FIDString, b"HWND"));
            assert!(!platform_type_is(hwnd.as_ptr() as FIDString, b"NSView"));
            assert!(platform_type_is(nsview.as_ptr() as FIDString, b"NSView"));
            assert!(!platform_type_is(
                ptr::null::<c_char>() as FIDString,
                b"HWND"
            ));
        }
    }
}
