//! Saturation waveshaper: a gain-normalized asymmetric tanh soft-clip.
//!
//! Ports the intent of hot-mic's saturation core — an asymmetric `tanh` with split curvature
//! (`kPos`/`kNeg`) so positive and negative halves saturate differently. Asymmetry adds even
//! harmonics ("warmth"); symmetric drive adds odd harmonics.
//!
//! The shaper is `tanh(k·x)/k`, so the small-signal gain is exactly 1 for any drive: `drive`
//! controls curvature only, never loudness, and the shape is continuous as `drive → 0`
//! (the un-normalized `tanh(k·x)` form had small-signal gain `k`, which made every drive knob a
//! volume knob and jumped ~−26 dB between drive 0 and drive ε).

/// Asymmetric tanh soft-clip with unity small-signal gain. `drive` (>= 0) sets curvature;
/// `asymmetry` in `[-1, 1]` biases the positive vs negative half's curvature — nonzero asymmetry
/// adds even harmonics without skewing the linear gain of either half.
pub fn soft_clip(input: f32, drive: f32, asymmetry: f32) -> f32 {
    let drive = drive.max(0.0);
    if drive <= f32::EPSILON {
        return input;
    }
    let asym = asymmetry.clamp(-1.0, 1.0);
    let k = if input >= 0.0 {
        drive * (1.0 + asym)
    } else {
        drive * (1.0 - asym)
    };
    if k <= f32::EPSILON {
        return input;
    }
    (k * input).tanh() / k
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::windowed_dft_magnitude_at;

    const SR: f32 = 48_000.0;
    const F0: f32 = 1_000.0;

    fn sine(amp: f32) -> Vec<f32> {
        (0..4_096)
            .map(|n| amp * (std::f32::consts::TAU * F0 * n as f32 / SR).sin())
            .collect()
    }

    #[test]
    fn small_signal_gain_is_unity_for_any_drive_and_asymmetry() {
        // The drive knob must control curvature only, never loudness: for a small input the
        // shaper is transparent at every drive (including the drive -> 0 limit), on both halves.
        for drive in [0.0_f32, 0.01, 0.5, 2.5, 5.0] {
            for asym in [0.0_f32, 0.3, -0.5, 1.0] {
                for input in [0.001_f32, -0.001] {
                    let output = soft_clip(input, drive, asym);
                    assert!(
                        (output - input).abs() < 1.0e-5,
                        "drive {drive} asym {asym}: {input} -> {output}"
                    );
                }
            }
        }
    }

    #[test]
    fn symmetric_soft_clip_is_monotonic_and_bounded() {
        let mut prev = f32::NEG_INFINITY;
        let mut x = -2.0;
        while x <= 2.0 {
            let y = soft_clip(x, 4.0, 0.0);
            assert!(y >= prev, "not monotonic at {x}");
            assert!(y.abs() <= 1.0, "unbounded at {x}: {y}");
            prev = y;
            x += 0.01;
        }
    }

    #[test]
    fn symmetric_drive_adds_odd_harmonics() {
        let clean = sine(0.8);
        let driven: Vec<f32> = clean.iter().map(|&s| soft_clip(s, 6.0, 0.0)).collect();
        let h3_clean = windowed_dft_magnitude_at(&clean, SR, 3.0 * F0);
        let h3_driven = windowed_dft_magnitude_at(&driven, SR, 3.0 * F0);
        assert!(
            h3_driven > h3_clean + 0.01,
            "3rd harmonic should rise: {h3_clean} -> {h3_driven}"
        );
    }

    #[test]
    fn asymmetry_adds_even_harmonics() {
        let clean = sine(0.8);
        let symmetric: Vec<f32> = clean.iter().map(|&s| soft_clip(s, 6.0, 0.0)).collect();
        let asymmetric: Vec<f32> = clean.iter().map(|&s| soft_clip(s, 6.0, 0.5)).collect();
        let h2_sym = windowed_dft_magnitude_at(&symmetric, SR, 2.0 * F0);
        let h2_asym = windowed_dft_magnitude_at(&asymmetric, SR, 2.0 * F0);
        assert!(
            h2_asym > h2_sym + 0.01,
            "asymmetry should add 2nd harmonic: {h2_sym} -> {h2_asym}"
        );
    }
}
