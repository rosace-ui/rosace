use rosace_core::types::{Point, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A circular progress indicator — a determinate ring (`value` 0..1) or an
/// indeterminate spinner. Draws a ring segment (the FillArc primitive).
pub struct CircularProgress {
    value: Option<f32>,     // None = indeterminate spinner
    diameter: f32,
    thickness: f32,
    color: Option<Color>,
    track: Option<Color>,
}

impl CircularProgress {
    /// Determinate ring filled to `value` (0..1).
    pub fn new(value: f32) -> Self {
        Self { value: Some(value.clamp(0.0, 1.0)), diameter: 36.0, thickness: 4.0, color: None, track: None }
    }
    /// Indeterminate spinner.
    pub fn spinner() -> Self {
        Self { value: None, diameter: 36.0, thickness: 4.0, color: None, track: None }
    }
    pub fn diameter(mut self, d: f32) -> Self { self.diameter = d; self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn track(mut self, c: Color) -> Self { self.track = Some(c); self }
}

impl Widget for CircularProgress {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.diameter, height: self.diameter })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let color = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        let center = Point { x: r.origin.x + r.size.width / 2.0, y: r.origin.y + r.size.height / 2.0 };
        let radius = (self.diameter - self.thickness) / 2.0;

        match self.value {
            Some(v) => {
                // Track ring + value arc from 12 o'clock, clockwise.
                let track = self.track.unwrap_or(Color::rgba(255, 255, 255, 28));
                ctx.fill_arc(center, radius, self.thickness, 0.0, 360.0, track);
                if v > 0.0 {
                    ctx.fill_arc(center, radius, self.thickness, -90.0, 360.0 * v, color);
                }
            }
            None => {
                // Spinner: a 270° arc whose start rotates with the clock.
                let t = super::anim_clock();
                let start = (t * 360.0) % 360.0;
                ctx.fill_arc(center, radius, self.thickness, start, 270.0, color);
                ctx.request_animation();
            }
        }
    }
}
