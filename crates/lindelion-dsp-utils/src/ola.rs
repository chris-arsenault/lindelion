pub fn accumulate_squared_window_at(window: &[f64], frame_start: isize, weights: &mut [f64]) {
    let output_len = weights.len() as isize;
    for (index, window_value) in window.iter().copied().enumerate() {
        let position = frame_start.saturating_add(index as isize);
        if (0..output_len).contains(&position) {
            weights[position as usize] += window_value * window_value;
        }
    }
}

pub fn steady_state_squared_window_sum(window: &[f64], hop_size: usize) -> Vec<f64> {
    let mut weights = vec![0.0; window.len()];
    if window.is_empty() {
        return weights;
    }

    let hop_size = hop_size.max(1);
    let hop = hop_size as isize;
    let len = window.len() as isize;
    let first_start = (1 - len).div_euclid(hop) * hop;
    let last_start = len - 1;
    for frame_start in (first_start..=last_start).step_by(hop_size) {
        accumulate_squared_window_at(window, frame_start, &mut weights);
    }
    weights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_squared_window_at_frame_start() {
        let mut weights = vec![0.0; 5];

        accumulate_squared_window_at(&[0.5, 1.0, 0.25], 1, &mut weights);

        assert_eq!(weights, vec![0.0, 0.25, 1.0, 0.0625, 0.0]);
    }

    #[test]
    fn accumulates_negative_frame_start_into_visible_region() {
        let mut weights = vec![0.0; 4];

        accumulate_squared_window_at(&[0.5, 1.0, 0.25], -1, &mut weights);

        assert_eq!(weights, vec![1.0, 0.0625, 0.0, 0.0]);
    }

    #[test]
    fn steady_state_sum_accumulates_all_overlapping_hops() {
        let weights = steady_state_squared_window_sum(&[1.0; 4], 2);

        assert_eq!(weights, vec![2.0, 2.0, 2.0, 2.0]);
    }
}
