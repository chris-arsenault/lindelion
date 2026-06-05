use lindelion_plugin_shell::{ParameterId, ParameterInfo, ParameterRange};

use crate::patch::TubePatch;

pub const PRESSURE_ID: u32 = 1;
pub const REED_STIFFNESS_ID: u32 = 2;
pub const EMBOUCHURE_ID: u32 = 3;
pub const BRIGHTNESS_ID: u32 = 4;
pub const DAMPING_ID: u32 = 5;
pub const BELL_ID: u32 = 6;
pub const OUTPUT_GAIN_ID: u32 = 7;
pub const HUMANIZE_ID: u32 = 8;

pub const PARAMETERS: &[ParameterInfo] = &[
    ParameterInfo::continuous(
        PRESSURE_ID,
        "Pressure",
        "",
        ParameterRange::linear(0.0, 1.0, 0.58),
    ),
    ParameterInfo::continuous(
        REED_STIFFNESS_ID,
        "Reed",
        "",
        ParameterRange::linear(0.0, 1.0, 0.48),
    ),
    ParameterInfo::continuous(
        EMBOUCHURE_ID,
        "Embouchure",
        "",
        ParameterRange::linear(0.0, 1.0, 0.52),
    ),
    ParameterInfo::continuous(
        HUMANIZE_ID,
        "Humanize",
        "",
        ParameterRange::linear(0.0, 1.0, 0.0),
    ),
    ParameterInfo::continuous(
        BRIGHTNESS_ID,
        "Brightness",
        "",
        ParameterRange::linear(0.0, 1.0, 0.52),
    ),
    ParameterInfo::continuous(
        DAMPING_ID,
        "Damping",
        "",
        ParameterRange::linear(0.0, 1.0, 0.28),
    ),
    ParameterInfo::continuous(BELL_ID, "Bell", "", ParameterRange::linear(0.0, 1.0, 1.0)),
    ParameterInfo::continuous(
        OUTPUT_GAIN_ID,
        "Output",
        "dB",
        ParameterRange::linear(-24.0, 12.0, -8.0),
    ),
];

pub const PARAMETER_COUNT: usize = PARAMETERS.len();

pub fn parameter_by_id(id: u32) -> Option<ParameterInfo> {
    PARAMETERS
        .iter()
        .copied()
        .find(|parameter| parameter.id.0 == id)
}

pub fn parameter_by_index(index: usize) -> Option<ParameterInfo> {
    PARAMETERS.get(index).copied()
}

pub fn parameter_index(id: u32) -> Option<usize> {
    PARAMETERS.iter().position(|parameter| parameter.id.0 == id)
}

pub fn normalized_value(patch: &TubePatch, id: u32) -> Option<f32> {
    let parameter = parameter_by_id(id)?;
    Some(parameter.range.normalize(plain_value(patch, id)?))
}

pub fn plain_value(patch: &TubePatch, id: u32) -> Option<f32> {
    match id {
        PRESSURE_ID => Some(patch.pressure),
        REED_STIFFNESS_ID => Some(patch.reed_stiffness),
        EMBOUCHURE_ID => Some(patch.embouchure),
        HUMANIZE_ID => Some(patch.humanize),
        BRIGHTNESS_ID => Some(patch.brightness),
        DAMPING_ID => Some(patch.damping),
        BELL_ID => Some(patch.bell),
        OUTPUT_GAIN_ID => Some(patch.output_gain_db),
        _ => None,
    }
}

pub fn apply_normalized(patch: &mut TubePatch, id: ParameterId, normalized: f32) -> bool {
    let Some(parameter) = parameter_by_id(id.0) else {
        return false;
    };
    apply_plain(patch, id.0, parameter.range.denormalize(normalized));
    true
}

pub fn apply_plain(patch: &mut TubePatch, id: u32, plain: f32) -> bool {
    match id {
        PRESSURE_ID => patch.pressure = unit(plain),
        REED_STIFFNESS_ID => patch.reed_stiffness = unit(plain),
        EMBOUCHURE_ID => patch.embouchure = unit(plain),
        HUMANIZE_ID => patch.humanize = unit(plain),
        BRIGHTNESS_ID => patch.brightness = unit(plain),
        DAMPING_ID => patch.damping = unit(plain),
        BELL_ID => patch.bell = unit(plain),
        OUTPUT_GAIN_ID => {
            patch.output_gain_db = if plain.is_finite() {
                plain.clamp(-24.0, 12.0)
            } else {
                -8.0
            }
        }
        _ => return false,
    }
    true
}

pub fn default_normalized_values() -> [f64; PARAMETER_COUNT] {
    std::array::from_fn(|index| {
        let parameter = PARAMETERS[index];
        f64::from(parameter.range.normalize(parameter.range.default))
    })
}

pub fn normalized_values_from_patch(patch: &TubePatch) -> [f64; PARAMETER_COUNT] {
    std::array::from_fn(|index| {
        let parameter = PARAMETERS[index];
        normalized_value(patch, parameter.id.0)
            .map(f64::from)
            .unwrap_or_else(|| f64::from(parameter.range.normalize(parameter.range.default)))
    })
}

pub fn format_plain_value(id: u32, plain: f32) -> String {
    if id == OUTPUT_GAIN_ID {
        format!("{plain:+.1}")
    } else {
        format!("{plain:.2}")
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
