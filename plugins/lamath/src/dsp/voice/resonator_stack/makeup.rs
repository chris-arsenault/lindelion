//! M11 P9 per-family output makeup. Split out of `resonator_stack.rs` to keep that
//! file within the 600-line size cap.

use crate::{DriverConfig, ResonatorConfig, WaveguideStyle};

/// Per-family output makeup. The resonator families emerge from the core at wildly
/// different levels — a single full-velocity voice peaks at Modal ≈ −1.7 dBFS but
/// String/Tube/Mesh at ≈ −36/−28/−40 dBFS (a ~38 dB spread), and the waveguide families
/// run ~30 dB under a healthy level. These makeups, applied **output-side of the M2
/// energy tap** (so P8's energy calibration is untouched), bring each family's single
/// full-velocity voice to ≈ −6 dBFS peak so the families are loudness-matched and the
/// instrument sits at a usable level. Modal is the reference (its sound is untouched; its
/// level only trims down). Matched on **peak**, not RMS — the families' crest factors
/// differ ~8× (plucky String vs sustained Modal), so peak-matching keeps any family from
/// clipping while RMS-matching would blow the plucky families' transients past full scale.
const MODAL_OUTPUT_MAKEUP: f32 = 0.6;
const STRING_OUTPUT_MAKEUP: f32 = 32.0;
const TUBE_OUTPUT_MAKEUP: f32 = 12.0;
const MESH_OUTPUT_MAKEUP: f32 = 49.0;

pub(super) fn family_output_makeup(config: ResonatorConfig) -> f32 {
    match config {
        ResonatorConfig::Modal(_) => MODAL_OUTPUT_MAKEUP,
        ResonatorConfig::Mesh(_) => MESH_OUTPUT_MAKEUP,
        ResonatorConfig::Waveguide(waveguide) => match waveguide.style {
            WaveguideStyle::String => STRING_OUTPUT_MAKEUP,
            WaveguideStyle::Tube => TUBE_OUTPUT_MAKEUP,
        },
    }
}

/// A self-oscillating driver (bow/reed) sustains a far hotter resonator output than the
/// struck/plucked excitation the per-family makeup is calibrated for, so it is trimmed
/// back here to land at a forte level rather than slamming the master clipper. The
/// feed-forward drivers (sample/pick) match the calibration and pass at unity.
fn driver_output_trim(driver: DriverConfig) -> f32 {
    match driver {
        DriverConfig::Bow(_) => 0.2,
        // The reed terminates the mouth and self-oscillates to a far hotter level than the
        // struck excitation `TUBE_OUTPUT_MAKEUP` (12×) was calibrated for — at unity it ran the
        // tube ~+15 dB over the other families, slamming the master clipper into a uniform
        // bit-crushed saw. This trim brings the reed-driven tube back to the matched forte level.
        DriverConfig::Reed(_) => 0.13,
        DriverConfig::Sample | DriverConfig::Pick(_) => 1.0,
    }
}

/// The output makeup for one resonator slot — its family makeup, scaled by the driver
/// trim on the waveguide path (the driver only acts on waveguides). Applied per slot
/// *before* the A/B mix so a loud family (Modal) and a quiet one (a waveguide) each land
/// at the matched level rather than sharing one blended makeup that over-amplifies the
/// loud one; and applied after the energy tap so the M2 bus stays the raw physical level.
pub(super) fn resonator_output_makeup(config: ResonatorConfig, driver: DriverConfig) -> f32 {
    match config {
        ResonatorConfig::Waveguide(_) => family_output_makeup(config) * driver_output_trim(driver),
        _ => family_output_makeup(config),
    }
}
