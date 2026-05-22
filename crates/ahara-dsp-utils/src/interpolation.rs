pub fn linear(samples: &[f32], index: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    if index <= 0.0 {
        return samples[0];
    }

    let max_index = samples.len() - 1;
    if index >= max_index as f32 {
        return samples[max_index];
    }

    let base = index.floor() as usize;
    let frac = index - base as f32;
    samples[base] + (samples[base + 1] - samples[base]) * frac
}

pub fn linear_wrapped(samples: &[f32], index: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let len = samples.len() as f32;
    let wrapped = index.rem_euclid(len);
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
}
