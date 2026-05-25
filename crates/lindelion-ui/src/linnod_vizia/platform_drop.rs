use std::path::PathBuf;

use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{Bool, NSObjectProtocol, ProtocolObject},
};
use objc2_app_kit::{NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use vizia::WindowHandle;

use crate::linnod_vizia::{LinnodEditorCommand, LinnodEditorHost};

const SOURCE_DROP_X: f64 = 26.0;
const SOURCE_DROP_TOP: f64 = 122.0;
const SOURCE_DROP_WIDTH: f64 = 294.0;
const SOURCE_DROP_HEIGHT: f64 = 164.0;

pub(super) struct NativeSourceDropTarget {
    view: Retained<LinnodSourceDropView>,
}

impl NativeSourceDropTarget {
    pub(super) fn install(window: &WindowHandle, host: LinnodEditorHost) -> Option<Self> {
        autoreleasepool(|_| unsafe { Self::install_inner(window, host) })
    }

    unsafe fn install_inner(window: &WindowHandle, host: LinnodEditorHost) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let parent = unsafe { ns_view_from_window(window)? };
        let frame = source_drop_frame(parent);
        let view = LinnodSourceDropView::new(mtm, frame, host);
        let types = crate::vizia_clipboard::file_pasteboard_types();
        view.registerForDraggedTypes(&types);
        parent.addSubview(view.as_super());
        Some(Self { view })
    }
}

impl Drop for NativeSourceDropTarget {
    fn drop(&mut self) {
        self.view.removeFromSuperview();
    }
}

#[derive(Clone, Copy)]
struct DropIvars {
    host: LinnodEditorHost,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DropIvars]
    struct LinnodSourceDropView;

    unsafe impl NSObjectProtocol for LinnodSourceDropView {}

    unsafe impl NSDraggingDestination for LinnodSourceDropView {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            drag_admission(sender)
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            drag_admission(sender)
        }

        #[unsafe(method(prepareForDragOperation:))]
        fn prepare_for_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> Bool {
            Bool::new(accepts_file_drag(sender))
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> Bool {
            let Some(path) = source_path_from_drag(sender) else {
                return Bool::NO;
            };
            let host = self.ivars().host;
            super::send_command(
                host,
                LinnodEditorCommand::LoadSource,
                Some(&path),
                None,
                None,
                None,
            );
            unsafe {
                host.request_status();
                host.request_telemetry();
            }
            Bool::YES
        }
    }
);

impl LinnodSourceDropView {
    fn new(mtm: MainThreadMarker, frame: NSRect, host: LinnodEditorHost) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DropIvars { host });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

unsafe fn ns_view_from_window(window: &WindowHandle) -> Option<&NSView> {
    let handle = window.raw_window_handle();
    let RawWindowHandle::AppKit(handle) = handle else {
        return None;
    };
    if handle.ns_view.is_null() {
        return None;
    }
    unsafe { (handle.ns_view as *const NSView).as_ref() }
}

fn source_drop_frame(parent: &NSView) -> NSRect {
    let bounds = parent.bounds();
    let y = if parent.isFlipped() {
        SOURCE_DROP_TOP
    } else {
        (bounds.size.height - SOURCE_DROP_TOP - SOURCE_DROP_HEIGHT).max(0.0)
    };
    NSRect::new(
        NSPoint::new(SOURCE_DROP_X, y),
        NSSize::new(SOURCE_DROP_WIDTH, SOURCE_DROP_HEIGHT),
    )
}

fn drag_admission(sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
    if accepts_file_drag(sender) {
        NSDragOperation::Copy
    } else {
        NSDragOperation::None
    }
}

fn accepts_file_drag(sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
    let pasteboard = sender.draggingPasteboard();
    crate::vizia_clipboard::pasteboard_has_file_source(&pasteboard)
}

fn source_path_from_drag(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Option<PathBuf> {
    let pasteboard = sender.draggingPasteboard();
    crate::vizia_clipboard::file_path_from_pasteboard(&pasteboard)
        .filter(|path| super::is_linnod_supported_audio_path(path))
        .or_else(|| {
            let path = super::transient_source_audio_path();
            crate::vizia_clipboard::read_file_contents_from_pasteboard(&pasteboard, &path)
                .filter(|path| super::is_linnod_supported_audio_path(path))
        })
}
