//! Calóma editor surface.
//!
//! Calóma is a self-contained, host-agnostic VST3 with **no host parameters**: its Vizia editor is
//! the sole control surface and writes the plugin's live settings directly (ADR-0023). To avoid a
//! circular crate dependency (`lindelion-ui` must not depend on `caloma`), the editor talks to the
//! plugin through the [`CalomaControlSurface`] trait — the same editor-service pattern the other
//! editors use (`PatchIoService`, `SampleSlotService`, …). `caloma` implements this trait on its
//! lock-free `SharedControls`, so an editor edit is a synchronous write to the very atomics the
//! audio thread reads — no host-relayed channel.
//!
//! The size/host/trait types here are platform-neutral and compile everywhere (including Linux
//! `make ci`). The Vizia view and the Windows `IPlugView`→`HWND` attach live in the
//! `windows`-gated [`platform`] submodule, because `vizia` is a Windows-target dependency for the
//! new VSTs (ADR-0023) — mirroring how the macOS instruments gate their Vizia code on macOS.

use std::sync::Arc;

pub const CALOMA_EDITOR_WIDTH: i32 = 560;
pub const CALOMA_EDITOR_HEIGHT: i32 = 660;

/// The editor's input/output level knob range, in dB.
pub const CALOMA_LEVEL_DB_MIN: f32 = -24.0;
pub const CALOMA_LEVEL_DB_MAX: f32 = 24.0;

/// One per-effect row the editor renders: a stable slot index, a label, and the live enable +
/// dry/wet intensity. The index is opaque to the editor — it is passed back to the surface's
/// setters verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct CalomaSlotView {
    pub index: usize,
    pub label: String,
    pub enabled: bool,
    pub intensity: f32,
}

/// The plugin's live control surface, as seen by the editor. Implemented by `caloma` on its
/// `SharedControls`; every method is a direct, lock-free read/write of the shared atomics (the same
/// object the audio thread reads), so there is no host parameter or message bridge. `Send + Sync`
/// because the editor runs on the UI thread while the audio thread also touches the atomics.
pub trait CalomaControlSurface: Send + Sync {
    /// The selected signal order's index (0-based).
    fn order_index(&self) -> u32;
    /// The signal orders' display labels, in index order.
    fn order_labels(&self) -> &'static [&'static str];
    /// Select an order by index, loading that order's default control set.
    fn select_order(&self, index: u32);

    /// Input trim (dB).
    fn input_level_db(&self) -> f32;
    fn set_input_level_db(&self, db: f32);
    /// Output level (dB).
    fn output_level_db(&self) -> f32;
    fn set_output_level_db(&self, db: f32);

    /// The per-effect rows for the current order (in chain order).
    fn active_slots(&self) -> Vec<CalomaSlotView>;
    /// Enable/bypass the slot at `index` (an index from [`CalomaSlotView::index`]).
    fn set_slot_enabled(&self, index: usize, enabled: bool);
    /// Set the dry/wet intensity (0..1) of the slot at `index`.
    fn set_slot_intensity(&self, index: usize, intensity: f32);
}

/// The editor host: the shared control surface the Vizia view reads and writes. Cloning shares the
/// same underlying atomics (it is an `Arc`).
#[derive(Clone)]
pub struct CalomaEditorHost {
    pub surface: Arc<dyn CalomaControlSurface>,
}

impl CalomaEditorHost {
    pub fn new(surface: Arc<dyn CalomaControlSurface>) -> Self {
        Self { surface }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CalomaEditorSize {
    pub width: i32,
    pub height: i32,
}

#[cfg(target_os = "windows")]
mod platform;

#[cfg(target_os = "windows")]
pub use platform::CalomaViziaEditor;
