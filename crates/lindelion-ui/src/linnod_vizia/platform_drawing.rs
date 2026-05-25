struct OutputMeterView {
    telemetry: Signal<LinnodEditorTelemetry>,
}

impl OutputMeterView {
    fn new(cx: &mut Context, telemetry: Signal<LinnodEditorTelemetry>) -> Handle<'_, Self> {
        Self { telemetry }
            .build(cx, |_| {})
            .bind(telemetry, |mut view| view.needs_redraw())
    }
}

impl View for OutputMeterView {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas, false);
        let telemetry = self.telemetry.get();
        draw_meter_bar(bounds, canvas, 0, telemetry.left_peak);
        draw_meter_bar(bounds, canvas, 1, telemetry.right_peak);
    }
}

fn draw_panel_background(bounds: BoundingBox, canvas: &Canvas, active: bool) {
    draw_rect(
        canvas,
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        if active {
            Color::rgb(18, 28, 24)
        } else {
            Color::rgb(14, 18, 20)
        },
    );
    let mut paint = vg::Paint::default();
    paint.set_color(if active {
        Color::rgba(139, 199, 161, 210)
    } else {
        Color::rgba(64, 80, 84, 160)
    });
    paint.set_stroke_width(1.0);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_anti_alias(true);
    canvas.draw_rect(
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        &paint,
    );
}

fn draw_meter_bar(bounds: BoundingBox, canvas: &Canvas, index: usize, peak: f32) {
    let top = bounds.y + 8.0 + index as f32 * 12.0;
    let left = bounds.x + 10.0;
    let width = (bounds.w - 20.0) * peak.clamp(0.0, 1.0);
    draw_rect(
        canvas,
        vg::Rect::new(left, top, left + width, top + 7.0),
        Color::rgb(130, 188, 152),
    );
}

fn draw_rect(canvas: &Canvas, rect: vg::Rect, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_style(vg::PaintStyle::Fill);
    paint.set_anti_alias(true);
    canvas.draw_rect(rect, &paint);
}
