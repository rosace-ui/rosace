use tezzera_core::types::Size;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A small filter/tag chip with optional selected state.
pub struct Chip {
    pub label: String,
    pub selected: bool,
    pub color: Color,
    pub selected_color: Color,
    pub text_color: Color,
    pub selected_text_color: Color,
    pub font_size: f32,
}

impl Chip {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            selected: false,
            color: Color::rgb(20, 22, 38),
            selected_color: Color::rgb(60, 40, 120),
            text_color: Color::rgb(140, 144, 175),
            selected_text_color: Color::rgb(110, 75, 210),
            font_size: 9.5,
        }
    }
    pub fn selected(mut self) -> Self { self.selected = true; self }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn selected_color(mut self, c: Color) -> Self { self.selected_color = c; self }
}

impl Widget for Chip {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        let w = self.label.len() as f32 * self.font_size * 0.6 + 20.0;
        Size { width: w, height: 24.0 }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let bg = if self.selected { self.selected_color } else { self.color };
        let fg = if self.selected { self.selected_text_color } else { self.text_color };
        draw_rounded_rect_pub(ctx, r, bg, 12.0);
        let border = if self.selected { Color::rgb(110, 75, 210) } else { Color::rgb(32, 35, 58) };
        ctx.stroke_rect(r, border, 1.0);
        let text_w = ctx.font.measure_text(&self.label, self.font_size);
        let tx = ((r.size.width - text_w) / 2.0).max(0.0);
        let line_h = ctx.font.line_height(self.font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);
        ctx.text(&self.label, tx, ty, fg, self.font_size);
    }
}
