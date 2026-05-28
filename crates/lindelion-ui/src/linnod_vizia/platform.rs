use std::{
    env,
    ffi::c_void,
    path::{Path, PathBuf},
    process,
    time::Duration,
};

use rfd::FileDialog;
use vizia::{
    ParentWindow, WindowHandle, WindowScalePolicy,
    icons::{
        ICON_ACTIVITY, ICON_ADJUSTMENTS_HORIZONTAL, ICON_ARROWS_SHUFFLE, ICON_DEVICE_FLOPPY,
        ICON_DOWNLOAD, ICON_FILE_MUSIC, ICON_FOLDER_OPEN, ICON_MINUS, ICON_MUSIC, ICON_PLUS,
        ICON_SETTINGS, ICON_X,
    },
    prelude::*,
    vg,
};

use super::{
    LINNOD_EDITOR_HEIGHT, LINNOD_EDITOR_PARAMETER_BINDING_COUNT, LINNOD_EDITOR_WIDTH,
    LinnodEditorCommand, LinnodEditorCommandRequest, LinnodEditorControlKind,
    LinnodEditorDetectionAlgorithm, LinnodEditorDetectionEdit, LinnodEditorEnvelope,
    LinnodEditorHost, LinnodEditorMarker, LinnodEditorMarkerEdit, LinnodEditorMarkerKind,
    LinnodEditorPadEdit, LinnodEditorPadSummary, LinnodEditorParameterBinding,
    LinnodEditorPatchSummary, LinnodEditorPitchShiftAlgorithm, LinnodEditorPlaybackEdit,
    LinnodEditorPlaybackMode, LinnodEditorSliceEdit, LinnodEditorSliceSummary,
    LinnodEditorSourceStatus, LinnodEditorStatus, LinnodEditorSurfaceSlot, LinnodEditorTelemetry,
    LinnodEditorTriggerMode,
};
use crate::{PadId, WaveformPoint};

#[path = "platform_drop.rs"]
mod platform_drop;

include!("platform_style.rs");
include!("platform_state.rs");
include!("platform_settings.rs");
include!("platform_layout.rs");
include!("platform_playback_controls.rs");
include!("platform_controls.rs");
include!("platform_detection_controls.rs");
include!("platform_text.rs");
include!("platform_waveform.rs");
include!("platform_waveform_geometry.rs");
include!("platform_waveform_drawing.rs");
include!("platform_drawing.rs");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinnodEditorSize {
    pub width: i32,
    pub height: i32,
}

impl Default for LinnodEditorSize {
    fn default() -> Self {
        Self {
            width: LINNOD_EDITOR_WIDTH,
            height: LINNOD_EDITOR_HEIGHT,
        }
    }
}

pub struct LinnodViziaEditor {
    window: WindowHandle,
    drop_target: Option<platform_drop::NativeSourceDropTarget>,
}

impl LinnodViziaEditor {
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LinnodEditorHost,
        size: LinnodEditorSize,
    ) -> Self {
        unsafe {
            host.request_status();
            host.request_telemetry();
        }
        let parent_view = parent as usize;
        let parent = ParentWindow(parent);
        let window = unsafe { build_linnod_application_with_parent(host, size, parent_view) }
            .open_parented(&parent);
        let drop_target = platform_drop::NativeSourceDropTarget::install(&window, host);
        Self {
            window,
            drop_target,
        }
    }
}

impl Drop for LinnodViziaEditor {
    fn drop(&mut self) {
        self.drop_target.take();
        if self.window.is_open() {
            self.window.close();
        }
    }
}
