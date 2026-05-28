pub fn hann(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    let phase = std::f32::consts::TAU * index as f32 / (len - 1) as f32;
    0.5 - 0.5 * phase.cos()
}

pub fn hann_f64(index: usize, len: usize) -> f64 {
    if len <= 1 {
        return 1.0;
    }
    let phase = std::f64::consts::TAU * index as f64 / (len - 1) as f64;
    0.5 - 0.5 * phase.cos()
}

pub fn sqrt_hann_f64(index: usize, len: usize) -> f64 {
    hann_f64(index, len).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_windows_are_zero_at_edges_and_one_near_center() {
        assert_eq!(hann(0, 9), 0.0);
        assert_eq!(hann(8, 9), 0.0);
        assert!((hann(4, 9) - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn f64_hann_matches_f32_shape() {
        for index in 0..16 {
            assert!((hann(index, 16) as f64 - hann_f64(index, 16)).abs() < 0.000_001);
        }
    }

    #[test]
    fn sqrt_hann_squares_back_to_hann() {
        for index in 0..32 {
            let value = sqrt_hann_f64(index, 32);
            assert!((value * value - hann_f64(index, 32)).abs() < 0.000_001);
        }
    }
}
