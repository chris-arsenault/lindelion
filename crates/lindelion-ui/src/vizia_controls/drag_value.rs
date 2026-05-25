use vizia::prelude::*;

use super::{Accent, tooltip};

const DRAG_VALUE_HELP: &str =
    "Drag vertically or use the wheel. Shift for fine control. Double-click to reset.";

#[derive(Clone, Copy)]
pub(crate) struct DragValueSpec {
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) default: f32,
    pub(crate) coarse_step: f32,
    pub(crate) fine_step: f32,
    pub(crate) width: f32,
    pub(crate) accent: Accent,
}

impl DragValueSpec {
    pub(crate) const fn new(
        min: f32,
        max: f32,
        default: f32,
        coarse_step: f32,
        fine_step: f32,
        width: f32,
        accent: Accent,
    ) -> Self {
        Self {
            min,
            max,
            default,
            coarse_step,
            fine_step,
            width,
            accent,
        }
    }
}

pub(crate) fn drag_value<S, V, F>(
    cx: &mut Context,
    label: &'static str,
    value_text: V,
    value: S,
    spec: DragValueSpec,
    on_change: F,
) where
    S: SignalGet<f32> + Copy + 'static,
    V: Res<String> + Clone + 'static,
    F: Fn(&mut EventContext, f32) + 'static,
{
    drag_value_with(
        cx,
        label,
        value_text,
        move || value.get(),
        move || spec,
        on_change,
    );
}

pub(crate) fn dynamic_drag_value<V, Value, Spec, F>(
    cx: &mut Context,
    label: &'static str,
    value_text: V,
    value: Value,
    spec: Spec,
    on_change: F,
) where
    V: Res<String> + Clone + 'static,
    Value: Fn() -> f32 + 'static,
    Spec: Fn() -> DragValueSpec + 'static,
    F: Fn(&mut EventContext, f32) + 'static,
{
    drag_value_with(cx, label, value_text, value, spec, on_change);
}

fn drag_value_with<V, Value, Spec, F>(
    cx: &mut Context,
    label: &'static str,
    value_text: V,
    value: Value,
    spec: Spec,
    on_change: F,
) where
    V: Res<String> + Clone + 'static,
    Value: Fn() -> f32 + 'static,
    Spec: Fn() -> DragValueSpec + 'static,
    F: Fn(&mut EventContext, f32) + 'static,
{
    let initial_spec = spec();
    DragValue::new(cx, label, value_text, value, spec, on_change)
        .class("ll-drag-value")
        .class(drag_value_class(initial_spec.accent))
        .width(Pixels(initial_spec.width))
        .height(Pixels(42.0))
        .tooltip(|cx| tooltip(cx, DRAG_VALUE_HELP));
}

struct DragValue {
    value: Box<dyn Fn() -> f32>,
    spec: Box<dyn Fn() -> DragValueSpec>,
    drag: Option<DragValueDrag>,
    on_change: Box<dyn Fn(&mut EventContext, f32)>,
}

#[derive(Clone, Copy)]
struct DragValueDrag {
    start_y: f32,
    start_value: f32,
    last_value: f32,
}

impl DragValue {
    fn new<'a, V, Value, Spec, F>(
        cx: &'a mut Context,
        label: &'static str,
        value_text: V,
        value: Value,
        spec: Spec,
        on_change: F,
    ) -> Handle<'a, Self>
    where
        V: Res<String> + Clone + 'static,
        Value: Fn() -> f32 + 'static,
        Spec: Fn() -> DragValueSpec + 'static,
        F: Fn(&mut EventContext, f32) + 'static,
    {
        Self {
            value: Box::new(value),
            spec: Box::new(spec),
            drag: None,
            on_change: Box::new(on_change),
        }
        .build(cx, move |cx| {
            VStack::new(cx, move |cx| {
                Label::new(cx, label)
                    .class("ll-control-label")
                    .alignment(Alignment::Center);
                Label::new(cx, value_text.clone())
                    .class("ll-control-value")
                    .alignment(Alignment::Center);
            })
            .vertical_gap(Pixels(2.0))
            .alignment(Alignment::Center);
        })
    }

    fn current_value(&self) -> f32 {
        self.clamp_value((self.value)())
    }

    fn clamp_value(&self, value: f32) -> f32 {
        let spec = (self.spec)();
        if value.is_finite() {
            value.clamp(spec.min, spec.max)
        } else {
            spec.default
        }
    }

    fn step(&self, cx: &EventContext) -> f32 {
        let spec = (self.spec)();
        if cx.modifiers().shift() {
            spec.fine_step
        } else {
            spec.coarse_step
        }
        .abs()
        .max(f32::EPSILON)
    }

    fn emit_value(&mut self, cx: &mut EventContext, value: f32) -> f32 {
        let value = self.clamp_value(value);
        (self.on_change)(cx, value);
        value
    }
}

impl View for DragValue {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                self.emit_value(cx, (self.spec)().default);
                self.drag = None;
                cx.release();
                meta.consume();
            }
            WindowEvent::MouseDown(MouseButton::Left) => {
                let value = self.current_value();
                self.drag = Some(DragValueDrag {
                    start_y: cx.mouse().left.pos_down.1,
                    start_value: value,
                    last_value: value,
                });
                cx.capture();
                meta.consume();
            }
            WindowEvent::MouseMove(_, y) => {
                let Some(mut drag) = self.drag else {
                    return;
                };
                let steps = ((drag.start_y - *y) / 7.0).round();
                let next = self.clamp_value(drag.start_value + steps * self.step(cx));
                if (next - drag.last_value).abs() > 0.000_1 {
                    drag.last_value = self.emit_value(cx, next);
                    self.drag = Some(drag);
                }
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.drag.take().is_some() {
                    cx.release();
                    meta.consume();
                }
            }
            WindowEvent::MouseScroll(_, y) => {
                let direction = if *y >= 0.0 { 1.0 } else { -1.0 };
                let next = self.current_value() + direction * self.step(cx);
                self.emit_value(cx, next);
                meta.consume();
            }
            _ => {}
        });
    }
}

fn drag_value_class(accent: Accent) -> &'static str {
    match accent {
        Accent::Audio => "ll-drag-value-audio",
        Accent::Slice => "ll-drag-value-slice",
        Accent::Mod => "ll-drag-value-mod",
        Accent::Transport => "ll-drag-value-transport",
        _ => "ll-drag-value-tone",
    }
}
