use lindelion_plugin_shell::{ParameterId, ParameterInfo, ParameterRange};

use crate::patch::StringPatch;

pub const BRIGHTNESS_ID: u32 = 1;
pub const DAMPING_ID: u32 = 2;
pub const STIFFNESS_ID: u32 = 3;
pub const STRIKE_POSITION_ID: u32 = 4;
pub const PICKUP_POSITION_ID: u32 = 5;
pub const BODY_BALANCE_ID: u32 = 6;
pub const OUTPUT_GAIN_ID: u32 = 7;
pub const HUMANIZE_ID: u32 = 8;
pub const PHRASING_ID: u32 = 9;
pub const VIBRATO_ID: u32 = 10;

pub const PARAMETERS: &[ParameterInfo] = &[
    ParameterInfo::continuous(
        BRIGHTNESS_ID,
        "Brightness",
        "",
        ParameterRange::linear(0.0, 1.0, 0.62),
    ),
    ParameterInfo::continuous(
        DAMPING_ID,
        "Damping",
        "",
        ParameterRange::linear(0.0, 1.0, 0.34),
    ),
    ParameterInfo::continuous(
        STIFFNESS_ID,
        "Stiffness",
        "",
        ParameterRange::linear(0.0, 1.0, 0.72),
    ),
    ParameterInfo::continuous(
        STRIKE_POSITION_ID,
        "Strike",
        "",
        ParameterRange::linear(0.0, 1.0, 0.36),
    ),
    ParameterInfo::continuous(
        PICKUP_POSITION_ID,
        "Pickup",
        "",
        ParameterRange::linear(0.0, 1.0, 0.82),
    ),
    ParameterInfo::continuous(
        BODY_BALANCE_ID,
        "Body mix",
        "",
        ParameterRange::linear(0.0, 1.0, 0.38),
    ),
    ParameterInfo::continuous(
        OUTPUT_GAIN_ID,
        "Output",
        "dB",
        ParameterRange::linear(-24.0, 12.0, -8.0),
    ),
    ParameterInfo::continuous(
        HUMANIZE_ID,
        "Humanize",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
    ),
    ParameterInfo::continuous(
        PHRASING_ID,
        "Phrasing",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
    ),
    ParameterInfo::continuous(
        VIBRATO_ID,
        "Vibrato",
        "",
        ParameterRange::linear(0.0, 1.0, 0.5),
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

pub fn plain_value(patch: &StringPatch, id: u32) -> Option<f32> {
    match id {
        BRIGHTNESS_ID => Some(patch.brightness),
        DAMPING_ID => Some(patch.damping),
        STIFFNESS_ID => Some(patch.stiffness),
        STRIKE_POSITION_ID => Some(patch.strike_position),
        PICKUP_POSITION_ID => Some(patch.pickup_position),
        BODY_BALANCE_ID => Some(patch.body_balance),
        OUTPUT_GAIN_ID => Some(patch.output_gain_db),
        HUMANIZE_ID => Some(patch.humanize),
        PHRASING_ID => Some(patch.phrasing),
        VIBRATO_ID => Some(patch.vibrato),
        _ => None,
    }
}

pub fn normalized_value(patch: &StringPatch, id: u32) -> Option<f32> {
    let parameter = parameter_by_id(id)?;
    Some(parameter.range.normalize(plain_value(patch, id)?))
}

pub fn apply_normalized(patch: &mut StringPatch, id: ParameterId, normalized: f32) -> bool {
    let Some(parameter) = parameter_by_id(id.0) else {
        return false;
    };
    apply_plain(patch, id.0, parameter.range.denormalize(normalized))
}

pub fn apply_plain(patch: &mut StringPatch, id: u32, plain: f32) -> bool {
    match id {
        BRIGHTNESS_ID => patch.brightness = unit(plain),
        DAMPING_ID => patch.damping = unit(plain),
        STIFFNESS_ID => patch.stiffness = unit(plain),
        STRIKE_POSITION_ID => patch.strike_position = unit(plain),
        PICKUP_POSITION_ID => patch.pickup_position = unit(plain),
        BODY_BALANCE_ID => patch.body_balance = unit(plain),
        OUTPUT_GAIN_ID => {
            patch.output_gain_db = if plain.is_finite() {
                plain.clamp(-24.0, 12.0)
            } else {
                -8.0
            };
        }
        HUMANIZE_ID => patch.humanize = unit(plain),
        PHRASING_ID => patch.phrasing = unit(plain),
        VIBRATO_ID => patch.vibrato = unit(plain),
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

pub fn normalized_values_from_patch(patch: &StringPatch) -> [f64; PARAMETER_COUNT] {
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
