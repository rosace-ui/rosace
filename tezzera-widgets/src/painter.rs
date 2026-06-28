use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::{Color, FontCache, SkiaCanvas};

/// The clipping/transform context passed to a CustomPainter.
/// Wraps SkiaCanvas and provides a local coordinate origin.
pub struct PainterContext<'a> {
    canvas: &'a mut SkiaCanvas,
    font: &'a FontCache,
    /// Local origin — all draw calls are offset by this.
    origin: Point,
    /// Size of the painter's area.
    pub size: Size,
}

impl<'a> PainterContext<'a> {
    pub fn new(canvas: &'a mut SkiaCanvas, font: &'a FontCache, origin: Point, size: Size) -> Self {
        Self { canvas, font, origin, size }
    }

    fn local(&self, p: Point) -> Point {
        Point { x: self.origin.x + p.x, y: self.origin.y + p.y }
    }

    fn local_rect(&self, r: Rect) -> Rect {
        Rect {
            origin: self.local(r.origin),
            size: r.size,
        }
    }

    /// Fill a rectangle in local coordinates.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let r = self.local_rect(rect);
        self.canvas.fill_rect(r, color);
    }

    /// Stroke a rectangle in local coordinates.
    pub fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32) {
        let r = self.local_rect(rect);
        self.canvas.stroke_rect(r, color, width);
    }

    /// Fill a circle in local coordinates.
    pub fn fill_circle(&mut self, center: Point, radius: f32, color: Color) {
        let c = self.local(center);
        self.canvas.fill_circle(c, radius, color);
    }

    /// Draw text in local coordinates.
    pub fn draw_text(&mut self, text: &str, pos: Point, color: Color, size: f32) {
        let p = self.local(pos);
        self.canvas.draw_text(text, p, color, self.font, size);
    }

    /// Fill the entire painter area with a color.
    pub fn fill_background(&mut self, color: Color) {
        let r = Rect { origin: Point { x: 0.0, y: 0.0 }, size: self.size };
        self.fill_rect(r, color);
    }

    /// Canvas width (full canvas, not just painter area).
    pub fn canvas_width(&self) -> u32 { self.canvas.width() }
    /// Canvas height.
    pub fn canvas_height(&self) -> u32 { self.canvas.height() }
}

/// Trait for custom draw logic, invoked by `PainterWidget` each frame.
pub trait CustomPainter: Send + 'static {
    fn paint(&self, ctx: &mut PainterContext);
}

/// A widget that delegates all rendering to a `CustomPainter`.
pub struct PainterWidget {
    pub width: f32,
    pub height: f32,
    painter: Box<dyn CustomPainter>,
}

impl PainterWidget {
    pub fn new(width: f32, height: f32, painter: impl CustomPainter) -> Self {
        Self { width, height, painter: Box::new(painter) }
    }

    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }

    /// Invoke the painter at position (x, y).
    pub fn render(&self, canvas: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
        let mut ctx = PainterContext::new(
            canvas,
            font,
            Point { x, y },
            Size { width: self.width, height: self.height },
        );
        self.painter.paint(&mut ctx);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct FillPainter(Color);
    impl CustomPainter for FillPainter {
        fn paint(&self, ctx: &mut PainterContext) {
            ctx.fill_background(self.0);
        }
    }

    struct RectPainter;
    impl CustomPainter for RectPainter {
        fn paint(&self, ctx: &mut PainterContext) {
            ctx.fill_rect(
                Rect {
                    origin: Point { x: 0.0, y: 0.0 },
                    size: Size { width: 10.0, height: 10.0 },
                },
                Color::RED,
            );
        }
    }

    struct StrokePainter;
    impl CustomPainter for StrokePainter {
        fn paint(&self, ctx: &mut PainterContext) {
            ctx.stroke_rect(
                Rect {
                    origin: Point { x: 1.0, y: 1.0 },
                    size: Size { width: 20.0, height: 20.0 },
                },
                Color::BLUE,
                2.0,
            );
        }
    }

    struct CirclePainter;
    impl CustomPainter for CirclePainter {
        fn paint(&self, ctx: &mut PainterContext) {
            ctx.fill_circle(Point { x: 25.0, y: 25.0 }, 10.0, Color::GREEN);
        }
    }

    struct TextPainter;
    impl CustomPainter for TextPainter {
        fn paint(&self, ctx: &mut PainterContext) {
            ctx.draw_text("Hello", Point { x: 0.0, y: 16.0 }, Color::BLACK, 14.0);
        }
    }

    fn make_canvas() -> SkiaCanvas {
        SkiaCanvas::new(200, 200)
    }

    fn make_font() -> FontCache {
        FontCache::system_mono().expect("system font required for painter tests")
    }

    #[test]
    fn painter_context_new() {
        let mut canvas = make_canvas();
        let font = make_font();
        let ctx = PainterContext::new(
            &mut canvas,
            &font,
            Point { x: 10.0, y: 20.0 },
            Size { width: 100.0, height: 80.0 },
        );
        assert_eq!(ctx.size.width, 100.0);
        assert_eq!(ctx.size.height, 80.0);
    }

    #[test]
    fn painter_widget_new() {
        let pw = PainterWidget::new(320.0, 240.0, FillPainter(Color::WHITE));
        assert_eq!(pw.width, 320.0);
        assert_eq!(pw.height, 240.0);
    }

    #[test]
    fn painter_widget_size_setters() {
        let pw = PainterWidget::new(0.0, 0.0, FillPainter(Color::BLACK))
            .width(100.0)
            .height(50.0);
        assert_eq!(pw.width, 100.0);
        assert_eq!(pw.height, 50.0);
    }

    #[test]
    fn painter_context_size() {
        let mut canvas = make_canvas();
        let font = make_font();
        let ctx = PainterContext::new(
            &mut canvas,
            &font,
            Point { x: 0.0, y: 0.0 },
            Size { width: 60.0, height: 40.0 },
        );
        assert_eq!(ctx.size.width, 60.0);
        assert_eq!(ctx.size.height, 40.0);
        assert_eq!(ctx.canvas_width(), 200);
        assert_eq!(ctx.canvas_height(), 200);
    }

    #[test]
    fn painter_context_fill_background_no_panic() {
        let mut canvas = make_canvas();
        let font = make_font();
        let pw = PainterWidget::new(100.0, 100.0, FillPainter(Color::RED));
        pw.render(&mut canvas, &font, 0.0, 0.0);
    }

    #[test]
    fn painter_context_fill_rect_no_panic() {
        let mut canvas = make_canvas();
        let font = make_font();
        let pw = PainterWidget::new(100.0, 100.0, RectPainter);
        pw.render(&mut canvas, &font, 5.0, 5.0);
    }

    #[test]
    fn painter_context_stroke_rect_no_panic() {
        let mut canvas = make_canvas();
        let font = make_font();
        let pw = PainterWidget::new(100.0, 100.0, StrokePainter);
        pw.render(&mut canvas, &font, 0.0, 0.0);
    }

    #[test]
    fn painter_context_fill_circle_no_panic() {
        let mut canvas = make_canvas();
        let font = make_font();
        let pw = PainterWidget::new(100.0, 100.0, CirclePainter);
        pw.render(&mut canvas, &font, 0.0, 0.0);
    }

    #[test]
    fn painter_context_draw_text_no_panic() {
        let mut canvas = make_canvas();
        let font = make_font();
        let pw = PainterWidget::new(100.0, 100.0, TextPainter);
        pw.render(&mut canvas, &font, 0.0, 0.0);
    }

    #[test]
    fn custom_painter_trait_is_object_safe() {
        // Verify Box<dyn CustomPainter> compiles correctly.
        let _boxed: Box<dyn CustomPainter> = Box::new(FillPainter(Color::WHITE));
    }
}
