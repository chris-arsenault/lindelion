pub const CAPTURE_BARS_PARAMETER_ID: u32 = 1;
pub const SYNC_MODE_PARAMETER_ID: u32 = 2;
pub const COUNT_IN_PARAMETER_ID: u32 = 3;
pub const CONFIDENCE_PARAMETER_ID: u32 = 10;
pub const ONSET_SENSITIVITY_PARAMETER_ID: u32 = 11;
pub const MIN_NOTE_PARAMETER_ID: u32 = 12;
pub const ROOT_PARAMETER_ID: u32 = 20;
pub const SCALE_PARAMETER_ID: u32 = 21;
pub const SNAP_PARAMETER_ID: u32 = 22;
pub const GRID_PARAMETER_ID: u32 = 23;
pub const TIMING_STRENGTH_PARAMETER_ID: u32 = 24;
pub const VELOCITY_AMOUNT_PARAMETER_ID: u32 = 25;
pub const AUDITION_VOLUME_PARAMETER_ID: u32 = 30;

mod codecs;
mod registry;

pub use registry::PARAMETERS;

pub(crate) use registry::{
    PARAMETER_BINDING_COUNT, PARAMETER_REGISTRY, ParameterApplyKind, apply_parameter_normalized,
    denormalized_parameter_value, dispatch_parameter_normalized, editor_parameter_bindings,
    format_parameter_plain_value, normalized_parameter_value, parameter_binding,
    parameter_binding_by_index, parameter_binding_index, parameter_info,
};

#[cfg(test)]
use crate::patch::GlirdirPatch;
#[cfg(test)]
use codecs::{
    CaptureBars, CountInBars, RootNoteParameter, ScaleParameter, SnapModeParameter,
    SyncModeParameter, TimingGridParameter,
};
#[cfg(test)]
use lindelion_midi::Scale;
#[cfg(test)]
use lindelion_plugin_shell::{ParameterCodec, ParameterRange};
#[cfg(test)]
use lindelion_ui::glirdir_vizia::GlirdirEditorSurfaceSlot;
#[cfg(test)]
use registry::PARAMETER_BINDINGS;

#[cfg(test)]
mod tests;
