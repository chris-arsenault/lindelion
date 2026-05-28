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

use crate::{
    PadId, UiCommand,
    resonator_vizia::{ResonatorEditorCommandRequest, ResonatorEditorHost},
};

const LAYER_DROP_LEFT: f64 = 26.0;
const LAYER_DROP_TOP_FROM_BOTTOM: f64 = 226.0;
const LAYER_DROP_WIDTH: f64 = 492.0;
const LAYER_DROP_HEIGHT: f64 = 58.0;
const LAYER_DROP_GAP: f64 = 6.0;
const LAYER_COUNT: usize = 4;

pub(super) struct NativeLayerDropTargets {
    views: Vec<Retained<ResonatorLayerDropView>>,
}

impl NativeLayerDropTargets {
    pub(super) fn install(window: &WindowHandle, host: ResonatorEditorHost) -> Option<Self> {
        autoreleasepool(|_| unsafe { Self::install_inner(window, host) })
    }

    unsafe fn install_inner(window: &WindowHandle, host: ResonatorEditorHost) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let parent = unsafe { ns_view_from_window(window)? };
        let types = crate::vizia_clipboard::file_pasteboard_types();
        let mut views = Vec::with_capacity(LAYER_COUNT);
        for index in 0..LAYER_COUNT {
            let frame = layer_drop_frame(parent, index);
            let slot = PadId((index + 1) as u8);
            let view = ResonatorLayerDropView::new(mtm, frame, DropIvars { host, slot });
            view.registerForDraggedTypes(&types);
            parent.addSubview(view.as_super());
            views.push(view);
        }
        Some(Self { views })
    }
}

impl Drop for NativeLayerDropTargets {
    fn drop(&mut self) {
        for view in self.views.drain(..) {
            view.removeFromSuperview();
        }
    }
}

#[derive(Clone, Copy)]
struct DropIvars {
    host: ResonatorEditorHost,
    slot: PadId,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DropIvars]
    struct ResonatorLayerDropView;

    unsafe impl NSObjectProtocol for ResonatorLayerDropView {}

    unsafe impl NSDraggingDestination for ResonatorLayerDropView {
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
            let DropIvars { host, slot } = *self.ivars();
            unsafe {
                host.handle_command(ResonatorEditorCommandRequest {
                    command: UiCommand::LoadExcitationSlot(slot),
                    patch_save_path: None,
                    patch_load_path: None,
                    patch_export_directory: None,
                    sample_path: Some(&path),
                    selected_library_sample: None,
                });
                host.request_telemetry();
            }
            Bool::YES
        }
    }
);

impl ResonatorLayerDropView {
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

fn layer_drop_frame(parent: &NSView, index: usize) -> NSRect {
    let bounds = parent.bounds();
    let card_width =
        (LAYER_DROP_WIDTH - LAYER_DROP_GAP * (LAYER_COUNT as f64 - 1.0)) / LAYER_COUNT as f64;
    let x = LAYER_DROP_LEFT + index as f64 * (card_width + LAYER_DROP_GAP);
    let top = (bounds.size.height - LAYER_DROP_TOP_FROM_BOTTOM).max(0.0);
    let y = if parent.isFlipped() {
        top
    } else {
        (bounds.size.height - top - LAYER_DROP_HEIGHT).max(0.0)
    };
    NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(card_width, LAYER_DROP_HEIGHT),
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

fn sample_path_from_drag(sender: &ProtocolObject<dyn NSDraggingInfo>) -> Option<PathBuf> {
    let pasteboard = sender.draggingPasteboard();
    crate::vizia_clipboard::file_path_from_pasteboard(&pasteboard)
        .filter(|path| super::is_resonator_supported_audio_path(path))
        .or_else(|| {
            let path = super::transient_exciter_audio_path();
            crate::vizia_clipboard::read_file_contents_from_pasteboard(&pasteboard, &path)
                .filter(|path| super::is_resonator_supported_audio_path(path))
        })
}
