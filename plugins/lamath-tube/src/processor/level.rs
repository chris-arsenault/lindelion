//! Native output-level calibration (the radiated makeup constants and the per-fingering gain)
//! and the vented register's air-support effort floor.

#![allow(clippy::wildcard_imports)]

use super::*;

// Native level calibration on the radiated output (radiation-only: nothing here feeds back into
// the bore or reed, so the oscillation physics are untouched). The raw model played ~18 dB
// under the -12 dBFS target at output 0, with the low register additionally tilting down ~9 dB
// toward the break (constant per-transit loss means loss-per-second grows with pitch) and the
// vented register calibrated ~14 dB hotter at the break boundary — an audible level snap in
// runs. The makeup is a flat gain on the radiated sum, plus a low-register-only slope
// proportional to sounding frequency (a compact source's radiation efficiency rises with
// frequency) that levels the chalumeau. The vented register anchors the instrument's loudness
// (auditioned as correct at -12.5 dBFS sustained); the chalumeau sits ~4 dB under it by ear —
// its dense, bright spectrum reads louder than RMS/LUFS suggest, and an equal-RMS match was
// auditioned as clearly too loud below the break.
pub(super) const RADIATED_LEVEL_MAKEUP: f32 = 4.467;
pub(super) const LOW_REGISTER_LEVEL_MATCH: f32 = 3.42;
/// Vented-register air-support floor on the effort line. The choked register mode has an
/// oscillation threshold near effort ~0.55 (measured at C5: -63 dBFS at velocity 0.3, -21 at
/// 0.5, full level by 0.6) — a player keeps air pressure above the second-mode threshold and
/// makes soft dynamics with a narrower pressure span, so the playable velocity range maps onto
/// the speaking region instead of falling off the cliff mid-run. Applies to the whole effort
/// line (reed pressure window, brightness steepening, loop-phase compensation) so the supported
/// breath stays physically consistent. The low register keeps the full span.
pub(super) const VENTED_EFFORT_FLOOR: f32 = 0.55;

impl TubeProcessor<'_> {
    /// Radiated-output level makeup for the sounding fingering (see
    /// [`RADIATED_LEVEL_MAKEUP`]); rides the bent frequency so the low-register slope stays
    /// continuous under pitch bend.
    pub(super) fn radiated_makeup(&self, frequency_hz: f32) -> f32 {
        let vented = register_key_state(&self.patch, self.sounding_note).vent_admittance > 0.0;
        if vented {
            RADIATED_LEVEL_MAKEUP
        } else {
            let break_hz = midi_note_to_hz(self.patch.register_break_note);
            RADIATED_LEVEL_MAKEUP
                * LOW_REGISTER_LEVEL_MATCH
                * math::finite_clamp(frequency_hz / break_hz.max(1.0), 0.0, 1.0, 1.0)
        }
    }
}
