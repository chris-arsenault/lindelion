struct SourceWaveformView {
    summary: Signal<LinnodEditorPatchSummary>,
    drop_active: Signal<bool>,
}

impl SourceWaveformView {
    fn new(
        cx: &mut Context,
        summary: Signal<LinnodEditorPatchSummary>,
        drop_active: Signal<bool>,
    ) -> Handle<'_, Self> {
        Self {
            summary,
            drop_active,
        }
        .build(cx, |_| {})
        .bind(summary, |mut view| view.needs_redraw())
        .bind(drop_active, |mut view| view.needs_redraw())
    }
}

impl View for SourceWaveformView {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas, self.drop_active.get());
        let summary = self.summary.get();
        draw_selected_slice_region(bounds, canvas, &summary);
        draw_center_line(bounds, canvas);
        draw_waveform(bounds, canvas, &summary.waveform);
        draw_markers(bounds, canvas, &summary);
    }
}

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
    canvas.draw_rect(
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        &paint,
    );
}

fn draw_selected_slice_region(
    bounds: BoundingBox,
    canvas: &Canvas,
    summary: &LinnodEditorPatchSummary,
) {
    let Some(slice_index) = summary.selected_slice_index else {
        return;
    };
    let Some((start, end)) = slice_bounds(summary, slice_index) else {
        return;
    };
    let span = source_span_samples(summary) as f32;
    if span <= 0.0 {
        return;
    }
    let left = bounds.x + 12.0 + start as f32 / span * (bounds.w - 24.0);
    let right = bounds.x + 12.0 + end as f32 / span * (bounds.w - 24.0);
    draw_rect(
        canvas,
        vg::Rect::new(left, bounds.y + 8.0, right.max(left + 3.0), bounds.y + bounds.h - 8.0),
        Color::rgba(58, 91, 70, 120),
    );
}

fn draw_waveform(bounds: BoundingBox, canvas: &Canvas, waveform: &[WaveformPoint]) {
    if waveform.is_empty() {
        draw_empty_wave(bounds, canvas);
        return;
    }
    let left = bounds.x + 12.0;
    let width = (bounds.w - 24.0).max(1.0);
    let center_y = bounds.y + bounds.h * 0.5;
    let scale_y = bounds.h * 0.38;
    for (index, point) in waveform.iter().enumerate() {
        let x = left + index as f32 / waveform.len().max(1) as f32 * width;
        draw_waveform_point(canvas, x, center_y, scale_y, point);
    }
}

fn draw_waveform_point(
    canvas: &Canvas,
    x: f32,
    center_y: f32,
    scale_y: f32,
    point: &WaveformPoint,
) {
    let y_min = center_y - point.max.clamp(-1.0, 1.0) * scale_y;
    let y_max = center_y - point.min.clamp(-1.0, 1.0) * scale_y;
    let mut path = vg::PathBuilder::new();
    path.move_to((x, y_min));
    path.line_to((x, y_max));
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgb(126, 190, 151));
    paint.set_stroke_width(1.5);
    paint.set_stroke_cap(vg::PaintCap::Round);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_empty_wave(bounds: BoundingBox, canvas: &Canvas) {
    let mut path = vg::PathBuilder::new();
    let center_y = bounds.y + bounds.h * 0.5;
    for index in 0..72 {
        let t = index as f32 / 71.0;
        let x = bounds.x + 12.0 + t * (bounds.w - 24.0);
        let y = center_y + (t * 28.0).sin() * bounds.h * 0.08;
        if index == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(117, 146, 138, 130));
    paint.set_stroke_width(1.4);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_markers(bounds: BoundingBox, canvas: &Canvas, summary: &LinnodEditorPatchSummary) {
    let span = source_span_samples(summary) as f32;
    if span <= 0.0 {
        return;
    }
    for marker in &summary.markers {
        let x = bounds.x + 12.0 + marker.position_samples as f32 / span * (bounds.w - 24.0);
        let mut path = vg::PathBuilder::new();
        path.move_to((x, bounds.y + 8.0));
        path.line_to((x, bounds.y + bounds.h - 8.0));
        let mut paint = vg::Paint::default();
        paint.set_color(match marker.kind {
            super::LinnodEditorMarkerKind::Auto => Color::rgba(132, 164, 202, 190),
            super::LinnodEditorMarkerKind::User => Color::rgba(226, 190, 122, 210),
        });
        paint.set_stroke_width(1.2);
        paint.set_style(vg::PaintStyle::Stroke);
        canvas.draw_path(&path.detach(), &paint);
    }
}

fn draw_center_line(bounds: BoundingBox, canvas: &Canvas) {
    let center_y = bounds.y + bounds.h * 0.5;
    let mut path = vg::PathBuilder::new();
    path.move_to((bounds.x + 10.0, center_y));
    path.line_to((bounds.x + bounds.w - 10.0, center_y));
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(82, 99, 101, 140));
    paint.set_stroke_width(1.0);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_path(&path.detach(), &paint);
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
    canvas.draw_rect(rect, &paint);
}
