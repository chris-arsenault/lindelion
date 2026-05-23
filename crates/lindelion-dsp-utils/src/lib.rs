pub mod analysis;
pub mod delay;
pub mod envelope;
pub mod filters;
pub mod interpolation;
pub mod math;
pub mod params;
pub mod smoothing;

pub fn db_to_gain(db: f32) -> f32 {
    if db.is_finite() {
        10.0_f32.powf(db / 20.0)
    } else if db.is_sign_negative() {
        0.0
    } else {
        1.0
    }
}

pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}

pub fn soft_saturate(input: f32, drive: f32) -> f32 {
    if !input.is_finite() {
        return 0.0;
    }

    let drive = drive.clamp(0.0, 1.0);
    if drive <= f32::EPSILON {
        return input;
    }

    let drive_gain = 1.0 + drive * 8.0;
    let biased = input * drive_gain + input.abs() * 0.015;
    biased.tanh() / drive_gain.sqrt()
}

pub fn equal_power_pan(mono: f32, pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (mono * angle.cos(), mono * angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_gain_roundtrip() {
        let gain = db_to_gain(-6.0);
        assert!((gain_to_db(gain) + 6.0).abs() < 0.000_01);
    }

    #[test]
    fn equal_power_pan_preserves_power() {
        for pan in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let (left, right) = equal_power_pan(1.0, pan);
            let power = left * left + right * right;
            assert!((power - 1.0).abs() < 0.000_01);
        }
    }

    #[test]
    fn saturator_is_finite_for_hot_inputs() {
        for input in [-100.0, -10.0, -1.0, 0.0, 1.0, 10.0, 100.0] {
            assert!(soft_saturate(input, 1.0).is_finite());
        }
    }

    #[test]
    fn saturator_zero_drive_is_transparent() {
        for input in [-10.0, -1.0, 0.0, 1.0, 10.0] {
            assert_eq!(soft_saturate(input, 0.0), input);
        }
    }

    #[test]
    fn saturator_high_drive_is_gain_compensated_for_nominal_sine() {
        let mut dry_sum = 0.0_f32;
        let mut wet_sum = 0.0_f32;

        for index in 0..4096 {
            let input = 0.3 * (std::f32::consts::TAU * index as f32 / 4096.0).sin();
            dry_sum += input * input;
            let wet = soft_saturate(input, 1.0);
            wet_sum += wet * wet;
        }

        let dry_rms = (dry_sum / 4096.0).sqrt();
        let wet_rms = (wet_sum / 4096.0).sqrt();
        assert!(
            wet_rms > dry_rms * 0.5,
            "wet_rms={wet_rms}, dry_rms={dry_rms}"
        );
        assert!(
            wet_rms < dry_rms * 2.0,
            "wet_rms={wet_rms}, dry_rms={dry_rms}"
        );
    }

    #[test]
    fn saturator_high_drive_keeps_mild_asymmetry() {
        let symmetry_error = soft_saturate(0.05, 1.0) + soft_saturate(-0.05, 1.0);

        assert!(symmetry_error.abs() > 0.000_1);
        assert!(symmetry_error.abs() < 0.02);
    }
}
