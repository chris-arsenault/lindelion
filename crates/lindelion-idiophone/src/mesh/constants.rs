//! Mesh tuning constants.

pub(super) const MIN_MESH_SIZE: usize = 3;
/// Maximum active grid the mesh can be configured to. Buffers are allocated once at
/// this size; `size`/`tension` select a smaller active sub-region (no reallocation).
/// Grid cell count is the timbral density lever in a unit-delay waveguide mesh — a
/// large grid carries a dense, inharmonic, cymbal-like spectrum, a small one a sparse,
/// near-pitched triangle. CPU is `O(width·height)` per sample; ample for a single
/// idiophone voice.
pub(super) const MAX_MESH_WIDTH: usize = 64;
pub(super) const MAX_MESH_HEIGHT: usize = 48;
pub(super) const MAX_MESH_CELLS: usize = MAX_MESH_WIDTH * MAX_MESH_HEIGHT;

/// Measured-energy (RMS) at which the geometric (von Kármán) coupling reaches its
/// target depth; the squared, normalized drive `(energy/REF)^2` keeps soft strikes
/// linear and concentrates the bloom on hard ones. Calibrated to the measured per-voice
/// energy bus so a full-velocity Mesh strike sits near ≈0.7 drive (the upward modal
/// bloom a hard gong/cymbal makes). The larger active grid spreads the strike energy
/// over more cells, lowering the measured RMS, so this REF was dropped from the old
/// 14×10-grid value (0.013) to keep the bloom engaging at musical strike levels.
pub(super) const GEOMETRIC_ENERGY_REF: f32 = 0.003;
/// Clamp on the normalized squared energy term (the coupling depth at peak energy).
pub(super) const GEOMETRIC_MAX_DRIVE: f32 = 1.0;
/// Maximum rotation `sin` factor at full coupling: the fraction of the low mode's
/// amplitude rotated up into the high-spatial-frequency mode each junction.
/// Bounded below 1 so the per-junction transfer stays gentle and the scheme stays
/// stable. Specified as `sin` (not an angle) so the rotation needs only a `sqrt`,
/// not `sin_cos` — cheap enough for the per-junction inner loop while staying
/// exactly energy-conserving (`cos = sqrt(1 - sin^2)`, so `sin^2 + cos^2 = 1`).
pub(super) const GEOMETRIC_MAX_SIN: f32 = 0.3;
/// Maps the local junction displacement to the [0,1] amplitude factor: high-
/// pressure junctions couple most (the large-deflection geometric nonlinearity).
pub(super) const GEOMETRIC_AMPLITUDE_SENS: f32 = 6.0;
/// Blend from pure aperture pressure toward a bending/curvature radiation term.
/// Cymbals do not radiate only by summing signed displacement over a broad area:
/// high-spatial-frequency bending also couples to air. The curvature tap keeps
/// dense plates from collapsing into a dark low-mode area integral after the
/// source/pickup normalization fix.
pub(super) const CURVATURE_RADIATION_GAIN: f32 = 4.0;
/// Dense cymbal meshes need a separate high-spatial-frequency radiation path:
/// a broad signed aperture carries body level, while this density-scaled narrow
/// tap lets shimmer/bloom radiate without adding a family output gain stage.
pub(super) const SHIMMER_RADIATION_GAIN: f32 = 12.0;
pub(super) const SHIMMER_RADIATION_DENSITY_EXP: f32 = 1.5;
pub(super) const SHIMMER_PICKUP_WIDTH_SCALE: f32 = 0.45;
pub(super) const SHIMMER_PICKUP_WIDTH_MIN: f32 = 0.012;
pub(super) const STRIKE_CONTACT_MAX_LOSS_FRACTION: f32 = 0.35;
pub(super) const STRIKE_CONTACT_MOTION_DAMPING: f32 = 0.62;
pub(super) const STRIKE_CONTACT_PRESSURE_DAMPING: f32 = 0.10;
