// Physical driver / contact configs (M8/M9). `include!`d into `patch.rs` so these
// types stay in the `crate::patch` module with unchanged paths and visibility; split
// out only to keep `patch.rs` under the repository file-size limit.

/// Selectable physical driver feeding the waveguide resonator (M8, ADR-0017). The
/// default `Sample` driver is a transparent pass-through of the existing sample /
/// sidechain excitation, so a patch without a driver behaves exactly as before.
/// `Pick` is a feed-forward contact transient; `Reed` is a self-oscillating wind
/// driver coupled two-way to the bore. Each variant's controls are normalised `0..1`
/// and mapped to their physical range inside the driver DSP (M11 calibrates ranges).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum DriverConfig {
    #[default]
    Sample,
    Pick(PickConfig),
    Reed(ReedConfig),
    Bow(BowConfig),
}

/// Pick/hammer contact driver: a force-shaped contact transient that brightens with
/// playing effort. The strike location stays the waveguide's own strike-position
/// control; this shapes the contact itself.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PickConfig {
    /// Contact hardness `0..1`: softer rounds the contact (darker), harder sharpens it.
    pub hardness: f32,
    /// Contact time `0..1`: longer spreads the contact pulse for a mellower attack.
    pub contact_time: f32,
}

impl Default for PickConfig {
    fn default() -> Self {
        Self {
            hardness: 0.5,
            contact_time: 0.5,
        }
    }
}

/// Reed/lip pressure-flow driver: a self-oscillating wind driver coupled two-way to
/// the bore. Mouth pressure is mapped from the effort bus; below a pressure threshold
/// the reed is quiescent, above it the bore self-oscillates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReedConfig {
    /// How much playing effort drives mouth pressure `0..1`.
    pub pressure_depth: f32,
    /// Reed stiffness `0..1`: sets the reed's natural cutoff (brighter when stiffer).
    pub stiffness: f32,
    /// Embouchure `0..1`: the reed's rest opening / bias toward the closing regime.
    pub embouchure: f32,
}

impl Default for ReedConfig {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            stiffness: 0.5,
            embouchure: 0.5,
        }
    }
}

/// Bow friction driver: a continuous stick-slip friction excitation coupled to the
/// string's velocity at the contact, so a held note sustains a bowed (Helmholtz)
/// tone. Mouth-pressure has no analogue here — the player effort sets the bow normal
/// force; below enough force the string is barely driven, above it a stable limit
/// cycle builds. (Ranges are calibrated in M11 P10.)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BowConfig {
    /// How much playing effort drives the bow normal force `0..1` (heavier = louder,
    /// brighter, more locked-in).
    pub pressure_depth: f32,
    /// Bow speed `0..1`: the bow's velocity magnitude, setting the limit-cycle
    /// amplitude and brightness (faster = brighter/louder).
    pub bow_speed: f32,
    /// Friction sharpness `0..1`: the stick-slip transition width. Smoother is a
    /// pure sustained tone; sharper is a scratchier, more articulate attack.
    pub friction: f32,
}

impl Default for BowConfig {
    fn default() -> Self {
        Self {
            pressure_depth: 0.5,
            bow_speed: 0.5,
            friction: 0.5,
        }
    }
}

/// Coupling/contact stage between the driver and the resonator (M9), shaping a strike
/// into a pick (tight) or a strum (spread). Both controls default to the transparent
/// pre-M9 values, so a default patch is unchanged; waveguide String/Tube path only.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ContactConfig {
    /// Excitation spread `0..1`: `0` is a tight pick (the narrow pre-M9 contact), `1`
    /// is a wide strum. Playing effort widens it further (the gesture half of M9).
    pub spread: f32,
    /// Contact time `0..1`: `0` is an instant contact (sharp onset, transparent),
    /// higher spreads the contact in time for a mellower, darker attack.
    pub contact_time: f32,
}

impl Default for ContactConfig {
    fn default() -> Self {
        Self {
            spread: 0.0,
            contact_time: 0.0,
        }
    }
}
