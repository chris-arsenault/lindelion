use lindelion_dsp_utils::math;

use super::{DirectionalWaves, MAX_MESH_CELLS, MeshPoint, STRIKE_CONTACT_PRESSURE_DAMPING};

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpatialWeight {
    index: usize,
    weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SpatialWeights {
    weights: Vec<SpatialWeight>,
    normalization: SpatialNormalization,
}

impl SpatialWeights {
    pub(super) fn new(
        point: MeshPoint,
        width: usize,
        height: usize,
        width_fraction: f32,
        normalization: SpatialNormalization,
    ) -> Self {
        let mut weights = Self {
            weights: Vec::with_capacity(MAX_MESH_CELLS),
            normalization,
        };
        weights.recompute(point, width, height, width_fraction);
        weights
    }

    pub(super) fn recompute(
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
        let mut sum = 0.0;

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
                sum += weight;
            }
        }

        if sum <= f32::EPSILON {
            self.weights.clear();
            let x = center_x.round().clamp(0.0, (width - 1) as f32) as usize;
            let y = center_y.round().clamp(0.0, (height - 1) as f32) as usize;
            self.weights.push(SpatialWeight {
                index: y * width + x,
                weight: 1.0,
            });
            return;
        }

        match self.normalization {
            SpatialNormalization::UnitEnergy => {
                let square_sum: f32 = self
                    .weights
                    .iter()
                    .map(|weight| weight.weight * weight.weight)
                    .sum();
                let normalizer = square_sum.sqrt().max(f32::EPSILON);
                for weight in &mut self.weights {
                    weight.weight /= normalizer;
                }
            }
            SpatialNormalization::ApertureIntegral => {}
        }
    }

    pub(super) fn inject_pressure(&self, waves: &mut DirectionalWaves, pressure: f32) {
        let component = math::snap_to_zero(pressure) * 0.25;
        for spatial_weight in &self.weights {
            waves.add_uniform(spatial_weight.index, component * spatial_weight.weight);
        }
    }

    pub(super) fn absorb_contact_motion(&self, waves: &mut DirectionalWaves, amount: f32) {
        let amount = math::finite_clamp(amount, 0.0, 0.95, 0.0);
        if amount <= f32::EPSILON {
            return;
        }
        for spatial_weight in &self.weights {
            let local_loss = (amount * spatial_weight.weight.abs()).min(0.95);
            waves.damp_directional_motion(spatial_weight.index, local_loss);
            waves.damp_pressure(
                spatial_weight.index,
                local_loss * STRIKE_CONTACT_PRESSURE_DAMPING,
            );
        }
    }

    pub(super) fn pressure(&self, waves: &DirectionalWaves) -> f32 {
        self.weights
            .iter()
            .map(|spatial_weight| waves.pressure(spatial_weight.index) * spatial_weight.weight)
            .sum()
    }

    pub(super) fn curvature_pressure(
        &self,
        waves: &DirectionalWaves,
        width: usize,
        height: usize,
    ) -> f32 {
        self.weights
            .iter()
            .map(|spatial_weight| {
                let index = spatial_weight.index;
                let x = index % width;
                let y = index / width;
                let center = waves.pressure(index);
                let left = if x > 0 {
                    waves.pressure(index - 1)
                } else {
                    center
                };
                let right = if x + 1 < width {
                    waves.pressure(index + 1)
                } else {
                    center
                };
                let top = if y > 0 {
                    waves.pressure(index - width)
                } else {
                    center
                };
                let bottom = if y + 1 < height {
                    waves.pressure(index + width)
                } else {
                    center
                };
                let curvature = center - 0.25 * (left + right + top + bottom);
                curvature * spatial_weight.weight
            })
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpatialNormalization {
    UnitEnergy,
    ApertureIntegral,
}
