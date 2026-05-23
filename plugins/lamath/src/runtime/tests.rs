use super::*;
use crate::{
    FilterMode, ModalConfig, ModalPreset, ModulationDestination, ModulationSlot, ModulationSource,
    OutputConfig, ResonatorConfig, ResonatorRouting, assert_no_allocations,
};
use lindelion_dsp_utils::{
    analysis::{assert_all_finite, dft_magnitude_at, peak_abs, rms},
    math::midi_note_to_hz,
};
use lindelion_plugin_shell::{ExpressionStream, ManualExpressionSource};

include!("basic_tests.rs");
include!("pressure_tests.rs");
include!("expression_tests.rs");
include!("test_helpers.rs");
