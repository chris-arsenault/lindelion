fn draw_waveform_grid(rect: WaveformRect, canvas: &Canvas) {
    let center_y = rect.y + rect.h * 0.5;
    draw_line(
        canvas,
        rect.x,
        center_y,
        rect.x + rect.w,
        center_y,
        Color::rgba(82, 99, 101, 145),
        1.0,
    );
    for index in 1..4 {
        let x = rect.x + rect.w * index as f32 / 4.0;
        draw_line(
            canvas,
            x,
            rect.y + 2.0,
            x,
            rect.y + rect.h - 2.0,
            Color::rgba(50, 62, 66, 90),
            1.0,
        );
    }
}

fn draw_waveform_body(
    rect: WaveformRect,
    canvas: &Canvas,
    waveform: &[WaveformPoint],
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
    compact: bool,
) {
    if waveform.is_empty() {
        draw_empty_wave(rect, canvas);
        return;
    }
    let source_span = source_span_samples(summary).max(1) as f32;
    let start = range.start as f32 / source_span;
    let end = range.end as f32 / source_span;
    let target_points = rect.w.ceil().clamp(16.0, 2048.0) as usize;
    let points = crate::waveform_points_for_view(waveform, start, end, target_points);
    draw_peak_area(rect, canvas, &points, compact);
    draw_rms_area(rect, canvas, &points, compact);
    draw_waveform_outline(rect, canvas, &points, compact);
}

fn draw_peak_area(rect: WaveformRect, canvas: &Canvas, points: &[WaveformPoint], compact: bool) {
    let mut path = vg::PathBuilder::new();
    let center_y = rect.y + rect.h * 0.5;
    let scale_y = rect.h * if compact { 0.34 } else { 0.42 };
    for (index, point) in points.iter().enumerate() {
        let x = waveform_point_x(rect, index, points.len());
        let y = center_y - point.max.clamp(-1.0, 1.0) * scale_y;
        if index == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    for (index, point) in points.iter().enumerate().rev() {
        let x = waveform_point_x(rect, index, points.len());
        let y = center_y - point.min.clamp(-1.0, 1.0) * scale_y;
        path.line_to((x, y));
    }
    let mut paint = vg::Paint::default();
    paint.set_color(if compact {
        Color::rgba(78, 126, 111, 135)
    } else {
        Color::rgba(64, 124, 103, 150)
    });
    paint.set_style(vg::PaintStyle::Fill);
    paint.set_anti_alias(true);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_rms_area(rect: WaveformRect, canvas: &Canvas, points: &[WaveformPoint], compact: bool) {
    let mut path = vg::PathBuilder::new();
    let center_y = rect.y + rect.h * 0.5;
    let scale_y = rect.h * if compact { 0.28 } else { 0.34 };
    for (index, point) in points.iter().enumerate() {
        let x = waveform_point_x(rect, index, points.len());
        let y = center_y - point.rms.abs().clamp(0.0, 1.0) * scale_y;
        if index == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    for (index, point) in points.iter().enumerate().rev() {
        let x = waveform_point_x(rect, index, points.len());
        let y = center_y + point.rms.abs().clamp(0.0, 1.0) * scale_y;
        path.line_to((x, y));
    }
    let mut paint = vg::Paint::default();
    paint.set_color(if compact {
        Color::rgba(126, 190, 151, 115)
    } else {
        Color::rgba(126, 218, 160, 135)
    });
    paint.set_style(vg::PaintStyle::Fill);
    paint.set_anti_alias(true);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_waveform_outline(
    rect: WaveformRect,
    canvas: &Canvas,
    points: &[WaveformPoint],
    compact: bool,
) {
    draw_waveform_edge(rect, canvas, points, true, compact);
    draw_waveform_edge(rect, canvas, points, false, compact);
}

fn draw_waveform_edge(
    rect: WaveformRect,
    canvas: &Canvas,
    points: &[WaveformPoint],
    upper: bool,
    compact: bool,
) {
    let mut path = vg::PathBuilder::new();
    let center_y = rect.y + rect.h * 0.5;
    let scale_y = rect.h * if compact { 0.34 } else { 0.42 };
    for (index, point) in points.iter().enumerate() {
        let x = waveform_point_x(rect, index, points.len());
        let value = if upper { point.max } else { point.min };
        let y = center_y - value.clamp(-1.0, 1.0) * scale_y;
        if index == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }
    let mut paint = vg::Paint::default();
    paint.set_color(if compact {
        Color::rgba(145, 194, 174, 150)
    } else {
        Color::rgba(164, 238, 194, 210)
    });
    paint.set_stroke_width(if compact { 1.0 } else { 1.35 });
    paint.set_stroke_cap(vg::PaintCap::Round);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_anti_alias(true);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_selected_slice_region(
    rect: WaveformRect,
    canvas: &Canvas,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
) {
    let Some(slice_index) = summary.selected_slice_index else {
        return;
    };
    let Some((start, end)) = slice_bounds(summary, slice_index) else {
        return;
    };
    draw_sample_region(rect, canvas, range, start, end, Color::rgba(48, 73, 58, 115));
    let slice = selected_slice(summary);
    if let Some((effective_start, effective_end)) = effective_slice_bounds(summary, &slice) {
        draw_sample_region(
            rect,
            canvas,
            range,
            effective_start,
            effective_end,
            Color::rgba(92, 135, 89, 92),
        );
    }
}

fn draw_sample_region(
    rect: WaveformRect,
    canvas: &Canvas,
    range: WaveformRange,
    start: usize,
    end: usize,
    color: Color,
) {
    if end <= range.start || start >= range.end {
        return;
    }
    let left = sample_to_x(rect, range, start.max(range.start));
    let right = sample_to_x(rect, range, end.min(range.end));
    draw_rect(
        canvas,
        vg::Rect::new(left, rect.y + 2.0, right.max(left + 2.0), rect.y + rect.h - 2.0),
        color,
    );
}

fn draw_markers(
    rect: WaveformRect,
    canvas: &Canvas,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
) {
    for marker in &summary.markers {
        if marker.position_samples < range.start || marker.position_samples > range.end {
            continue;
        }
        let x = sample_to_x(rect, range, marker.position_samples);
        let color = match marker.kind {
            LinnodEditorMarkerKind::Auto => Color::rgba(132, 164, 202, 190),
            LinnodEditorMarkerKind::User => Color::rgba(226, 190, 122, 220),
        };
        draw_line(canvas, x, rect.y + 4.0, x, rect.y + rect.h - 4.0, color, 1.25);
        draw_marker_cap(canvas, x, rect.y + 4.0, color);
    }
}

fn draw_selected_trim_handles(
    rect: WaveformRect,
    canvas: &Canvas,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
) {
    let slice = selected_slice(summary);
    let Some((start, end)) = effective_slice_bounds(summary, &slice) else {
        return;
    };
    if start >= range.start && start <= range.end {
        let x = sample_to_x(rect, range, start);
        draw_trim_handle(canvas, rect, x, Color::rgba(126, 218, 160, 245));
    }
    if end >= range.start && end <= range.end {
        let x = sample_to_x(rect, range, end);
        draw_trim_handle(canvas, rect, x, Color::rgba(242, 168, 75, 245));
    }
}

fn draw_marker_preview(
    rect: WaveformRect,
    canvas: &Canvas,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
    position_samples: usize,
    color: Color,
) {
    if position_samples < range.start || position_samples > range.end {
        return;
    }
    let x = sample_to_x(rect, range, position_samples.min(source_span_samples(summary)));
    draw_line(canvas, x, rect.y + 2.0, x, rect.y + rect.h - 2.0, color, 2.3);
    draw_marker_cap(canvas, x, rect.y + 2.0, color);
}

fn draw_trim_handle(canvas: &Canvas, rect: WaveformRect, x: f32, color: Color) {
    draw_line(canvas, x, rect.y + 3.0, x, rect.y + rect.h - 3.0, color, 2.0);
    draw_rect(
        canvas,
        vg::Rect::new(x - 4.0, rect.y + 3.0, x + 4.0, rect.y + 11.0),
        color,
    );
    draw_rect(
        canvas,
        vg::Rect::new(x - 4.0, rect.y + rect.h - 11.0, x + 4.0, rect.y + rect.h - 3.0),
        color,
    );
}

fn draw_marker_cap(canvas: &Canvas, x: f32, y: f32, color: Color) {
    draw_rect(canvas, vg::Rect::new(x - 3.0, y, x + 3.0, y + 7.0), color);
}

fn draw_viewport_overview(
    rect: WaveformRect,
    canvas: &Canvas,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
) {
    let span = source_span_samples(summary).max(1) as f32;
    let left = rect.x + range.start as f32 / span * rect.w;
    let right = rect.x + range.end as f32 / span * rect.w;
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(237, 245, 239, 225));
    paint.set_stroke_width(1.2);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_anti_alias(true);
    canvas.draw_rect(
        vg::Rect::new(left, rect.y + 2.0, right.max(left + 3.0), rect.y + rect.h - 2.0),
        &paint,
    );
}

fn draw_empty_wave(rect: WaveformRect, canvas: &Canvas) {
    let mut path = vg::PathBuilder::new();
    let center_y = rect.y + rect.h * 0.5;
    for index in 0..72 {
        let t = index as f32 / 71.0;
        let x = rect.x + t * rect.w;
        let y = center_y + (t * 28.0).sin() * rect.h * 0.08;
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
    paint.set_anti_alias(true);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_line(
    canvas: &Canvas,
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
    color: Color,
    width: f32,
) {
    let mut path = vg::PathBuilder::new();
    path.move_to((start_x, start_y));
    path.line_to((end_x, end_y));
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_stroke_width(width);
    paint.set_style(vg::PaintStyle::Stroke);
    paint.set_anti_alias(true);
    canvas.draw_path(&path.detach(), &paint);
}
