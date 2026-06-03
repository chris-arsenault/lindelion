use std::{path::PathBuf, sync::Arc};

use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{Bool, NSObjectProtocol, ProtocolObject},
};
use objc2_app_kit::{NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use vizia::WindowHandle;

use crate::audio_file_slot::{AudioFileSlotId, AudioFileSlotListSurface, is_supported_audio_file};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioDropGrid {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub gap: f64,
    pub slot_count: usize,
}

pub struct NativeAudioFileDropTargets {
    views: Vec<Retained<AudioFileDropView>>,
}

impl NativeAudioFileDropTargets {
    pub fn install(
        window: &WindowHandle,
        surface: Arc<dyn AudioFileSlotListSurface>,
        grid: AudioDropGrid,
    ) -> Option<Self> {
        autoreleasepool(|_| unsafe { Self::install_inner(window, surface, grid) })
    }

    unsafe fn install_inner(
        window: &WindowHandle,
        surface: Arc<dyn AudioFileSlotListSurface>,
        grid: AudioDropGrid,
    ) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let parent = unsafe { ns_view_from_window(window)? };
        let types = crate::vizia_clipboard::file_pasteboard_types();
        let count = grid.slot_count.max(1);
        let mut views = Vec::with_capacity(count);
        for index in 0..count {
            let frame = drop_frame(parent, grid, index);
            let view = AudioFileDropView::new(
                mtm,
                frame,
                DropIvars {
                    surface: Arc::clone(&surface),
                    slot: AudioFileSlotId(index),
                },
            );
            view.registerForDraggedTypes(&types);
            parent.addSubview(view.as_super());
            views.push(view);
        }
        Some(Self { views })
    }
}

impl Drop for NativeAudioFileDropTargets {
    fn drop(&mut self) {
        for view in self.views.drain(..) {
            view.removeFromSuperview();
        }
    }
}

struct DropIvars {
    surface: Arc<dyn AudioFileSlotListSurface>,
    slot: AudioFileSlotId,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DropIvars]
    struct AudioFileDropView;

    unsafe impl NSObjectProtocol for AudioFileDropView {}

    unsafe impl NSDraggingDestination for AudioFileDropView {
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
            let Some(path) = sample_path_from_drag(sender) else {
                return Bool::NO;
            };
            let ivars = self.ivars();
            ivars.surface.select_slot(ivars.slot);
            ivars.surface.load_audio_file(ivars.slot, &path);
            Bool::YES
        }
    }
);

impl AudioFileDropView {
    fn new(mtm: MainThreadMarker, frame: NSRect, ivars: DropIvars) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ivars);
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

fn drop_frame(parent: &NSView, grid: AudioDropGrid, index: usize) -> NSRect {
    let count = grid.slot_count.max(1);
    let bounds = parent.bounds();
    let slot_width = (grid.width - grid.gap * (count as f64 - 1.0)) / count as f64;
    let x = grid.left + index as f64 * (slot_width + grid.gap);
    let y = if parent.isFlipped() {
        grid.top
    } else {
        (bounds.size.height - grid.top - grid.height).max(0.0)
    };
    NSRect::new(NSPoint::new(x, y), NSSize::new(slot_width, grid.height))
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

fn sample_path_from_drag(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Option<PathBuf> {
    let pasteboard = sender.draggingPasteboard();
    crate::vizia_clipboard::file_path_from_pasteboard(&pasteboard)
        .filter(|path| is_supported_audio_file(path))
        .or_else(|| {
            let path = std::env::temp_dir().join("lindelion-dropped-audio.wav");
            crate::vizia_clipboard::read_file_contents_from_pasteboard(&pasteboard, &path)
                .filter(|path| is_supported_audio_file(path))
        })
}
