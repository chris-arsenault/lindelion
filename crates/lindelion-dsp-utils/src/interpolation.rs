pub fn linear(samples: &[f32], index: f32) -> f32 {
    linear_f64(samples, index as f64)
}

pub fn linear_f64(samples: &[f32], index: f64) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    if index <= 0.0 {
        return samples[0];
    }

    let max_index = samples.len() - 1;
    if index >= max_index as f64 {
        return samples[max_index];
    }

    let base = index.floor() as usize;
    let frac = index - base as f64;
    (samples[base] as f64 + (samples[base + 1] as f64 - samples[base] as f64) * frac) as f32
}

pub fn cubic_f64(samples: &[f32], index: f64) -> f32 {
    if samples.len() < 4 {
        return linear_f64(samples, index);
    }

    if index <= 0.0 {
        return samples[0];
    }

    let max_index = samples.len() - 1;
    if index >= max_index as f64 {
        return samples[max_index];
    }

    let base = index.floor() as usize;
    let frac = index - base as f64;
    let y0 = samples[base.saturating_sub(1)] as f64;
    let y1 = samples[base] as f64;
    let y2 = samples[(base + 1).min(max_index)] as f64;
    let y3 = samples[(base + 2).min(max_index)] as f64;
    let frac2 = frac * frac;
    let frac3 = frac2 * frac;

    (0.5 * (2.0 * y1
        + (y2 - y0) * frac
        + (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3) * frac2
        + (3.0 * y1 - y0 - 3.0 * y2 + y3) * frac3)) as f32
}

pub fn linear_wrapped(samples: &[f32], index: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let len = samples.len() as f32;
    let wrapped = if index.is_finite() {
        let wrapped = index.rem_euclid(len);
        if wrapped >= len { 0.0 } else { wrapped }
    } else {
        0.0
    };
    let base = wrapped.floor() as usize;
    let next = (base + 1) % samples.len();
    let frac = wrapped - base as f32;

    samples[base] + (samples[next] - samples[base]) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_interpolates_between_neighbors() {
        let samples = [0.0, 10.0, 20.0];
        assert_eq!(linear(&samples, 0.5), 5.0);
        assert_eq!(linear(&samples, 1.25), 12.5);
    }

    #[test]
    fn linear_f64_preserves_fractional_position_at_large_indices() {
        let base = 4_194_304;
        let mut samples = vec![0.0; base + 2];
        samples[base + 1] = 1.0;

        assert_eq!(linear_f64(&samples, base as f64 + 0.25), 0.25);
    }

    #[test]
    fn cubic_f64_interpolates_smooth_curve() {
        let samples = [0.0, 1.0, 4.0, 9.0, 16.0];

        assert!((cubic_f64(&samples, 2.5) - 6.25).abs() < 0.000_001);
    }

    #[test]
    fn linear_clamps_non_wrapped_edges() {
        let samples = [1.0, 2.0];
        assert_eq!(linear(&samples, -10.0), 1.0);
        assert_eq!(linear(&samples, 10.0), 2.0);
    }

    #[test]
    fn linear_wrapped_wraps_negative_indices() {
        let samples = [0.0, 10.0, 20.0, 30.0];
        assert_eq!(linear_wrapped(&samples, -1.0), 30.0);
        assert_eq!(linear_wrapped(&samples, 3.5), 15.0);
    }

    #[test]
    fn linear_wrapped_handles_boundary_and_non_finite_indices() {
        let samples = [0.0, 10.0, 20.0, 30.0];
        assert_eq!(linear_wrapped(&samples, 4.0), 0.0);
        assert_eq!(linear_wrapped(&samples, f32::NAN), 0.0);
    }
}
