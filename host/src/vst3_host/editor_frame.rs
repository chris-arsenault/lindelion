//! Host `IPlugFrame` — the callback a plugin uses to ask the host to resize the editor window.
//! `resizeView` records the requested size; the Windows window code (Step 5) reads it and applies it
//! to the `HWND`. Single-threaded (the editor lives on the control/UI thread).

use std::cell::Cell;

use vst3::{Class, ComWrapper, Steinberg::*};

/// A host frame that records the plugin's most recent resize request.
pub struct HostPlugFrame {
    requested: Cell<Option<(i32, i32)>>,
}

impl HostPlugFrame {
    /// A new frame, wrapped for COM.
    pub fn new() -> ComWrapper<HostPlugFrame> {
        ComWrapper::new(HostPlugFrame {
            requested: Cell::new(None),
        })
    }

    /// The most recently requested `(width, height)`, if any.
    pub fn requested_size(&self) -> Option<(i32, i32)> {
        self.requested.get()
    }

    /// Take (and clear) the most recent resize request.
    pub fn take_requested_size(&self) -> Option<(i32, i32)> {
        self.requested.take()
    }
}

impl Class for HostPlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for HostPlugFrame {
    unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if new_size.is_null() {
            return kInvalidArgument;
        }
        let rect = *new_size;
        self.requested
            .set(Some((rect.right - rect.left, rect.bottom - rect.top)));
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::*;

    #[test]
    fn resize_view_records_requested_size() {
        let frame = HostPlugFrame::new();
        let iface = frame.to_com_ptr::<IPlugFrame>().expect("IPlugFrame");

        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 640,
            bottom: 480,
        };
        let result = unsafe { iface.resizeView(ptr::null_mut(), &mut rect) };

        assert_eq!(result, kResultOk);
        assert_eq!(frame.requested_size(), Some((640, 480)));
    }
}
