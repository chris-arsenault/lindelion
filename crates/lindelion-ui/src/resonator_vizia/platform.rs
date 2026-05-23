use std::{ffi::c_void, time::Duration};

use rfd::FileDialog;
use vizia::{
    ParentWindow, WindowHandle, WindowScalePolicy,
    icons::{
        ICON_ACTIVITY, ICON_ADJUSTMENTS_HORIZONTAL, ICON_DOWNLOAD, ICON_FILTER, ICON_FOLDER_OPEN,
        ICON_LIBRARY, ICON_ROUTE, ICON_TRASH, ICON_VOLUME_2, ICON_WAVE_SINE,
    },
    prelude::*,
    vg,
};

use super::{
    RESONATOR_EDITOR_HEIGHT, RESONATOR_EDITOR_PARAMETER_BINDING_COUNT, RESONATOR_EDITOR_WIDTH,
    ResonatorEditorCommandRequest, ResonatorEditorControlKind, ResonatorEditorDirectories,
    ResonatorEditorHost, ResonatorEditorParameterBinding, ResonatorEditorPatchSummary,
    ResonatorEditorSampleSummary, ResonatorEditorSlotSummary, ResonatorEditorSurfaceSlot,
    ResonatorEditorTelemetry, ResonatorEditorWaveformPoint,
};
use crate::{EditorCommandBus, PadId, UiCommand, command_label};

include!("platform_state.rs");
include!("platform_layout.rs");
include!("platform_controls.rs");
include!("platform_drawing.rs");
