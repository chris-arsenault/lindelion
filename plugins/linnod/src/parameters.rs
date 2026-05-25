use lindelion_dsp_utils::math::finite_clamp;
use lindelion_plugin_shell::{
    ParameterBinding as RegistryParameterBinding, ParameterEditorBindingProjection,
    ParameterFormatter, ParameterId, ParameterInfo, ParameterPatchPath, ParameterRange,
    ParameterRegistry,
};
use lindelion_ui::linnod_vizia::{
    LinnodEditorControlKind, LinnodEditorParameterBinding, LinnodEditorSurfaceSlot,
};

use crate::patch::{
    DEFAULT_MASTER_GAIN_DB, DEFAULT_TUNING_REFERENCE_HZ, LinnodPatch, MAX_MASTER_GAIN_DB,
    MAX_TUNING_REFERENCE_HZ, MIN_MASTER_GAIN_DB, MIN_TUNING_REFERENCE_HZ,
};

pub const MASTER_GAIN_PARAMETER_ID: u32 = 1;
pub const DETECTION_SENSITIVITY_PARAMETER_ID: u32 = 2;
pub const TUNING_REFERENCE_PARAMETER_ID: u32 = 3;

pub type ParameterBinding = RegistryParameterBinding<
    ParameterPath,
    ParameterApplyKind,
    (),
    (),
    ParameterFormatter,
    EditorParameterMetadata,
>;

pub const PARAMETER_REGISTRY: ParameterRegistry<ParameterBinding> =
    ParameterRegistry::new(PARAMETER_BINDINGS);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterApplyKind {
    Output,
    Analysis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterPath {
    MasterGain,
    DetectionSensitivity,
    TuningReference,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorParameterMetadata {
    slot: LinnodEditorSurfaceSlot,
    label: &'static str,
    control: LinnodEditorControlKind,
}

impl EditorParameterMetadata {
    const fn new(
        slot: LinnodEditorSurfaceSlot,
        label: &'static str,
        control: LinnodEditorControlKind,
    ) -> Self {
        Self {
            slot,
            label,
            control,
        }
    }
}

impl ParameterEditorBindingProjection<LinnodEditorParameterBinding> for EditorParameterMetadata {
    fn project_editor_binding(self, id: ParameterId) -> LinnodEditorParameterBinding {
        LinnodEditorParameterBinding::new(id.0, self.slot, self.label, self.control)
    }
}

impl ParameterPatchPath<LinnodPatch> for ParameterPath {
    fn plain_value(self, patch: &LinnodPatch) -> f32 {
        match self {
            Self::MasterGain => patch.output.master_gain_db,
            Self::DetectionSensitivity => patch.detection.sensitivity,
            Self::TuningReference => patch.tuning.reference_hz,
        }
    }

    fn apply_plain(self, patch: &mut LinnodPatch, value: f32) {
        match self {
            Self::MasterGain => {
                patch.output.master_gain_db = finite_clamp(
                    value,
                    MIN_MASTER_GAIN_DB,
                    MAX_MASTER_GAIN_DB,
                    DEFAULT_MASTER_GAIN_DB,
                );
            }
            Self::DetectionSensitivity => {
                patch.detection.sensitivity = finite_clamp(value, 0.0, 1.0, 0.5);
            }
            Self::TuningReference => {
                patch.tuning.reference_hz = finite_clamp(
                    value,
                    MIN_TUNING_REFERENCE_HZ,
                    MAX_TUNING_REFERENCE_HZ,
                    DEFAULT_TUNING_REFERENCE_HZ,
                );
            }
        }
    }
}

lindelion_plugin_shell::define_parameter_bindings! {
    binding: ParameterBinding;
    parameters: pub const PARAMETERS;
    bindings: pub const PARAMETER_BINDINGS;
    defaults {
        runtime: (),
        smoothing: None::<()>,
    }

    ParameterInfo::continuous(
        MASTER_GAIN_PARAMETER_ID,
        "Master Gain",
        "dB",
        ParameterRange::linear(MIN_MASTER_GAIN_DB, MAX_MASTER_GAIN_DB, DEFAULT_MASTER_GAIN_DB),
    ) => {
        path: ParameterPath::MasterGain,
        apply: ParameterApplyKind::Output,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            LinnodEditorSurfaceSlot::MasterGain,
            "Master",
            LinnodEditorControlKind::Slider { width: 168.0 },
        )),
    },
    ParameterInfo::continuous(
        DETECTION_SENSITIVITY_PARAMETER_ID,
        "Detection Sensitivity",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
    ) => {
        path: ParameterPath::DetectionSensitivity,
        apply: ParameterApplyKind::Analysis,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            LinnodEditorSurfaceSlot::DetectionSensitivity,
            "Sensitivity",
            LinnodEditorControlKind::Knob,
        )),
    },
    ParameterInfo::continuous(
        TUNING_REFERENCE_PARAMETER_ID,
        "Tuning Reference",
        "Hz",
        ParameterRange::linear(
            MIN_TUNING_REFERENCE_HZ,
            MAX_TUNING_REFERENCE_HZ,
            DEFAULT_TUNING_REFERENCE_HZ,
        ),
    ) => {
        path: ParameterPath::TuningReference,
        apply: ParameterApplyKind::Analysis,
        format: ParameterFormatter::Plain,
        editor: Some(EditorParameterMetadata::new(
            LinnodEditorSurfaceSlot::TuningReference,
            "Reference",
            LinnodEditorControlKind::Slider { width: 168.0 },
        )),
    },
}

pub fn parameter_binding(id: u32) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding(id)
}

pub fn parameter_binding_by_index(index: usize) -> Option<&'static ParameterBinding> {
    PARAMETER_REGISTRY.binding_by_index(index)
}

pub const PARAMETER_BINDING_COUNT: usize = PARAMETER_REGISTRY.len();

pub fn parameter_binding_index(id: u32) -> Option<usize> {
    PARAMETER_REGISTRY.binding_index(id)
}

pub fn parameter_info(id: u32) -> Option<ParameterInfo> {
    PARAMETER_REGISTRY.info(id)
}

pub fn normalized_parameter_value(id: u32, plain: f32) -> Option<f32> {
    PARAMETER_REGISTRY.normalized_value(id, plain)
}

pub fn denormalized_parameter_value(id: u32, normalized: f32) -> Option<f32> {
    PARAMETER_REGISTRY.denormalized_value(id, normalized)
}

pub fn format_parameter_plain_value(id: u32, plain: f32) -> String {
    PARAMETER_REGISTRY.formatted_plain_value(id, plain)
}

pub fn editor_parameter_bindings() -> impl Iterator<Item = LinnodEditorParameterBinding> {
    PARAMETER_REGISTRY.projected_editor_bindings()
}

pub fn apply_parameter_normalized(
    patch: &mut LinnodPatch,
    id: u32,
    normalized: f32,
) -> Option<ParameterApplyKind> {
    PARAMETER_REGISTRY
        .apply_normalized(patch, id, normalized)
        .map(|outcome| outcome.apply_kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_are_backed_by_shared_registry() {
        assert_eq!(PARAMETER_REGISTRY.len(), PARAMETERS.len());
        assert_eq!(
            PARAMETER_REGISTRY
                .binding(MASTER_GAIN_PARAMETER_ID)
                .unwrap()
                .path(),
            ParameterPath::MasterGain
        );
    }

    #[test]
    fn host_parameters_update_patch_level_fields() {
        let mut patch = LinnodPatch::default();

        assert_eq!(
            apply_parameter_normalized(&mut patch, MASTER_GAIN_PARAMETER_ID, 1.0),
            Some(ParameterApplyKind::Output)
        );
        assert_eq!(
            apply_parameter_normalized(&mut patch, DETECTION_SENSITIVITY_PARAMETER_ID, 0.25),
            Some(ParameterApplyKind::Analysis)
        );
        assert_eq!(
            apply_parameter_normalized(&mut patch, TUNING_REFERENCE_PARAMETER_ID, 0.5),
            Some(ParameterApplyKind::Analysis)
        );

        assert_eq!(patch.output.master_gain_db, MAX_MASTER_GAIN_DB);
        assert_eq!(patch.detection.sensitivity, 0.25);
        assert_eq!(patch.tuning.reference_hz, DEFAULT_TUNING_REFERENCE_HZ);
    }

    #[test]
    fn slice_fields_are_not_host_parameters() {
        assert!(PARAMETERS.iter().all(|parameter| {
            !parameter.name.contains("Slice")
                && !parameter.name.contains("Attack")
                && !parameter.name.contains("Pan")
                && !parameter.name.contains("Pitch")
        }));
    }
}
