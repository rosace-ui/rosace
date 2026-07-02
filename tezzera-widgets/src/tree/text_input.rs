use tezzera_core::types::{Rect, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};
use super::container::draw_rounded_rect_pub;

/// A single-line text input field.
pub struct TextInput {
    pub value: String,
    pub placeholder: String,
    pub focused: bool,
    pub obscure: bool,
    pub width: Option<f32>,
    pub height: f32,
    pub font_size: f32,
    pub radius: f32,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: String::from("Type here..."),
            focused: false,
            obscure: false,
            width: None,
            height: 36.0,
            font_size: 11.0,
            radius: 6.0,
        }
    }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = v.into(); self }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self { self.placeholder = p.into(); self }
    pub fn focused(mut self) -> Self { self.focused = true; self }
    pub fn obscure(mut self) -> Self { self.obscure = true; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
}

impl Default for TextInput {
    fn default() -> Self { Self::new() }
}

impl Widget for TextInput {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size {
            width:  self.width.unwrap_or(avail_w(constraints)),
            height: self.height,
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let bg = Color::rgb(15, 16, 28);
        let border = if self.focused { Color::rgb(110, 75, 210) } else { Color::rgb(32, 35, 58) };

        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        ctx.stroke_rrect(r, self.radius, border, if self.focused { 1.5 } else { 1.0 });

        let has_value = !self.value.is_empty();
        let display = if has_value {
            if self.obscure {
                "•".repeat(self.value.len())
            } else {
                self.value.clone()
            }
        } else {
            self.placeholder.clone()
        };

        let text_color = if has_value {
            Color::rgb(220, 222, 240)
        } else {
            Color::rgb(80, 85, 118)
        };

        let line_h = ctx.font.line_height(self.font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);
        ctx.text(&display, 10.0, ty, text_color, self.font_size);

        if self.focused && has_value {
            let text_w = ctx.font.measure_text(&self.value, self.font_size);
            use tezzera_core::types::Point;
            ctx.fill_rect(Rect {
                origin: Point { x: r.origin.x + 10.0 + text_w + 1.0, y: r.origin.y + ty + 2.0 },
                size: Size { width: 1.5, height: self.font_size - 2.0 },
            }, Color::rgb(110, 75, 210));
        }
    }
}
