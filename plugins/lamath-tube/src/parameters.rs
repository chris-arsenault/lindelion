use lindelion_plugin_shell::{ParameterId, ParameterInfo, ParameterRange};

use crate::patch::{OUTPUT_GAIN_MAX_DB, OUTPUT_GAIN_MIN_DB, TubePatch};

pub const PRESSURE_ID: u32 = 1;
pub const REED_STIFFNESS_ID: u32 = 2;
pub const EMBOUCHURE_ID: u32 = 3;
pub const BRIGHTNESS_ID: u32 = 4;
pub const DAMPING_ID: u32 = 5;
pub const BELL_ID: u32 = 6;
pub const OUTPUT_GAIN_ID: u32 = 7;
pub const HUMANIZE_ID: u32 = 8;
pub const REGISTER_BREAK_ID: u32 = 9;
pub const PHRASING_ID: u32 = 10;
pub const VIBRATO_ID: u32 = 11;

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
    ParameterInfo::stepped(
        REGISTER_BREAK_ID,
        "Register Break",
        "MIDI",
        ParameterRange::linear(48.0, 96.0, 69.0),
        48,
    ),
    ParameterInfo::continuous(
        PHRASING_ID,
        "Phrasing",
        "",
        ParameterRange::linear(0.0, 1.0, 0.0),
    ),
    ParameterInfo::continuous(
        VIBRATO_ID,
        "Vibrato",
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
        ParameterRange::linear(OUTPUT_GAIN_MIN_DB, OUTPUT_GAIN_MAX_DB, -8.0),
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
        PHRASING_ID => Some(patch.phrasing),
        VIBRATO_ID => Some(patch.vibrato),
        REGISTER_BREAK_ID => Some(patch.register_break_note),
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
        PHRASING_ID => patch.phrasing = unit(plain),
        VIBRATO_ID => patch.vibrato = unit(plain),
        REGISTER_BREAK_ID => {
            patch.register_break_note = if plain.is_finite() {
                plain.clamp(48.0, 96.0).round()
            } else {
                69.0
            }
        }
        BRIGHTNESS_ID => patch.brightness = unit(plain),
        DAMPING_ID => patch.damping = unit(plain),
        BELL_ID => patch.bell = unit(plain),
        OUTPUT_GAIN_ID => {
            patch.output_gain_db = if plain.is_finite() {
                plain.clamp(OUTPUT_GAIN_MIN_DB, OUTPUT_GAIN_MAX_DB)
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
    match id {
        OUTPUT_GAIN_ID => format!("{plain:+.1}"),
        REGISTER_BREAK_ID => format!("{:.0}", plain.round()),
        _ => format!("{plain:.2}"),
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
