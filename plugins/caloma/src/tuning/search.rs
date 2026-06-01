//! Seeded **coordinate descent** over the [`ParamDim`](super::config::ParamDim) search space
//! (M5). Cheap, deterministic, and reproducible: each pass sweeps every dimension on its grid and
//! keeps the argmax; passes repeat until no dimension improves. Optional seeded random restarts
//! (a deterministic LCG) explore the space from several starting points and keep the global best.
//!
//! The objective is injected — `Fn(&CalomaPatch) -> Option<f32>` (`None` = a constraint-rejected
//! candidate). Step 6 supplies the real full-chain battery evaluator; tests supply a toy one. No
//! chain or NN here, so this is `make ci`-testable.

use crate::patch::CalomaPatch;

use super::config::ParamDim;

/// Search controls. Deterministic given `seed`.
#[derive(Debug, Clone, Copy)]
pub struct SearchOpts {
    pub seed: u64,
    /// Hard cap on coordinate-descent passes (a pass that improves nothing stops early anyway).
    pub max_passes: usize,
    /// Extra randomized start points (beyond the supplied `start`); the global best is kept.
    pub restarts: usize,
}

impl Default for SearchOpts {
    fn default() -> Self {
        Self {
            seed: 0x5EED_C0FFEE,
            max_passes: 8,
            restarts: 0,
        }
    }
}

/// A tiny deterministic LCG (Numerical Recipes constants) for restart perturbation.
struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    /// Uniform in `[0, n)`.
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

/// The inclusive grid of a dimension: `min, min+step, …` up to `max`.
fn grid(dim: &ParamDim) -> Vec<f32> {
    let mut values = Vec::new();
    let mut v = dim.min;
    while v <= dim.max + dim.step * 1e-3 {
        values.push(v.min(dim.max));
        v += dim.step;
    }
    if values.is_empty() {
        values.push(dim.min);
    }
    values
}

fn evaluate(patch: &CalomaPatch, objective: &mut impl FnMut(&CalomaPatch) -> Option<f32>) -> f32 {
    objective(patch).unwrap_or(f32::NEG_INFINITY)
}

/// One coordinate-descent run from `start`. Sweeps each dimension's grid, keeping the best value,
/// and repeats until a full pass improves nothing or `max_passes` is reached.
fn descend(
    start: CalomaPatch,
    dims: &[ParamDim],
    objective: &mut impl FnMut(&CalomaPatch) -> Option<f32>,
    max_passes: usize,
) -> (CalomaPatch, f32) {
    let mut best = start;
    let mut best_score = evaluate(&best, objective);
    for _ in 0..max_passes.max(1) {
        let mut improved = false;
        for dim in dims {
            let mut local_value = (dim.get)(&best);
            let mut local_score = best_score;
            for v in grid(dim) {
                let mut trial = best.clone();
                (dim.set)(&mut trial, v);
                let s = evaluate(&trial, &mut *objective);
                if s > local_score {
                    local_score = s;
                    local_value = v;
                }
            }
            if local_score > best_score {
                (dim.set)(&mut best, local_value);
                best_score = local_score;
                improved = true;
            }
        }
        if !improved {
            break;
        }
    }
    (best, best_score)
}

/// Optimize the patch over `dims` for the injected `objective`. Runs coordinate descent from `start`
/// plus `opts.restarts` seeded-random start points and returns the global argmax `(patch, score)`.
/// Deterministic given `opts`.
pub fn coordinate_descent(
    start: CalomaPatch,
    dims: &[ParamDim],
    mut objective: impl FnMut(&CalomaPatch) -> Option<f32>,
    opts: SearchOpts,
) -> (CalomaPatch, f32) {
    let (mut best_patch, mut best_score) =
        descend(start.clone(), dims, &mut objective, opts.max_passes);

    let mut rng = Lcg(opts.seed);
    for _ in 0..opts.restarts {
        // A randomized start: each dimension snapped to a random grid point.
        let mut seed_patch = start.clone();
        for dim in dims {
            let g = grid(dim);
            (dim.set)(&mut seed_patch, g[rng.below(g.len())]);
        }
        let (patch, score) = descend(seed_patch, dims, &mut objective, opts.max_passes);
        if score > best_score {
            best_patch = patch;
            best_score = score;
        }
    }
    (best_patch, best_score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot::SlotId;

    fn level_dim() -> ParamDim {
        ParamDim {
            label: "output_level_db",
            slot: None,
            get: |p| p.output_level_db,
            set: |p, v| p.output_level_db = v,
            min: -12.0,
            max: 6.0,
            step: 2.0,
        }
    }

    #[test]
    fn converges_to_the_grid_point_nearest_the_optimum() {
        // Objective peaks at output_level_db = −8; the grid (−12,−10,−8,…,6) contains −8 exactly.
        let dims = [level_dim()];
        let objective = |p: &CalomaPatch| Some(-(p.output_level_db + 8.0).powi(2));
        let (best, score) = coordinate_descent(
            CalomaPatch::default(),
            &dims,
            objective,
            SearchOpts::default(),
        );
        assert!(
            (best.output_level_db - (-8.0)).abs() < 1e-6,
            "got {}",
            best.output_level_db
        );
        assert!((score - 0.0).abs() < 1e-6);
    }

    #[test]
    fn is_bit_identical_across_runs_with_the_same_seed() {
        let dims = [
            level_dim(),
            ParamDim {
                label: "compressor.threshold",
                slot: Some(SlotId::Compressor),
                get: |p| p.compressor.params.threshold,
                set: |p, v| p.compressor.params.threshold = v,
                min: -30.0,
                max: -14.0,
                step: 4.0,
            },
        ];
        // A two-dimensional objective with an interior optimum, plus restarts to exercise the RNG.
        let objective = |p: &CalomaPatch| {
            Some(
                -(p.output_level_db + 6.0).powi(2) - (p.compressor.params.threshold + 22.0).powi(2),
            )
        };
        let opts = SearchOpts {
            seed: 12345,
            max_passes: 8,
            restarts: 4,
        };
        let a = coordinate_descent(CalomaPatch::default(), &dims, objective, opts);
        let b = coordinate_descent(CalomaPatch::default(), &dims, objective, opts);
        assert_eq!(a.0.output_level_db, b.0.output_level_db);
        assert_eq!(
            a.0.compressor.params.threshold,
            b.0.compressor.params.threshold
        );
        assert_eq!(a.1, b.1);
        // And it found the grid optimum (−6, −22).
        assert!((a.0.output_level_db - (-6.0)).abs() < 1e-6);
        assert!((a.0.compressor.params.threshold - (-22.0)).abs() < 1e-6);
    }

    #[test]
    fn rejected_candidates_do_not_derail_the_search() {
        // Objective rejects everything except a narrow good band; CD must still find the best inside.
        let dims = [level_dim()];
        let objective = |p: &CalomaPatch| {
            if (p.output_level_db + 4.0).abs() <= 2.0 {
                Some(-(p.output_level_db + 4.0).powi(2))
            } else {
                None
            }
        };
        let (best, _) = coordinate_descent(
            CalomaPatch::default(),
            &dims,
            objective,
            SearchOpts::default(),
        );
        assert!(
            (best.output_level_db - (-4.0)).abs() < 1e-6,
            "got {}",
            best.output_level_db
        );
    }
}
