// Shared-body idiophone-mode parameter paths (M0). Included into the `parameters`
// module by `parameters.rs`, so it shares scope with `SharedBodyConfig`, the patch
// types, and the `finite_value` / `bool_*` helpers. The shared-body controls are a
// single patch-level `SharedBodyConfig`, so these read/write `patch.shared_body`
// directly. No runtime DSP reads these yet — M0 is the control surface only
// ([ADR-0031](../../docs/adr/0031-shared-body-idiophone-mode.md)).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SharedBodyParameter {
    Enabled,
    DampKeyLow,
    DampKeyHigh,
}

impl SharedBodyParameter {
    fn plain_value(self, config: SharedBodyConfig) -> f32 {
        match self {
            Self::Enabled => bool_plain(config.enabled),
            Self::DampKeyLow => f32::from(config.damp_key_low),
            Self::DampKeyHigh => f32::from(config.damp_key_high),
        }
    }

    fn apply_plain(self, config: &mut SharedBodyConfig, value: f32) {
        match self {
            Self::Enabled => config.enabled = bool_from_plain(value),
            Self::DampKeyLow => {
                config.damp_key_low = finite_value(value, 0.0, 127.0, 0.0).round() as u8;
            }
            Self::DampKeyHigh => {
                config.damp_key_high = finite_value(value, 0.0, 127.0, 11.0).round() as u8;
            }
        }
    }
}
