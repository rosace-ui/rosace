use tezzera_core::types::{Rect, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};

/// A horizontal progress bar (0.0 – 1.0).
pub struct ProgressBar {
    pub value: f32,
    pub track_color: Color,
    pub fill_color: Color,
    pub height: f32,
    pub radius: f32,
    pub width: Option<f32>,
}

impl ProgressBar {
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            track_color: Color::rgb(32, 35, 58),
            fill_color: Color::rgb(110, 75, 210),
            height: 6.0,
            radius: 3.0,
            width: None,
        }
    }
    pub fn color(mut self, c: Color) -> Self { self.fill_color = c; self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
}

impl Widget for ProgressBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size {
            width:  self.width.unwrap_or(avail_w(constraints)),
            height: self.height,
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(tezzera_core::Role::ProgressBar)
            .value(format!("{:.0}%", self.value * 100.0)));
        use super::container::draw_rounded_rect_pub;
        let r = ctx.rect;
        // Track
        draw_rounded_rect_pub(ctx,r, self.track_color, self.radius);
        // Fill
        if self.value > 0.001 {
            let fill = Rect {
                origin: r.origin,
                size: Size { width: r.size.width * self.value, height: r.size.height },
            };
            draw_rounded_rect_pub(ctx,fill, self.fill_color, self.radius);
        }
    }
}
