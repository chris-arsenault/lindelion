//! Gaussian spatial weights over the plate's displacement grid: the strike injects and the
//! pickup reads through these apertures. Ported from the membrane's spatial weights with
//! the field generalized to a plain `&[f32]` grid and a single normalization —
//! unit-energy (`Σw² = 1`) — since the membrane's aperture-integral radiation pickup is
//! replaced by a derived radiation model in a later milestone (ADR-0050).

use lindelion_dsp_utils::math;

use super::PLATE_MAX_CELLS;

/// Normalized plate-surface position in `[0, 1]²`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshPoint {
    pub x: f32,
    pub y: f32,
}

impl MeshPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x: math::finite_clamp(x, 0.0, 1.0, 0.5),
            y: math::finite_clamp(y, 0.0, 1.0, 0.5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpatialWeight {
    index: usize,
    weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpatialWeights {
    weights: Vec<SpatialWeight>,
}

impl SpatialWeights {
    pub(crate) fn new(point: MeshPoint, width: usize, height: usize, width_fraction: f32) -> Self {
        let mut weights = Self {
            weights: Vec::with_capacity(PLATE_MAX_CELLS),
        };
        weights.recompute(point, width, height, width_fraction);
        weights
    }

    pub(crate) fn recompute(
        &mut self,
        point: MeshPoint,
        width: usize,
        height: usize,
        width_fraction: f32,
    ) {
        self.weights.clear();
        let center_x = point.x * (width - 1) as f32;
        let center_y = point.y * (height - 1) as f32;
        let sigma = (width.min(height) as f32 * width_fraction).max(0.35);
        let radius = (sigma * 2.5).ceil() as isize;
        let mut square_sum = 0.0;

        for y in (center_y.floor() as isize - radius)..=(center_y.floor() as isize + radius) {
            for x in (center_x.floor() as isize - radius)..=(center_x.floor() as isize + radius) {
                if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
                    continue;
                }
                let dx = (x as f32 - center_x) / sigma;
                let dy = (y as f32 - center_y) / sigma;
                let weight = (-0.5 * (dx * dx + dy * dy)).exp();
                if weight <= 1.0e-6 {
                    continue;
                }
                self.weights.push(SpatialWeight {
                    index: y as usize * width + x as usize,
                    weight,
                });
                square_sum += weight * weight;
            }
        }

        if square_sum <= f32::EPSILON {
            self.weights.clear();
            let x = center_x.round().clamp(0.0, (width - 1) as f32) as usize;
            let y = center_y.round().clamp(0.0, (height - 1) as f32) as usize;
            self.weights.push(SpatialWeight {
                index: y * width + x,
                weight: 1.0,
            });
            return;
        }

        let normalizer = square_sum.sqrt().max(f32::EPSILON);
        for weight in &mut self.weights {
            weight.weight /= normalizer;
        }
    }

    /// Sparse point-cluster tap: the center cell plus four cells at `radius_fraction` of
    /// the grid's short side around it, each at weight `1/count`. Point taps carry no
    /// spatial aperture response (no treble loss) — this is the radiation pickup (M2,
    /// ADR-0050); the Gaussian constructor remains the strike's contact aperture.
    pub(crate) fn point_cluster(
        point: MeshPoint,
        width: usize,
        height: usize,
        radius_fraction: f32,
    ) -> Self {
        let mut weights = Self {
            weights: Vec::with_capacity(PLATE_MAX_CELLS),
        };
        weights.recompute_point_cluster(point, width, height, radius_fraction);
        weights
    }

    pub(crate) fn recompute_point_cluster(
        &mut self,
        point: MeshPoint,
        width: usize,
        height: usize,
        radius_fraction: f32,
    ) {
        self.weights.clear();
        let center_x = point.x * (width - 1) as f32;
        let center_y = point.y * (height - 1) as f32;
        let radius = (width.min(height) as f32 * radius_fraction).max(1.0);
        let clamp_cell = |x: f32, y: f32| -> usize {
            let x = x.round().clamp(0.0, (width - 1) as f32) as usize;
            let y = y.round().clamp(0.0, (height - 1) as f32) as usize;
            y * width + x
        };
        let mut push_unique = |index: usize| {
            if !self.weights.iter().any(|w| w.index == index) {
                self.weights.push(SpatialWeight { index, weight: 1.0 });
            }
        };
        push_unique(clamp_cell(center_x, center_y));
        push_unique(clamp_cell(center_x + radius, center_y));
        push_unique(clamp_cell(center_x - radius, center_y));
        push_unique(clamp_cell(center_x, center_y + radius));
        push_unique(clamp_cell(center_x, center_y - radius));
        let normalizer = self.weights.len() as f32;
        for weight in &mut self.weights {
            weight.weight /= normalizer;
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, f32)> + '_ {
        self.weights
            .iter()
            .map(|spatial_weight| (spatial_weight.index, spatial_weight.weight))
    }
}
