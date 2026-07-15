use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// A checkbox with optional label.
pub struct Checkbox {
    pub checked: bool,
    on_change: Option<std::sync::Arc<dyn Fn(bool) + Send + Sync>>,
    pub indeterminate: bool,
    pub label: Option<String>,
    pub box_size: f32,
    pub color: Color,
    pub border_color: Color,
    pub font_size: f32,
}

impl Checkbox {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            indeterminate: false,
            label: None,
            box_size: 16.0,
            color: Color::rgb(110, 75, 210),
            border_color: Color::rgb(60, 65, 95),
            font_size: 11.0,
            on_change: None,
        }
    }
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }

    /// Called with the NEW value when the control is tapped (D094).
    pub fn on_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_change = Some(std::sync::Arc::new(f));
        self
    }
    pub fn indeterminate(mut self) -> Self { self.indeterminate = true; self }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn size(mut self, s: f32) -> Self { self.box_size = s; self.font_size = s * 0.7; self }
}

impl Widget for Checkbox {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        let label_w = self.label.as_ref()
            .map(|l| l.len() as f32 * self.font_size * 0.6 + 8.0)
            .unwrap_or(0.0);
        Size { width: self.box_size + label_w, height: self.box_size }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Interactive-by-identity (Phase 32): always absorb the click.
        match &self.on_change {
            Some(f) => {
                let f = f.clone();
                let next = !self.checked;
                ctx.on_press(move || f(next));
            }
            None => ctx.on_press(|| {}),
        }
        ctx.semantics(super::Semantics::new(rosace_core::Role::Checkbox)
            .value(if self.checked { "checked" } else { "unchecked" }));
        let r = ctx.rect;
        let box_rect = Rect {
            origin: r.origin,
            size: Size { width: self.box_size, height: self.box_size },
        };

        let t = ctx.animate_to(if self.checked || self.indeterminate { 1.0 } else { 0.0 }, 0.0);
        // Empty state underneath (fades out as t rises).
        ctx.fill_rect(box_rect, super::lerp_color(Color::rgb(15, 16, 28), self.color, t));
        if t < 0.99 {
            ctx.stroke_rect(box_rect, super::lerp_color(self.border_color, self.color, t), 1.5);
        }
        if self.checked || self.indeterminate {
            if self.indeterminate {
                let dash = Rect {
                    origin: Point { x: box_rect.origin.x + self.box_size * 0.2, y: box_rect.origin.y + self.box_size * 0.45 },
                    size: Size { width: self.box_size * 0.6, height: self.box_size * 0.1 },
                };
                ctx.fill_rect(dash, Color::rgba(230, 232, 245, (255.0 * t) as u8));
            } else {
                let ink = Color::rgba(255, 255, 255, (255.0 * t) as u8);
                // A real angled checkmark, drawn as the ✓ glyph (crisp at
                // any size via the glyph pipeline). The previous version
                // approximated it with two axis-aligned rects, which read
                // as a broken "L" (user-reported, Phase 32 bug list).
                let px = self.box_size * 0.85;
                let tw = ctx.font.measure_text("\u{2713}", px);
                let lh = ctx.font.line_height(px);
                ctx.draw_text_at(
                    "\u{2713}",
                    Point {
                        x: box_rect.origin.x + (self.box_size - tw) / 2.0,
                        y: box_rect.origin.y + (self.box_size - lh) / 2.0,
                    },
                    ink,
                    px,
                );
            }
        }

        // Label
        if let Some(label) = &self.label {
            let line_h = ctx.font.line_height(self.font_size);
            let ty = ((self.box_size - line_h) / 2.0).max(0.0);
            ctx.text(label, self.box_size + 8.0, ty, Color::rgb(200, 202, 225), self.font_size);
        }
    }
}
