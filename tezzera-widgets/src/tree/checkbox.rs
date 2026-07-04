use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::Color;
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
        if let Some(f) = &self.on_change {
            let f = f.clone();
            let next = !self.checked;
            ctx.on_press(move || f(next));
        }
        ctx.semantics(super::Semantics::new(tezzera_core::Role::Checkbox)
            .value(if self.checked { "checked" } else { "unchecked" }));
        let r = ctx.rect;
        let box_rect = Rect {
            origin: r.origin,
            size: Size { width: self.box_size, height: self.box_size },
        };

        if self.checked || self.indeterminate {
            ctx.fill_rect(box_rect, self.color);
            if self.indeterminate {
                let dash = Rect {
                    origin: Point { x: box_rect.origin.x + self.box_size * 0.2, y: box_rect.origin.y + self.box_size * 0.45 },
                    size: Size { width: self.box_size * 0.6, height: self.box_size * 0.1 },
                };
                ctx.fill_rect(dash, Color::rgb(230, 232, 245));
            } else {
                let tick1 = Rect {
                    origin: Point { x: box_rect.origin.x + self.box_size * 0.2, y: box_rect.origin.y + self.box_size * 0.5 },
                    size: Size { width: self.box_size * 0.25, height: self.box_size * 0.1 },
                };
                let tick2 = Rect {
                    origin: Point { x: box_rect.origin.x + self.box_size * 0.4, y: box_rect.origin.y + self.box_size * 0.3 },
                    size: Size { width: self.box_size * 0.1, height: self.box_size * 0.35 },
                };
                ctx.fill_rect(tick1, Color::rgb(230, 232, 245));
                ctx.fill_rect(tick2, Color::rgb(230, 232, 245));
            }
        } else {
            ctx.fill_rect(box_rect, Color::rgb(15, 16, 28));
            ctx.stroke_rect(box_rect, self.border_color, 1.5);
        }

        // Label
        if let Some(label) = &self.label {
            let line_h = ctx.font.line_height(self.font_size);
            let ty = ((self.box_size - line_h) / 2.0).max(0.0);
            ctx.text(label, self.box_size + 8.0, ty, Color::rgb(200, 202, 225), self.font_size);
        }
    }
}
