use std::{cell::Cell, ffi::c_void, ptr};

#[cfg(target_os = "macos")]
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
            if is_ns_view_platform(r#type) {
                kResultTrue
            } else {
                kResultFalse
            }
        }

        #[cfg(not(target_os = "macos"))]
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

        #[cfg(target_os = "macos")]
        {
            self.delegate.attached(parent, self.size.get())
        }

        #[cfg(not(target_os = "macos"))]
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

    unsafe fn onKeyDown(&self, _key: char16, _keyCode: int16, _modifiers: int16) -> tresult {
        kNotImplemented
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

#[cfg(target_os = "macos")]
unsafe fn is_ns_view_platform(platform: FIDString) -> bool {
    !platform.is_null() && CStr::from_ptr(platform as *const c_char).to_bytes() == b"NSView"
}
