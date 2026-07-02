//! Named cymbal voices.
//!
//! Each preset is a curated point in the body's tonal parameter space (material/size/damping/
//! tension/strike/spread/gain), named after the instrument it evokes. These are the same voicings
//! the render-catalog auditions exercise, surfaced to the editor's Basic tab so a player can pick a
//! sound without learning the physical controls. A preset only sets the seven tonal parameters; the
//! striker slots, loaded samples, and selected striker are left untouched when one is applied.

use crate::patch::CymbalPatch;

/// A named cymbal voice: a label, a one-line description, and the seven tonal parameter values it
/// sets. Stored as plain (denormalized) values so they read like the patch fields they map onto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CymbalPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub material: f32,
    pub size: f32,
    pub damping: f32,
    pub tension: f32,
    pub strike_position: f32,
    pub pickup_spread: f32,
    pub output_gain_db: f32,
}

impl CymbalPreset {
    /// Write this preset's tonal parameters onto `patch`, preserving the striker slots, the loaded
    /// excitation samples, and the selected striker.
    pub fn apply_to(&self, patch: &mut CymbalPatch) {
        patch.material = self.material;
        patch.size = self.size;
        patch.damping = self.damping;
        patch.tension = self.tension;
        patch.strike_position = self.strike_position;
        patch.pickup_spread = self.pickup_spread;
        patch.output_gain_db = self.output_gain_db;
    }

    /// Whether `patch`'s tonal parameters match this preset (within editor-resolution tolerance), so
    /// the editor can highlight the active voice after the patch is loaded or hand-edited.
    pub fn matches(&self, patch: &CymbalPatch) -> bool {
        const UNIT_EPS: f32 = 0.005;
        const GAIN_EPS: f32 = 0.05;
        (patch.material - self.material).abs() <= UNIT_EPS
            && (patch.size - self.size).abs() <= UNIT_EPS
            && (patch.damping - self.damping).abs() <= UNIT_EPS
            && (patch.tension - self.tension).abs() <= UNIT_EPS
            && (patch.strike_position - self.strike_position).abs() <= UNIT_EPS
            && (patch.pickup_spread - self.pickup_spread).abs() <= UNIT_EPS
            && (patch.output_gain_db - self.output_gain_db).abs() <= GAIN_EPS
    }
}

/// The shipped cymbal voices, ordered for the editor's Basic-tab picker. `Default` mirrors the
/// shipped [`CymbalPatch::default`] so the picker always has a selected starting voice.
pub const CYMBAL_PRESETS: &[CymbalPreset] = &[
    CymbalPreset {
        name: "Default",
        description: "The shipped balance — a medium plate with a moderate bloom.",
        material: 0.65,
        size: 0.74,
        damping: 0.28,
        tension: 0.55,
        strike_position: 0.42,
        pickup_spread: 0.42,
        output_gain_db: -3.0,
    },
    CymbalPreset {
        name: "Ride",
        description: "Bright, sustained ping with a controlled wash.",
        material: 0.72,
        size: 0.50,
        damping: 0.35,
        tension: 0.40,
        strike_position: 0.72,
        pickup_spread: 0.42,
        output_gain_db: -3.0,
    },
    CymbalPreset {
        name: "Kit Ride",
        description: "Focused kit ride — defined stick, fast-settling body.",
        material: 0.95,
        size: 0.86,
        damping: 0.56,
        tension: 0.84,
        strike_position: 0.82,
        pickup_spread: 0.24,
        output_gain_db: -5.0,
    },
    CymbalPreset {
        name: "Crash",
        description: "Wide explosive bloom with a long shimmering tail.",
        material: 0.40,
        size: 0.95,
        damping: 0.10,
        tension: 0.88,
        strike_position: 0.90,
        pickup_spread: 0.42,
        output_gain_db: -5.0,
    },
    CymbalPreset {
        name: "Kit Crash",
        description: "Tight kit crash — quick attack, dense high air.",
        material: 0.95,
        size: 0.98,
        damping: 0.14,
        tension: 0.98,
        strike_position: 0.90,
        pickup_spread: 0.18,
        output_gain_db: -8.0,
    },
    CymbalPreset {
        name: "Splash",
        description: "Small fast accent — bright onset, short decay.",
        material: 0.50,
        size: 0.42,
        damping: 0.55,
        tension: 0.60,
        strike_position: 0.85,
        pickup_spread: 0.35,
        output_gain_db: -4.0,
    },
    CymbalPreset {
        name: "China",
        description: "Trashy, aggressive — bright and metallic with a fast cutoff.",
        material: 0.92,
        size: 0.90,
        damping: 0.20,
        tension: 0.95,
        strike_position: 0.92,
        pickup_spread: 0.20,
        output_gain_db: -7.0,
    },
    CymbalPreset {
        name: "Gong",
        description: "Huge, dark wash — slow bloom and a very long sustain.",
        material: 0.32,
        size: 0.96,
        damping: 0.07,
        tension: 0.50,
        strike_position: 0.50,
        pickup_spread: 0.55,
        output_gain_db: -6.0,
    },
    CymbalPreset {
        name: "Triangle",
        description: "Tiny bright ting — a clear high body with little spread.",
        material: 0.95,
        size: 0.02,
        damping: 0.25,
        tension: 0.10,
        strike_position: 0.50,
        pickup_spread: 0.30,
        output_gain_db: -3.0,
    },
];

/// Index of the preset whose tonal parameters match `patch`, if any.
pub fn active_preset_index(patch: &CymbalPatch) -> Option<usize> {
    CYMBAL_PRESETS
        .iter()
        .position(|preset| preset.matches(patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preset_matches_shipped_patch_default() {
        let preset = &CYMBAL_PRESETS[0];
        assert_eq!(preset.name, "Default");
        assert!(preset.matches(&CymbalPatch::default()));
        assert_eq!(active_preset_index(&CymbalPatch::default()), Some(0));
    }

    #[test]
    fn apply_preserves_striker_state() {
        let mut patch = CymbalPatch {
            selected_striker: 2,
            ..CymbalPatch::default()
        };
        let before = patch.strikers.clone();
        CYMBAL_PRESETS[3].apply_to(&mut patch);
        assert_eq!(patch.selected_striker, 2);
        assert_eq!(patch.strikers, before);
        assert!(CYMBAL_PRESETS[3].matches(&patch));
    }

    #[test]
    fn preset_values_are_in_range() {
        for preset in CYMBAL_PRESETS {
            for value in [
                preset.material,
                preset.size,
                preset.damping,
                preset.tension,
                preset.strike_position,
                preset.pickup_spread,
            ] {
                assert!(
                    (0.0..=1.0).contains(&value),
                    "{} has out-of-range unit value {value}",
                    preset.name
                );
            }
            assert!(
                (-24.0..=12.0).contains(&preset.output_gain_db),
                "{} gain out of range",
                preset.name
            );
        }
    }
}
