//! Runtime driver that promotes the stiff-plate kernel to the product-facing resonator.
//! Buffers are allocated once at the maximum grid, so every [`MeshResonator::configure`]
//! re-tunes the active plate (its physically derived grid included) in place without
//! allocating.
//!
//! Control mapping (M1, ADR-0050): `size` → physical plate dimensions (0.15–0.65 m at a
//! fixed 0.82 aspect), `material` → stiffness κ across the bronze thickness span plus the
//! boundary kind (free below 0.5, simply supported above) and contact width, `damping` →
//! a geometric low-band T60 band fed through the closed-form σ₀/σ₁ calibration at the
//! per-voice κ, `tension` → the membrane-term wave speed (pure plate ↔ drum-like morph;
//! the knob midpoint is the intended nominal blend). The played note moves the strike and
//! pickup over the plate (modal color selection), never the tuning.

use lindelion_dsp_utils::math::{self, finite_clamp};

use crate::plate::spatial::{MeshPoint, SpatialWeights};
use crate::plate::{DecayTarget, PlateBoundary, PlateConfig, PlateKernel, losses_for};
use crate::sanitize_sample_rate;

/// Physical, per-voice parameters for the plate resonator. Every control is normalised to
/// `0..1` except `frequency_hz`. NB: `frequency_hz` does **not** tune the plate; it moves
/// the strike and pickup over the surface so notes select different modal color mixes
/// while `size`/`tension`/`material` carry the body timbre.
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
    kernel: PlateKernel,
    source_weights: SpatialWeights,
    pickup_weights: SpatialWeights,
    /// Previous pickup tap, crossfaded out after a re-tune: a note moves the tap cells on
    /// a still-ringing plate, and an instantaneous switch between two unrelated readings
    /// of the same field is an audible click.
    pickup_weights_old: SpatialWeights,
    pickup_fade_remaining: u32,
    contact_absorption: f32,
    /// Radiation stage state: previous cluster velocity (for the volume-acceleration
    /// difference) and the one-pole low-pass that flattens the tilt above the per-voice
    /// coincidence frequency.
    previous_velocity: f32,
    radiation_lowpass: f32,
    radiation_coeff: f32,
    /// Re-prime countdown: a re-tune re-strides the active grid, leaving the (u, u_prev)
    /// pair the tap reads internally inconsistent for a couple of samples — differencing
    /// across that ×fs would click. While counting down, the low-pass holds and the
    /// difference state re-seats.
    radiation_hold: u32,
}

impl MeshResonator {
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = sanitize_sample_rate(sample_rate);
        let params = MeshVoiceParams::default();
        let layout = voice_layout(sample_rate, params);
        let kernel = PlateKernel::new(layout.config);
        let grid = kernel.grid();
        let mut resonator = Self {
            sample_rate,
            source_weights: SpatialWeights::new(
                layout.strike,
                grid.width,
                grid.height,
                layout.excitation_width,
            ),
            pickup_weights: SpatialWeights::point_cluster(
                layout.pickup,
                grid.width,
                grid.height,
                layout.pickup_width,
            ),
            pickup_weights_old: SpatialWeights::point_cluster(
                layout.pickup,
                grid.width,
                grid.height,
                layout.pickup_width,
            ),
            pickup_fade_remaining: 0,
            contact_absorption: layout.contact_absorption,
            previous_velocity: 0.0,
            radiation_lowpass: 0.0,
            radiation_coeff: 1.0,
            radiation_hold: RADIATION_HOLD_SAMPLES,
            kernel,
        };
        resonator.configure(params);
        resonator
    }

    pub fn configure(&mut self, params: MeshVoiceParams) {
        let layout = voice_layout(self.sample_rate, params);
        let old_grid = self.kernel.grid();
        self.kernel.reconfigure(layout.config);
        let grid = self.kernel.grid();
        // The differencer hold is for true grid re-strides (patch edits), where the state
        // pair becomes momentarily inconsistent. Note retunes keep the grid and only move
        // the taps — the pickup crossfade already makes the blended velocity continuous,
        // and holding there freezes the output under a fresh attack and *creates* a click.
        let stride_changed = grid.width != old_grid.width || grid.height != old_grid.height;
        self.source_weights.recompute(
            layout.strike,
            grid.width,
            grid.height,
            layout.excitation_width,
        );
        std::mem::swap(&mut self.pickup_weights, &mut self.pickup_weights_old);
        self.pickup_weights.recompute_point_cluster(
            layout.pickup,
            grid.width,
            grid.height,
            layout.pickup_width,
        );
        self.pickup_fade_remaining = PICKUP_FADE_SAMPLES;
        self.contact_absorption = layout.contact_absorption;
        self.radiation_coeff = radiation_lowpass_coeff(layout.config.kappa, self.sample_rate);
        if stride_changed {
            self.radiation_hold = RADIATION_HOLD_SAMPLES;
        }
        self.kernel
            .set_drive_normalization(BLOOM_DRIVE_NORMALIZATION);
        self.kernel.set_bloom_depth(BLOOM_DEPTH);
        // Glide dose ∝ 1/κ² (Berger: relative stiffening scales with the inverse square
        // of thickness): thin gongs keep most of their pitch glide, stiff rides barely
        // move. GLIDE_DOSE_SCALE compensates the floor-clipped DC subtraction (quiet
        // edges cannot soften far enough to fully balance the mean).
        let kappa_ratio = PLATE_KAPPA_SOFT / layout.config.kappa;
        self.kernel
            .set_glide_dose(kappa_ratio * kappa_ratio * GLIDE_DOSE_SCALE);
    }

    pub fn reset(&mut self) {
        self.kernel.reset();
        self.previous_velocity = 0.0;
        self.radiation_lowpass = 0.0;
        self.radiation_hold = RADIATION_HOLD_SAMPLES;
        self.pickup_fade_remaining = 0;
    }

    pub fn process_sample(&mut self, excitation: f32) -> f32 {
        let excitation = math::snap_to_zero(excitation);
        let contact_amount = if excitation.abs() > f32::EPSILON {
            let contact_envelope = excitation.abs().sqrt().min(1.0);
            self.contact_absorption * contact_envelope * STRIKE_CONTACT_MOTION_DAMPING
        } else {
            0.0
        };
        // Read the (crossfading) taps from the current state, then step — preserving the
        // pickup-ordering contract.
        let velocity_new = self.kernel.weighted_velocity(&self.pickup_weights);
        let velocity = if self.pickup_fade_remaining > 0 {
            let velocity_old = self.kernel.weighted_velocity(&self.pickup_weights_old);
            let fade = self.pickup_fade_remaining as f32 / PICKUP_FADE_SAMPLES as f32;
            self.pickup_fade_remaining -= 1;
            velocity_old * fade + velocity_new * (1.0 - fade)
        } else {
            velocity_new
        };
        self.kernel.step_voice(
            excitation * EXCITATION_FORCE_SCALE,
            &self.source_weights,
            contact_amount,
        );
        // Radiation stage (M2, ADR-0050): far-field pressure follows volume acceleration
        // below coincidence and flattens above it. Band-limited differentiator: first
        // difference × fs through a one-pole low-pass at f_c(κ), normalized so the
        // radiated level matches the raw velocity tap at RADIATION_REFERENCE_HZ.
        if self.radiation_hold == 0 {
            let acceleration = (velocity - self.previous_velocity) * self.sample_rate;
            self.radiation_lowpass +=
                self.radiation_coeff * (acceleration - self.radiation_lowpass);
        } else {
            // Post-re-tune hold: differencing across the re-strided state would click;
            // hold the low-pass (no zero-slam on a ringing tail) and re-seat the
            // difference state until the pair is consistent again.
            self.radiation_hold -= 1;
        }
        self.previous_velocity = velocity;
        self.radiation_lowpass * RADIATION_LEVEL_NORM
    }
}

/// Smoothed mean bending strain → bloom drive (M3): calibrated so a full-velocity hard
/// strike on the default voice peaks near 0.7 drive (measured 7.0e-7 mean strain).
const BLOOM_DRIVE_NORMALIZATION: f32 = 2.65e6;

/// Current bloom drive (calibration/observability).
#[cfg(test)]
impl MeshResonator {
    pub(crate) fn tension_drive(&self) -> f32 {
        self.kernel.tension_drive()
    }
}

/// Minimum σ₁ when the bloom is enabled (≈ the Crash voicing's value, which is stable).
const BLOOM_SIGMA1_FLOOR: f32 = 2.8e-4;

/// Fraction of the stability headroom the bloom spends at full drive: ≈0.1 puts the
/// full-velocity low-mode pitch glide near the real-gong ~25% (the full budget would
/// glide tension-dominated lows past 2×).
const BLOOM_DEPTH: f32 = 0.5;

/// Scale on the κ-derived glide dose (see `configure`).
const GLIDE_DOSE_SCALE: f32 = 0.3;

/// Reserved tension-modulation headroom for the bloom (M3): the depth budget the cascade
/// may spend, uniform across voicings because the grid is sized for it.
const BLOOM_HEADROOM_SPEED: f32 = 120.0;

/// Crossfade length between the old and new pickup taps after a re-tune (~2.7 ms): long
/// enough to be click-free, short enough to be inaudible as movement.
const PICKUP_FADE_SAMPLES: u32 = 512;

/// Samples the radiation differencer holds after a re-tune (the re-strided state pair
/// needs two clean steps to become consistent; one spare).
const RADIATION_HOLD_SAMPLES: u32 = 3;

/// Speed of sound in air, for the coincidence frequency `f_c = c_air²/(2πκ)`.
const SPEED_OF_SOUND: f32 = 343.0;
/// The radiated level matches the raw velocity tap at this frequency (the tilt pivots
/// here: lows sit below the old tap level, treble above). Pivoting at the low body band
/// keeps the overall strike level in the audibility window the invariant guards pin —
/// the tilt itself is unchanged by the pivot choice.
const RADIATION_REFERENCE_HZ: f32 = 100.0;
/// Output makeup over the bare pivot normalization: the M2 audition measured renders
/// peaking near −25 dBFS with their (correct, per-law) tails inaudible at listening
/// level; +12 dB lands the loudest voicings near the soft-limit knee like the other
/// Lamath families.
const RADIATION_MAKEUP: f32 = 0.7;
const RADIATION_LEVEL_NORM: f32 =
    RADIATION_MAKEUP / (std::f32::consts::TAU * RADIATION_REFERENCE_HZ);

/// One-pole coefficient for the per-voice coincidence corner, clamped inside Nyquist.
fn radiation_lowpass_coeff(kappa: f32, sample_rate: f32) -> f32 {
    let coincidence_hz = (SPEED_OF_SOUND * SPEED_OF_SOUND
        / (std::f32::consts::TAU * kappa.max(0.05)))
    .min(sample_rate * 0.45);
    1.0 - (-std::f32::consts::TAU * coincidence_hz / sample_rate).exp()
}

/// Plugin excitation (a unit-scale strike pulse) → physical force density per unit
/// surface mass. Calibrated against the M1 unipolar contact pulses (impulse ≈ Σ samples ≈
/// 10 sample-units for the hard stick): a full-velocity strike must land the raw body
/// output in the linear zone of the output soft-limit — the membrane-era 1e5 was tuned
/// for sign-alternating wavelets whose in-band impulse was a tiny fraction of their peak.
const EXCITATION_FORCE_SCALE: f32 = 1.0e4;
/// Local motion absorbed under the stick at full contact (membrane value carried over).
const STRIKE_CONTACT_MOTION_DAMPING: f32 = 0.62;

/// Physical plate the `size` control spans, in meters. The map is piecewise: above
/// `size = 0.2` it is the original linear span (≈10"–26", unchanged for the
/// audition-approved voicings); the bottom segment extends down to bell/triangle
/// register (a 6 cm stiff plate's fundamental sits ≈ 2 kHz — the old 15 cm floor could
/// not leave sheet-metal register, M4 Triangle verdict).
const PLATE_MIN_LENGTH_M: f32 = 0.15;
const PLATE_BELL_LENGTH_M: f32 = 0.06;
const PLATE_BELL_KNEE: f32 = 0.2;
const PLATE_MAX_LENGTH_M: f32 = 0.65;
/// Fixed aspect: breaks the square plate's degenerate mode pairs.
const PLATE_ASPECT: f32 = 0.82;
/// Stiffness span of the `material` control: `κ = √(D/ρh) ∝ thickness` for a given alloy,
/// so bronze at 0.6–2.0 mm spans κ ≈ 0.64–2.14 m²/s (1 mm ≈ 1.07, ADR-0050). The modal
/// band top is κ-independent (`f_max ≈ fs/π`); material moves mode density and spacing.
/// Thin (soft) + large voicings hit the cell budget and coarsen h — accepted behavior.
const PLATE_KAPPA_SOFT: f32 = 0.64;
const PLATE_KAPPA_HARD: f32 = 2.14;
/// Membrane-term wave speed at full `tension` (drum-like end of the morph).
const PLATE_TENSION_MAX_SPEED: f32 = 80.0;
/// Reference frequencies for the two-target loss calibration.
const PLATE_T60_LOW_HZ: f32 = 200.0;
const PLATE_T60_HIGH_HZ: f32 = 4_000.0;
/// High-band T60 as a fraction of the low band: highs die first (plate physics). Raised
/// from 0.22 on the M2 audition verdict — the upper partials carry the audible tail.
const PLATE_T60_HIGH_FRACTION: f32 = 0.35;

/// Longest / shortest ring the `damping` control spans, as a −60 dB decay time at the low
/// reference band. The control maps geometrically (perceptually uniform in decay ratio),
/// so **no value in `0..1` is a dead thud**. The 10 s ceiling is the real-cymbal band
/// (a large crash rings 5–15 s; the membrane-era 4 s ceiling read as "no tail" in the
/// M2 audition).
const MESH_T60_MAX_S: f32 = 10.0;
const MESH_T60_MIN_S: f32 = 0.30;

/// Low-band −60 dB ring time the `damping` control resolves to. Exact by construction:
/// the closed-form σ₀/σ₁ calibration hits this target at `PLATE_T60_LOW_HZ`.
pub(crate) fn damping_t60_low(control: f32) -> f32 {
    MESH_T60_MAX_S * (MESH_T60_MIN_S / MESH_T60_MAX_S).powf(clamp01(control))
}

pub(crate) fn damping_targets(control: f32) -> (DecayTarget, DecayTarget) {
    let t60_low = damping_t60_low(control);
    (
        DecayTarget {
            frequency_hz: PLATE_T60_LOW_HZ,
            t60_s: t60_low,
        },
        DecayTarget {
            frequency_hz: PLATE_T60_HIGH_HZ,
            t60_s: t60_low * PLATE_T60_HIGH_FRACTION,
        },
    )
}

struct VoiceLayout {
    config: PlateConfig,
    strike: MeshPoint,
    pickup: MeshPoint,
    excitation_width: f32,
    pickup_width: f32,
    contact_absorption: f32,
}

/// Map the six physical controls onto a plate configuration plus strike/pickup layout.
fn voice_layout(sample_rate: f32, params: MeshVoiceParams) -> VoiceLayout {
    let size = clamp01(params.size);
    let material = clamp01(params.material);
    let tension = clamp01(params.tension);
    let length = if size >= PLATE_BELL_KNEE {
        lerp(PLATE_MIN_LENGTH_M, PLATE_MAX_LENGTH_M, size)
    } else {
        let knee_length = lerp(PLATE_MIN_LENGTH_M, PLATE_MAX_LENGTH_M, PLATE_BELL_KNEE);
        lerp(PLATE_BELL_LENGTH_M, knee_length, size / PLATE_BELL_KNEE)
    };
    let kappa = lerp(PLATE_KAPPA_SOFT, PLATE_KAPPA_HARD, material);
    let (low, high) = damping_targets(params.damping);
    let tension_speed = lerp(0.0, PLATE_TENSION_MAX_SPEED, tension);
    let (sigma0, sigma1) = losses_for(low, high, kappa, tension_speed);
    // Bloom stability floor: the cascade pumps energy upward, and σ₁ is its drain — a
    // voicing whose σ₁ falls below the pump's accumulation rate self-oscillates (observed
    // on the Gong: 3.6× weaker σ₁ than the stable Crash). The floor costs only some
    // high-band ring length on the darkest voicings.
    let sigma1 = sigma1.max(BLOOM_SIGMA1_FLOOR);
    // The membrane's "fixed" edge was a pure sign-flip reflection — a displacement node
    // with a free slope, i.e. the simply-supported analog. Clamped additionally pins the
    // slope and opens wide low-response zones around edge strikes, so the pinned kind is
    // simply supported.
    let boundary = if material < 0.5 {
        PlateBoundary::Free
    } else {
        PlateBoundary::SimplySupported
    };
    let note_position = mesh_note_position(params.frequency_hz);
    VoiceLayout {
        config: PlateConfig {
            length_x_m: length,
            length_y_m: length * PLATE_ASPECT,
            kappa,
            tension_speed,
            tension_headroom_speed: BLOOM_HEADROOM_SPEED,
            sigma0,
            sigma1,
            sample_rate,
            boundary,
        },
        strike: mesh_note_strike_position(params.strike_position, note_position),
        pickup: mesh_note_pickup_position(params.strike_position, note_position),
        excitation_width: contact_excitation_width(material),
        pickup_width: lerp(0.015, 0.16, clamp01(params.pickup_spread)),
        contact_absorption: strike_contact_absorption(material, params.damping, size, tension),
    }
}

/// How far across the plate the played note moves the strike position (peak-to-peak). The
/// note maps to a ±half-this offset around the patch strike position.
const MESH_NOTE_STRIKE_SPREAD: f32 = 0.7;
/// Vertical strike travel (peak-to-peak) driven by the note's independent Lissajous path.
const MESH_NOTE_STRIKE_Y_SPREAD: f32 = 0.62;
/// How much the strike control biases the independent vertical path.
const MESH_STRIKE_CONTROL_Y_BIAS: f32 = 0.24;
/// Horizontal pickup travel (peak-to-peak), intentionally opposing the strike's note
/// travel to expose more modal combinations.
const MESH_NOTE_PICKUP_X_SPREAD: f32 = 0.36;
/// Vertical pickup travel (peak-to-peak) driven by a different note path than the strike.
const MESH_NOTE_PICKUP_Y_SPREAD: f32 = 0.48;
/// How much the strike control nudges the pickup away from the biased strike side.
const MESH_STRIKE_CONTROL_PICKUP_BIAS: f32 = 0.16;
/// Minimum normalized source-to-pickup separation. The pickup is a radiating aperture, not
/// a contact mic at the strike point; keeping it outside the strike footprint prevents the
/// hammer impulse from becoming a two-sample output spike when the note path crosses the
/// pickup path.
pub(crate) const MESH_MIN_STRIKE_PICKUP_DISTANCE: f32 = 0.34;
/// Keep moving strike/pickup targets off the exact boundaries and centerline singular
/// spots while preserving most of the playable plate area.
const MESH_POSITION_INSET: f32 = 0.06;

fn strike_contact_absorption(material: f32, damping: f32, size: f32, tension: f32) -> f32 {
    let stiffness = clamp01(material);
    let damping = clamp01(damping);
    let density = 0.5 * (clamp01(size) + clamp01(tension));
    let contact_impedance = 0.7 * stiffness + 0.3 * damping;
    let dense_plate_escape = 1.0 - 0.55 * density;
    finite_clamp(
        0.04 + 0.50 * contact_impedance * dense_plate_escape + 0.18 * damping,
        0.0,
        0.55,
        0.2,
    )
}

/// Normalised position of a played pitch across the C2–C6 register (`0..1`), used to move
/// the strike position with the note. Returns the centre (0.5) for an invalid pitch.
fn mesh_note_position(frequency_hz: f32) -> f32 {
    const C2_HZ: f32 = 65.41;
    if frequency_hz > 0.0 && frequency_hz.is_finite() {
        clamp01((frequency_hz / C2_HZ).log2() / 4.0)
    } else {
        0.5
    }
}

fn mesh_note_strike_position(strike_control: f32, note_position: f32) -> MeshPoint {
    let strike_control = clamp01(strike_control);
    let note_position = clamp01(note_position);
    let x = strike_control + (note_position - 0.5) * MESH_NOTE_STRIKE_SPREAD;
    let y = 0.5
        + (strike_control - 0.5) * MESH_STRIKE_CONTROL_Y_BIAS
        + 0.5
            * MESH_NOTE_STRIKE_Y_SPREAD
            * (std::f32::consts::TAU * (1.5 * note_position + 0.25)).sin();
    MeshPoint::new(inset01(x), inset01(y))
}

fn mesh_note_pickup_position(strike_control: f32, note_position: f32) -> MeshPoint {
    let strike_control = clamp01(strike_control);
    let note_position = clamp01(note_position);
    let strike = mesh_note_strike_position(strike_control, note_position);
    let x = 0.7
        - (note_position - 0.5) * MESH_NOTE_PICKUP_X_SPREAD
        - (strike_control - 0.5) * MESH_STRIKE_CONTROL_PICKUP_BIAS;
    let y = 0.55
        + 0.5
            * MESH_NOTE_PICKUP_Y_SPREAD
            * (std::f32::consts::TAU * (1.25 * note_position + 0.375)).sin();
    pickup_position_with_min_separation(strike, MeshPoint::new(inset01(x), inset01(y)))
}

fn pickup_position_with_min_separation(strike: MeshPoint, pickup: MeshPoint) -> MeshPoint {
    let dx = pickup.x - strike.x;
    let dy = pickup.y - strike.y;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance >= MESH_MIN_STRIKE_PICKUP_DISTANCE {
        return pickup;
    }

    let (unit_x, unit_y) = if distance > f32::EPSILON {
        (dx / distance, dy / distance)
    } else {
        (1.0, 0.0)
    };
    let along_path = MeshPoint::new(
        inset01(strike.x + unit_x * MESH_MIN_STRIKE_PICKUP_DISTANCE),
        inset01(strike.y + unit_y * MESH_MIN_STRIKE_PICKUP_DISTANCE),
    );
    if mesh_point_distance(strike, along_path) >= MESH_MIN_STRIKE_PICKUP_DISTANCE - 0.005 {
        return along_path;
    }

    [
        MeshPoint::new(
            inset01(strike.x + MESH_MIN_STRIKE_PICKUP_DISTANCE),
            strike.y,
        ),
        MeshPoint::new(
            inset01(strike.x - MESH_MIN_STRIKE_PICKUP_DISTANCE),
            strike.y,
        ),
        MeshPoint::new(
            strike.x,
            inset01(strike.y + MESH_MIN_STRIKE_PICKUP_DISTANCE),
        ),
        MeshPoint::new(
            strike.x,
            inset01(strike.y - MESH_MIN_STRIKE_PICKUP_DISTANCE),
        ),
    ]
    .into_iter()
    .filter(|candidate| {
        mesh_point_distance(strike, *candidate) >= MESH_MIN_STRIKE_PICKUP_DISTANCE - 0.005
    })
    .max_by(|left, right| {
        pickup_alignment(strike, *left, unit_x, unit_y)
            .total_cmp(&pickup_alignment(strike, *right, unit_x, unit_y))
    })
    .unwrap_or(along_path)
}

fn mesh_point_distance(a: MeshPoint, b: MeshPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn pickup_alignment(strike: MeshPoint, pickup: MeshPoint, unit_x: f32, unit_y: f32) -> f32 {
    (pickup.x - strike.x) * unit_x + (pickup.y - strike.y) * unit_y
}

const CONTACT_WIDTH_SOFT: f32 = 0.13;
const CONTACT_WIDTH_HARD: f32 = 0.018;
const CONTACT_HARDNESS_CURVE: f32 = 1.4;

fn contact_excitation_width(material: f32) -> f32 {
    let hardness = clamp01(material).powf(CONTACT_HARDNESS_CURVE);
    lerp(CONTACT_WIDTH_SOFT, CONTACT_WIDTH_HARD, hardness)
}

fn lerp(low: f32, high: f32, fraction: f32) -> f32 {
    low + (high - low) * fraction
}

fn clamp01(value: f32) -> f32 {
    finite_clamp(value, 0.0, 1.0, 0.0)
}

fn inset01(value: f32) -> f32 {
    finite_clamp(value, MESH_POSITION_INSET, 1.0 - MESH_POSITION_INSET, 0.5)
}

#[cfg(test)]
#[path = "runtime/bloom_tests.rs"]
mod bloom_tests;
#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;
