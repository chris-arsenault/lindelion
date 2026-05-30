//! Runtime driver that promotes the rectangular 2D waveguide mesh to a
//! selectable resonator model. The grid is fixed, so a voice allocates its mesh
//! once at construction and every later [`MeshResonator::configure`] re-tunes it
//! in place without allocating.

use lindelion_dsp_utils::math::finite_clamp;

use super::{MeshBoundaryConfig, MeshPoint, RectangularMesh2d, RectangularMesh2dConfig};

const RUNTIME_MESH_WIDTH: usize = 14;
const RUNTIME_MESH_HEIGHT: usize = 10;

/// Physical, per-voice parameters for the 2D-mesh resonator. Every control is
/// normalised to `0..1` except `frequency_hz`, the tuned fundamental that the
/// mesh's lowest mode is steered to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVoiceParams {
    pub frequency_hz: f32,
    pub material: f32,
    pub size: f32,
    pub damping: f32,
    pub tension: f32,
    pub strike_position: f32,
    pub pickup_spread: f32,
}

impl Default for MeshVoiceParams {
    fn default() -> Self {
        Self {
            frequency_hz: 220.0,
            material: 0.5,
            size: 0.5,
            damping: 0.3,
            tension: 0.5,
            strike_position: 0.4,
            pickup_spread: 0.3,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshResonator {
    sample_rate: f32,
    mesh: RectangularMesh2d,
}

impl MeshResonator {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = crate::dsp::waveguide::core::sanitize_sample_rate(sample_rate);
        let mesh = RectangularMesh2d::new(RectangularMesh2dConfig {
            width: RUNTIME_MESH_WIDTH,
            height: RUNTIME_MESH_HEIGHT,
            sample_rate,
            ..RectangularMesh2dConfig::default()
        });
        Self { sample_rate, mesh }
    }

    pub fn configure(&mut self, params: MeshVoiceParams) {
        self.mesh
            .reconfigure(voice_config(self.sample_rate, params));
    }

    pub fn reset(&mut self) {
        self.mesh.reset();
    }

    pub fn process_sample(&mut self, excitation: f32) -> f32 {
        self.mesh.process_sample(excitation)
    }
}

/// Map the six physical controls onto a mesh configuration, steering the lowest
/// `(1, 1)` mode to the played pitch by solving for the wave speed.
fn voice_config(sample_rate: f32, params: MeshVoiceParams) -> RectangularMesh2dConfig {
    let size = clamp01(params.size);
    let tension = clamp01(params.tension);
    // `size` and `tension` are the two plate dimensions; their ratio sets the
    // inharmonic mode lattice while the wave speed re-tunes the fundamental.
    let physical_width_m = lerp(0.35, 0.95, size);
    let physical_height_m = lerp(0.35, 0.95, tension);
    let lattice =
        0.5 * ((1.0 / physical_width_m).powi(2) + (1.0 / physical_height_m).powi(2)).sqrt();
    let frequency_hz = if params.frequency_hz.is_finite() && params.frequency_hz > 0.0 {
        params.frequency_hz
    } else {
        220.0
    };
    let wave_speed_mps = (frequency_hz / lattice.max(1.0e-6)).clamp(1.0, 4_000.0);

    // `material` morphs membrane (free, drum-like) to plate (fixed, stiff) and
    // sets how hard/spread the strike couples in.
    let material = clamp01(params.material);
    let damping = lerp(0.02, 0.6, clamp01(params.damping));
    let boundary = if material < 0.5 {
        MeshBoundaryConfig::free(damping)
    } else {
        MeshBoundaryConfig::fixed(damping)
    };

    let strike = clamp01(params.strike_position);
    RectangularMesh2dConfig {
        width: RUNTIME_MESH_WIDTH,
        height: RUNTIME_MESH_HEIGHT,
        sample_rate,
        wave_speed_mps,
        physical_width_m,
        physical_height_m,
        boundary,
        strike_position: MeshPoint::new(strike, lerp(0.3, 0.7, strike)),
        pickup_position: MeshPoint::new(0.7, 0.55),
        excitation_width: lerp(0.03, 0.12, material),
        pickup_width: lerp(0.02, 0.25, clamp01(params.pickup_spread)),
    }
}

fn lerp(low: f32, high: f32, fraction: f32) -> f32 {
    low + (high - low) * fraction
}

fn clamp01(value: f32) -> f32 {
    finite_clamp(value, 0.0, 1.0, 0.0)
}
