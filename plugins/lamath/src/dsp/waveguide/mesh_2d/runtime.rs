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
            // Lightly damped by default so the mesh rings with a long metallic
            // shimmer (M11 P2 step 3); the squared map in `voice_config` puts this
            // near the ~3 s ring-out region.
            damping: 0.05,
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

    /// Forward the measured-energy bus (M2) to the mesh's geometric (von Kármán)
    /// coupling (M6). Set once per host sample by the resonator engine.
    pub fn set_geometric_drive(&mut self, drive: f32) {
        self.mesh.set_geometric_drive(drive);
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
    // A waveguide mesh propagates one cell per sample, so its boundaries reflect
    // ~2000×/s: ringing for seconds needs a near-unity reflection (damping ~1e-3),
    // while even a few-percent damping dies in ~100 ms. Map the control *cubed*
    // from a very low floor so the long-ring metallic-shimmer region (the voice
    // this is built around, M11 P2 step 3) gets the resolution and a moderate
    // control still damps hard.
    let damping = lerp(0.000_08, 0.5, clamp01(params.damping).powi(3));
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

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_dsp_utils::analysis::{
        assert_all_finite, peak_abs, rms, spectral_centroid_trajectory,
    };

    fn render_default_mesh(sample_rate: f32, seconds: f32) -> Vec<f32> {
        let mut mesh = MeshResonator::new(sample_rate);
        mesh.configure(MeshVoiceParams::default());
        let len = (sample_rate * seconds) as usize;
        (0..len)
            .map(|index| mesh.process_sample((index == 0) as u8 as f32))
            .collect()
    }

    /// Per-window RMS envelope (`window` samples, non-overlapping).
    fn rms_envelope(samples: &[f32], window: usize) -> Vec<f32> {
        let mut env = Vec::new();
        let mut start = 0;
        while start + window <= samples.len() {
            env.push(rms(&samples[start..start + window]));
            start += window;
        }
        env
    }

    /// Audible ring-out: time (s) at which the RMS envelope last sits above
    /// `floor_db` below its peak window. Robust to the mesh's two-rate decay (a
    /// linear T60 fit is fooled by the fast high-mode death over the slow tail).
    fn ring_out_seconds(samples: &[f32], sample_rate: f32, window: usize, floor_db: f32) -> f32 {
        let env = rms_envelope(samples, window);
        let peak = env.iter().copied().fold(0.0_f32, f32::max).max(1.0e-12);
        let threshold = peak * 10.0_f32.powf(floor_db / 20.0);
        let last = env
            .iter()
            .rposition(|&level| level > threshold)
            .unwrap_or(0);
        (last * window + window / 2) as f32 / sample_rate
    }

    /// M11 P2 step 3: the default mesh voice rings with a long metallic shimmer
    /// (the old uniform-boundary mesh died to silence in ~100 ms), its high modes
    /// die first via the frequency-shaped boundary (centroid falls), and the long
    /// near-lossless ring stays bounded and non-growing.
    #[cfg_attr(
        not(feature = "integration-tests"),
        ignore = "see make test-integration"
    )]
    #[test]
    fn default_mesh_rings_with_metallic_shimmer_and_stays_stable() {
        let sample_rate = 48_000.0;
        let output = render_default_mesh(sample_rate, 5.0);
        assert_all_finite(&output);
        assert!(peak_abs(&output) < 4.0, "peak_abs={}", peak_abs(&output));

        // Ring-out far past the old ~100 ms dead tail (target ~2–4 s shimmer).
        let ring_out = ring_out_seconds(&output, sample_rate, 4_800, -40.0);
        assert!(
            (1.5..=4.0).contains(&ring_out),
            "mesh -40 dB ring-out: {ring_out}"
        );

        // High modes die first → the spectral centroid falls over the tail.
        let trajectory = spectral_centroid_trajectory(&output, sample_rate, 2_048, 16_384);
        assert!(
            trajectory.len() >= 2,
            "trajectory too short: {trajectory:?}"
        );
        assert!(
            trajectory.first().unwrap() > trajectory.last().unwrap(),
            "mesh tail should darken: {trajectory:?}"
        );

        // Non-growing.
        let early = rms(&output[..24_000]);
        let late = rms(&output[192_000..]);
        assert!(
            late < early,
            "mesh tail should decay, not grow: early={early}, late={late}"
        );
    }
}
