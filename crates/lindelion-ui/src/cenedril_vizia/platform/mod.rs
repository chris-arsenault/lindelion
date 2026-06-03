//! Cenedril's Windows Vizia editor: the layout/build, the editor controls (view / scale / colormap /
//! range), the stylesheet, and the `IPlugView`→`HWND` attach. The spectrogram view and the meter
//! panel live in the [`spectrogram_view`] / [`meter_panel`] submodules.

use std::{ffi::c_void, sync::Arc};

use vizia::{WindowScalePolicy, prelude::*};

use super::spectrogram::{ColorMap, FreqScale};
use super::{
    CENEDRIL_EDITOR_HEIGHT, CENEDRIL_EDITOR_WIDTH, CenedrilEditorHost, CenedrilEditorSize,
    SettingsStore,
};
use crate::vizia_window::ViziaWindowEditor;

mod meter_panel;
mod spectrogram_view;

use meter_panel::{MeterPanelState, meter_panel};
use spectrogram_view::{SpectrogramView, emit_spectrogram_refresh};

/// Spectrogram render resolution (rows = log-frequency bands, columns = time history). Upsampled to
/// the view bounds with Skia bilinear sampling for a smooth, high-fidelity image.
pub(super) const SPECTROGRAM_ROWS: usize = 320;
pub(super) const SPECTROGRAM_COLUMNS: usize = 512;
/// Editor refresh cadence (≈15 fps), matching the other Lindelion editors.
pub(super) const REFRESH: std::time::Duration = std::time::Duration::from_millis(66);
const CENEDRIL_EDITOR_TAG: &str = "onetimer-20260603-1";
const CENEDRIL_EDITOR_TITLE: &str = "Cenedril [onetimer-20260603-1]";

#[derive(Clone, Copy)]
struct CenedrilLayout {
    outer_width: f32,
    outer_height: f32,
    padding: f32,
    row_gap: f32,
    body_gap: f32,
    topbar_height: f32,
    controls_height: f32,
    panel_width: f32,
}

impl CenedrilLayout {
    fn inner_width(self) -> f32 {
        self.outer_width - (self.padding * 2.0)
    }

    fn body_height(self) -> f32 {
        self.outer_height
            - (self.padding * 2.0)
            - self.topbar_height
            - self.controls_height
            - (self.row_gap * 2.0)
    }

    fn spectrogram_width(self) -> f32 {
        self.inner_width() - self.body_gap - self.panel_width
    }
}

fn cenedril_layout(size: CenedrilEditorSize) -> CenedrilLayout {
    CenedrilLayout {
        outer_width: size.width.max(CENEDRIL_EDITOR_WIDTH) as f32,
        outer_height: size.height.max(CENEDRIL_EDITOR_HEIGHT) as f32,
        padding: 12.0,
        row_gap: 8.0,
        body_gap: 10.0,
        topbar_height: 30.0,
        controls_height: 30.0,
        panel_width: 248.0,
    }
}

fn start_cenedril_refresh(cx: &mut Context, meters: MeterPanelState, spectrogram: Entity) {
    crate::vizia_window::debug_log("cenedril-vizia: shared timer begin");
    let timer = cx.add_timer(REFRESH, None, move |cx, action| {
        if matches!(action, TimerAction::Tick(_)) {
            meters.update();
            emit_spectrogram_refresh(cx, spectrogram);
        }
    });
    crate::vizia_window::debug_log("cenedril-vizia: shared timer add done");
    cx.start_timer(timer);
    crate::vizia_window::debug_log("cenedril-vizia: shared timer start done");
}

/// Which time-frequency view the editor draws. Both models are kept alive so switching is instant.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum ViewMode {
    Magnitude,
    Reassigned,
}

const STYLE: &str = r#"
    .cenedril-root {
        background-color: #0c1013;
    }
    .cenedril-topbar {
        horizontal-gap: 10px;
        alignment: center;
    }
    .cenedril-title {
        color: #d8e0e4;
        font-size: 18px;
    }
    .cenedril-tag {
        color: #7f8c92;
        font-size: 10px;
    }
    .cenedril-segmented {
        background-color: #101515;
        border-width: 1px;
        border-color: #303b39;
        border-radius: 6px;
        padding: 2px;
        col-between: 2px;
        height: auto;
        width: auto;
    }
    button.cenedril-seg {
        background-color: transparent;
        border-width: 0px;
        border-radius: 4px;
        color: #909c97;
        font-size: 11px;
        child-left: 10px;
        child-right: 10px;
    }
    button.cenedril-seg:hover {
        background-color: #252d2b;
        color: #e0e8e3;
    }
    button.cenedril-seg-active {
        background-color: #315040;
        color: #f0f8f2;
    }
    .cenedril-controls {
        horizontal-gap: 10px;
        alignment: center;
    }
    .cenedril-ctl-label {
        color: #9aa7ad;
        font-size: 11px;
        width: auto;
    }
    .cenedril-slider {
        width: 120px;
        height: 22px;
    }
    .cenedril-slider .track {
        background-color: #1b2226;
        border-radius: 4px;
    }
    .cenedril-slider .active {
        background-color: #2bb8c6;
        border-radius: 4px;
    }
    .cenedril-slider .thumb {
        background-color: #eef6f0;
        border-color: #0e1112;
        border-width: 1px;
        border-radius: 6px;
        width: 13px;
        height: 18px;
    }
    .cenedril-body {
        horizontal-gap: 10px;
    }
    .cenedril-spectrogram {
        border-radius: 4px;
    }
    .cenedril-panels {
        child-space: 10px;
        row-between: 6px;
        background-color: #0e1318;
        border-radius: 4px;
        border-width: 1px;
        border-color: #1d262b;
    }
    .cenedril-section {
        color: #c2ccd1;
        font-size: 13px;
        top: 6px;
    }
"#;

// UI-neutral `u32` ⇄ enum mappings for the persisted settings (the `SettingsStore` discriminants).
fn view_mode_to_u32(m: ViewMode) -> u32 {
    match m {
        ViewMode::Magnitude => 0,
        ViewMode::Reassigned => 1,
    }
}
fn view_mode_from_u32(v: u32) -> ViewMode {
    match v {
        1 => ViewMode::Reassigned,
        _ => ViewMode::Magnitude,
    }
}
fn freq_scale_to_u32(s: FreqScale) -> u32 {
    match s {
        FreqScale::Log => 0,
        FreqScale::Linear => 1,
    }
}
fn freq_scale_from_u32(v: u32) -> FreqScale {
    match v {
        1 => FreqScale::Linear,
        _ => FreqScale::Log,
    }
}
fn color_map_to_u32(m: ColorMap) -> u32 {
    match m {
        ColorMap::Magma => 0,
        ColorMap::Viridis => 1,
        ColorMap::Grayscale => 2,
    }
}
fn color_map_from_u32(v: u32) -> ColorMap {
    match v {
        1 => ColorMap::Viridis,
        2 => ColorMap::Grayscale,
        _ => ColorMap::Magma,
    }
}

/// One button of the Magnitude / Reassigned segmented selector: sets `view_mode` and persists it via
/// the settings store on press, and highlights when it is the active mode.
fn view_mode_button(
    cx: &mut Context,
    label: &'static str,
    mode: ViewMode,
    view_mode: Signal<ViewMode>,
    store: Arc<dyn SettingsStore>,
) {
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("cenedril-seg")
        .on_press(move |_cx| {
            view_mode.set(mode);
            store.set_active_view(view_mode_to_u32(mode));
        })
        .toggle_class(
            "cenedril-seg-active",
            view_mode.map(move |active| *active == mode),
        );
}

/// One button of the Log / Linear frequency-scale segmented selector.
fn freq_scale_button(
    cx: &mut Context,
    label: &'static str,
    scale: FreqScale,
    signal: Signal<FreqScale>,
    store: Arc<dyn SettingsStore>,
) {
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("cenedril-seg")
        .on_press(move |_cx| {
            signal.set(scale);
            store.set_freq_scale(freq_scale_to_u32(scale));
        })
        .toggle_class(
            "cenedril-seg-active",
            signal.map(move |active| *active == scale),
        );
}

/// One button of the Magma / Viridis / Grayscale color-map segmented selector.
fn color_map_button(
    cx: &mut Context,
    label: &'static str,
    map: ColorMap,
    signal: Signal<ColorMap>,
    store: Arc<dyn SettingsStore>,
) {
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("cenedril-seg")
        .on_press(move |_cx| {
            signal.set(map);
            store.set_color_map(color_map_to_u32(map));
        })
        .toggle_class(
            "cenedril-seg-active",
            signal.map(move |active| *active == map),
        );
}

fn build_cenedril_application(
    host: CenedrilEditorHost,
    size: CenedrilEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    crate::vizia_window::debug_log(format!(
        "cenedril-vizia: build_application begin size={}x{}",
        size.width, size.height
    ));
    let layout = cenedril_layout(size);
    let width = layout.outer_width as u32;
    let height = layout.outer_height as u32;
    crate::vizia_window::debug_log(format!(
        "cenedril-vizia: build_application layout outer={}x{} body={} spec_w={} panel_w={}",
        layout.outer_width,
        layout.outer_height,
        layout.body_height(),
        layout.spectrogram_width(),
        layout.panel_width
    ));
    vizia::Application::new(move |cx| {
        crate::vizia_window::debug_log("cenedril-vizia: app closure begin");
        cx.add_stylesheet(STYLE)
            .expect("failed to add cenedril editor style");
        crate::vizia_window::debug_log("cenedril-vizia: app style added");
        cx.add_stylesheet(crate::vizia_meter::METER_STYLE)
            .expect("failed to add cenedril meter style");
        crate::vizia_window::debug_log("cenedril-vizia: meter style added");
        // Initialize each control from the persisted settings (restored on editor open).
        let store = host.settings.clone();
        crate::vizia_window::debug_log("cenedril-vizia: settings clone done");
        let view_mode = Signal::new(view_mode_from_u32(store.active_view()));
        let scale = Signal::new(freq_scale_from_u32(store.freq_scale()));
        let color_map = Signal::new(color_map_from_u32(store.color_map()));
        let db_floor = Signal::new(store.db_floor());
        let db_ceil = Signal::new(store.db_ceil());
        crate::vizia_window::debug_log("cenedril-vizia: settings signals done");
        VStack::new(cx, |cx| {
            crate::vizia_window::debug_log("cenedril-vizia: topbar begin");
            HStack::new(cx, |cx| {
                Label::new(cx, "Cenedril — Spectrogram")
                    .class("cenedril-title")
                    .width(Stretch(1.0))
                    .min_width(Pixels(0.0));
                Label::new(cx, CENEDRIL_EDITOR_TAG).class("cenedril-tag");
                HStack::new(cx, |cx| {
                    view_mode_button(
                        cx,
                        "Magnitude",
                        ViewMode::Magnitude,
                        view_mode,
                        store.clone(),
                    );
                    view_mode_button(
                        cx,
                        "Reassigned",
                        ViewMode::Reassigned,
                        view_mode,
                        store.clone(),
                    );
                })
                .class("cenedril-segmented");
            })
            .class("cenedril-topbar")
            .width(Pixels(layout.inner_width()))
            .height(Pixels(layout.topbar_height));
            crate::vizia_window::debug_log("cenedril-vizia: topbar done");
            crate::vizia_window::debug_log("cenedril-vizia: controls begin");
            HStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    freq_scale_button(cx, "Log", FreqScale::Log, scale, store.clone());
                    freq_scale_button(cx, "Linear", FreqScale::Linear, scale, store.clone());
                })
                .class("cenedril-segmented");
                HStack::new(cx, |cx| {
                    color_map_button(cx, "Magma", ColorMap::Magma, color_map, store.clone());
                    color_map_button(cx, "Viridis", ColorMap::Viridis, color_map, store.clone());
                    color_map_button(cx, "Gray", ColorMap::Grayscale, color_map, store.clone());
                })
                .class("cenedril-segmented");
                Label::new(cx, "Floor").class("cenedril-ctl-label");
                let floor_store = store.clone();
                Slider::new(cx, db_floor)
                    .range(-120.0..-40.0)
                    .step(1.0f32)
                    .on_change(move |_cx, v| {
                        db_floor.set(v);
                        floor_store.set_db_floor(v);
                    })
                    .class("cenedril-slider");
                Label::new(cx, "Ceil").class("cenedril-ctl-label");
                let ceil_store = store.clone();
                Slider::new(cx, db_ceil)
                    .range(-40.0..0.0)
                    .step(1.0f32)
                    .on_change(move |_cx, v| {
                        db_ceil.set(v);
                        ceil_store.set_db_ceil(v);
                    })
                    .class("cenedril-slider");
            })
            .class("cenedril-controls")
            .width(Pixels(layout.inner_width()))
            .height(Pixels(layout.controls_height));
            crate::vizia_window::debug_log("cenedril-vizia: controls done");
            crate::vizia_window::debug_log("cenedril-vizia: body begin");
            HStack::new(cx, |cx| {
                let meter_state = MeterPanelState::new(host.meters.clone());
                crate::vizia_window::debug_log("cenedril-vizia: meter panel begin");
                meter_panel(cx, meter_state.clone())
                    .class("cenedril-panels")
                    .width(Pixels(layout.panel_width))
                    .height(Pixels(layout.body_height()));
                crate::vizia_window::debug_log("cenedril-vizia: meter panel done");
                crate::vizia_window::debug_log("cenedril-vizia: spectrogram view begin");
                let spectrogram = SpectrogramView::new(
                    cx,
                    host.source.clone(),
                    host.reassigned.clone(),
                    view_mode,
                    scale,
                    color_map,
                    db_floor,
                    db_ceil,
                )
                .class("cenedril-spectrogram")
                .width(Pixels(layout.spectrogram_width()))
                .height(Pixels(layout.body_height()))
                .entity();
                start_cenedril_refresh(cx, meter_state, spectrogram);
                crate::vizia_window::debug_log("cenedril-vizia: spectrogram view done");
            })
            .class("cenedril-body")
            .width(Pixels(layout.inner_width()))
            .height(Pixels(layout.body_height()));
            crate::vizia_window::debug_log("cenedril-vizia: body done");
        })
        .class("cenedril-root")
        .width(Pixels(layout.outer_width))
        .height(Pixels(layout.outer_height))
        .padding(Pixels(layout.padding))
        .vertical_gap(Pixels(layout.row_gap));
        crate::vizia_window::debug_log("cenedril-vizia: app closure done");
    })
    .ignore_default_theme()
    .title(CENEDRIL_EDITOR_TITLE)
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
        crate::vizia_window::debug_log(format!(
            "cenedril-vizia: attach begin parent=0x{:x} size={}x{}",
            parent as usize, size.width, size.height
        ));
        let application = build_cenedril_application(host, size);
        crate::vizia_window::debug_log("cenedril-vizia: application built");
        crate::vizia_window::debug_log("cenedril-vizia: shared attach begin");
        let editor = Self(unsafe { ViziaWindowEditor::attach(parent, application) });
        crate::vizia_window::debug_log("cenedril-vizia: shared attach done");
        editor
    }
}
