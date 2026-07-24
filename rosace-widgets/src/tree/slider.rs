use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
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
    on_change: Option<std::sync::Arc<dyn Fn(f32) + Send + Sync>>,
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
            on_change: None,
        }
    }
    pub fn range(mut self, min: f32, max: f32, value: f32) -> Self {
        self.min = min; self.max = max;
        self.value = ((value - min) / (max - min)).clamp(0.0, 1.0);
        self
    }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn fill_color(mut self, c: Color) -> Self { self.fill_color = c; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = c; self }

    /// Called with the new value (in `min..max`) when the track is clicked.
    /// (Continuous dragging lands with gesture/move events.)
    pub fn on_change(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_change = Some(std::sync::Arc::new(f));
        self
    }
}

impl Widget for Slider {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: self.width.unwrap_or(avail_w(constraints)), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if self.on_change.is_none() {
            // Interactive-by-identity (Phase 32): an unwired slider still
            // owns its press region (absorbs, does nothing) so a click on
            // the track never pans the scroll view behind it.
            ctx.on_press_at(|_, _| {});
        }
        if let Some(f) = &self.on_change {
            let f = f.clone();
            let (min, max) = (self.min, self.max);
            let r = ctx.rect;
            ctx.on_press_at(move |px, _py| {
                let t = ((px - r.origin.x) / r.size.width.max(1.0)).clamp(0.0, 1.0);
                f(min + t * (max - min));
            });
        }
        ctx.semantics(super::Semantics::new(rosace_core::Role::Slider)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn customization_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = Slider::new(0.5);
        let customized = Slider::new(0.5)
            .height(30.0)
            .track_color(Color::rgb(10, 10, 10))
            .fill_color(Color::rgb(255, 0, 0))
            .thumb_color(Color::rgb(255, 255, 255));
        assert_eq!(base.layout(&ctx).width, customized.layout(&ctx).width);
        assert_eq!(customized.layout(&ctx).height, 30.0);
    }
}
