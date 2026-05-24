pub(super) fn default_parameter_values() -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = [0.0; VST3_PARAMETER_COUNT];
    for (index, binding) in (0..PARAMETER_BINDING_COUNT)
        .filter_map(parameter_binding_by_index)
        .enumerate()
    {
        values[index] = parameter_default_normalized_value_by_index(index).unwrap_or_else(|| {
            let parameter = binding.info();
            parameter.range.normalize(parameter.range.default)
        }) as f64;
    }
    values[PITCH_BEND_PARAMETER_INDEX] = 0.5;
    values
}

pub(super) fn parameter_values_from_patch(
    patch: &crate::ResonatorSynthPatch,
) -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = default_parameter_values();
    for binding in (0..PARAMETER_BINDING_COUNT).filter_map(parameter_binding_by_index) {
        let parameter = binding.info();
        if let Some(normalized) = patch_parameter_normalized_value(patch, parameter.id.0)
            && let Some(index) = parameter_index(parameter.id.0)
        {
            values[index] = normalized as f64;
        }
    }
    values
}

pub(super) fn parameter_index(id: u32) -> Option<usize> {
    if id == PITCH_BEND_PARAMETER_ID {
        return Some(PITCH_BEND_PARAMETER_INDEX);
    }

    parameter_binding_index(id)
}

fn parameter_by_id(id: u32) -> Option<lindelion_plugin_shell::ParameterInfo> {
    parameter_info(id)
}

pub(super) fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    registry_normalized_parameter_value(id, plain)
        .map(f64::from)
        .unwrap_or(0.0)
}

pub(super) fn pitch_bend_plain_from_normalized(normalized: f64) -> f64 {
    let normalized = if normalized.is_finite() {
        normalized
    } else {
        0.5
    };
    (normalized.clamp(0.0, 1.0) * 2.0 - 1.0) * f64::from(DEFAULT_PITCH_BEND_RANGE_SEMITONES)
}

pub(super) fn pitch_bend_normalized_from_plain(plain: f32) -> f64 {
    let plain = plain.clamp(
        -DEFAULT_PITCH_BEND_RANGE_SEMITONES,
        DEFAULT_PITCH_BEND_RANGE_SEMITONES,
    );
    f64::from((plain / DEFAULT_PITCH_BEND_RANGE_SEMITONES + 1.0) * 0.5)
}

pub(super) fn format_parameter_plain_value(parameter_id: u32, value: f32) -> String {
    crate::parameters::format_parameter_plain_value(parameter_id, value)
}
