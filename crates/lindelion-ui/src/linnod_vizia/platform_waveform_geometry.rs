impl WaveformRange {
    fn normalized(self, source_span: usize) -> Self {
        let start = self.start.min(source_span.saturating_sub(1));
        let end = self.end.max(start + 1).min(source_span);
        Self { start, end }
    }
}

fn main_waveform_rect(bounds: BoundingBox, mode: WaveformViewMode) -> WaveformRect {
    let overview_space = if mode == WaveformViewMode::SourceEditor {
        WAVEFORM_OVERVIEW_HEIGHT + 10.0
    } else {
        0.0
    };
    WaveformRect {
        x: bounds.x + WAVEFORM_SIDE_PAD,
        y: bounds.y + WAVEFORM_TOP_PAD,
        w: (bounds.w - WAVEFORM_SIDE_PAD * 2.0).max(1.0),
        h: (bounds.h - WAVEFORM_TOP_PAD - WAVEFORM_BOTTOM_PAD - overview_space).max(1.0),
    }
}

fn overview_waveform_rect(bounds: BoundingBox) -> WaveformRect {
    WaveformRect {
        x: bounds.x + WAVEFORM_SIDE_PAD,
        y: bounds.y + bounds.h - WAVEFORM_OVERVIEW_HEIGHT - 8.0,
        w: (bounds.w - WAVEFORM_SIDE_PAD * 2.0).max(1.0),
        h: WAVEFORM_OVERVIEW_HEIGHT,
    }
}

fn nearest_marker_at_x(
    rect: WaveformRect,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
    x: f32,
) -> Option<LinnodEditorMarker> {
    summary
        .markers
        .iter()
        .copied()
        .filter(|marker| {
            marker.position_samples >= range.start && marker.position_samples <= range.end
        })
        .map(|marker| {
            let distance = (sample_to_x(rect, range, marker.position_samples) - x).abs();
            (marker, distance)
        })
        .filter(|(_, distance)| *distance <= MARKER_HIT_RADIUS_PX)
        .min_by(|(_, left), (_, right)| {
            left.partial_cmp(right)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(marker, _)| marker)
}

fn nearest_trim_handle_at_x(
    rect: WaveformRect,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
    x: f32,
) -> Option<WaveformDrag> {
    let slice = selected_slice(summary);
    let (start, end) = effective_slice_bounds(summary, &slice)?;
    let start_distance = (sample_to_x(rect, range, start) - x).abs();
    let end_distance = (sample_to_x(rect, range, end) - x).abs();
    if start_distance.min(end_distance) > SLICE_HANDLE_HIT_RADIUS_PX {
        return None;
    }
    let drag = SliceTrimDrag {
        slice_index: slice.index,
        start_offset_ms: slice.start_offset_ms,
        end_offset_ms: slice.end_offset_ms,
        current_position_samples: if start_distance <= end_distance {
            start
        } else {
            end
        },
        started: false,
    };
    if start_distance <= end_distance {
        Some(WaveformDrag::SliceStart(drag))
    } else {
        Some(WaveformDrag::SliceEnd(drag))
    }
}

fn sample_at_x(
    rect: WaveformRect,
    summary: &LinnodEditorPatchSummary,
    range: WaveformRange,
    x: f32,
) -> usize {
    let position = ((x - rect.x) / rect.w.max(1.0)).clamp(0.0, 1.0);
    let sample = range.start as f32 + position * range.end.saturating_sub(range.start) as f32;
    sample.round().clamp(0.0, source_span_samples(summary) as f32) as usize
}

fn sample_to_x(rect: WaveformRect, range: WaveformRange, position_samples: usize) -> f32 {
    let span = range.end.saturating_sub(range.start).max(1) as f32;
    let position = position_samples.saturating_sub(range.start) as f32 / span;
    rect.x + position.clamp(0.0, 1.0) * rect.w
}

fn waveform_point_x(rect: WaveformRect, index: usize, len: usize) -> f32 {
    if len <= 1 {
        return rect.x;
    }
    rect.x + index as f32 / (len - 1) as f32 * rect.w
}

fn can_edit_waveform(summary: &LinnodEditorPatchSummary) -> bool {
    !summary.waveform.is_empty()
        && summary
            .slices
            .iter()
            .any(|slice| slice.end_sample > slice.start_sample)
}

fn selected_slice_focus_range(
    summary: &LinnodEditorPatchSummary,
    source_span: usize,
) -> WaveformRange {
    let slice = selected_slice(summary);
    let (start, end) = effective_slice_bounds(summary, &slice)
        .or_else(|| slice_bounds(summary, slice.index))
        .unwrap_or((0, source_span));
    let slice_span = end.saturating_sub(start).max(1);
    let pad = (slice_span / 3).max(summary.source_sample_rate as usize / 200);
    WaveformRange {
        start: start.saturating_sub(pad),
        end: end.saturating_add(pad).min(source_span),
    }
    .normalized(source_span)
}

fn slice_index_at_sample(
    summary: &LinnodEditorPatchSummary,
    position_samples: usize,
) -> Option<usize> {
    summary
        .slices
        .iter()
        .find(|slice| {
            slice_bounds(summary, slice.index)
                .is_some_and(|(start, end)| position_samples >= start && position_samples < end)
        })
        .map(|slice| slice.index)
}

fn slice_start_offset_ms(
    summary: &LinnodEditorPatchSummary,
    slice_index: usize,
    position_samples: usize,
    end_offset_ms: f32,
) -> Option<f32> {
    let (base_start, base_end) = slice_bounds(summary, slice_index)?;
    let current_end =
        base_end.saturating_sub(ms_to_samples(end_offset_ms, summary.source_sample_rate));
    let position_samples = position_samples.clamp(base_start, current_end.max(base_start));
    Some(samples_to_ms(
        position_samples.saturating_sub(base_start),
        summary.source_sample_rate,
    ))
}

fn slice_end_offset_ms(
    summary: &LinnodEditorPatchSummary,
    slice_index: usize,
    position_samples: usize,
    start_offset_ms: f32,
) -> Option<f32> {
    let (base_start, base_end) = slice_bounds(summary, slice_index)?;
    let current_start =
        base_start.saturating_add(ms_to_samples(start_offset_ms, summary.source_sample_rate));
    let position_samples = position_samples.clamp(current_start.min(base_end), base_end);
    Some(samples_to_ms(
        base_end.saturating_sub(position_samples),
        summary.source_sample_rate,
    ))
}

fn samples_to_ms(samples: usize, sample_rate: u32) -> f32 {
    samples as f32 * 1_000.0 / sample_rate.max(1) as f32
}

fn min_view_span(summary: &LinnodEditorPatchSummary) -> f32 {
    let span = source_span_samples(summary).max(1) as f32;
    (MIN_ZOOM_SAMPLES / span).clamp(0.001, 1.0)
}

fn point_in_rect(rect: WaveformRect, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.x + rect.w && y >= rect.y && y <= rect.y + rect.h
}

fn left_drag_distance(cx: &EventContext) -> f32 {
    let (dx, dy) = cx.mouse().button_delta(MouseButton::Left);
    dx.hypot(dy)
}
