use vizia::prelude::*;

#[path = "vizia_controls/drag_value.rs"]
mod drag_value;

pub(crate) use drag_value::{DragValueSpec, drag_value, dynamic_drag_value};

pub(crate) const COMMON_CONTROL_STYLE: &str = r#"
    .ll-shell {
        background-color: #111517;
    }
    .ll-top-strip {
        background-color: #181e20;
        border-width: 1px;
        border-color: #30383b;
        border-radius: 6px;
        padding: 10px;
    }
    .ll-panel {
        background-color: #161c1e;
        border-width: 1px;
        border-color: #2c3437;
        border-radius: 6px;
        padding: 10px;
    }
    .ll-panel-audio { border-color: #315667; }
    .ll-panel-tone { border-color: #3c5d3a; }
    .ll-panel-slice { border-color: #6b4d24; }
    .ll-panel-mod { border-color: #4b3f76; }
    .ll-panel-transport { border-color: #704050; }
    .ll-section-title {
        color: #f1f5f2;
        font-size: 12px;
    }
    .ll-section-subtitle {
        color: #8d9994;
        font-size: 10px;
    }
    .ll-accent {
        width: 4px;
        height: 18px;
        border-radius: 2px;
    }
    .ll-accent-neutral { background-color: #65716c; }
    .ll-accent-audio { background-color: #59b6d8; }
    .ll-accent-tone { background-color: #7ed06d; }
    .ll-accent-slice { background-color: #f2a84b; }
    .ll-accent-mod { background-color: #9a78ff; }
    .ll-accent-transport { background-color: #ef6f88; }
    .ll-accent-warn { background-color: #d7a540; }
    .ll-visual-frame {
        background-color: #0f1416;
        border-width: 1px;
        border-color: #303a3e;
        border-radius: 5px;
    }
    .ll-visual-audio { border-color: #315667; }
    .ll-visual-tone { border-color: #3c5d3a; }
    .ll-visual-slice { border-color: #6b4d24; }
    .ll-visual-mod { border-color: #4b3f76; }
    .ll-radiator-label {
        color: #aeb8b2;
        font-size: 10px;
    }
    .ll-status-chip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        border-radius: 5px;
        color: #c8d4cf;
        font-size: 10px;
        padding-left: 7px;
        padding-right: 7px;
    }
    .ll-chip-ready { background-color: #24372d; border-color: #79ad89; color: #dff3e3; }
    .ll-chip-warn { background-color: #3a3022; border-color: #d7a540; color: #f6dfb5; }
    .ll-chip-error { background-color: #3a2528; border-color: #e05f5f; color: #ffd3d3; }
    .ll-chip-audio { background-color: #1e3540; border-color: #59b6d8; color: #d5f5ff; }
    .ll-chip-tone { background-color: #26392d; border-color: #7ed06d; color: #e5f8df; }
    .ll-chip-slice { background-color: #3f3020; border-color: #f2a84b; color: #ffe3b7; }
    .ll-chip-mod { background-color: #302a4e; border-color: #9a78ff; color: #e5dcff; }
    .ll-metric-label {
        color: #76827d;
        font-size: 9px;
    }
    .ll-metric-value {
        color: #eef5f0;
        font-size: 11px;
    }
    .ll-control-label {
        color: #96a29d;
        font-size: 10px;
    }
    .ll-control-value {
        color: #e8f1eb;
        font-size: 11px;
    }
    .ll-control-row:hover .ll-control-label,
    .ll-drag-value:hover .ll-control-label,
    .ll-knob-cell:hover .ll-control-label,
    .ll-stepper:hover .ll-control-label {
        color: #c4cec8;
    }
    .ll-control-row:hover .ll-control-value,
    .ll-drag-value:hover .ll-control-value,
    .ll-knob-cell:hover .ll-control-value,
    .ll-stepper:hover .ll-control-value {
        color: #f6fbf7;
    }
    .ll-drag-value {
        background-color: #111719;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 5px;
        padding-left: 6px;
        padding-right: 6px;
    }
    .ll-drag-value:hover {
        background-color: #1d2528;
        border-color: #59b6d8;
    }
    .ll-drag-value-audio:hover { border-color: #59b6d8; }
    .ll-drag-value-tone:hover { border-color: #7ed06d; }
    .ll-drag-value-slice:hover { border-color: #f2a84b; }
    .ll-drag-value-mod:hover { border-color: #9a78ff; }
    .ll-drag-value-transport:hover { border-color: #ef6f88; }
    knob.ll-knob {
        width: 42px;
        height: 42px;
    }
    .ll-knob .knob-track {
        color: #7ed06d;
        background-color: #263136;
    }
    .ll-knob-audio .knob-track { color: #59b6d8; }
    .ll-knob-slice .knob-track { color: #f2a84b; }
    .ll-knob-mod .knob-track { color: #9a78ff; }
    .ll-knob-transport .knob-track { color: #ef6f88; }
    .ll-knob .knob-head { color: #f0f8f2; }
    .ll-knob .knob-tick {
        background-color: #f0f8f2;
        width: 3px;
        height: 12px;
        border-radius: 2px;
    }
    .ll-range-mark {
        color: #67746f;
        font-size: 8px;
    }
    slider.ll-slider { height: 18px; }
    slider.ll-slider .track {
        background-color: #252f34;
        border-radius: 4px;
    }
    slider.ll-slider .active {
        background-color: #59b6d8;
        border-radius: 4px;
    }
    slider.ll-slider .thumb {
        background-color: #eef6f0;
        border-color: #0e1112;
        border-width: 1px;
        border-radius: 5px;
        width: 11px;
        height: 16px;
    }
    .ll-segmented {
        background-color: #0f1416;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 5px;
        padding: 2px;
    }
    button.ll-seg-button {
        background-color: transparent;
        border-width: 0px;
        border-radius: 3px;
        color: #9ba7a2;
        font-size: 10px;
        padding-left: 5px;
        padding-right: 5px;
    }
    button.ll-seg-button:hover {
        background-color: #242d30;
        color: #e6eee9;
    }
    button.ll-seg-active {
        background-color: #315667;
        color: #edf8ff;
    }
    button.ll-tool-button,
    button.ll-step-button {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        border-radius: 5px;
        color: #dce6e0;
    }
    button.ll-tool-button:hover,
    button.ll-step-button:hover {
        background-color: #273235;
        border-color: #59b6d8;
    }
    .ll-toolbar-icon {
        color: #dce6e0;
        width: 15px;
        height: 15px;
    }
    .ll-pad-button {
        background-color: #20282b;
        border-width: 1px;
        border-color: #3c4548;
        border-radius: 5px;
        color: #dce6e0;
    }
    .ll-pad-button:hover { border-color: #f2a84b; }
    .ll-pad-selected { background-color: #3f3020; border-color: #f2a84b; }
    .ll-pad-choked { border-color: #9a78ff; }
    .ll-tooltip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #4d5b61;
        border-radius: 5px;
        color: #dce6e0;
    }
"#;
const KNOB_HELP: &str =
    "Drag or wheel to adjust. Shift-drag for fine control. Double-click to reset.";
#[derive(Clone, Copy)]
pub(crate) enum Accent {
    Audio,
    Tone,
    Slice,
    Mod,
    Transport,
}
#[derive(Clone, Copy)]
pub(crate) enum ChipKind {
    Neutral,
    Audio,
    Slice,
}
pub(crate) fn section_header(
    cx: &mut Context,
    title: &'static str,
    detail: impl Res<String> + Clone + 'static,
    accent: Accent,
) {
    HStack::new(cx, move |cx| {
        Element::new(cx)
            .class("ll-accent")
            .class(accent_class(accent));
        VStack::new(cx, move |cx| {
            Label::new(cx, title).class("ll-section-title");
            Label::new(cx, detail.clone()).class("ll-section-subtitle");
        })
        .vertical_gap(Pixels(1.0));
    })
    .height(Pixels(28.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(7.0));
}
pub(crate) fn static_section_header(
    cx: &mut Context,
    title: &'static str,
    detail: &'static str,
    accent: Accent,
) {
    section_header(cx, title, detail.to_string(), accent);
}
pub(crate) fn status_chip<T>(cx: &mut Context, text: T, kind: ChipKind)
where
    T: Res<String> + Clone + 'static,
{
    Label::new(cx, text)
        .class("ll-status-chip")
        .class(chip_class(kind))
        .height(Pixels(22.0))
        .alignment(Alignment::Center);
}
pub(crate) fn metric<T>(cx: &mut Context, label: &'static str, value: T)
where
    T: Res<String> + Clone + 'static,
{
    VStack::new(cx, move |cx| {
        Label::new(cx, label).class("ll-metric-label");
        Label::new(cx, value.clone()).class("ll-metric-value");
    })
    .height(Pixels(32.0))
    .width(Pixels(78.0))
    .vertical_gap(Pixels(1.0));
}
pub(crate) fn parameter_knob<V, F>(
    cx: &mut Context,
    label: &'static str,
    value_text: V,
    normalized: Signal<f32>,
    default_normalized: f32,
    centered: bool,
    accent: Accent,
    on_change: F,
) where
    V: Res<String> + Clone + 'static,
    F: Fn(&mut EventContext, f32) + Copy + 'static,
{
    VStack::new(cx, move |cx| {
        Knob::new(cx, default_normalized, normalized, centered)
            .on_change(move |cx, normalized| on_change(cx, normalized))
            .class("ll-knob")
            .class(knob_class(accent));
        Label::new(cx, label)
            .class("ll-control-label")
            .alignment(Alignment::Center)
            .width(Pixels(74.0));
        Label::new(cx, value_text.clone())
            .class("ll-control-value")
            .alignment(Alignment::Center)
            .width(Pixels(74.0));
        range_marks(cx, centered);
    })
    .class("ll-knob-cell")
    .width(Pixels(80.0))
    .height(Pixels(82.0))
    .alignment(Alignment::Center)
    .vertical_gap(Pixels(2.0))
    .tooltip(|cx| tooltip(cx, KNOB_HELP));
}

pub(crate) fn parameter_stepper<V, F>(
    cx: &mut Context,
    label: &'static str,
    value_text: V,
    normalized: Signal<f32>,
    count: usize,
    on_change: F,
) where
    V: Res<String> + Clone + 'static,
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("ll-control-label")
            .width(Pixels(52.0));
        step_button(cx, "-", normalized, count, -1, on_change);
        Label::new(cx, value_text.clone())
            .class("ll-control-value")
            .width(Stretch(1.0))
            .alignment(Alignment::Center);
        step_button(cx, "+", normalized, count, 1, on_change);
    })
    .class("ll-stepper")
    .height(Pixels(24.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(5.0));
}
pub(crate) fn inline_parameter_segmented<F>(
    cx: &mut Context,
    label: &'static str,
    normalized: Signal<f32>,
    labels: &'static [&'static str],
    width: f32,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("ll-control-label")
            .width(Pixels(56.0));
        segmented_buttons(cx, normalized, labels, width, on_change);
    })
    .height(Pixels(24.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}

pub(crate) fn inline_binary_segmented<F>(
    cx: &mut Context,
    label: &'static str,
    normalized: Signal<f32>,
    left_label: &'static str,
    right_label: &'static str,
    width: f32,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("ll-control-label")
            .width(Pixels(56.0));
        HStack::new(cx, move |cx| {
            segmented_button(cx, normalized, 2, 0, left_label, on_change);
            segmented_button(cx, normalized, 2, 1, right_label, on_change);
        })
        .class("segmented")
        .class("ll-segmented")
        .height(Pixels(24.0))
        .width(Pixels(width))
        .horizontal_gap(Pixels(2.0));
    })
    .height(Pixels(24.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
}
pub(crate) fn icon_tool_button<'a>(
    cx: &'a mut Context,
    icon: &'static str,
    tooltip_text: &'static str,
) -> Handle<'a, Button> {
    Button::new(cx, move |cx| {
        Svg::new(cx, icon)
            .class("toolbar-icon")
            .class("ll-toolbar-icon")
    })
    .class("toolbar-button")
    .class("ll-tool-button")
    .width(Pixels(28.0))
    .height(Pixels(26.0))
    .tooltip(move |cx| tooltip(cx, tooltip_text))
}
pub(crate) fn compact_text_button<'a>(
    cx: &'a mut Context,
    label: &'static str,
    tooltip_text: &'static str,
) -> Handle<'a, Button> {
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .class("step-button")
    .class("ll-step-button")
    .height(Pixels(24.0))
    .tooltip(move |cx| tooltip(cx, tooltip_text))
}
fn segmented_buttons<F>(
    cx: &mut Context,
    normalized: Signal<f32>,
    labels: &'static [&'static str],
    width: f32,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    HStack::new(cx, move |cx| {
        for (index, label) in labels.iter().copied().enumerate() {
            segmented_button(cx, normalized, labels.len(), index, label, on_change);
        }
    })
    .class("segmented")
    .class("ll-segmented")
    .height(Pixels(24.0))
    .width(Pixels(width))
    .horizontal_gap(Pixels(2.0));
}
fn segmented_button<F>(
    cx: &mut Context,
    normalized: Signal<f32>,
    count: usize,
    index: usize,
    label: &'static str,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    let next = normalized_for_index(index, count);
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| on_change(cx, next))
    .class("seg-button")
    .class("ll-seg-button")
    .toggle_class(
        "seg-active",
        normalized.map(move |value| selected_index(*value, count) == Some(index)),
    )
    .toggle_class(
        "ll-seg-active",
        normalized.map(move |value| selected_index(*value, count) == Some(index)),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}
fn step_button<F>(
    cx: &mut Context,
    label: &'static str,
    normalized: Signal<f32>,
    count: usize,
    direction: isize,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    Button::new(cx, move |cx| {
        Label::new(cx, label).alignment(Alignment::Center)
    })
    .on_press(move |cx| {
        let selected = selected_index(normalized.get(), count).unwrap_or(0) as isize;
        let next = (selected + direction).clamp(0, count.saturating_sub(1) as isize) as usize;
        on_change(cx, normalized_for_index(next, count));
    })
    .class("step-button")
    .class("ll-step-button")
    .width(Pixels(24.0))
    .height(Pixels(22.0));
}
fn range_marks(cx: &mut Context, centered: bool) {
    HStack::new(cx, move |cx| {
        Label::new(cx, "min").class("ll-range-mark");
        Spacer::new(cx);
        Label::new(cx, if centered { "ctr" } else { "def" }).class("ll-range-mark");
        Spacer::new(cx);
        Label::new(cx, "max").class("ll-range-mark");
    })
    .width(Pixels(66.0))
    .height(Pixels(8.0))
    .alignment(Alignment::Center);
}
fn tooltip<'a>(cx: &'a mut Context, text: &'static str) -> Handle<'a, Tooltip> {
    Tooltip::new(cx, move |cx| {
        Label::new(cx, text).padding(Pixels(5.0));
    })
    .class("tooltip")
    .class("ll-tooltip")
    .padding(Pixels(3.0))
    .size(Auto)
    .placement(Placement::Bottom)
}
fn accent_class(accent: Accent) -> &'static str {
    match accent {
        Accent::Audio => "ll-accent-audio",
        Accent::Tone => "ll-accent-tone",
        Accent::Slice => "ll-accent-slice",
        Accent::Mod => "ll-accent-mod",
        Accent::Transport => "ll-accent-transport",
    }
}
fn knob_class(accent: Accent) -> &'static str {
    match accent {
        Accent::Audio => "ll-knob-audio",
        Accent::Slice => "ll-knob-slice",
        Accent::Mod => "ll-knob-mod",
        Accent::Transport => "ll-knob-transport",
        _ => "ll-knob-tone",
    }
}
fn chip_class(kind: ChipKind) -> &'static str {
    match kind {
        ChipKind::Neutral => "ll-chip-neutral",
        ChipKind::Audio => "ll-chip-audio",
        ChipKind::Slice => "ll-chip-slice",
    }
}
pub(crate) const fn normalized_for_index(index: usize, count: usize) -> f32 {
    if count <= 1 {
        0.0
    } else {
        index as f32 / (count - 1) as f32
    }
}
pub(crate) fn selected_index(value: f32, count: usize) -> Option<usize> {
    (count > 0).then(|| (value.clamp(0.0, 1.0) * (count - 1) as f32).round() as usize)
}
