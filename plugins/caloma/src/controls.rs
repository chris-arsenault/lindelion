//! `SharedControls` — the live control surface shared between Calóma's Vizia editor (UI thread,
//! the writer) and `process` (audio thread, the reader). Calóma surfaces **no host-automatable
//! parameters** (ADR-0023's self-contained design); the editor edits these atomics directly and the
//! DSP reads them each block, so there is no host-relayed parameter channel.
//!
//! Every field is an atomic and every access is `Relaxed`: the controls are independent scalars with
//! no cross-field invariant, the audio thread only ever *reads* a recent value, and the editor only
//! ever *writes* — so there is nothing to order between them (ADR-0001: no locks on the audio path).
//! f32 controls are stored as their `to_bits`/`from_bits` representation in an `AtomicU32`, mirroring
//! the `speech/signals` analysis worker.
//!
//! Slots are indexed by `SlotId as usize`: `SlotId` is a fieldless enum declared in the same order as
//! `SlotId::ALL`, so the discriminant is the slot's stable index in `0..20`.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use lindelion_plugin_shell::ParameterCodec;

use crate::order::SignalOrder;
use crate::patch::CalomaPatch;
use crate::slot::SlotId;

const SLOT_COUNT: usize = SlotId::ALL.len();

/// Lock-free live settings: the selected order, per-slot enable + dry/wet intensity, and the
/// input/output levels. The editor writes; the audio thread reads.
pub struct SharedControls {
    order: AtomicU32,
    enabled: [AtomicBool; SLOT_COUNT],
    intensity: [AtomicU32; SLOT_COUNT],
    input_level_db: AtomicU32,
    output_level_db: AtomicU32,
}

impl Default for SharedControls {
    fn default() -> Self {
        Self::from_patch(&CalomaPatch::default())
    }
}

impl SharedControls {
    /// Build the controls seeded from `patch` (the editor and DSP then share this one object).
    pub fn from_patch(patch: &CalomaPatch) -> Self {
        let controls = Self {
            order: AtomicU32::new(patch.order.to_index()),
            enabled: std::array::from_fn(|_| AtomicBool::new(false)),
            intensity: std::array::from_fn(|_| AtomicU32::new(1.0_f32.to_bits())),
            input_level_db: AtomicU32::new(patch.input_level_db.to_bits()),
            output_level_db: AtomicU32::new(patch.output_level_db.to_bits()),
        };
        for id in SlotId::ALL {
            controls.set_slot_enabled(id, patch.slot_enabled(id));
            controls.set_slot_intensity(id, patch.slot_intensity(id));
        }
        controls
    }

    /// Select `order` and load that order's default control set (ADR-0020: selecting an order loads
    /// its default tuning). Off the audio thread (the editor's order selector); builds a patch.
    pub fn load_order_defaults(&self, order: SignalOrder) {
        self.load_from_patch(&crate::patch::default_patch_for(order));
    }

    /// Overwrite every control from `patch` (e.g. after `setState` or an order-default load).
    pub fn load_from_patch(&self, patch: &CalomaPatch) {
        self.set_order(patch.order);
        self.set_input_level_db(patch.input_level_db);
        self.set_output_level_db(patch.output_level_db);
        for id in SlotId::ALL {
            self.set_slot_enabled(id, patch.slot_enabled(id));
            self.set_slot_intensity(id, patch.slot_intensity(id));
        }
    }

    /// Write the live controls back into `patch` (e.g. before `getState`), leaving the patch's
    /// per-effect knob params untouched.
    pub fn store_to_patch(&self, patch: &mut CalomaPatch) {
        patch.order = self.order();
        patch.input_level_db = self.input_level_db();
        patch.output_level_db = self.output_level_db();
        for id in SlotId::ALL {
            patch.set_slot_enabled(id, self.slot_enabled(id));
            patch.set_slot_intensity(id, self.slot_intensity(id));
        }
    }

    /// The selected signal order.
    pub fn order(&self) -> SignalOrder {
        SignalOrder::from_index(self.order.load(Ordering::Relaxed))
    }

    /// Select the signal order (the audio thread flips to the pre-built chain for it).
    pub fn set_order(&self, order: SignalOrder) {
        self.order.store(order.to_index(), Ordering::Relaxed);
    }

    /// Whether the slot is enabled.
    pub fn slot_enabled(&self, id: SlotId) -> bool {
        self.enabled[id as usize].load(Ordering::Relaxed)
    }

    /// Enable or bypass the slot.
    pub fn set_slot_enabled(&self, id: SlotId, enabled: bool) {
        self.enabled[id as usize].store(enabled, Ordering::Relaxed);
    }

    /// The slot's dry/wet intensity (0..1, 1.0 = fully wet).
    pub fn slot_intensity(&self, id: SlotId) -> f32 {
        f32::from_bits(self.intensity[id as usize].load(Ordering::Relaxed))
    }

    /// Set the slot's dry/wet intensity.
    pub fn set_slot_intensity(&self, id: SlotId, intensity: f32) {
        self.intensity[id as usize].store(intensity.to_bits(), Ordering::Relaxed);
    }

    /// Input trim in dB (applied at the chain head).
    pub fn input_level_db(&self) -> f32 {
        f32::from_bits(self.input_level_db.load(Ordering::Relaxed))
    }

    /// Set the input trim in dB.
    pub fn set_input_level_db(&self, db: f32) {
        self.input_level_db.store(db.to_bits(), Ordering::Relaxed);
    }

    /// Output level in dB (applied at the chain tail).
    pub fn output_level_db(&self) -> f32 {
        f32::from_bits(self.output_level_db.load(Ordering::Relaxed))
    }

    /// Set the output level in dB.
    pub fn set_output_level_db(&self, db: f32) {
        self.output_level_db.store(db.to_bits(), Ordering::Relaxed);
    }
}

/// The order labels exposed to the editor, in `SignalOrder::to_index` order.
const ORDER_LABELS: [&str; SignalOrder::ALL.len()] = ["Clarity", "Broadcast", "Light"];

/// Bridge `SharedControls` to the editor's control surface (defined in `lindelion-ui` to avoid a
/// circular dependency). Every method is a direct, lock-free read/write of the shared atomics — the
/// same object the audio thread reads (ADR-0023, no host bridge). Inherent methods take priority in
/// resolution, so the `self.input_level_db()` etc. calls below dispatch to the atomic accessors, not
/// back into the trait.
impl lindelion_ui::caloma_vizia::CalomaControlSurface for SharedControls {
    fn order_index(&self) -> u32 {
        self.order().to_index()
    }

    fn order_labels(&self) -> &'static [&'static str] {
        &ORDER_LABELS
    }

    fn select_order(&self, index: u32) {
        self.load_order_defaults(SignalOrder::from_index(index));
    }

    fn input_level_db(&self) -> f32 {
        SharedControls::input_level_db(self)
    }

    fn set_input_level_db(&self, db: f32) {
        SharedControls::set_input_level_db(self, db);
    }

    fn output_level_db(&self) -> f32 {
        SharedControls::output_level_db(self)
    }

    fn set_output_level_db(&self, db: f32) {
        SharedControls::set_output_level_db(self, db);
    }

    fn active_slots(&self) -> Vec<lindelion_ui::caloma_vizia::CalomaSlotView> {
        crate::topology::order_topology(self.order())
            .iter()
            .map(|&id| lindelion_ui::caloma_vizia::CalomaSlotView {
                index: id as usize,
                label: id.label().to_string(),
                enabled: self.slot_enabled(id),
                intensity: self.slot_intensity(id),
            })
            .collect()
    }

    fn set_slot_enabled(&self, index: usize, enabled: bool) {
        if let Some(&id) = SlotId::ALL.get(index) {
            SharedControls::set_slot_enabled(self, id, enabled);
        }
    }

    fn set_slot_intensity(&self, index: usize, intensity: f32) {
        if let Some(&id) = SlotId::ALL.get(index) {
            SharedControls::set_slot_intensity(self, id, intensity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::default_patch_for;
    use lindelion_ui::caloma_vizia::CalomaControlSurface;

    #[test]
    fn each_control_round_trips_through_its_atomic() {
        let controls = SharedControls::default();

        controls.set_order(SignalOrder::Light);
        assert_eq!(controls.order(), SignalOrder::Light);

        controls.set_input_level_db(-3.5);
        controls.set_output_level_db(2.25);
        assert_eq!(controls.input_level_db(), -3.5);
        assert_eq!(controls.output_level_db(), 2.25);

        controls.set_slot_enabled(SlotId::Compressor, true);
        controls.set_slot_intensity(SlotId::Compressor, 0.4);
        assert!(controls.slot_enabled(SlotId::Compressor));
        assert_eq!(controls.slot_intensity(SlotId::Compressor), 0.4);
        // A different slot is independent.
        assert!(!controls.slot_enabled(SlotId::Limiter));
    }

    #[test]
    fn default_controls_match_a_default_patch() {
        let controls = SharedControls::default();
        let patch = CalomaPatch::default();
        assert_eq!(controls.order(), patch.order);
        for id in SlotId::ALL {
            assert_eq!(controls.slot_enabled(id), patch.slot_enabled(id));
            // Default intensity is fully wet for every slot.
            assert_eq!(controls.slot_intensity(id), 1.0);
        }
    }

    #[test]
    fn load_then_store_reproduces_the_patch_controls() {
        let mut patch = default_patch_for(SignalOrder::Broadcast);
        patch.input_level_db = -6.0;
        patch.output_level_db = 1.5;
        patch.compressor.intensity = 0.3;
        patch.air_exciter.intensity = 0.8;
        patch.set_slot_enabled(SlotId::DeEsser, true);

        let controls = SharedControls::default();
        controls.load_from_patch(&patch);

        // A fresh patch the controls write themselves into must carry the same control values.
        let mut restored = CalomaPatch::default();
        controls.store_to_patch(&mut restored);

        assert_eq!(restored.order, patch.order);
        assert_eq!(restored.input_level_db, patch.input_level_db);
        assert_eq!(restored.output_level_db, patch.output_level_db);
        for id in SlotId::ALL {
            assert_eq!(restored.slot_enabled(id), patch.slot_enabled(id), "{id:?}");
            assert_eq!(
                restored.slot_intensity(id),
                patch.slot_intensity(id),
                "{id:?}"
            );
        }
    }

    #[test]
    fn load_order_defaults_matches_that_orders_default_patch() {
        let controls = SharedControls::default();
        for order in SignalOrder::ALL {
            controls.load_order_defaults(order);
            let patch = default_patch_for(order);
            assert_eq!(controls.order(), order);
            for id in SlotId::ALL {
                assert_eq!(
                    controls.slot_enabled(id),
                    patch.slot_enabled(id),
                    "{order:?} {id:?}"
                );
            }
        }
    }

    #[test]
    fn control_surface_bridges_edits_to_the_atomics() {
        // The editor talks to the plugin only through `CalomaControlSurface`; each call must land on
        // the same atomics the audio thread reads.
        let controls = SharedControls::from_patch(&default_patch_for(SignalOrder::Clarity));
        let surface: &dyn CalomaControlSurface = &controls;

        // The active rows are the current order's topology, with live enable/intensity.
        let slots = surface.active_slots();
        assert_eq!(
            slots.iter().map(|s| s.index).collect::<Vec<_>>(),
            crate::topology::order_topology(SignalOrder::Clarity)
                .iter()
                .map(|&id| id as usize)
                .collect::<Vec<_>>()
        );

        // Editing a slot through the surface flips the shared atomic.
        let first = slots[0].index;
        let was = controls.slot_enabled(SlotId::ALL[first]);
        surface.set_slot_enabled(first, !was);
        assert_eq!(controls.slot_enabled(SlotId::ALL[first]), !was);
        surface.set_slot_intensity(first, 0.25);
        assert_eq!(controls.slot_intensity(SlotId::ALL[first]), 0.25);

        // Levels round-trip through the surface.
        surface.set_input_level_db(-4.0);
        surface.set_output_level_db(3.0);
        assert_eq!(surface.input_level_db(), -4.0);
        assert_eq!(surface.output_level_db(), 3.0);

        // Selecting an order loads its defaults and re-targets the active rows.
        surface.select_order(2);
        assert_eq!(surface.order_index(), 2);
        assert_eq!(controls.order(), SignalOrder::Light);
        assert_eq!(surface.order_labels(), ["Clarity", "Broadcast", "Light"]);
    }

    #[test]
    fn from_patch_seeds_every_control() {
        let patch = default_patch_for(SignalOrder::Clarity);
        let controls = SharedControls::from_patch(&patch);
        assert_eq!(controls.order(), SignalOrder::Clarity);
        for id in SlotId::ALL {
            assert_eq!(controls.slot_enabled(id), patch.slot_enabled(id), "{id:?}");
        }
    }
}
