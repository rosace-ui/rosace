use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};

/// A horizontal range slider.
pub struct Slider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub track_color: Color,
    pub fill_color: Color,
    pub thumb_color: Color,
    pub height: f32,
    pub width: Option<f32>,
}

impl Slider {
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            min: 0.0,
            max: 1.0,
            track_color: Color::rgb(32, 35, 58),
            fill_color: Color::rgb(110, 75, 210),
            thumb_color: Color::rgb(200, 202, 225),
            height: 20.0,
            width: None,
        }
    }
    pub fn range(mut self, min: f32, max: f32, value: f32) -> Self {
        self.min = min; self.max = max;
        self.value = ((value - min) / (max - min)).clamp(0.0, 1.0);
        self
    }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
}

impl Widget for Slider {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: self.width.unwrap_or(avail_w(constraints)), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(tezzera_core::Role::Slider)
            .value(format!("{:.2}", self.value)));
        let r = ctx.rect;
        let track_h = 4.0;
        let cy = r.origin.y + r.size.height / 2.0;

        ctx.fill_rect(Rect {
            origin: Point { x: r.origin.x, y: cy - track_h / 2.0 },
            size: Size { width: r.size.width, height: track_h },
        }, self.track_color);

        let fill_w = r.size.width * self.value;
        if fill_w > 0.5 {
            ctx.fill_rect(Rect {
                origin: Point { x: r.origin.x, y: cy - track_h / 2.0 },
                size: Size { width: fill_w, height: track_h },
            }, self.fill_color);
        }

        ctx.fill_circle(
            Point { x: r.origin.x + fill_w, y: cy },
            8.0, self.thumb_color,
        );
    }
}
