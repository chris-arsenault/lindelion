struct WaveformPreviewView {
    preview: Signal<GlirdirEditorPreview>,
}

impl WaveformPreviewView {
    fn new(cx: &mut Context, preview: Signal<GlirdirEditorPreview>) -> Handle<'_, Self> {
        Self { preview }
            .build(cx, |_| {})
            .bind(preview, |mut view| view.needs_redraw())
    }
}

impl View for WaveformPreviewView {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);
        let preview = self.preview.get();
        draw_waveform(bounds, canvas, &preview.waveform);
    }
}

struct PianoRollPreviewView {
    preview: Signal<GlirdirEditorPreview>,
    host: GlirdirEditorHost,
    parent_view: usize,
    drag_started: bool,
}

impl PianoRollPreviewView {
    fn new(
        cx: &mut Context,
        preview: Signal<GlirdirEditorPreview>,
        host: GlirdirEditorHost,
        parent_view: usize,
    ) -> Handle<'_, Self> {
        Self {
            preview,
            host,
            parent_view,
            drag_started: false,
        }
            .build(cx, |_| {})
            .bind(preview, |mut view| view.needs_redraw())
    }

    fn begin_midi_drag(&self) -> bool {
        let GlirdirEditorMidiDrag::Ready { path } = (unsafe { self.host.prepare_midi_drag() })
        else {
            return false;
        };
        let started = unsafe {
            platform_drag::start_file_drag(self.parent_view as *mut c_void, path.as_path())
        };
        started || unsafe { platform_drag::copy_file_url_to_pasteboard(path.as_path()) }
    }
}

impl View for PianoRollPreviewView {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.drag_started = false;
                meta.consume();
            }
            WindowEvent::MouseMove(_, _) if self.should_start_drag(cx) => {
                self.drag_started = true;
                let _ = self.begin_midi_drag();
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                self.drag_started = false;
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);
        let preview = self.preview.get();
        draw_piano_roll(bounds, canvas, &preview.piano_roll.notes);
    }
}

impl PianoRollPreviewView {
    fn should_start_drag(&self, cx: &EventContext) -> bool {
        if self.drag_started || cx.mouse().left.state != MouseButtonState::Pressed {
            return false;
        }
        let (dx, dy) = cx.mouse().button_delta(MouseButton::Left);
        dx.hypot(dy) >= 6.0
    }
}

fn draw_panel_background(bounds: BoundingBox, canvas: &Canvas) {
    draw_rect(
        canvas,
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        Color::rgb(15, 19, 20),
    );
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(64, 80, 76, 160));
    paint.set_stroke_width(1.0);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_rect(
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        &paint,
    );
}

fn draw_waveform(
    bounds: BoundingBox,
    canvas: &Canvas,
    preview: &GlirdirEditorWaveformPreview,
) {
    draw_center_line(bounds, canvas);
    if preview.points.is_empty() {
        draw_empty_wave(bounds, canvas);
        return;
    }

    let left = bounds.x + 12.0;
    let right = bounds.x + bounds.w - 12.0;
    let center_y = bounds.y + bounds.h * 0.5;
    let scale_y = bounds.h * 0.38;
    let width = (right - left).max(1.0);
    for (index, point) in preview.points.iter().enumerate() {
        let x = left + index as f32 / preview.points.len().max(1) as f32 * width;
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
    let min = point.min.clamp(-1.0, 1.0);
    let max = point.max.clamp(-1.0, 1.0);
    let y_min = center_y - max * scale_y;
    let y_max = center_y - min * scale_y;
    let mut path = vg::PathBuilder::new();
    path.move_to((x, y_min));
    path.line_to((x, y_max));

    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgb(124, 188, 148));
    paint.set_stroke_width(1.6);
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
        let y = center_y + (t * 24.0).sin() * bounds.h * 0.08;
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

fn draw_piano_roll(
    bounds: BoundingBox,
    canvas: &Canvas,
    notes: &[GlirdirEditorPianoRollNote],
) {
    draw_grid(bounds, canvas);
    if notes.is_empty() {
        return;
    }
    let max_tick = notes
        .iter()
        .map(|note| note.start_tick.saturating_add(note.duration_ticks))
        .max()
        .unwrap_or(1)
        .max(1);
    let min_note = notes.iter().map(|note| note.midi_note).min().unwrap_or(48);
    let max_note = notes.iter().map(|note| note.midi_note).max().unwrap_or(72);
    for note in notes {
        draw_note(bounds, canvas, note, max_tick, min_note, max_note);
    }
}

fn draw_note(
    bounds: BoundingBox,
    canvas: &Canvas,
    note: &GlirdirEditorPianoRollNote,
    max_tick: u32,
    min_note: u8,
    max_note: u8,
) {
    let note_span = u32::from(max_note.saturating_sub(min_note)).max(1) as f32;
    let x = bounds.x + 12.0 + note.start_tick as f32 / max_tick as f32 * (bounds.w - 24.0);
    let width = (note.duration_ticks as f32 / max_tick as f32 * (bounds.w - 24.0)).max(5.0);
    let note_offset = u32::from(max_note.saturating_sub(note.midi_note)) as f32 / note_span;
    let y = bounds.y + 12.0 + note_offset * (bounds.h - 34.0);
    let height = 14.0;
    draw_rect(
        canvas,
        vg::Rect::new(x, y, (x + width).min(bounds.x + bounds.w - 12.0), y + height),
        Color::rgb(124, 156, 204),
    );
}

fn draw_grid(bounds: BoundingBox, canvas: &Canvas) {
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(58, 73, 70, 120));
    paint.set_stroke_width(1.0);
    paint.set_style(vg::PaintStyle::Stroke);
    for step in 1..8 {
        let x = bounds.x + step as f32 * bounds.w / 8.0;
        let mut path = vg::PathBuilder::new();
        path.move_to((x, bounds.y + 8.0));
        path.line_to((x, bounds.y + bounds.h - 8.0));
        canvas.draw_path(&path.detach(), &paint);
    }
    for step in 1..5 {
        let y = bounds.y + step as f32 * bounds.h / 5.0;
        let mut path = vg::PathBuilder::new();
        path.move_to((bounds.x + 8.0, y));
        path.line_to((bounds.x + bounds.w - 8.0, y));
        canvas.draw_path(&path.detach(), &paint);
    }
}

fn draw_center_line(bounds: BoundingBox, canvas: &Canvas) {
    let center_y = bounds.y + bounds.h * 0.5;
    let mut path = vg::PathBuilder::new();
    path.move_to((bounds.x + 10.0, center_y));
    path.line_to((bounds.x + bounds.w - 10.0, center_y));

    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(82, 98, 94, 140));
    paint.set_stroke_width(1.0);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_rect(canvas: &Canvas, rect: vg::Rect, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_style(vg::PaintStyle::Fill);
    canvas.draw_rect(rect, &paint);
}
