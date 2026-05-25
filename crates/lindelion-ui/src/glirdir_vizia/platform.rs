use std::{ffi::c_void, path::PathBuf, time::Duration};

use vizia::{
    ParentWindow, WindowHandle, WindowScalePolicy,
    icons::{
        ICON_ACTIVITY, ICON_DEVICE_FLOPPY, ICON_DOWNLOAD, ICON_MICROPHONE, ICON_PLAYER_PLAY,
        ICON_PLAYER_STOP, ICON_REPEAT, ICON_TRASH,
    },
    prelude::*,
    vg,
};

use super::{
    GLIRDIR_EDITOR_HEIGHT, GLIRDIR_EDITOR_PARAMETER_BINDING_COUNT, GLIRDIR_EDITOR_WIDTH,
    GlirdirEditorAnalysisStatus, GlirdirEditorCaptureState, GlirdirEditorCommand,
    GlirdirEditorControlKind, GlirdirEditorHost, GlirdirEditorLibraryStatus, GlirdirEditorMidiDrag,
    GlirdirEditorParameterBinding, GlirdirEditorPianoRollNote, GlirdirEditorPreview,
    GlirdirEditorSize, GlirdirEditorStatus, GlirdirEditorSurfaceSlot, GlirdirEditorWaveformPreview,
};
use crate::WaveformPoint;

#[path = "platform_drag.rs"]
mod platform_drag;

include!("platform_state.rs");
include!("platform_layout.rs");
include!("platform_controls.rs");
include!("platform_drawing.rs");
