use tezzera_core::types::{Point, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A toggle switch (pill shape with thumb).
pub struct Switch {
    pub on: bool,
    pub on_color: Color,
    pub off_color: Color,
    pub thumb_color: Color,
}

impl Switch {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            on_color:    Color::rgb(110, 75, 210),
            off_color:   Color::rgb(32, 35, 58),
            thumb_color: Color::rgb(230, 232, 245),
        }
    }
    pub fn on_color(mut self, c: Color) -> Self { self.on_color = c; self }
    pub fn off_color(mut self, c: Color) -> Self { self.off_color = c; self }
}

impl Widget for Switch {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: 36.0, height: 20.0 }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let track_color = if self.on { self.on_color } else { self.off_color };
        draw_rounded_rect_pub(ctx, r, track_color, 10.0);

        let thumb_x = if self.on { r.origin.x + r.size.width - 18.0 } else { r.origin.x + 2.0 };
        ctx.fill_circle(
            Point { x: thumb_x + 8.0, y: r.origin.y + 10.0 },
            8.0,
            self.thumb_color,
        );
    }
}
