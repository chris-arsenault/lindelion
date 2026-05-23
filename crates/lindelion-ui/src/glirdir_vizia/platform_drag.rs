use std::{ffi::c_void, path::Path};

use objc2::{MainThreadMarker, msg_send, rc::autoreleasepool, runtime::ProtocolObject};
use objc2_app_kit::{NSApplication, NSEvent, NSPasteboard, NSPasteboardWriting, NSView};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};

pub(super) unsafe fn start_file_drag(parent: *mut c_void, path: &Path) -> bool {
    if parent.is_null() || !path.exists() {
        return false;
    }

    autoreleasepool(|_| unsafe { start_file_drag_inner(parent, path) })
}

pub(super) unsafe fn copy_file_url_to_pasteboard(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    autoreleasepool(|_| copy_file_url_to_pasteboard_inner(path))
}

unsafe fn start_file_drag_inner(parent: *mut c_void, path: &Path) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    let Some(event) = app.currentEvent() else {
        return false;
    };
    let Some(view) = (unsafe { (parent as *const NSView).as_ref() }) else {
        return false;
    };

    start_appkit_file_drag(view, &event, &ns_path_string(path))
}

fn start_appkit_file_drag(view: &NSView, event: &NSEvent, path: &NSString) -> bool {
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0));
    unsafe { msg_send![view, dragFile: path, fromRect: rect, slideBack: true, event: event] }
}

fn copy_file_url_to_pasteboard_inner(path: &Path) -> bool {
    let path = ns_path_string(path);
    let url = NSURL::fileURLWithPath(&path);
    let url = ProtocolObject::<dyn NSPasteboardWriting>::from_ref(&*url);
    let objects = NSArray::from_slice(&[url]);
    let pasteboard = NSPasteboard::generalPasteboard();

    pasteboard.clearContents();
    pasteboard.writeObjects(&objects)
}

fn ns_path_string(path: &Path) -> objc2::rc::Retained<NSString> {
    NSString::from_str(path.to_string_lossy().as_ref())
}
