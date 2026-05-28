use vizia::prelude::*;

const CYCLE_SELECTOR_HELP: &str = "Click to cycle choices. Right-click for previous.";

#[derive(Clone, Copy)]
pub(crate) struct IconSegmentedChoice {
    pub icon: &'static str,
    pub tooltip: &'static str,
}

impl IconSegmentedChoice {
    pub const fn new(icon: &'static str, tooltip: &'static str) -> Self {
        Self { icon, tooltip }
    }
}

pub(crate) fn parameter_cycle_selector<V, F>(
    cx: &mut Context,
    label: &'static str,
    value_text: V,
    normalized: Signal<f32>,
    count: usize,
    width: f32,
    on_change: F,
) where
    V: Res<String> + Clone + 'static,
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    let width = selector_width(width);
    HStack::new(cx, move |cx| {
        Label::new(cx, label)
            .class("ll-control-label")
            .width(Pixels(52.0));
        Button::new(cx, move |cx| {
            Label::new(cx, value_text.clone())
                .class("ll-control-value")
                .alignment(Alignment::Center)
        })
        .on_mouse_up(move |cx, button| match button {
            MouseButton::Left => {
                on_change(cx, cycled_normalized(normalized.get(), count, 1));
            }
            MouseButton::Right => {
                on_change(cx, cycled_normalized(normalized.get(), count, -1));
            }
            _ => {}
        })
        .class("ll-cycle-button")
        .width(Pixels(width))
        .height(Pixels(24.0));
    })
    .class("ll-cycle-selector")
    .height(Pixels(24.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0))
    .tooltip(|cx| super::tooltip(cx, CYCLE_SELECTOR_HELP));
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

pub(crate) fn compact_binary_segmented<F>(
    cx: &mut Context,
    normalized: Signal<f32>,
    left_label: &'static str,
    right_label: &'static str,
    width: f32,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    HStack::new(cx, move |cx| {
        segmented_button(cx, normalized, 2, 0, left_label, on_change);
        segmented_button(cx, normalized, 2, 1, right_label, on_change);
    })
    .class("segmented")
    .class("ll-segmented")
    .height(Pixels(24.0))
    .width(Pixels(width))
    .horizontal_gap(Pixels(2.0));
}

pub(crate) fn inline_icon_segmented<F>(
    cx: &mut Context,
    label: &'static str,
    normalized: Signal<f32>,
    choices: &'static [IconSegmentedChoice],
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
            for (index, choice) in choices.iter().copied().enumerate() {
                icon_segmented_button(cx, normalized, choices.len(), index, choice, on_change);
            }
        })
        .class("segmented")
        .class("ll-segmented")
        .height(Pixels(24.0))
        .width(Pixels(width.max(28.0 * choices.len() as f32)))
        .horizontal_gap(Pixels(2.0));
    })
    .height(Pixels(24.0))
    .alignment(Alignment::Center)
    .horizontal_gap(Pixels(6.0));
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

fn icon_segmented_button<F>(
    cx: &mut Context,
    normalized: Signal<f32>,
    count: usize,
    index: usize,
    choice: IconSegmentedChoice,
    on_change: F,
) where
    F: Fn(&mut EventContext, f32) + Copy + Send + Sync + 'static,
{
    let next = normalized_for_index(index, count);
    Button::new(cx, move |cx| {
        Svg::new(cx, choice.icon).class("ll-choice-icon")
    })
    .on_press(move |cx| on_change(cx, next))
    .class("ll-icon-seg-button")
    .toggle_class(
        "ll-icon-seg-active",
        normalized.map(move |value| selected_index(*value, count) == Some(index)),
    )
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .tooltip(move |cx| super::tooltip(cx, choice.tooltip));
}

const fn normalized_for_index(index: usize, count: usize) -> f32 {
    if count <= 1 {
        0.0
    } else {
        index as f32 / (count - 1) as f32
    }
}

fn selected_index(value: f32, count: usize) -> Option<usize> {
    (count > 0).then(|| (value.clamp(0.0, 1.0) * (count - 1) as f32).round() as usize)
}

fn selector_width(width: f32) -> f32 {
    if width.is_finite() && width >= 64.0 {
        width
    } else {
        92.0
    }
}

fn cycled_normalized(value: f32, count: usize, direction: isize) -> f32 {
    if count <= 1 {
        return 0.0;
    }
    let selected = selected_index(value, count).unwrap_or(0) as isize;
    let count = count as isize;
    let next = (selected + direction).rem_euclid(count) as usize;
    normalized_for_index(next, count as usize)
}
