//! The selectable magnitude / reassigned spectrogram view (Windows Vizia): drains the plugin's frame
//! sources into the active model and draws it as a Skia image (bilinear-sampled, selected colormap).

use std::sync::Arc;

use vizia::{prelude::*, vg};

use super::{REFRESH, SPECTROGRAM_COLUMNS, SPECTROGRAM_ROWS, ViewMode};
use crate::cenedril_vizia::reassigned::ReassignedSpectrogram;
use crate::cenedril_vizia::spectrogram::{ColorMap, FreqScale, Spectrogram, colormap_for};
use crate::cenedril_vizia::{ReassignedSource, SpectrogramSource};

/// Tick event emitted by the refresh timer, targeting the `SpectrogramView`.
enum SpectrogramTick {
    Tick,
}

/// Start the live spectrogram refresh from the parent layout after the custom view has been built.
///
/// Starting the timer from inside the custom view's own build closure can block Cenedril's VST3
/// `attached` call under Galad before the window reaches its message loop. The timer still targets
/// the spectrogram entity directly, but the parent owns the timer hookup.
pub(super) fn start_spectrogram_refresh(cx: &mut Context, target: Entity) {
    crate::vizia_window::debug_log("cenedril-vizia: spectrogram timer begin");
    let timer = cx.add_timer(REFRESH, None, move |cx, action| {
        if matches!(action, TimerAction::Tick(_)) {
            cx.emit_to(target, SpectrogramTick::Tick);
        }
    });
    crate::vizia_window::debug_log("cenedril-vizia: spectrogram timer add done");
    cx.start_timer(timer);
    crate::vizia_window::debug_log("cenedril-vizia: spectrogram timer start done");
}

/// A custom Vizia view that drains the plugin's frame sources into either the magnitude
/// [`Spectrogram`] or the [`ReassignedSpectrogram`] (whichever the active [`ViewMode`] selects) and
/// draws it as a Skia image. Both models are kept alive so switching views is instant; only the
/// active view's source is drained each tick. Changing the scale/range rebuilds the models.
pub(super) struct SpectrogramView {
    spectrogram: Spectrogram,
    reassigned: ReassignedSpectrogram,
    mag_source: Arc<dyn SpectrogramSource>,
    reassigned_source: Arc<dyn ReassignedSource>,
    view_mode: Signal<ViewMode>,
    scale: Signal<FreqScale>,
    color_map: Signal<ColorMap>,
    db_floor: Signal<f32>,
    db_ceil: Signal<f32>,
    /// The `(scale, db_floor, db_ceil)` the models were last built with; a change rebuilds them.
    applied: (FreqScale, f32, f32),
    rgba: Vec<u8>,
}

impl SpectrogramView {
    #[allow(clippy::too_many_arguments)] // the editor controls each contribute one reactive signal
    pub(super) fn new(
        cx: &mut Context,
        mag_source: Arc<dyn SpectrogramSource>,
        reassigned_source: Arc<dyn ReassignedSource>,
        view_mode: Signal<ViewMode>,
        scale: Signal<FreqScale>,
        color_map: Signal<ColorMap>,
        db_floor: Signal<f32>,
        db_ceil: Signal<f32>,
    ) -> Handle<'_, Self> {
        crate::vizia_window::debug_log("cenedril-vizia: spectrogram new begin");
        let s = scale.get();
        let floor = db_floor.get();
        let ceil = db_ceil.get();
        crate::vizia_window::debug_log(format!(
            "cenedril-vizia: spectrogram source sr={} bins={} frame={}",
            mag_source.sample_rate(),
            mag_source.bins(),
            mag_source.frame_size()
        ));
        let spectrogram = Spectrogram::new(
            SPECTROGRAM_ROWS,
            SPECTROGRAM_COLUMNS,
            mag_source.sample_rate(),
            mag_source.bins(),
            mag_source.frame_size(),
            s,
            floor,
            ceil,
        );
        crate::vizia_window::debug_log("cenedril-vizia: magnitude model done");
        let reassigned = ReassignedSpectrogram::new(
            SPECTROGRAM_ROWS,
            SPECTROGRAM_COLUMNS,
            reassigned_source.sample_rate(),
            reassigned_source.bins(),
            reassigned_source.frame_size(),
            s,
            floor,
            ceil,
        );
        crate::vizia_window::debug_log("cenedril-vizia: reassigned model done");
        let mut rgba = Vec::new();
        compose_rgba(&spectrogram, &mut rgba, color_map.get());
        crate::vizia_window::debug_log(format!(
            "cenedril-vizia: initial rgba done bytes={}",
            rgba.len()
        ));
        Self {
            spectrogram,
            reassigned,
            mag_source,
            reassigned_source,
            view_mode,
            scale,
            color_map,
            db_floor,
            db_ceil,
            applied: (s, floor, ceil),
            rgba,
        }
        .build(cx, |_cx| {
            crate::vizia_window::debug_log("cenedril-vizia: spectrogram build begin");
            crate::vizia_window::debug_log("cenedril-vizia: spectrogram build done");
        })
    }

    /// Rebuild both models when the frequency scale or dB range changed (resets the scroll history).
    fn rebuild_if_needed(&mut self) {
        let target = (self.scale.get(), self.db_floor.get(), self.db_ceil.get());
        if target == self.applied {
            return;
        }
        let (scale, floor, ceil) = target;
        self.spectrogram = Spectrogram::new(
            SPECTROGRAM_ROWS,
            SPECTROGRAM_COLUMNS,
            self.mag_source.sample_rate(),
            self.mag_source.bins(),
            self.mag_source.frame_size(),
            scale,
            floor,
            ceil,
        );
        self.reassigned = ReassignedSpectrogram::new(
            SPECTROGRAM_ROWS,
            SPECTROGRAM_COLUMNS,
            self.reassigned_source.sample_rate(),
            self.reassigned_source.bins(),
            self.reassigned_source.frame_size(),
            scale,
            floor,
            ceil,
        );
        self.applied = target;
    }
}

impl View for SpectrogramView {
    fn element(&self) -> Option<&'static str> {
        Some("cenedril-spectrogram")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|tick, _meta| match tick {
            SpectrogramTick::Tick => {
                self.rebuild_if_needed();
                let map = self.color_map.get();
                // Drain only the active view's source (single consumer of the shared ring) and
                // recompose from its model; switching views recomposes on the next tick.
                match self.view_mode.get() {
                    ViewMode::Magnitude => {
                        let source = self.mag_source.clone();
                        source.drain_frames(&mut |magnitudes| {
                            self.spectrogram.push_column(magnitudes)
                        });
                        compose_rgba(&self.spectrogram, &mut self.rgba, map);
                    }
                    ViewMode::Reassigned => {
                        let source = self.reassigned_source.clone();
                        source.drain_frames(&mut |frame| {
                            self.reassigned.push_frame(
                                frame.magnitudes,
                                frame.freq_offsets,
                                frame.time_offsets,
                            )
                        });
                        compose_rgba(&self.reassigned, &mut self.rgba, map);
                    }
                }
                cx.needs_redraw();
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let rows = SPECTROGRAM_ROWS;
        let columns = SPECTROGRAM_COLUMNS;
        if self.rgba.len() < rows * columns * 4 {
            return; // not composed yet (no frames drained)
        }
        let info = vg::ImageInfo::new(
            (columns as i32, rows as i32),
            vg::ColorType::RGBA8888,
            vg::AlphaType::Opaque,
            None,
        );
        let Some(image) =
            vg::images::raster_from_data(&info, vg::Data::new_copy(&self.rgba), columns * 4)
        else {
            return;
        };
        let dst = vg::Rect::from_xywh(bounds.x, bounds.y, bounds.w, bounds.h);
        let sampling = vg::SamplingOptions::from(vg::FilterMode::Linear);
        canvas.draw_image_rect_with_sampling_options(
            &image,
            None,
            dst,
            sampling,
            &vg::Paint::default(),
        );
    }
}

/// A scrolling time-frequency image (magnitude or reassigned) the editor can render uniformly: time
/// left→right (oldest→newest), low frequency at the bottom.
trait ScrollImage {
    fn rows(&self) -> usize;
    fn columns(&self) -> usize;
    fn column_in_display_order(&self, display_index: usize) -> &[f32];
}

impl ScrollImage for Spectrogram {
    fn rows(&self) -> usize {
        Spectrogram::rows(self)
    }
    fn columns(&self) -> usize {
        Spectrogram::columns(self)
    }
    fn column_in_display_order(&self, display_index: usize) -> &[f32] {
        Spectrogram::column_in_display_order(self, display_index)
    }
}

impl ScrollImage for ReassignedSpectrogram {
    fn rows(&self) -> usize {
        ReassignedSpectrogram::rows(self)
    }
    fn columns(&self) -> usize {
        ReassignedSpectrogram::columns(self)
    }
    fn column_in_display_order(&self, display_index: usize) -> &[f32] {
        ReassignedSpectrogram::column_in_display_order(self, display_index)
    }
}

/// Compose a scrolling image's intensities into an RGBA buffer: time left→right (oldest→newest),
/// low frequency at the bottom, through the selected color map.
fn compose_rgba(image: &dyn ScrollImage, out: &mut Vec<u8>, map: ColorMap) {
    let rows = image.rows();
    let columns = image.columns();
    out.resize(rows * columns * 4, 0);
    for x in 0..columns {
        let column = image.column_in_display_order(x);
        for r in 0..rows {
            let y = rows - 1 - r; // low frequency at the bottom
            let pixel = (y * columns + x) * 4;
            out[pixel..pixel + 4].copy_from_slice(&colormap_for(map, column[r]));
        }
    }
}
