use lindelion_plugin_shell::{ParameterId, ParameterInfo, ParameterRange};

use crate::patch::CymbalPatch;

pub const MATERIAL_ID: u32 = 1;
pub const SIZE_ID: u32 = 2;
pub const DAMPING_ID: u32 = 3;
pub const TENSION_ID: u32 = 4;
pub const STRIKE_POSITION_ID: u32 = 5;
pub const PICKUP_SPREAD_ID: u32 = 6;
pub const OUTPUT_GAIN_ID: u32 = 7;

pub const PARAMETERS: &[ParameterInfo] = &[
    ParameterInfo::continuous(
        MATERIAL_ID,
        "Material",
        "",
        ParameterRange::linear(0.0, 1.0, 0.65),
    ),
    ParameterInfo::continuous(SIZE_ID, "Size", "", ParameterRange::linear(0.0, 1.0, 0.74)),
    ParameterInfo::continuous(
        DAMPING_ID,
        "Damping",
        "",
        ParameterRange::linear(0.0, 1.0, 0.28),
    ),
    ParameterInfo::continuous(
        TENSION_ID,
        "Tension",
        "",
        ParameterRange::linear(0.0, 1.0, 0.55),
    ),
    ParameterInfo::continuous(
        STRIKE_POSITION_ID,
        "Strike",
        "",
        ParameterRange::linear(0.0, 1.0, 0.42),
    ),
    ParameterInfo::continuous(
        PICKUP_SPREAD_ID,
        "Spread",
        "",
        ParameterRange::linear(0.0, 1.0, 0.42),
    ),
    ParameterInfo::continuous(
        OUTPUT_GAIN_ID,
        "Output",
        "dB",
        ParameterRange::linear(-24.0, 12.0, -3.0),
    ),
];

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

pub fn normalized_value(patch: &CymbalPatch, id: u32) -> Option<f32> {
    let parameter = parameter_by_id(id)?;
    Some(parameter.range.normalize(plain_value(patch, id)?))
}

pub fn plain_value(patch: &CymbalPatch, id: u32) -> Option<f32> {
    match id {
        MATERIAL_ID => Some(patch.material),
        SIZE_ID => Some(patch.size),
        DAMPING_ID => Some(patch.damping),
        TENSION_ID => Some(patch.tension),
        STRIKE_POSITION_ID => Some(patch.strike_position),
        PICKUP_SPREAD_ID => Some(patch.pickup_spread),
        OUTPUT_GAIN_ID => Some(patch.output_gain_db),
        _ => None,
    }
}

pub fn apply_normalized(patch: &mut CymbalPatch, id: ParameterId, normalized: f32) -> bool {
    let Some(parameter) = parameter_by_id(id.0) else {
        return false;
    };
    apply_plain(patch, id.0, parameter.range.denormalize(normalized));
    true
}

pub fn apply_plain(patch: &mut CymbalPatch, id: u32, plain: f32) -> bool {
    match id {
        MATERIAL_ID => patch.material = unit(plain),
        SIZE_ID => patch.size = unit(plain),
        DAMPING_ID => patch.damping = unit(plain),
        TENSION_ID => patch.tension = unit(plain),
        STRIKE_POSITION_ID => patch.strike_position = unit(plain),
        PICKUP_SPREAD_ID => patch.pickup_spread = unit(plain),
        OUTPUT_GAIN_ID => {
            patch.output_gain_db = if plain.is_finite() {
                plain.clamp(-24.0, 12.0)
            } else {
                -3.0
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

pub fn normalized_values_from_patch(patch: &CymbalPatch) -> [f64; PARAMETER_COUNT] {
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

pub const PARAMETER_COUNT: usize = PARAMETERS.len();

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
