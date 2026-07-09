use tezzera_core::types::{Point, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// Circular avatar with initials or a colored fill.
pub struct Avatar {
    pub initials: String,
    pub color: Color,
    pub text_color: Color,
    pub size: f32,
    pub font_size: f32,
}

impl Avatar {
    pub fn new(initials: impl Into<String>) -> Self {
        let s = initials.into();
        Self {
            initials: s,
            color: Color::rgb(110, 75, 210),
            text_color: Color::rgb(230, 232, 245),
            size: 32.0,
            font_size: 12.0,
        }
    }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn text_color(mut self, c: Color) -> Self { self.text_color = c; self }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self.font_size = s * 0.38; self }
}

impl Widget for Avatar {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: self.size, height: self.size }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(tezzera_core::Role::Image).label(&self.initials));
        let cx = ctx.rect.origin.x + self.size / 2.0;
        let cy = ctx.rect.origin.y + self.size / 2.0;
        ctx.fill_circle(Point { x: cx, y: cy }, self.size / 2.0, self.color);

        // Centered initials
        let text_w = ctx.font.measure_text(&self.initials, self.font_size);
        let line_h = ctx.font.line_height(self.font_size);
        ctx.text(&self.initials,
            (self.size - text_w) / 2.0,
            (self.size - line_h) / 2.0,
            self.text_color, self.font_size);
    }
}
