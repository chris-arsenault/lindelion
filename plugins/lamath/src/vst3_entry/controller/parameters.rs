pub(super) fn default_parameter_values() -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = [0.0; VST3_PARAMETER_COUNT];
    for (index, parameter) in PARAMETERS.iter().enumerate() {
        values[index] = parameter.range.normalize(parameter.range.default) as f64;
    }
    values[PITCH_BEND_PARAMETER_INDEX] = 0.5;
    values
}

pub(super) fn parameter_values_from_patch(
    patch: &crate::ResonatorSynthPatch,
) -> [f64; VST3_PARAMETER_COUNT] {
    let mut values = default_parameter_values();
    for binding in (0..PARAMETERS.len()).filter_map(parameter_binding_by_index) {
        let parameter = binding.info();
        if let Some(plain) = patch_parameter_plain_value(patch, parameter.id.0)
            && let Some(index) = parameter_index(parameter.id.0)
        {
            values[index] = parameter.range.normalize(plain) as f64;
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

fn parameter_by_id(id: u32) -> Option<&'static lindelion_plugin_shell::ParameterInfo> {
    parameter_binding(id).map(|binding| {
        let info = binding.info();
        PARAMETERS
            .iter()
            .find(|parameter| parameter.id == info.id)
            .expect("binding info should be mirrored in PARAMETERS")
    })
}

pub(super) fn normalized_parameter_value(id: u32, plain: f32) -> f64 {
    parameter_binding(id)
        .map(|binding| binding.info().range.normalize(plain) as f64)
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
    parameter_binding(parameter_id)
        .map(|binding| binding.format_plain_value(value))
        .unwrap_or_else(|| format_plain_value(value))
}

fn format_plain_value(value: f32) -> String {
    if value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}
