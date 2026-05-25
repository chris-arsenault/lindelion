use std::{ffi::c_void, path::Path, time::Duration};

use rfd::FileDialog;
use vizia::{
    ParentWindow, WindowHandle, WindowScalePolicy,
    icons::{ICON_ACTIVITY, ICON_DOWNLOAD, ICON_FOLDER_OPEN, ICON_LIBRARY, ICON_TRASH},
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

include!("platform_state.rs");
include!("platform_layout.rs");
include!("platform_bridge.rs");
include!("platform_controls.rs");
include!("platform_drawing.rs");
