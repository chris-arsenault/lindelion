pub const A4_HZ: f32 = 440.0;
pub const A4_MIDI_NOTE: f32 = 69.0;
pub const DENORMAL_THRESHOLD: f32 = 1.0e-20;

pub fn semitones_to_ratio(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0)
}

pub fn midi_note_to_hz(note: f32) -> f32 {
    A4_HZ * semitones_to_ratio(note - A4_MIDI_NOTE)
}

pub fn snap_to_zero(value: f32) -> f32 {
    if !value.is_finite() || value.abs() < DENORMAL_THRESHOLD {
        0.0
    } else {
        value
    }
}

pub fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

pub fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).clamp(min, max)
}

pub fn is_finite_normalized(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_note_69_is_a4() {
        assert!((midi_note_to_hz(69.0) - 440.0).abs() < 0.000_01);
    }

    #[test]
    fn twelve_semitones_doubles_frequency() {
        assert!((semitones_to_ratio(12.0) - 2.0).abs() < 0.000_01);
    }

    #[test]
    fn denormals_snap_to_zero() {
        assert_eq!(snap_to_zero(1.0e-30), 0.0);
        assert_eq!(snap_to_zero(-1.0e-30), 0.0);
        assert_eq!(snap_to_zero(1.0e-10), 1.0e-10);
    }

    #[test]
    fn non_finite_values_snap_to_zero() {
        assert_eq!(snap_to_zero(f32::NAN), 0.0);
        assert_eq!(snap_to_zero(f32::INFINITY), 0.0);
        assert_eq!(snap_to_zero(f32::NEG_INFINITY), 0.0);
    }
}
