use vizia::prelude::*;

#[path = "vizia_controls/choice.rs"]
mod choice;
#[path = "vizia_controls/drag_value.rs"]
mod drag_value;

pub(crate) use choice::{
    IconSegmentedChoice, compact_binary_segmented, inline_binary_segmented, inline_icon_segmented,
    inline_parameter_segmented, parameter_cycle_selector,
};
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
    .ll-settings-panel {
        background-color: #151b1d;
        border-width: 1px;
        border-color: #4b565a;
        border-radius: 6px;
        padding: 12px;
    }
    .ll-settings-title {
        color: #f2f7f3;
        font-size: 14px;
    }
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
    .ll-cycle-selector:hover .ll-control-label {
        color: #c4cec8;
    }
    .ll-control-row:hover .ll-control-value,
    .ll-drag-value:hover .ll-control-value,
    .ll-knob-cell:hover .ll-control-value,
    .ll-cycle-selector:hover .ll-control-value {
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
        width: 31px;
        height: 31px;
    }
    .ll-knob .knob-track {
        color: #7ed06d;
        background-color: #30383c;
    }
    .ll-knob-audio .knob-track { color: #59b6d8; }
    .ll-knob-slice .knob-track { color: #f2a84b; }
    .ll-knob-mod .knob-track { color: #9a78ff; }
    .ll-knob-transport .knob-track { color: #ef6f88; }
    .ll-knob .knob-head {
        background-color: #1c2427;
        border-width: 1px;
        border-color: #5a676b;
        color: #f0f8f2;
    }
    .ll-knob-tone .knob-head { border-color: #536a50; }
    .ll-knob-audio .knob-head { border-color: #416777; }
    .ll-knob-slice .knob-head { border-color: #73582f; }
    .ll-knob-mod .knob-head { border-color: #5f5290; }
    .ll-knob-transport .knob-head { border-color: #794655; }
    .ll-knob:hover .knob-head {
        background-color: #232d30;
        border-color: #d8e3dc;
    }
    .ll-knob .knob-tick {
        background-color: #edf6f1;
        width: 2px;
        height: 8px;
        border-radius: 1px;
    }
    .ll-knob-label { font-size: 9px; }
    .ll-knob-value { font-size: 10px; }
    .ll-range-mark {
        color: #67746f;
        font-size: 7px;
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
    button.ll-icon-seg-button {
        background-color: transparent;
        border-width: 0px;
        border-radius: 3px;
        color: #93a19a;
    }
    button.ll-icon-seg-button:hover {
        background-color: #242d30;
        color: #f0f7f2;
    }
    button.ll-icon-seg-active {
        background-color: #315667;
        color: #edf8ff;
    }
    .ll-choice-icon {
        color: #dce6e0;
        width: 15px;
        height: 15px;
    }
    button.ll-cycle-button {
        background-color: #111719;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 5px;
        color: #dce6e0;
        padding-left: 7px;
        padding-right: 7px;
    }
    button.ll-cycle-button:hover {
        background-color: #1d2528;
        border-color: #59b6d8;
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
            .class("ll-knob-label")
            .alignment(Alignment::Center)
            .width(Pixels(62.0));
        Label::new(cx, value_text.clone())
            .class("ll-control-value")
            .class("ll-knob-value")
            .alignment(Alignment::Center)
            .width(Pixels(62.0));
        range_marks(cx, centered);
    })
    .class("ll-knob-cell")
    .width(Pixels(66.0))
    .height(Pixels(66.0))
    .alignment(Alignment::Center)
    .vertical_gap(Pixels(1.0))
    .tooltip(|cx| tooltip(cx, KNOB_HELP));
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
fn range_marks(cx: &mut Context, centered: bool) {
    HStack::new(cx, move |cx| {
        Label::new(cx, "min").class("ll-range-mark");
        Spacer::new(cx);
        Label::new(cx, if centered { "ctr" } else { "def" }).class("ll-range-mark");
        Spacer::new(cx);
        Label::new(cx, "max").class("ll-range-mark");
    })
    .width(Pixels(54.0))
    .height(Pixels(7.0))
    .alignment(Alignment::Center);
}
pub(crate) fn tooltip<'a>(cx: &'a mut Context, text: &'static str) -> Handle<'a, Tooltip> {
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
