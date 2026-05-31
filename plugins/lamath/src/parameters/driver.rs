// Driver-layer parameter paths (M8). Included into the `parameters` module by
// `parameters.rs`, so it shares scope with `DriverType`/`DriverConfig`, the patch
// types, and the `*_config_from` / `finite_value` helpers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DriverParameter {
    Type,
    Pick(PickParameter),
    Reed(ReedParameter),
}

impl DriverParameter {
    fn plain_value(self, config: DriverConfig) -> f32 {
        match self {
            Self::Type => DriverType::from_config(config).plain(),
            Self::Pick(parameter) => parameter.plain_value(pick_config_from(config)),
            Self::Reed(parameter) => parameter.plain_value(reed_config_from(config)),
        }
    }

    fn apply_plain(self, config: &mut DriverConfig, value: f32) {
        match self {
            Self::Type => *config = DriverType::from_plain(value).config_from(*config),
            Self::Pick(parameter) => parameter.apply_if_selected(config, value),
            Self::Reed(parameter) => parameter.apply_if_selected(config, value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickParameter {
    Hardness,
    ContactTime,
}

impl PickParameter {
    fn plain_value(self, config: PickConfig) -> f32 {
        match self {
            Self::Hardness => config.hardness,
            Self::ContactTime => config.contact_time,
        }
    }

    fn apply_if_selected(self, config: &mut DriverConfig, value: f32) {
        if let DriverConfig::Pick(pick) = config {
            self.apply_plain(pick, value);
        }
    }

    fn apply_plain(self, config: &mut PickConfig, value: f32) {
        let value = finite_value(value, 0.0, 1.0, 0.5);
        match self {
            Self::Hardness => config.hardness = value,
            Self::ContactTime => config.contact_time = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReedParameter {
    PressureDepth,
    Stiffness,
    Embouchure,
}

impl ReedParameter {
    fn plain_value(self, config: ReedConfig) -> f32 {
        match self {
            Self::PressureDepth => config.pressure_depth,
            Self::Stiffness => config.stiffness,
            Self::Embouchure => config.embouchure,
        }
    }

    fn apply_if_selected(self, config: &mut DriverConfig, value: f32) {
        if let DriverConfig::Reed(reed) = config {
            self.apply_plain(reed, value);
        }
    }

    fn apply_plain(self, config: &mut ReedConfig, value: f32) {
        let value = finite_value(value, 0.0, 1.0, 0.5);
        match self {
            Self::PressureDepth => config.pressure_depth = value,
            Self::Stiffness => config.stiffness = value,
            Self::Embouchure => config.embouchure = value,
        }
    }
}
