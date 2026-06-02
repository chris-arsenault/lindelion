use std::ffi::c_void;
use std::sync::Arc;

#[cfg(target_os = "windows")]
use std::cell::RefCell;

use lindelion_plugin_shell::vst3::{
    FixedSizePlugView, FixedSizePlugViewDelegate, FixedSizePlugViewSize,
};
use lindelion_ui::lumedir_vizia::{
    CoachBands, CoachConfigSurface, DeliverySample, DeliverySource, LUMEDIR_EDITOR_HEIGHT,
    LUMEDIR_EDITOR_WIDTH, SampleStatuses,
};
use vst3::{ComWrapper, Steinberg::*};

use crate::config::TargetBands;
use crate::delivery::DeliverySnapshot;
use crate::{DeliveryReader, SharedConfig};

use super::LumedirVst3Processor;

const EDITOR_SIZE: FixedSizePlugViewSize =
    FixedSizePlugViewSize::new(LUMEDIR_EDITOR_WIDTH, LUMEDIR_EDITOR_HEIGHT);

/// Adapts the worker's [`DeliveryReader`] to the editor's [`DeliverySource`] trait: each accessor
/// reads the latest published snapshot and returns the matching field. Returns the default snapshot
/// (all zeros) when the worker has not been spawned yet (before `reset`), so the editor never panics
/// on an early open.
struct LumedirDeliverySource {
    reader: Option<DeliveryReader>,
}

impl LumedirDeliverySource {
    fn new(reader: Option<DeliveryReader>) -> Self {
        Self { reader }
    }

    fn snapshot(&self) -> DeliverySnapshot {
        self.reader
            .as_ref()
            .map(DeliveryReader::latest_delivery)
            .unwrap_or_default()
    }
}

impl DeliverySource for LumedirDeliverySource {
    fn syllables_per_second(&self) -> f32 {
        self.snapshot().syllables_per_second
    }
    fn words_per_minute(&self) -> f32 {
        self.snapshot().words_per_minute
    }
    fn pitch_dynamism_semitones(&self) -> f32 {
        self.snapshot().pitch_dynamism_semitones
    }
    fn pause_fraction(&self) -> f32 {
        self.snapshot().pause_fraction
    }
    fn pause_count(&self) -> u32 {
        self.snapshot().pause_count
    }
    fn clarity(&self) -> f32 {
        self.snapshot().clarity
    }
}

/// Adapts the shared coaching config to the editor's [`CoachConfigSurface`]: factor + band edges
/// read/written through [`SharedConfig`], and live scoring computed plugin-side via [`TargetBands`]
/// (the plugin owns the bands and the scoring; `lindelion-ui` cannot, since it must not depend on
/// this crate).
struct LumedirConfigSurface {
    shared: Arc<SharedConfig>,
}

impl LumedirConfigSurface {
    fn new(shared: Arc<SharedConfig>) -> Self {
        Self { shared }
    }
}

impl CoachConfigSurface for LumedirConfigSurface {
    fn syllables_per_word(&self) -> f32 {
        self.shared.syllables_per_word()
    }

    fn set_syllables_per_word(&self, value: f32) {
        self.shared.set_syllables_per_word(value);
    }

    fn bands(&self) -> CoachBands {
        let bands = self.shared.bands();
        CoachBands {
            rate_min: bands.rate_syl_per_s_min,
            rate_max: bands.rate_syl_per_s_max,
            wpm_min: bands.wpm_min,
            wpm_max: bands.wpm_max,
            dynamism_min: bands.dynamism_semitones_min,
            pause_min: bands.pause_fraction_min,
            pause_max: bands.pause_fraction_max,
            clarity_min: bands.clarity_min,
        }
    }

    fn set_bands(&self, bands: CoachBands) {
        self.shared.set_bands(TargetBands {
            rate_syl_per_s_min: bands.rate_min,
            rate_syl_per_s_max: bands.rate_max,
            wpm_min: bands.wpm_min,
            wpm_max: bands.wpm_max,
            dynamism_semitones_min: bands.dynamism_min,
            pause_fraction_min: bands.pause_min,
            pause_fraction_max: bands.pause_max,
            clarity_min: bands.clarity_min,
        });
    }

    fn status(&self, sample: DeliverySample) -> SampleStatuses {
        let bands = self.shared.bands();
        SampleStatuses {
            rate: bands.rate_status(sample.rate),
            wpm: bands.wpm_status(sample.wpm),
            dynamism: bands.dynamism_status(sample.dynamism),
            pause_fraction: bands.pause_status(sample.pause_fraction),
            clarity: bands.clarity_status(sample.clarity),
        }
    }
}

pub(super) fn create_editor_view(controller: &LumedirVst3Processor) -> *mut IPlugView {
    ComWrapper::new(FixedSizePlugView::new(
        LumedirEditorView::new(controller),
        EDITOR_SIZE,
    ))
    .to_com_ptr::<IPlugView>()
    .unwrap()
    .into_raw()
}

struct LumedirEditorView {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    controller: *const LumedirVst3Processor,
    #[cfg(target_os = "windows")]
    editor: RefCell<Option<lindelion_ui::lumedir_vizia::LumedirViziaEditor>>,
}

impl LumedirEditorView {
    fn new(controller: &LumedirVst3Processor) -> Self {
        Self {
            controller,
            #[cfg(target_os = "windows")]
            editor: RefCell::new(None),
        }
    }
}

impl FixedSizePlugViewDelegate for LumedirEditorView {
    unsafe fn attached(&self, parent: *mut c_void, size: ViewRect) -> tresult {
        #[cfg(target_os = "windows")]
        {
            let mut editor = self.editor.borrow_mut();
            *editor = None;
            // Single-component: `self.controller` is the processor, so the editor reads the worker's
            // delivery snapshots and edits the shared coaching config directly — no marshaling.
            let component = unsafe { &*self.controller };
            let source: Arc<dyn DeliverySource> =
                Arc::new(LumedirDeliverySource::new(component.delivery_reader()));
            let config: Arc<dyn CoachConfigSurface> =
                Arc::new(LumedirConfigSurface::new(component.shared_config()));
            let host = lindelion_ui::lumedir_vizia::LumedirEditorHost::new(source, config);
            *editor = Some(unsafe {
                lindelion_ui::lumedir_vizia::LumedirViziaEditor::attach(
                    parent,
                    host,
                    lindelion_ui::lumedir_vizia::LumedirEditorSize {
                        width: size.right - size.left,
                        height: size.bottom - size.top,
                    },
                )
            });
            kResultOk
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = parent;
            let _ = size;
            kNotImplemented
        }
    }

    unsafe fn removed(&self) -> tresult {
        #[cfg(target_os = "windows")]
        {
            self.editor.borrow_mut().take();
        }
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lindelion_plugin_shell::{AudioPlugin, ProcessSetup};

    #[test]
    fn create_editor_view_returns_non_null_plug_view() {
        let controller = LumedirVst3Processor::new();
        let view = create_editor_view(&controller);
        assert!(!view.is_null());
        // Reclaim the leaked COM reference created by `into_raw` so the test allocates nothing net.
        unsafe { vst3::ComPtr::<IPlugView>::from_raw(view) };
    }

    #[test]
    fn delivery_source_without_a_worker_reads_all_zeros() {
        // No worker: every accessor returns the default snapshot's zeros.
        let none = LumedirDeliverySource::new(None);
        assert_eq!(none.syllables_per_second(), 0.0);
        assert_eq!(none.words_per_minute(), 0.0);
        assert_eq!(none.pitch_dynamism_semitones(), 0.0);
        assert_eq!(none.pause_fraction(), 0.0);
        assert_eq!(none.pause_count(), 0);
        assert_eq!(none.clarity(), 0.0);
    }

    #[test]
    fn delivery_source_with_a_worker_reads_finite_values() {
        // With a spawned worker (via a reset plugin), every reading is finite.
        let mut plugin = crate::Lumedir::default();
        plugin.reset(ProcessSetup::default());
        let some = LumedirDeliverySource::new(plugin.delivery_reader());
        assert!(some.syllables_per_second().is_finite());
        assert!(some.words_per_minute().is_finite());
        assert!(some.pitch_dynamism_semitones().is_finite());
        assert!(some.pause_fraction().is_finite());
        assert!(some.clarity().is_finite());
    }

    #[test]
    fn config_surface_round_trips_bands_and_scores_against_them() {
        use lindelion_ui::lumedir_vizia::BandStatus;

        let surface = LumedirConfigSurface::new(Arc::new(SharedConfig::default()));
        // Edit through the surface; the change is reflected back.
        let mut bands = surface.bands();
        bands.wpm_max = 175.0;
        surface.set_bands(bands);
        surface.set_syllables_per_word(1.8);
        assert_eq!(surface.bands().wpm_max, 175.0);
        assert_eq!(surface.syllables_per_word(), 1.8);

        // Scoring runs plugin-side against the current bands (defaults: rate 3.0–4.0, clarity ≥0.6).
        let status = surface.status(DeliverySample {
            rate: 3.5,
            wpm: 140.0,
            dynamism: 5.0,
            pause_fraction: 0.2,
            clarity: 0.3,
        });
        assert_eq!(status.rate, BandStatus::InBand);
        assert_eq!(status.clarity, BandStatus::Below);
    }
}
