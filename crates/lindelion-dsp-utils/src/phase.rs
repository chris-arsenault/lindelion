pub fn principal_angle(phase: f64) -> f64 {
    (phase + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

pub fn principal_angle_f32(phase: f32) -> f32 {
    (phase + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_angle_wraps_to_symmetric_pi_range() {
        assert!((principal_angle(0.0) - 0.0).abs() < f64::EPSILON);
        assert!((principal_angle(std::f64::consts::TAU) - 0.0).abs() < 0.000_001);
        assert!(principal_angle(3.5).abs() <= std::f64::consts::PI);
        assert!(principal_angle(-3.5).abs() <= std::f64::consts::PI);
    }

    #[test]
    fn f32_and_f64_wrapping_match() {
        for phase in [-9.0, -3.5, -1.0, 0.0, 1.0, 3.5, 9.0] {
            assert!(
                (principal_angle(phase) as f32 - principal_angle_f32(phase as f32)).abs()
                    < 0.000_001
            );
        }
    }
}
