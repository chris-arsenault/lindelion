// Modulation-slot parameter path. `include!`d into `parameters/paths.rs` so it stays in
// that module with unchanged paths and visibility; split out only to keep `paths.rs`
// under the repository file-size limit.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModulationSlotParameter {
    Enabled,
    Source,
    Destination,
    Amount,
}

impl ModulationSlotParameter {
    fn plain_value(self, config: &ModulationConfig, slot: usize) -> Option<f32> {
        let slot = config.slots.get(slot)?;
        Some(match self {
            Self::Enabled => bool_plain(slot.enabled),
            Self::Source => slot.source.plain(),
            Self::Destination => slot.destination.plain(),
            Self::Amount => slot.amount,
        })
    }

    fn apply_plain(self, config: &mut ModulationConfig, slot: usize, value: f32) {
        let Some(slot) = config.slots.get_mut(slot) else {
            return;
        };
        match self {
            Self::Enabled => slot.enabled = bool_from_plain(value),
            Self::Source => slot.source = ModulationSource::from_plain(value),
            Self::Destination => slot.destination = ModulationDestination::from_plain(value),
            Self::Amount => slot.amount = finite_value(value, -1.0, 1.0, 0.0),
        }
    }
}
