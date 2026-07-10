use rosace_core::types::{Point, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A small count badge or status dot.
pub struct Badge {
    pub label: String,
    pub dot: bool,
    pub color: Color,
    pub text_color: Color,
    pub font_size: f32,
}

impl Badge {
    /// D093: new() must exist wherever named constructors exist.
    pub fn new(text: impl Into<String>) -> Self {
        Self::label(text)
    }

    pub fn count(n: u32) -> Self {
        Self::label(n.to_string())
    }

    pub fn label(text: impl Into<String>) -> Self {
        Self {
            label: text.into(),
            dot: false,
            color: Color::rgb(110, 75, 210),
            text_color: Color::rgb(230, 232, 245),
            font_size: 8.5,
        }
    }

    pub fn dot() -> Self {
        Self { dot: true, label: String::new(), color: Color::rgb(235, 75, 75),
               text_color: Color::rgb(255,255,255), font_size: 8.5 }
    }

    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn text_color(mut self, c: Color) -> Self { self.text_color = c; self }
}

impl Widget for Badge {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        if self.dot {
            return Size { width: 8.0, height: 8.0 };
        }
        let w = self.label.len() as f32 * self.font_size * 0.6 + 12.0;
        Size { width: w.max(16.0), height: 16.0 }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if self.dot {
            // A bare status dot carries no text — nothing to announce.
            let cx = ctx.rect.origin.x + 4.0;
            let cy = ctx.rect.origin.y + 4.0;
            ctx.fill_circle(Point { x: cx, y: cy }, 4.0, self.color);
            return;
        }
        ctx.semantics(super::Semantics::new(rosace_core::Role::Text).label(&self.label));
        let r = ctx.rect;
        draw_rounded_rect_pub(ctx, r, self.color, r.size.height / 2.0);
        let text_w = ctx.font.measure_text(&self.label, self.font_size);
        let tx = ((r.size.width - text_w) / 2.0).max(0.0);
        let line_h = ctx.font.line_height(self.font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);
        ctx.text(&self.label, tx, ty, self.text_color, self.font_size);
    }
}
