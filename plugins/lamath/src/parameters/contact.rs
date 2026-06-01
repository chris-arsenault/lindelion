// Coupling/contact-stage parameter paths (M9). Included into the `parameters`
// module by `parameters.rs`, so it shares scope with `ContactConfig`, the patch
// types, and the `finite_value` helper. The contact stage is a single patch-level
// `ContactConfig` (not per-slot), so these read/write `patch.contact` directly.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContactParameter {
    Spread,
    ContactTime,
}

impl ContactParameter {
    fn plain_value(self, config: ContactConfig) -> f32 {
        match self {
            Self::Spread => config.spread,
            Self::ContactTime => config.contact_time,
        }
    }

    fn apply_plain(self, config: &mut ContactConfig, value: f32) {
        let value = finite_value(value, 0.0, 1.0, 0.0);
        match self {
            Self::Spread => config.spread = value,
            Self::ContactTime => config.contact_time = value,
        }
    }
}
