const MARKER_HIT_RADIUS_PX: f32 = 8.0;
const SLICE_HANDLE_HIT_RADIUS_PX: f32 = 9.0;
const WAVEFORM_DRAG_THRESHOLD_PX: f32 = 4.0;
const WAVEFORM_SIDE_PAD: f32 = 12.0;
const WAVEFORM_TOP_PAD: f32 = 10.0;
const WAVEFORM_BOTTOM_PAD: f32 = 10.0;
const WAVEFORM_OVERVIEW_HEIGHT: f32 = 24.0;
const MIN_ZOOM_SAMPLES: f32 = 128.0;
const WAVEFORM_HELP: &str = "Wheel or +/- zooms. Shift-wheel or overview drag pans. Drag trim handles. Double-click focuses the active slice.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaveformViewMode {
    SourceEditor,
    SliceDetail,
}

struct SourceWaveformView {
    summary: Signal<LinnodEditorPatchSummary>,
    drop_active: Signal<bool>,
    mode: WaveformViewMode,
    view_start: f32,
    view_end: f32,
    focus_key: Option<WaveformFocusKey>,
    drag: Option<WaveformDrag>,
}

#[derive(Clone, Copy)]
enum WaveformDrag {
    Marker(MarkerDrag),
    SliceStart(SliceTrimDrag),
    SliceEnd(SliceTrimDrag),
    Pan(PanDrag),
    Overview,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WaveformControl {
    ZoomOut,
    Focus,
    ZoomIn,
}

#[derive(Clone, Copy)]
struct MarkerDrag {
    original_position_samples: usize,
    current_position_samples: usize,
    started: bool,
}

#[derive(Clone, Copy)]
struct SliceTrimDrag {
    slice_index: usize,
    start_offset_ms: f32,
    end_offset_ms: f32,
    current_position_samples: usize,
    started: bool,
}

#[derive(Clone, Copy)]
struct PanDrag {
    start_x: f32,
    down_sample: usize,
    view_start: f32,
    view_end: f32,
    started: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct WaveformFocusKey {
    source_label: String,
    source_sample_rate: u32,
    source_span_samples: usize,
    waveform_len: usize,
    selected_slice_index: Option<usize>,
    selected_slice_start: usize,
    selected_slice_end: usize,
}

#[derive(Clone, Copy)]
struct WaveformRange {
    start: usize,
    end: usize,
}

#[derive(Clone, Copy)]
struct WaveformRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl SourceWaveformView {
    fn new(
        cx: &mut Context,
        summary: Signal<LinnodEditorPatchSummary>,
        drop_active: Signal<bool>,
    ) -> Handle<'_, Self> {
        Self::create(cx, summary, drop_active, WaveformViewMode::SliceDetail)
    }

    fn new_editable(
        cx: &mut Context,
        summary: Signal<LinnodEditorPatchSummary>,
        drop_active: Signal<bool>,
    ) -> Handle<'_, Self> {
        Self::create(cx, summary, drop_active, WaveformViewMode::SourceEditor)
    }

    fn create(
        cx: &mut Context,
        summary: Signal<LinnodEditorPatchSummary>,
        drop_active: Signal<bool>,
        mode: WaveformViewMode,
    ) -> Handle<'_, Self> {
        Self {
            summary,
            drop_active,
            mode,
            view_start: 0.0,
            view_end: 1.0,
            focus_key: None,
            drag: None,
        }
        .build(cx, |_| {})
        .bind(summary, move |view| {
            let next_summary = summary.get();
            view.modify(|view| view.sync_view_to_summary(&next_summary));
        })
        .bind(drop_active, |mut view| view.needs_redraw())
    }
}

impl View for SourceWaveformView {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                if self.handle_double_click(cx) {
                    meta.consume();
                }
            }
            WindowEvent::MouseDown(MouseButton::Left) => {
                if self.begin_drag(cx) {
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(x, _) => {
                if self.update_drag(cx, *x) {
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.finish_drag(cx) {
                    meta.consume();
                }
            }
            WindowEvent::MouseScroll(_, y) => {
                if self.scroll_view(cx, *y) {
                    meta.consume();
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas, self.drop_active.get());
        let summary = self.summary.get();
        let range = self.visible_range(&summary);
        let main = main_waveform_rect(bounds, self.mode);
        draw_waveform_grid(main, canvas);
        draw_selected_slice_region(main, canvas, &summary, range);
        draw_waveform_body(main, canvas, &summary.waveform, &summary, range, false);
        draw_markers(main, canvas, &summary, range);
        draw_selected_trim_handles(main, canvas, &summary, range);
        self.draw_drag_preview(main, canvas, &summary, range);
        if waveform_has_overview(self.mode) {
            let overview = overview_waveform_rect(bounds);
            draw_waveform_overview(overview, canvas, &summary, range);
        }
        if can_edit_waveform(&summary) {
            draw_waveform_controls(bounds, canvas);
        }
    }
}

impl SourceWaveformView {
    fn handle_double_click(&mut self, cx: &mut EventContext) -> bool {
        let summary = self.summary.get();
        if !can_edit_waveform(&summary) {
            return false;
        }
        let x = cx.mouse().cursor_x;
        let y = cx.mouse().cursor_y;
        if waveform_has_overview(self.mode) && point_in_rect(overview_waveform_rect(cx.bounds()), x, y) {
            self.reset_view_for_mode(&summary);
            self.focus_key = Some(WaveformFocusKey::for_view(self.mode, &summary));
            cx.needs_redraw();
            return true;
        }
        if self.mode == WaveformViewMode::SourceEditor {
            let range = self.visible_range(&summary);
            let main = main_waveform_rect(cx.bounds(), self.mode);
            if let Some(marker) = nearest_marker_at_x(main, &summary, range, x) {
                cx.emit(EditorEvent::MarkerEdit(LinnodEditorMarkerEdit::RemoveAt {
                    position_samples: marker.position_samples,
                }));
            } else {
                cx.emit(EditorEvent::MarkerEdit(LinnodEditorMarkerEdit::AddUser {
                    position_samples: sample_at_x(main, &summary, range, x),
                }));
            }
            return true;
        }
        self.focus_selected_slice(&summary);
        self.focus_key = Some(WaveformFocusKey::for_view(self.mode, &summary));
        cx.needs_redraw();
        true
    }

    fn begin_drag(&mut self, cx: &mut EventContext) -> bool {
        let summary = self.summary.get();
        if !can_edit_waveform(&summary) {
            return false;
        }
        let bounds = cx.bounds();
        let x = cx.mouse().cursor_x;
        let y = cx.mouse().cursor_y;
        self.ensure_slice_detail_focus(&summary);
        if let Some(control) = waveform_control_at(bounds, x, y) {
            self.activate_control(control, &summary);
            cx.needs_redraw();
            return true;
        }
        if waveform_has_overview(self.mode) && point_in_rect(overview_waveform_rect(bounds), x, y) {
            self.drag = Some(WaveformDrag::Overview);
            self.center_view_on_overview_x(bounds, &summary, x);
            cx.capture();
            cx.needs_redraw();
            return true;
        }
        let main = main_waveform_rect(bounds, self.mode);
        let range = self.visible_range(&summary);
        if cx.modifiers().alt() && self.mode == WaveformViewMode::SourceEditor {
            if let Some(marker) = nearest_marker_at_x(main, &summary, range, x) {
                cx.emit(EditorEvent::MarkerEdit(LinnodEditorMarkerEdit::RemoveAt {
                    position_samples: marker.position_samples,
                }));
                return true;
            }
        }
        if let Some(handle) = nearest_trim_handle_at_x(main, &summary, range, x) {
            self.freeze_visible_range(&summary);
            self.drag = Some(handle);
            cx.capture();
            cx.needs_redraw();
            return true;
        }
        if self.mode == WaveformViewMode::SourceEditor {
            if let Some(marker) = nearest_marker_at_x(main, &summary, range, x) {
                self.drag = Some(WaveformDrag::Marker(MarkerDrag {
                    original_position_samples: marker.position_samples,
                    current_position_samples: marker.position_samples,
                    started: false,
                }));
                cx.capture();
                cx.needs_redraw();
                return true;
            }
        }
        self.ensure_slice_detail_focus(&summary);
        let range = self.visible_range(&summary);
        self.drag = Some(WaveformDrag::Pan(PanDrag {
            start_x: x,
            down_sample: sample_at_x(main, &summary, range, x),
            view_start: self.view_start,
            view_end: self.view_end,
            started: false,
        }));
        cx.capture();
        true
    }

    fn update_drag(&mut self, cx: &mut EventContext, x: f32) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        let summary = self.summary.get();
        let main = main_waveform_rect(cx.bounds(), self.mode);
        let range = self.visible_range(&summary);
        match drag {
            WaveformDrag::Marker(mut drag) => {
                drag.started = drag.started || left_drag_distance(cx) >= WAVEFORM_DRAG_THRESHOLD_PX;
                drag.current_position_samples = sample_at_x(main, &summary, range, x);
                self.drag = Some(WaveformDrag::Marker(drag));
            }
            WaveformDrag::SliceStart(mut drag) => {
                drag.started = drag.started || left_drag_distance(cx) >= WAVEFORM_DRAG_THRESHOLD_PX;
                drag.current_position_samples = sample_at_x(main, &summary, range, x);
                if drag.started {
                    self.emit_slice_start(cx, &summary, drag);
                }
                self.drag = Some(WaveformDrag::SliceStart(drag));
            }
            WaveformDrag::SliceEnd(mut drag) => {
                drag.started = drag.started || left_drag_distance(cx) >= WAVEFORM_DRAG_THRESHOLD_PX;
                drag.current_position_samples = sample_at_x(main, &summary, range, x);
                if drag.started {
                    self.emit_slice_end(cx, &summary, drag);
                }
                self.drag = Some(WaveformDrag::SliceEnd(drag));
            }
            WaveformDrag::Pan(mut drag) => {
                drag.started = drag.started || left_drag_distance(cx) >= WAVEFORM_DRAG_THRESHOLD_PX;
                if drag.started {
                    let width = main.w.max(1.0);
                    let span = (drag.view_end - drag.view_start).max(min_view_span(&summary));
                    let delta = -(x - drag.start_x) / width * span;
                    self.set_view(drag.view_start + delta, drag.view_end + delta, &summary);
                }
                self.drag = Some(WaveformDrag::Pan(drag));
            }
            WaveformDrag::Overview => self.center_view_on_overview_x(cx.bounds(), &summary, x),
        }
        cx.needs_redraw();
        true
    }

    fn finish_drag(&mut self, cx: &mut EventContext) -> bool {
        let Some(drag) = self.drag.take() else {
            return false;
        };
        cx.release();
        match drag {
            WaveformDrag::Marker(drag) => {
                if drag.started
                    && drag.current_position_samples != drag.original_position_samples
                {
                    cx.emit(EditorEvent::MarkerEdit(LinnodEditorMarkerEdit::RemoveAt {
                        position_samples: drag.original_position_samples,
                    }));
                    cx.emit(EditorEvent::MarkerEdit(LinnodEditorMarkerEdit::AddUser {
                        position_samples: drag.current_position_samples,
                    }));
                }
            }
            WaveformDrag::Pan(drag) if !drag.started => {
                let summary = self.summary.get();
                if let Some(slice_index) = slice_index_at_sample(&summary, drag.down_sample) {
                    cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::Select { slice_index }));
                }
            }
            _ => {}
        }
        cx.needs_redraw();
        true
    }

    fn scroll_view(&mut self, cx: &mut EventContext, y: f32) -> bool {
        let summary = self.summary.get();
        if !can_edit_waveform(&summary) {
            return false;
        }
        self.ensure_slice_detail_focus(&summary);
        let main = main_waveform_rect(cx.bounds(), self.mode);
        let x = cx.mouse().cursor_x;
        let focus = ((x - main.x) / main.w.max(1.0)).clamp(0.0, 1.0);
        if cx.modifiers().shift() {
            let span = (self.view_end - self.view_start).max(min_view_span(&summary));
            self.set_view(
                self.view_start - y.signum() * span * 0.08,
                self.view_end - y.signum() * span * 0.08,
                &summary,
            );
        } else {
            let zoom = if y >= 0.0 { 0.82 } else { 1.22 };
            self.zoom_around(focus, zoom, &summary);
        }
        cx.needs_redraw();
        true
    }

    fn visible_range(&self, summary: &LinnodEditorPatchSummary) -> WaveformRange {
        let span = source_span_samples(summary);
        WaveformRange {
            start: (self.view_start.clamp(0.0, 1.0) * span as f32).round() as usize,
            end: (self.view_end.clamp(0.0, 1.0) * span as f32).round() as usize,
        }
        .normalized(span)
    }

    fn draw_drag_preview(
        &self,
        rect: WaveformRect,
        canvas: &Canvas,
        summary: &LinnodEditorPatchSummary,
        range: WaveformRange,
    ) {
        match self.drag {
            Some(WaveformDrag::Marker(drag)) => draw_marker_preview(
                rect,
                canvas,
                summary,
                range,
                drag.current_position_samples,
                Color::rgba(245, 198, 106, 245),
            ),
            Some(WaveformDrag::SliceStart(drag)) => draw_marker_preview(
                rect,
                canvas,
                summary,
                range,
                drag.current_position_samples,
                Color::rgba(126, 218, 160, 245),
            ),
            Some(WaveformDrag::SliceEnd(drag)) => draw_marker_preview(
                rect,
                canvas,
                summary,
                range,
                drag.current_position_samples,
                Color::rgba(242, 168, 75, 245),
            ),
            _ => {}
        }
    }

    fn emit_slice_start(
        &self,
        cx: &mut EventContext,
        summary: &LinnodEditorPatchSummary,
        drag: SliceTrimDrag,
    ) {
        if let Some(start_offset_ms) = slice_start_offset_ms(
            summary,
            drag.slice_index,
            drag.current_position_samples,
            drag.end_offset_ms,
        ) {
            cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::Offsets {
                slice_index: drag.slice_index,
                start_offset_ms,
                end_offset_ms: drag.end_offset_ms,
            }));
        }
    }

    fn emit_slice_end(
        &self,
        cx: &mut EventContext,
        summary: &LinnodEditorPatchSummary,
        drag: SliceTrimDrag,
    ) {
        if let Some(end_offset_ms) = slice_end_offset_ms(
            summary,
            drag.slice_index,
            drag.current_position_samples,
            drag.start_offset_ms,
        ) {
            cx.emit(EditorEvent::SliceEdit(LinnodEditorSliceEdit::Offsets {
                slice_index: drag.slice_index,
                start_offset_ms: drag.start_offset_ms,
                end_offset_ms,
            }));
        }
    }

    fn reset_zoom(&mut self) {
        self.view_start = 0.0;
        self.view_end = 1.0;
    }

    fn reset_view_for_mode(&mut self, summary: &LinnodEditorPatchSummary) {
        match self.mode {
            WaveformViewMode::SourceEditor => self.reset_zoom(),
            WaveformViewMode::SliceDetail => self.focus_selected_slice(summary),
        }
    }

    fn activate_control(&mut self, control: WaveformControl, summary: &LinnodEditorPatchSummary) {
        self.ensure_slice_detail_focus(summary);
        match control {
            WaveformControl::ZoomOut => self.zoom_around(0.5, 1.38, summary),
            WaveformControl::Focus => self.reset_view_for_mode(summary),
            WaveformControl::ZoomIn => self.zoom_around(0.5, 0.72, summary),
        }
        self.focus_key = Some(WaveformFocusKey::for_view(self.mode, summary));
    }

    fn zoom_around(&mut self, focus: f32, factor: f32, summary: &LinnodEditorPatchSummary) {
        let min_span = min_view_span(summary);
        let current_span = (self.view_end - self.view_start).clamp(min_span, 1.0);
        let next_span = (current_span * factor).clamp(min_span, 1.0);
        let anchor = self.view_start + focus * current_span;
        self.set_view(
            anchor - focus * next_span,
            anchor + (1.0 - focus) * next_span,
            summary,
        );
    }

    fn set_view(&mut self, start: f32, end: f32, summary: &LinnodEditorPatchSummary) {
        let span = (end - start).clamp(min_view_span(summary), 1.0);
        let start = start.clamp(0.0, 1.0 - span);
        self.view_start = start;
        self.view_end = start + span;
    }

    fn freeze_visible_range(&mut self, summary: &LinnodEditorPatchSummary) {
        let span = source_span_samples(summary).max(1) as f32;
        let range = self.visible_range(summary);
        self.set_view(range.start as f32 / span, range.end as f32 / span, summary);
    }

    fn ensure_slice_detail_focus(&mut self, summary: &LinnodEditorPatchSummary) {
        if self.mode != WaveformViewMode::SliceDetail || self.focus_key.is_some() {
            return;
        }
        self.focus_selected_slice(summary);
        self.focus_key = Some(WaveformFocusKey::for_view(self.mode, summary));
    }

    fn sync_view_to_summary(&mut self, summary: &LinnodEditorPatchSummary) {
        if self.drag.is_some() {
            return;
        }
        let next_key = WaveformFocusKey::for_view(self.mode, summary);
        if self.focus_key.as_ref() == Some(&next_key) {
            return;
        }
        self.reset_view_for_mode(summary);
        self.focus_key = Some(next_key);
    }

    fn focus_selected_slice(&mut self, summary: &LinnodEditorPatchSummary) {
        if self.mode != WaveformViewMode::SliceDetail {
            self.reset_zoom();
            return;
        }
        let span = source_span_samples(summary).max(1) as f32;
        let range = selected_slice_focus_range(summary, span as usize);
        self.set_view(range.start as f32 / span, range.end as f32 / span, summary);
    }

    fn center_view_on_overview_x(
        &mut self,
        bounds: BoundingBox,
        summary: &LinnodEditorPatchSummary,
        x: f32,
    ) {
        let overview = overview_waveform_rect(bounds);
        let span = (self.view_end - self.view_start).clamp(min_view_span(summary), 1.0);
        let center = ((x - overview.x) / overview.w.max(1.0)).clamp(0.0, 1.0);
        self.set_view(center - span * 0.5, center + span * 0.5, summary);
    }
}
