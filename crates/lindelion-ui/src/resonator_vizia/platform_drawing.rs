struct WaveformStrip {
    emphasis: f32,
}

impl WaveformStrip {
    fn new(cx: &mut Context, emphasis: f32) -> Handle<'_, Self> {
        Self {
            emphasis: emphasis.clamp(0.0, 1.0),
        }
        .build(cx, |_| {})
    }
}

impl View for WaveformStrip {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);

        let center_y = bounds.y + bounds.h * 0.5;
        let mut baseline = vg::Paint::default();
        baseline.set_color(Color::rgba(93, 111, 117, 150));
        baseline.set_stroke_width(1.0);
        baseline.set_style(vg::PaintStyle::Stroke);
        let mut baseline_path = vg::PathBuilder::new();
        baseline_path.move_to((bounds.x + 8.0, center_y));
        baseline_path.line_to((bounds.x + bounds.w - 8.0, center_y));
        canvas.draw_path(&baseline_path.detach(), &baseline);

        let mut path = vg::PathBuilder::new();
        let steps = 72;
        for index in 0..steps {
            let t = index as f32 / (steps - 1) as f32;
            let x = bounds.x + 10.0 + t * (bounds.w - 20.0);
            let envelope = (1.0 - t).powf(1.7);
            let wave = (t * 36.0).sin() * 0.62 + (t * 91.0).sin() * 0.25;
            let y = center_y - wave * envelope * bounds.h * (0.20 + self.emphasis * 0.20);
            if index == 0 {
                path.move_to((x, y));
            } else {
                path.line_to((x, y));
            }
        }

        let mut paint = vg::Paint::default();
        paint.set_color(Color::rgb(128, 196, 158));
        paint.set_stroke_width(2.0);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_style(vg::PaintStyle::Stroke);
        canvas.draw_path(&path.detach(), &paint);
    }
}

struct MiniWaveform {
    phase: Memo<f32>,
}

impl MiniWaveform {
    fn new(cx: &mut Context, phase: Memo<f32>) -> Handle<'_, Self> {
        Self { phase }
            .build(cx, |_| {})
            .bind(phase, |mut view| view.needs_redraw())
    }
}

impl View for MiniWaveform {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);

        let mut path = vg::PathBuilder::new();
        let phase = self.phase.get();
        for index in 0..28 {
            let t = index as f32 / 27.0;
            let x = bounds.x + 5.0 + t * (bounds.w - 10.0);
            let y = bounds.y
                + bounds.h * 0.5
                + ((t + phase) * 22.0).sin() * (1.0 - t * 0.55) * bounds.h * 0.28;
            if index == 0 {
                path.move_to((x, y));
            } else {
                path.line_to((x, y));
            }
        }

        let mut paint = vg::Paint::default();
        paint.set_color(Color::rgb(121, 156, 204));
        paint.set_stroke_width(1.6);
        paint.set_stroke_cap(vg::PaintCap::Round);
        paint.set_style(vg::PaintStyle::Stroke);
        canvas.draw_path(&path.detach(), &paint);
    }
}

struct LibraryWaveform {
    samples: Signal<Vec<ResonatorEditorSampleSummary>>,
    page_start: Signal<usize>,
    row: usize,
}

impl LibraryWaveform {
    fn new(
        cx: &mut Context,
        samples: Signal<Vec<ResonatorEditorSampleSummary>>,
        page_start: Signal<usize>,
        row: usize,
    ) -> Handle<'_, Self> {
        Self {
            samples,
            page_start,
            row,
        }
            .build(cx, |_| {})
            .bind(samples, |mut view| view.needs_redraw())
            .bind(page_start, |mut view| view.needs_redraw())
    }
}

impl View for LibraryWaveform {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);

        let samples = self.samples.get();
        let index = self.page_start.get().saturating_add(self.row);
        let Some(sample) = samples.get(index) else {
            return;
        };
        draw_waveform_preview(bounds, canvas, &sample.preview, Color::rgb(121, 156, 204));
    }
}

struct ResonatorScope {
    left_rms: Signal<f32>,
    right_rms: Signal<f32>,
    active_voices: Signal<f32>,
}

impl ResonatorScope {
    fn new(
        cx: &mut Context,
        left_rms: Signal<f32>,
        right_rms: Signal<f32>,
        active_voices: Signal<f32>,
    ) -> Handle<'_, Self> {
        Self {
            left_rms,
            right_rms,
            active_voices,
        }
        .build(cx, |_| {})
        .bind(left_rms, |mut view| view.needs_redraw())
        .bind(right_rms, |mut view| view.needs_redraw())
        .bind(active_voices, |mut view| view.needs_redraw())
    }
}

impl View for ResonatorScope {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);

        let left_amount = meter_amount(self.left_rms.get());
        let right_amount = meter_amount(self.right_rms.get());
        let voice_amount = (self.active_voices.get() / 8.0).clamp(0.0, 1.0);
        let left = (bounds.x + bounds.w * 0.32, bounds.y + bounds.h * 0.52);
        let right = (bounds.x + bounds.w * 0.68, bounds.y + bounds.h * 0.52);

        draw_connection(canvas, left, right, voice_amount);
        draw_resonator(canvas, left, 38.0, left_amount, Color::rgb(124, 188, 148));
        draw_resonator(canvas, right, 34.0, right_amount, Color::rgb(196, 151, 81));
    }
}

struct LevelMeter {
    left_peak: Signal<f32>,
    right_peak: Signal<f32>,
}

impl LevelMeter {
    fn new(
        cx: &mut Context,
        left_peak: Signal<f32>,
        right_peak: Signal<f32>,
    ) -> Handle<'_, Self> {
        Self {
            left_peak,
            right_peak,
        }
        .build(cx, |_| {})
        .bind(left_peak, |mut view| view.needs_redraw())
        .bind(right_peak, |mut view| view.needs_redraw())
    }
}

impl View for LevelMeter {
    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        draw_panel_background(bounds, canvas);

        let level = meter_amount(self.left_peak.get().max(self.right_peak.get())).max(0.02);
        for index in 0..18 {
            let t = index as f32 / 17.0;
            let x = bounds.x + 8.0 + index as f32 * ((bounds.w - 16.0) / 18.0);
            let h = 5.0 + (t * std::f32::consts::PI).sin().abs() * 14.0;
            let y = bounds.y + bounds.h - h - 6.0;
            let active = t <= level;
            let color = if !active {
                Color::rgba(70, 82, 88, 150)
            } else if t > 0.84 {
                Color::rgb(211, 133, 92)
            } else if t > 0.66 {
                Color::rgb(196, 151, 81)
            } else {
                Color::rgb(124, 188, 148)
            };
            draw_rect(
                canvas,
                vg::Rect::new(x, y, x + 5.0, bounds.y + bounds.h - 6.0),
                color,
            );
        }
    }
}

fn draw_panel_background(bounds: BoundingBox, canvas: &Canvas) {
    draw_rect(
        canvas,
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        Color::rgb(17, 22, 25),
    );
}

fn draw_meter_track(bounds: BoundingBox, canvas: &Canvas, amount: f32, color: Color) {
    draw_rect(
        canvas,
        vg::Rect::new(bounds.x, bounds.y, bounds.x + bounds.w, bounds.y + bounds.h),
        Color::rgb(35, 44, 50),
    );
    draw_rect(
        canvas,
        vg::Rect::new(
            bounds.x,
            bounds.y,
            bounds.x + bounds.w * amount.clamp(0.0, 1.0),
            bounds.y + bounds.h,
        ),
        color,
    );
}

fn draw_waveform_preview(
    bounds: BoundingBox,
    canvas: &Canvas,
    points: &[ResonatorEditorWaveformPoint],
    color: Color,
) {
    let center_y = bounds.y + bounds.h * 0.5;
    let mut baseline = vg::Paint::default();
    baseline.set_color(Color::rgba(81, 96, 102, 120));
    baseline.set_stroke_width(1.0);
    baseline.set_style(vg::PaintStyle::Stroke);
    let mut baseline_path = vg::PathBuilder::new();
    baseline_path.move_to((bounds.x + 5.0, center_y));
    baseline_path.line_to((bounds.x + bounds.w - 5.0, center_y));
    canvas.draw_path(&baseline_path.detach(), &baseline);

    if points.is_empty() {
        return;
    }

    let mut path = vg::PathBuilder::new();
    for (index, point) in points.iter().enumerate() {
        let t = if points.len() <= 1 {
            0.0
        } else {
            index as f32 / (points.len() - 1) as f32
        };
        let x = bounds.x + 5.0 + t * (bounds.w - 10.0);
        let extent = point
            .max
            .abs()
            .max(point.min.abs())
            .max(point.rms)
            .clamp(0.0, 1.0);
        let y = center_y - extent * bounds.h * 0.38;
        if index == 0 {
            path.move_to((x, y));
        } else {
            path.line_to((x, y));
        }
    }

    let mut mirror = vg::PathBuilder::new();
    for (index, point) in points.iter().enumerate() {
        let t = if points.len() <= 1 {
            0.0
        } else {
            index as f32 / (points.len() - 1) as f32
        };
        let x = bounds.x + 5.0 + t * (bounds.w - 10.0);
        let extent = point
            .max
            .abs()
            .max(point.min.abs())
            .max(point.rms)
            .clamp(0.0, 1.0);
        let y = center_y + extent * bounds.h * 0.38;
        if index == 0 {
            mirror.move_to((x, y));
        } else {
            mirror.line_to((x, y));
        }
    }

    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_stroke_width(1.4);
    paint.set_stroke_cap(vg::PaintCap::Round);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_path(&path.detach(), &paint);
    canvas.draw_path(&mirror.detach(), &paint);
}

fn meter_amount(value: f32) -> f32 {
    if value.is_finite() {
        value.abs().sqrt().clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn draw_connection(canvas: &Canvas, left: (f32, f32), right: (f32, f32), amount: f32) {
    let mut path = vg::PathBuilder::new();
    path.move_to((left.0 + 42.0, left.1));
    path.cubic_to(
        (left.0 + 74.0, left.1 - 40.0),
        (right.0 - 74.0, right.1 + 40.0),
        (right.0 - 42.0, right.1),
    );
    let mut paint = vg::Paint::default();
    paint.set_color(Color::rgba(112, 144, 170, (95.0 + amount * 115.0) as u8));
    paint.set_stroke_width(3.0);
    paint.set_stroke_cap(vg::PaintCap::Round);
    paint.set_style(vg::PaintStyle::Stroke);
    canvas.draw_path(&path.detach(), &paint);
}

fn draw_resonator(canvas: &Canvas, center: (f32, f32), radius: f32, amount: f32, color: Color) {
    let amount = amount.clamp(0.0, 1.0);
    let rings = 4;
    for ring in 0..rings {
        let r = radius + ring as f32 * 10.0;
        let alpha = (70.0 + amount * 110.0 - ring as f32 * 17.0).clamp(20.0, 190.0) as u8;
        let mut paint = vg::Paint::default();
        paint.set_color(with_alpha(color, alpha));
        paint.set_stroke_width(2.0);
        paint.set_style(vg::PaintStyle::Stroke);
        paint.set_anti_alias(true);
        canvas.draw_arc(
            vg::Rect::new(center.0 - r, center.1 - r, center.0 + r, center.1 + r),
            -140.0 + ring as f32 * 14.0,
            220.0 + amount * 100.0,
            false,
            &paint,
        );
    }

    draw_rect(
        canvas,
        vg::Rect::new(
            center.0 - 4.0,
            center.1 - 4.0,
            center.0 + 4.0,
            center.1 + 4.0,
        ),
        Color::rgb(235, 242, 237),
    );
}

fn draw_rect(canvas: &Canvas, rect: vg::Rect, color: Color) {
    let mut paint = vg::Paint::default();
    paint.set_color(color);
    paint.set_anti_alias(true);
    canvas.draw_rect(rect, &paint);
}

fn with_alpha(color: Color, alpha: u8) -> Color {
    Color::rgba(color.r(), color.g(), color.b(), alpha)
}
