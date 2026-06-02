use std::{ffi::c_void, sync::Arc, time::Duration};

use vizia::{WindowScalePolicy, prelude::*, vg};

use super::{
    CENEDRIL_EDITOR_HEIGHT, CENEDRIL_EDITOR_WIDTH, CenedrilEditorHost, CenedrilEditorSize,
    SpectrogramSource,
    spectrogram::{Spectrogram, colormap},
};
use crate::vizia_window::ViziaWindowEditor;

/// Spectrogram render resolution (rows = log-frequency bands, columns = time history). Upsampled to
/// the view bounds with Skia bilinear sampling for a smooth, high-fidelity image.
const SPECTROGRAM_ROWS: usize = 320;
const SPECTROGRAM_COLUMNS: usize = 512;
/// Editor refresh cadence (≈15 fps), matching the other Lindelion editors.
const REFRESH: Duration = Duration::from_millis(66);

const STYLE: &str = r#"
    .cenedril-root {
        background-color: #0c1013;
        width: 1s;
        height: 1s;
        child-space: 10px;
        row-between: 8px;
    }
    .cenedril-title {
        color: #d8e0e4;
        font-size: 18px;
    }
    .cenedril-spectrogram {
        width: 1s;
        height: 1s;
        border-radius: 4px;
    }
"#;

/// Tick event emitted by the refresh timer, targeting the `SpectrogramView`.
enum SpectrogramTick {
    Tick,
}

/// A custom Vizia view that drains the plugin's frame source into a [`Spectrogram`] and draws it as
/// a Skia image (bilinear-sampled, magma color map).
struct SpectrogramView {
    spectrogram: Spectrogram,
    source: Arc<dyn SpectrogramSource>,
    rgba: Vec<u8>,
}

impl SpectrogramView {
    fn new(cx: &mut Context, source: Arc<dyn SpectrogramSource>) -> Handle<'_, Self> {
        let spectrogram = Spectrogram::new(
            SPECTROGRAM_ROWS,
            SPECTROGRAM_COLUMNS,
            source.sample_rate(),
            source.bins(),
            source.frame_size(),
        );
        Self {
            spectrogram,
            source,
            rgba: Vec::new(),
        }
        .build(cx, |cx| {
            // Events emitted in the timer callback target this view (see `start_timer`).
            let timer = cx.add_timer(REFRESH, None, |cx, action| {
                if matches!(action, TimerAction::Tick(_)) {
                    cx.emit(SpectrogramTick::Tick);
                }
            });
            cx.start_timer(timer);
        })
    }
}

impl View for SpectrogramView {
    fn element(&self) -> Option<&'static str> {
        Some("cenedril-spectrogram")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|tick, _meta| match tick {
            SpectrogramTick::Tick => {
                let source = self.source.clone();
                source.drain_frames(&mut |magnitudes| self.spectrogram.push_column(magnitudes));
                compose_rgba(&self.spectrogram, &mut self.rgba);
                cx.needs_redraw();
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let rows = self.spectrogram.rows();
        let columns = self.spectrogram.columns();
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

/// Compose the spectrogram intensities into an RGBA image: time left→right (oldest→newest), low
/// frequency at the bottom, magma color map.
fn compose_rgba(spectrogram: &Spectrogram, out: &mut Vec<u8>) {
    let rows = spectrogram.rows();
    let columns = spectrogram.columns();
    out.resize(rows * columns * 4, 0);
    for x in 0..columns {
        let column = spectrogram.column_in_display_order(x);
        for r in 0..rows {
            let y = rows - 1 - r; // low frequency at the bottom
            let pixel = (y * columns + x) * 4;
            out[pixel..pixel + 4].copy_from_slice(&colormap(column[r]));
        }
    }
}

fn build_cenedril_application(
    host: CenedrilEditorHost,
    size: CenedrilEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(CENEDRIL_EDITOR_WIDTH) as u32;
    let height = size.height.max(CENEDRIL_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add cenedril editor style");
        VStack::new(cx, |cx| {
            Label::new(cx, "Cenedril — Spectrogram").class("cenedril-title");
            SpectrogramView::new(cx, host.source.clone()).class("cenedril-spectrogram");
        })
        .class("cenedril-root");
    })
    .ignore_default_theme()
    .title("Cenedril")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

/// The Cenedril editor: a thin newtype over the shared [`ViziaWindowEditor`], which owns the
/// `IPlugView`→`HWND` attach and the close-on-drop teardown. The inner editor is an RAII guard —
/// held only so its `Drop` closes the host window — hence never read directly.
pub struct CenedrilViziaEditor(#[allow(dead_code)] ViziaWindowEditor);

impl CenedrilViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle (an `HWND` on Windows).
    pub unsafe fn attach(
        parent: *mut c_void,
        host: CenedrilEditorHost,
        size: CenedrilEditorSize,
    ) -> Self {
        let application = build_cenedril_application(host, size);
        Self(unsafe { ViziaWindowEditor::attach(parent, application) })
    }
}
