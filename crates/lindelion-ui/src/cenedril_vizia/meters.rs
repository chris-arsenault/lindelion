//! Platform-neutral readout DTOs and value→fill / value→text mappings for Cenedril's level/LUFS
//! meters and analysis-signal panel.
//!
//! The plugin fills the neutral [`MeterReadout`] / [`SignalReadout`] (no realtime types cross the UI
//! boundary); the Windows Vizia panel turns each field into a `0.0..=1.0` bar fraction and a
//! formatted string via the `*_reading` functions, feeding [`crate::vizia_meter::meter_row`]. These
//! are pure and `make ci`-tested; the display ranges are the M5 tunables. `lindelion-ui` has no
//! `dsp-utils` dependency, so the dB math lives here.

/// Level/loudness readout the plugin fills from its `MeterCell` (linear `peak`/`rms`, `crest` ratio,
/// and the three BS.1770-4 LUFS readings).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MeterReadout {
    pub peak: f32,
    pub rms: f32,
    pub crest: f32,
    pub lufs_momentary: f32,
    pub lufs_short: f32,
    pub lufs_integrated: f32,
}

/// Analysis-signal readout the plugin fills from the worker snapshot (+ inline speech presence).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SignalReadout {
    pub pitch_hz: f32,
    pub pitch_confidence: f32,
    pub voicing_score: f32,
    pub voicing_state: f32,
    pub onset_flux_high: f32,
    pub spectral_flux: f32,
    pub hnr_db: f32,
    pub speech_presence: f32,
}

/// One rendered reading: a `0.0..=1.0` bar fill and the formatted value text.
#[derive(Debug, Clone, PartialEq)]
pub struct MeterReading {
    pub fill: f32,
    pub text: String,
}

// --- M5 display ranges (tunable; rankings only require them monotonic) ---
const DBFS_FLOOR: f32 = -60.0;
const CREST_MAX_DB: f32 = 24.0;
const LUFS_FLOOR: f32 = -40.0;
const HNR_MAX_DB: f32 = 30.0;
/// Flux normalization reference (spectral/onset flux is unbounded-positive; this only sets where the
/// bar saturates — ranking "flat vs animated" holds for any positive ref). Refine on the Windows host.
const FLUX_REF: f32 = 1.0;
const PITCH_MIN_HZ: f32 = 50.0;
const PITCH_MAX_HZ: f32 = 400.0;

fn lin_to_dbfs(linear: f32) -> f32 {
    if linear > 0.0 {
        20.0 * linear.log10()
    } else {
        f32::NEG_INFINITY
    }
}

/// Normalize `value` over `[floor, ceil]` to `0.0..=1.0`; non-finite maps to floor (0) or, for `+inf`,
/// the ceil (1).
fn norm(value: f32, floor: f32, ceil: f32) -> f32 {
    if !value.is_finite() {
        return if value > 0.0 { 1.0 } else { 0.0 };
    }
    ((value - floor) / (ceil - floor)).clamp(0.0, 1.0)
}

fn fmt_db(db: f32) -> String {
    if db.is_finite() {
        format!("{db:.1} dB")
    } else {
        "-inf dB".to_string()
    }
}

/// A linear level (peak or RMS) → dBFS over `[DBFS_FLOOR, 0]`.
pub fn level_reading(linear: f32) -> MeterReading {
    let db = lin_to_dbfs(linear);
    MeterReading {
        fill: norm(db, DBFS_FLOOR, 0.0),
        text: fmt_db(db),
    }
}

/// Crest factor (peak/rms ratio) → crest dB over `[0, CREST_MAX_DB]`.
pub fn crest_reading(ratio: f32) -> MeterReading {
    let db = if ratio > 0.0 {
        20.0 * ratio.log10()
    } else {
        0.0
    };
    MeterReading {
        fill: norm(db, 0.0, CREST_MAX_DB),
        text: fmt_db(db),
    }
}

/// A LUFS reading → fill over `[LUFS_FLOOR, 0]`.
pub fn lufs_reading(lufs: f32) -> MeterReading {
    MeterReading {
        fill: norm(lufs, LUFS_FLOOR, 0.0),
        text: if lufs.is_finite() {
            format!("{lufs:.1} LUFS")
        } else {
            "-inf LUFS".to_string()
        },
    }
}

/// An already-`0..=1` signal (voicing score, speech presence, pitch confidence) → fill directly.
pub fn unit_reading(value: f32) -> MeterReading {
    let v = if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    };
    MeterReading {
        fill: v,
        text: format!("{v:.2}"),
    }
}

/// Spectral/onset flux → fill over `[0, FLUX_REF]` (clamped, monotonic).
pub fn flux_reading(value: f32) -> MeterReading {
    let v = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    MeterReading {
        fill: (v / FLUX_REF).clamp(0.0, 1.0),
        text: format!("{v:.2}"),
    }
}

/// Harmonic-to-noise ratio in dB → fill over `[0, HNR_MAX_DB]`.
pub fn hnr_reading(db: f32) -> MeterReading {
    MeterReading {
        fill: norm(db, 0.0, HNR_MAX_DB),
        text: fmt_db(db),
    }
}

/// Pitch in Hz → fill over `[PITCH_MIN_HZ, PITCH_MAX_HZ]`; non-positive shows a dash.
pub fn pitch_reading(hz: f32) -> MeterReading {
    if hz.is_finite() && hz > 0.0 {
        MeterReading {
            fill: norm(hz, PITCH_MIN_HZ, PITCH_MAX_HZ),
            text: format!("{hz:.0} Hz"),
        }
    } else {
        MeterReading {
            fill: 0.0,
            text: "—".to_string(),
        }
    }
}

/// Voicing-state label (`0` silence, `1` unvoiced, `2` voiced).
pub fn voicing_label(state: f32) -> &'static str {
    match state.round() as i32 {
        0 => "Silence",
        1 => "Unvoiced",
        2 => "Voiced",
        _ => "—",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_maps_dbfs_reference_points() {
        assert!((level_reading(1.0).fill - 1.0).abs() < 1.0e-3); // 0 dBFS → full
        assert_eq!(level_reading(0.0).fill, 0.0); // silence (-inf) → floor
        // -30 dBFS ≈ linear 0.03162 → mid of the [-60, 0] window.
        assert!((level_reading(0.031_623).fill - 0.5).abs() < 0.02);
    }

    #[test]
    fn lufs_maps_known_reference_and_clamps() {
        // -23 LUFS over the [-40, 0] window = 17/40 = 0.425.
        assert!((lufs_reading(-23.0).fill - 0.425).abs() < 1.0e-3);
        assert_eq!(lufs_reading(0.0).fill, 1.0);
        assert_eq!(lufs_reading(-50.0).fill, 0.0); // below floor clamps
    }

    #[test]
    fn hnr_maps_db_window() {
        assert!((hnr_reading(30.0).fill - 1.0).abs() < 1.0e-3);
        assert_eq!(hnr_reading(0.0).fill, 0.0);
        assert!((hnr_reading(15.0).fill - 0.5).abs() < 1.0e-3);
    }

    #[test]
    fn voicing_labels_the_three_states() {
        assert_eq!(voicing_label(0.0), "Silence");
        assert_eq!(voicing_label(1.0), "Unvoiced");
        assert_eq!(voicing_label(2.0), "Voiced");
    }

    #[test]
    fn unit_and_flux_rankings_hold() {
        // Voiced (high score) fills more than unvoiced; animated (high flux) more than flat.
        assert!(unit_reading(0.9).fill > unit_reading(0.1).fill);
        assert!(flux_reading(0.8).fill > flux_reading(0.2).fill);
        // Bounded.
        assert!((0.0..=1.0).contains(&unit_reading(0.5).fill));
        assert!((0.0..=1.0).contains(&flux_reading(1.0e9).fill));
    }

    #[test]
    fn crest_maps_zero_db_to_floor_and_is_monotonic() {
        assert_eq!(crest_reading(1.0).fill, 0.0); // peak == rms → 0 dB crest
        assert!(crest_reading(8.0).fill > crest_reading(2.0).fill);
    }

    #[test]
    fn readout_dtos_carry_their_fields() {
        let m = MeterReadout {
            peak: 0.5,
            rms: 0.2,
            crest: 2.5,
            lufs_momentary: -18.0,
            lufs_short: -19.0,
            lufs_integrated: -20.0,
        };
        assert_eq!(m.peak, 0.5);
        let s = SignalReadout {
            pitch_hz: 147.0,
            pitch_confidence: 0.8,
            voicing_score: 0.7,
            voicing_state: 2.0,
            onset_flux_high: 0.4,
            spectral_flux: 0.5,
            hnr_db: 12.0,
            speech_presence: 0.6,
        };
        assert_eq!(s.voicing_state, 2.0);
    }
}
