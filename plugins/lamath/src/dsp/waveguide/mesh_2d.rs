use lindelion_dsp_utils::math;

use super::core;

mod runtime;
pub use runtime::{MeshResonator, MeshVoiceParams};

const MIN_MESH_SIZE: usize = 3;
const MAX_MESH_SIZE: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshPoint {
    x: f32,
    y: f32,
}

impl MeshPoint {
    fn new(x: f32, y: f32) -> Self {
        Self {
            x: math::finite_clamp(x, 0.0, 1.0, 0.5),
            y: math::finite_clamp(y, 0.0, 1.0, 0.5),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeshBoundaryKind {
    Fixed,
    Free,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshBoundaryEdge {
    kind: MeshBoundaryKind,
    damping: f32,
}

impl MeshBoundaryEdge {
    fn fixed(damping: f32) -> Self {
        Self {
            kind: MeshBoundaryKind::Fixed,
            damping,
        }
    }

    fn free(damping: f32) -> Self {
        Self {
            kind: MeshBoundaryKind::Free,
            damping,
        }
    }

    fn reflection(self) -> f32 {
        let sign = match self.kind {
            MeshBoundaryKind::Fixed => -1.0,
            MeshBoundaryKind::Free => 1.0,
        };
        sign * (1.0 - math::finite_clamp(self.damping, 0.0, 1.0, 0.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct MeshBoundaryConfig {
    left: MeshBoundaryEdge,
    right: MeshBoundaryEdge,
    top: MeshBoundaryEdge,
    bottom: MeshBoundaryEdge,
}

impl MeshBoundaryConfig {
    fn fixed(damping: f32) -> Self {
        Self {
            left: MeshBoundaryEdge::fixed(damping),
            right: MeshBoundaryEdge::fixed(damping),
            top: MeshBoundaryEdge::fixed(damping),
            bottom: MeshBoundaryEdge::fixed(damping),
        }
    }

    fn free(damping: f32) -> Self {
        Self {
            left: MeshBoundaryEdge::free(damping),
            right: MeshBoundaryEdge::free(damping),
            top: MeshBoundaryEdge::free(damping),
            bottom: MeshBoundaryEdge::free(damping),
        }
    }

    #[cfg(test)]
    fn fixed_edges(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left: MeshBoundaryEdge::fixed(left),
            right: MeshBoundaryEdge::fixed(right),
            top: MeshBoundaryEdge::fixed(top),
            bottom: MeshBoundaryEdge::fixed(bottom),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RectangularMesh2dConfig {
    width: usize,
    height: usize,
    sample_rate: f32,
    wave_speed_mps: f32,
    physical_width_m: f32,
    physical_height_m: f32,
    boundary: MeshBoundaryConfig,
    strike_position: MeshPoint,
    pickup_position: MeshPoint,
    excitation_width: f32,
    pickup_width: f32,
}

impl RectangularMesh2dConfig {
    fn sanitized(self) -> Self {
        Self {
            width: self.width.clamp(MIN_MESH_SIZE, MAX_MESH_SIZE),
            height: self.height.clamp(MIN_MESH_SIZE, MAX_MESH_SIZE),
            sample_rate: core::sanitize_sample_rate(self.sample_rate),
            wave_speed_mps: math::finite_clamp(self.wave_speed_mps, 1.0, 4_000.0, 220.0),
            physical_width_m: math::finite_clamp(self.physical_width_m, 0.01, 10.0, 0.7),
            physical_height_m: math::finite_clamp(self.physical_height_m, 0.01, 10.0, 0.45),
            boundary: self.boundary,
            strike_position: self.strike_position,
            pickup_position: self.pickup_position,
            excitation_width: math::finite_clamp(self.excitation_width, 0.005, 0.4, 0.06),
            pickup_width: math::finite_clamp(self.pickup_width, 0.005, 0.4, 0.025),
        }
    }

    #[cfg(test)]
    fn mode_frequency_hz(self, mode_x: usize, mode_y: usize) -> f32 {
        let config = self.sanitized();
        let kx = mode_x as f32 / config.physical_width_m;
        let ky = mode_y as f32 / config.physical_height_m;
        config.wave_speed_mps * 0.5 * (kx * kx + ky * ky).sqrt()
    }
}

impl Default for RectangularMesh2dConfig {
    fn default() -> Self {
        Self {
            width: 14,
            height: 10,
            sample_rate: 48_000.0,
            wave_speed_mps: 220.0,
            physical_width_m: 0.72,
            physical_height_m: 0.48,
            boundary: MeshBoundaryConfig::fixed(0.16),
            strike_position: MeshPoint::new(0.35, 0.42),
            pickup_position: MeshPoint::new(0.72, 0.58),
            excitation_width: 0.055,
            pickup_width: 0.025,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SpatialWeight {
    index: usize,
    weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct SpatialWeights {
    weights: Vec<SpatialWeight>,
}

impl SpatialWeights {
    fn new(point: MeshPoint, width: usize, height: usize, width_fraction: f32) -> Self {
        // Capacity is the full grid, so later in-place recomputes never reallocate.
        let mut weights = Self {
            weights: Vec::with_capacity(width * height),
        };
        weights.recompute(point, width, height, width_fraction);
        weights
    }

    /// Recompute the Gaussian spatial weights in place: `clear` + `push` reuses
    /// the grid-sized capacity, so re-tuning a live voice never allocates.
    fn recompute(&mut self, point: MeshPoint, width: usize, height: usize, width_fraction: f32) {
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

        for weight in &mut self.weights {
            weight.weight /= sum;
        }
    }

    fn inject_pressure(&self, waves: &mut DirectionalWaves, pressure: f32) {
        let component = math::snap_to_zero(pressure) * 0.25;
        for spatial_weight in &self.weights {
            waves.add_uniform(spatial_weight.index, component * spatial_weight.weight);
        }
    }

    fn pressure(&self, waves: &DirectionalWaves) -> f32 {
        self.weights
            .iter()
            .map(|spatial_weight| waves.pressure(spatial_weight.index) * spatial_weight.weight)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionalWaves {
    from_left: Vec<f32>,
    from_right: Vec<f32>,
    from_top: Vec<f32>,
    from_bottom: Vec<f32>,
}

impl DirectionalWaves {
    fn new(len: usize) -> Self {
        Self {
            from_left: vec![0.0; len],
            from_right: vec![0.0; len],
            from_top: vec![0.0; len],
            from_bottom: vec![0.0; len],
        }
    }

    fn clear(&mut self) {
        self.from_left.fill(0.0);
        self.from_right.fill(0.0);
        self.from_top.fill(0.0);
        self.from_bottom.fill(0.0);
    }

    fn pressure(&self, index: usize) -> f32 {
        math::snap_to_zero(
            0.5 * (self.from_left[index]
                + self.from_right[index]
                + self.from_top[index]
                + self.from_bottom[index]),
        )
    }

    fn add_uniform(&mut self, index: usize, component: f32) {
        let component = math::snap_to_zero(component);
        self.from_left[index] = math::snap_to_zero(self.from_left[index] + component);
        self.from_right[index] = math::snap_to_zero(self.from_right[index] + component);
        self.from_top[index] = math::snap_to_zero(self.from_top[index] + component);
        self.from_bottom[index] = math::snap_to_zero(self.from_bottom[index] + component);
    }

    #[cfg(test)]
    fn energy(&self) -> f32 {
        self.from_left
            .iter()
            .chain(self.from_right.iter())
            .chain(self.from_top.iter())
            .chain(self.from_bottom.iter())
            .map(|sample| sample * sample)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RectangularMesh2d {
    config: RectangularMesh2dConfig,
    current: DirectionalWaves,
    next: DirectionalWaves,
    source_weights: SpatialWeights,
    pickup_weights: SpatialWeights,
}

impl RectangularMesh2d {
    fn new(config: RectangularMesh2dConfig) -> Self {
        let config = config.sanitized();
        let len = config.width * config.height;
        Self {
            config,
            current: DirectionalWaves::new(len),
            next: DirectionalWaves::new(len),
            source_weights: SpatialWeights::new(
                config.strike_position,
                config.width,
                config.height,
                config.excitation_width,
            ),
            pickup_weights: SpatialWeights::new(
                config.pickup_position,
                config.width,
                config.height,
                config.pickup_width,
            ),
        }
    }

    /// Adopt a new configuration without reallocating: the grid (and therefore
    /// all buffers) is fixed at construction, so only the non-grid fields and the
    /// in-place spatial weights change. Allocation-free for live re-tuning.
    fn reconfigure(&mut self, config: RectangularMesh2dConfig) {
        let config = RectangularMesh2dConfig {
            width: self.config.width,
            height: self.config.height,
            ..config
        }
        .sanitized();
        self.config = config;
        self.source_weights.recompute(
            config.strike_position,
            config.width,
            config.height,
            config.excitation_width,
        );
        self.pickup_weights.recompute(
            config.pickup_position,
            config.width,
            config.height,
            config.pickup_width,
        );
    }

    fn process_sample(&mut self, excitation: f32) -> f32 {
        self.source_weights
            .inject_pressure(&mut self.current, excitation);
        let output = self.pickup_weights.pressure(&self.current);
        self.scatter_and_propagate();
        math::snap_to_zero(output)
    }

    fn reset(&mut self) {
        self.current.clear();
        self.next.clear();
    }

    #[cfg(test)]
    fn total_energy(&self) -> f32 {
        self.current.energy()
    }

    #[cfg(test)]
    fn mode_frequency_hz(&self, mode_x: usize, mode_y: usize) -> f32 {
        self.config.mode_frequency_hz(mode_x, mode_y)
    }

    fn scatter_and_propagate(&mut self) {
        self.next.clear();
        for y in 0..self.config.height {
            for x in 0..self.config.width {
                self.scatter_junction(x, y);
            }
        }
        std::mem::swap(&mut self.current, &mut self.next);
    }

    fn scatter_junction(&mut self, x: usize, y: usize) {
        let index = self.index(x, y);
        let pressure = self.current.pressure(index);
        let left = pressure - self.current.from_left[index];
        let right = pressure - self.current.from_right[index];
        let top = pressure - self.current.from_top[index];
        let bottom = pressure - self.current.from_bottom[index];

        self.propagate_left(x, y, left);
        self.propagate_right(x, y, right);
        self.propagate_top(x, y, top);
        self.propagate_bottom(x, y, bottom);
    }

    fn propagate_left(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if x == 0 {
            self.next.from_left[index] += sample * self.config.boundary.left.reflection();
        } else {
            let neighbor = self.index(x - 1, y);
            self.next.from_right[neighbor] += sample;
        }
    }

    fn propagate_right(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if x + 1 == self.config.width {
            self.next.from_right[index] += sample * self.config.boundary.right.reflection();
        } else {
            let neighbor = self.index(x + 1, y);
            self.next.from_left[neighbor] += sample;
        }
    }

    fn propagate_top(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if y == 0 {
            self.next.from_top[index] += sample * self.config.boundary.top.reflection();
        } else {
            let neighbor = self.index(x, y - 1);
            self.next.from_bottom[neighbor] += sample;
        }
    }

    fn propagate_bottom(&mut self, x: usize, y: usize, sample: f32) {
        let index = self.index(x, y);
        if y + 1 == self.config.height {
            self.next.from_bottom[index] += sample * self.config.boundary.bottom.reflection();
        } else {
            let neighbor = self.index(x, y + 1);
            self.next.from_top[neighbor] += sample;
        }
    }

    fn index(&self, x: usize, y: usize) -> usize {
        y * self.config.width + x
    }
}

#[cfg(test)]
mod promotion_tests;

#[cfg(test)]
fn render_mesh(
    config: RectangularMesh2dConfig,
    sample_count: usize,
    excitation: crate::dsp::render_metrics::RenderExcitation,
) -> Vec<f32> {
    let config = config.sanitized();
    let mut mesh = RectangularMesh2d::new(config);
    crate::dsp::render_metrics::render_response(
        config.sample_rate,
        mesh.mode_frequency_hz(1, 1),
        sample_count,
        excitation,
        |sample| mesh.process_sample(sample),
    )
}

#[cfg(test)]
fn mode_frequency(config: RectangularMesh2dConfig) -> f32 {
    config.mode_frequency_hz(1, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::render_metrics::{RenderExcitation, render_metric_profile, render_response};
    use lindelion_dsp_utils::analysis::{assert_all_finite, rms_difference};

    #[test]
    fn rectangular_mesh_renders_finite_decaying_audio() {
        let config = RectangularMesh2dConfig::default();
        let output = render_mesh(config, 24_000, RenderExcitation::ShapedPluck);
        let profile = render_metric_profile(&output, config.sample_rate, mode_frequency(config));

        assert_all_finite(&output);
        assert!(profile.early.rms > 1.0e-8, "profile={profile:?}");
        assert!(
            profile.late.rms < profile.early.rms * 0.8,
            "profile={profile:?}"
        );
        assert!(profile.harmonic_decay.len() >= 4);
    }

    #[test]
    fn lossless_boundary_scattering_is_passive_without_new_excitation() {
        let mut mesh = RectangularMesh2d::new(RectangularMesh2dConfig {
            boundary: MeshBoundaryConfig::free(0.0),
            ..RectangularMesh2dConfig::default()
        });
        mesh.process_sample(1.0);
        let initial_energy = mesh.total_energy();

        for _ in 0..256 {
            mesh.process_sample(0.0);
            assert!(
                mesh.total_energy() <= initial_energy * 1.000_5,
                "initial={}, current={}",
                initial_energy,
                mesh.total_energy()
            );
        }
    }

    #[test]
    fn boundary_loss_and_asymmetry_change_the_render() {
        let lossless = render_mesh(
            RectangularMesh2dConfig {
                boundary: MeshBoundaryConfig::fixed(0.0),
                ..RectangularMesh2dConfig::default()
            },
            18_000,
            RenderExcitation::Impulse,
        );
        let lossy = render_mesh(
            RectangularMesh2dConfig::default(),
            18_000,
            RenderExcitation::Impulse,
        );
        let asymmetric = render_mesh(
            RectangularMesh2dConfig {
                boundary: MeshBoundaryConfig::fixed_edges(0.45, 0.04, 0.16, 0.28),
                ..RectangularMesh2dConfig::default()
            },
            18_000,
            RenderExcitation::Impulse,
        );

        assert_all_finite(&lossless);
        assert_all_finite(&lossy);
        assert_all_finite(&asymmetric);
        assert!(rms_difference(&lossless[4_096..], &lossy[4_096..]) > 1.0e-6);
        assert!(rms_difference(&lossy[512..], &asymmetric[512..]) > 1.0e-6);
    }

    #[test]
    fn strike_and_pickup_positions_change_mesh_response() {
        let center_strike = render_mesh(
            RectangularMesh2dConfig {
                strike_position: MeshPoint::new(0.5, 0.5),
                pickup_position: MeshPoint::new(0.72, 0.58),
                ..RectangularMesh2dConfig::default()
            },
            12_000,
            RenderExcitation::NoiseBurst,
        );
        let off_axis_strike = render_mesh(
            RectangularMesh2dConfig {
                strike_position: MeshPoint::new(0.18, 0.73),
                pickup_position: MeshPoint::new(0.28, 0.24),
                ..RectangularMesh2dConfig::default()
            },
            12_000,
            RenderExcitation::NoiseBurst,
        );

        assert_all_finite(&center_strike);
        assert_all_finite(&off_axis_strike);
        assert!(rms_difference(&center_strike[512..], &off_axis_strike[512..]) > 1.0e-5);
    }

    #[test]
    fn wave_speed_controls_reported_physical_mode_frequency() {
        let slow = RectangularMesh2d::new(RectangularMesh2dConfig {
            wave_speed_mps: 180.0,
            ..RectangularMesh2dConfig::default()
        });
        let fast = RectangularMesh2d::new(RectangularMesh2dConfig {
            wave_speed_mps: 360.0,
            ..RectangularMesh2dConfig::default()
        });

        assert!((fast.mode_frequency_hz(1, 1) / slow.mode_frequency_hz(1, 1) - 2.0).abs() < 0.01);
    }

    #[test]
    fn reset_clears_mesh_state() {
        let config = RectangularMesh2dConfig::default();
        let mut mesh = RectangularMesh2d::new(config);
        let _ = render_response(
            config.sample_rate,
            mode_frequency(config),
            2_048,
            RenderExcitation::Impulse,
            |sample| mesh.process_sample(sample),
        );
        mesh.reset();

        let output = (0..512)
            .map(|_| mesh.process_sample(0.0))
            .collect::<Vec<_>>();

        assert_all_finite(&output);
        assert!(output.iter().all(|sample| sample.abs() < 1.0e-8));
    }
}
