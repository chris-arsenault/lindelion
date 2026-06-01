//! The default-tuning configuration (D6), confirmed with the user and grounded in spoken-word
//! practice:
//! - **Loudness target −16 LUFS** — Apple Podcasts' spoken-word standard (Spotify −14, EBU −18 are
//!   the neighbours). The **−1 dBTP** intent is the *limiter/loudness target* (the limiter's ceiling
//!   param), not the clip-*reject* threshold: a −1 dB limiter overshoots to ~−0.8 dBFS, so a literal
//!   −1 dBFS reject would reject the limiter's own normal output. The hard clip constraint therefore
//!   rejects peak > 0.95 (≈ −0.45 dBFS) — strict (catches real clipping) but physically achievable.
//! - **Balanced weights** — the five scored terms (loudness, noise reduction, dereverb, clarity,
//!   low coloration) weighted ~evenly (0.2 each), so no single objective dominates the defaults.
//! - **Strict constraints** — reject non-finite, peak > −0.45 dBFS, or pumping (chain gain-envelope
//!   variance **over speech-present frames**) above a tight bound.
//!
//! The **search space** (which params each order sweeps) and the **regression floor** are
//! implementation choices: the search space is the few perceptually-relevant params per enabled
//! effect (derived from each effect's semantics); the floor is provisional and is re-pinned against
//! the achieved scores once `make tune-defaults` has run (Step 6).

use crate::order::SignalOrder;
use crate::patch::{CalomaPatch, default_patch_for};
use crate::slot::SlotId;

/// Integrated-loudness target (LUFS) and the deviation (LU) beyond which the loudness reward → 0.
#[derive(Debug, Clone, Copy)]
pub struct LoudnessTarget {
    pub lufs: f32,
    pub tolerance_lu: f32,
}

/// Hard no-artifact constraints; any violation on any fixture rejects the candidate.
#[derive(Debug, Clone, Copy)]
pub struct Constraints {
    /// Peak ceiling (linear). 0.891 ≈ −1 dBFS, matching the −1 dBTP intent.
    pub peak_ceiling: f32,
    /// Maximum chain gain-envelope variance (dB²) before it counts as pumping.
    pub pumping_variance_db2: f32,
}

/// Balanced weights over the five scored terms (sum to 1).
#[derive(Debug, Clone, Copy)]
pub struct Weights {
    pub loudness: f32,
    pub noise_reduction: f32,
    pub dereverb: f32,
    pub clarity: f32,
    pub low_coloration: f32,
}

impl Weights {
    pub fn sum(&self) -> f32 {
        self.loudness + self.noise_reduction + self.dereverb + self.clarity + self.low_coloration
    }
}

/// The scales that map each raw metric onto a 0..1 reward in the scorer (Step 3).
#[derive(Debug, Clone, Copy)]
pub struct TermScales {
    /// SNR improvement (dB) that earns full noise-reduction reward.
    pub noise_improvement_db: f32,
    /// |HF-presence change| (dB) that earns full clarity reward above the preserved baseline.
    pub clarity_scale_db: f32,
    /// Core-band coloration (dB) at which the low-coloration reward reaches 0.
    pub coloration_scale_db: f32,
}

/// Analysis bands (Hz).
#[derive(Debug, Clone, Copy)]
pub struct Bands {
    pub core_lo: f32,
    pub core_hi: f32,
    pub hf_lo: f32,
    pub hf_hi: f32,
}

/// The full D6 tuning configuration.
#[derive(Debug, Clone, Copy)]
pub struct TuningConfig {
    pub loudness: LoudnessTarget,
    pub constraints: Constraints,
    pub weights: Weights,
    pub scales: TermScales,
    pub bands: Bands,
}

/// The confirmed D6 configuration.
pub const fn default_config() -> TuningConfig {
    TuningConfig {
        loudness: LoudnessTarget {
            lufs: -16.0,
            tolerance_lu: 3.0,
        },
        constraints: Constraints {
            peak_ceiling: 0.95, // ≈ −0.45 dBFS clip-reject; the −1 dBTP target lives in the limiter
            pumping_variance_db2: 9.0, // dB² over speech frames (~3 dB gain-swing stdev); empirical
        },
        weights: Weights {
            loudness: 0.2,
            noise_reduction: 0.2,
            dereverb: 0.2,
            clarity: 0.2,
            low_coloration: 0.2,
        },
        scales: TermScales {
            noise_improvement_db: 6.0,
            clarity_scale_db: 6.0,
            coloration_scale_db: 6.0,
        },
        bands: Bands {
            core_lo: 300.0,
            core_hi: 3_000.0,
            hf_lo: 4_000.0,
            hf_hi: 8_000.0,
        },
    }
}

/// One search dimension: a named, typed handle on a single patch parameter plus its swept grid.
/// `slot = Some(..)` dimensions are only searched for an order whose default patch enables that slot
/// (`slot = None` would be a patch-level param, but gain staging is no longer searched). `get`/`set`
/// are non-capturing
/// fn-pointers into the typed `slot_params`/level fields — type-safe, no stringly indices.
#[derive(Clone, Copy)]
pub struct ParamDim {
    pub label: &'static str,
    pub slot: Option<SlotId>,
    pub get: fn(&CalomaPatch) -> f32,
    pub set: fn(&mut CalomaPatch, f32),
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

/// The candidate search dimensions — the few high-impact, perceptually-relevant **tonal/dynamics**
/// params per effect. `search_space` filters these to a given order's enabled slots.
///
/// **Gain staging is not searched.** A default is a robust starting point, not a mastering target:
/// the committed defaults ship at **unity** input/output level with the limiter for peak safety, and
/// the user sets level via the editor. Tuning only the tonal/dynamics character keeps the defaults
/// from depending on a fragile loudness optimization (which proved brittle on real material).
const ALL_DIMS: &[ParamDim] = &[
    ParamDim {
        label: "high_pass.cutoff",
        slot: Some(SlotId::HighPass),
        get: |p| p.high_pass.params.cutoff,
        set: |p, v| p.high_pass.params.cutoff = v,
        min: 60.0,
        max: 140.0,
        step: 20.0,
    },
    ParamDim {
        label: "compressor.threshold",
        slot: Some(SlotId::Compressor),
        get: |p| p.compressor.params.threshold,
        set: |p, v| p.compressor.params.threshold = v,
        min: -30.0,
        max: -14.0,
        step: 4.0,
    },
    ParamDim {
        label: "compressor.makeup",
        slot: Some(SlotId::Compressor),
        get: |p| p.compressor.params.makeup,
        set: |p, v| p.compressor.params.makeup = v,
        min: 0.0,
        max: 9.0,
        step: 3.0,
    },
    ParamDim {
        label: "de_esser.threshold",
        slot: Some(SlotId::DeEsser),
        get: |p| p.de_esser.params.threshold,
        set: |p, v| p.de_esser.params.threshold = v,
        min: -40.0,
        max: -22.0,
        step: 6.0,
    },
    ParamDim {
        label: "five_band_eq.low_shelf_gain",
        slot: Some(SlotId::FiveBandEq),
        get: |p| p.five_band_eq.params.low_shelf_gain,
        set: |p, v| p.five_band_eq.params.low_shelf_gain = v,
        min: -2.0,
        max: 5.0,
        step: 1.0,
    },
    ParamDim {
        label: "five_band_eq.high_shelf_gain",
        slot: Some(SlotId::FiveBandEq),
        get: |p| p.five_band_eq.params.high_shelf_gain,
        set: |p, v| p.five_band_eq.params.high_shelf_gain = v,
        min: -2.0,
        max: 5.0,
        step: 1.0,
    },
    ParamDim {
        label: "bass_enhancer.amount",
        slot: Some(SlotId::BassEnhancer),
        get: |p| p.bass_enhancer.params.amount,
        set: |p, v| p.bass_enhancer.params.amount = v,
        min: 20.0,
        max: 70.0,
        step: 15.0,
    },
    ParamDim {
        label: "air_exciter.amount",
        slot: Some(SlotId::AirExciter),
        get: |p| p.air_exciter.params.amount,
        set: |p, v| p.air_exciter.params.amount = v,
        min: 20.0,
        max: 60.0,
        step: 10.0,
    },
    ParamDim {
        label: "limiter.ceiling",
        slot: Some(SlotId::Limiter),
        get: |p| p.limiter.params.ceiling,
        set: |p, v| p.limiter.params.ceiling = v,
        min: -2.0,
        max: -0.5,
        step: 0.5,
    },
];

/// The search dimensions for `order`: every patch-level dim, plus the slot dims whose slot is
/// **enabled** by that order's default patch.
pub fn search_space(order: SignalOrder) -> Vec<ParamDim> {
    let defaults = default_patch_for(order);
    ALL_DIMS
        .iter()
        .copied()
        .filter(|dim| match dim.slot {
            None => true,
            Some(slot) => defaults.slot_enabled(slot),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_are_finite_and_normalized() {
        let cfg = default_config();
        let w = cfg.weights;
        assert!(
            [
                w.loudness,
                w.noise_reduction,
                w.dereverb,
                w.clarity,
                w.low_coloration
            ]
            .iter()
            .all(|x| x.is_finite() && *x >= 0.0)
        );
        assert!(
            (w.sum() - 1.0).abs() < 1e-6,
            "weights must sum to 1: {}",
            w.sum()
        );
    }

    #[test]
    fn confirmed_d6_values() {
        let cfg = default_config();
        assert_eq!(cfg.loudness.lufs, -16.0);
        assert!((cfg.constraints.peak_ceiling - 0.95).abs() < 1e-6); // ≈ −0.45 dBFS clip-reject
        assert!(cfg.constraints.pumping_variance_db2 > 0.0);
    }

    #[test]
    fn every_dim_has_a_valid_grid_and_round_trips() {
        for dim in ALL_DIMS {
            assert!(dim.min < dim.max, "{}: empty range", dim.label);
            assert!(dim.step > 0.0, "{}: non-positive step", dim.label);
            // get∘set round-trips a mid value.
            let mut patch = CalomaPatch::default();
            let mid = 0.5 * (dim.min + dim.max);
            (dim.set)(&mut patch, mid);
            assert!(
                ((dim.get)(&patch) - mid).abs() < 1e-4,
                "{}: get/set mismatch",
                dim.label
            );
        }
    }

    #[test]
    fn search_space_only_references_enabled_slots() {
        for order in SignalOrder::ALL {
            let defaults = default_patch_for(order);
            let dims = search_space(order);
            assert!(!dims.is_empty(), "{order:?}: no search dims");
            for dim in &dims {
                if let Some(slot) = dim.slot {
                    assert!(
                        defaults.slot_enabled(slot),
                        "{order:?}: dim {} references a disabled slot {slot:?}",
                        dim.label
                    );
                }
            }
            // Gain staging is not searched — loudness is normalized deterministically post-search,
            // so neither the input nor the output level appears as a search dimension.
            assert!(!dims.iter().any(|d| d.label == "input_level_db"));
            assert!(!dims.iter().any(|d| d.label == "output_level_db"));
            // The dims that remain are all tonal/dynamics (slot params).
            assert!(dims.iter().all(|d| d.slot.is_some()));
        }
    }
}
