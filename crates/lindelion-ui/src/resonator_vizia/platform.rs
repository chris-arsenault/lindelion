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
        ICON_ARROW_BACK, ICON_ARROW_FORWARD, ICON_ARROW_MERGE, ICON_ARROWS_SPLIT, ICON_DOWNLOAD,
        ICON_FOLDER_OPEN, ICON_LIBRARY, ICON_PLUS, ICON_REFRESH, ICON_REPEAT, ICON_SETTINGS,
        ICON_TRASH, ICON_X,
    },
    prelude::*,
    vg,
};

use super::{
    RESONATOR_EDITOR_HEIGHT, RESONATOR_EDITOR_PARAMETER_BINDING_COUNT, RESONATOR_EDITOR_WIDTH,
    ResonatorEditorCommandRequest, ResonatorEditorControlKind, ResonatorEditorHost,
    ResonatorEditorParameterBinding, ResonatorEditorPatchSummary, ResonatorEditorSampleSummary,
    ResonatorEditorSlotSummary, ResonatorEditorSurfaceSlot, ResonatorEditorTelemetry,
    ResonatorEditorWaveformPoint,
};
use crate::{
    EditorCommandBus, PadId, UiCommand, command_label,
    vizia_file_dialogs::{
        patch_export_directory_dialog, patch_load_file_dialog, patch_save_file_dialog,
        wav_audio_dialog,
    },
};

#[path = "platform_drop.rs"]
mod platform_drop;

include!("platform_style.rs");
include!("platform_state.rs");
include!("platform_settings.rs");
include!("platform_layout_text.rs");
include!("platform_layout.rs");
include!("platform_bridge.rs");
include!("platform_controls.rs");
include!("platform_drawing.rs");
