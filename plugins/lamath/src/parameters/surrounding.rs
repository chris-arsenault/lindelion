// Surrounding-effects parameter paths (M10). Included into the `parameters` module
// by `parameters.rs`, so it shares scope with `SurroundingConfig`, the patch types,
// and the `finite_value` helper. The surrounding effects are a single patch-level
// `SurroundingConfig` (not per-slot), so these read/write `patch.surrounding` directly.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurroundingParameter {
    MechanicalNoise,
    RadiationBrightness,
    Sympathetic,
}

impl SurroundingParameter {
    fn plain_value(self, config: SurroundingConfig) -> f32 {
        match self {
            Self::MechanicalNoise => config.mechanical_noise,
            Self::RadiationBrightness => config.radiation_brightness,
            Self::Sympathetic => config.sympathetic,
        }
    }

    fn apply_plain(self, config: &mut SurroundingConfig, value: f32) {
        let value = finite_value(value, 0.0, 1.0, 0.0);
        match self {
            Self::MechanicalNoise => config.mechanical_noise = value,
            Self::RadiationBrightness => config.radiation_brightness = value,
            Self::Sympathetic => config.sympathetic = value,
        }
    }
}
