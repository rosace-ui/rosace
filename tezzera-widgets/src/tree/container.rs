use tezzera_core::types::{Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};
use super::padding::EdgeInsets;

/// The most fundamental building block — a box with optional background,
/// border, rounded corners, shadow, fixed dimensions, padding, and child.
///
/// Analogous to a CSS `div` or Flutter's `Container`.
pub struct Container {
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub border_radius: f32,
    pub shadow_blur: f32,
    pub shadow_color: Color,
    pub padding: EdgeInsets,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: f32,
    pub min_height: f32,
    pub child: Option<BoxedWidget>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            background: None,
            border_color: None,
            border_width: 1.0,
            border_radius: 0.0,
            shadow_blur: 0.0,
            shadow_color: Color::rgba(0, 0, 0, 0),
            padding: EdgeInsets::default(),
            width: None,
            height: None,
            min_width: 0.0,
            min_height: 0.0,
            child: None,
        }
    }

    pub fn color(mut self, c: Color) -> Self { self.background = Some(c); self }
    pub fn border(mut self, c: Color, w: f32) -> Self {
        self.border_color = Some(c); self.border_width = w; self
    }
    pub fn radius(mut self, r: f32) -> Self { self.border_radius = r; self }
    pub fn shadow(mut self, color: Color, blur: f32) -> Self {
        self.shadow_color = color; self.shadow_blur = blur; self
    }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = Some(h); self }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w); self.height = Some(h); self
    }
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_width = w; self.min_height = h; self
    }
    pub fn child(mut self, w: impl Widget + 'static) -> Self {
        self.child = Some(Box::new(w)); self
    }
}

impl Default for Container {
    fn default() -> Self { Self::new() }
}

impl Widget for Container {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let child_size = self.child.as_ref().map(|c| {
            let inner_c = Constraints::loose(
                (avail_w(constraints) - self.padding.total_h()).max(0.0),
                (avail_h(constraints) - self.padding.total_v()).max(0.0),
            );
            self.padding.grow(c.layout(&ctx.with_constraints(inner_c)))
        }).unwrap_or(Size { width: 0.0, height: 0.0 });

        let w = self.width.unwrap_or(child_size.width.max(self.min_width));
        let h = self.height.unwrap_or(child_size.height.max(self.min_height));

        constraints.constrain(Size {
            width:  w.max(self.min_width),
            height: h.max(self.min_height),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;

        // Drop shadow
        if self.shadow_blur > 0.5 {
            ctx.fill_shadow(rect, self.shadow_color, self.shadow_blur);
        }

        // Background
        if let Some(bg) = self.background {
            if self.border_radius > 0.5 {
                ctx.fill_rrect(rect, self.border_radius, bg);
            } else {
                ctx.fill_rect(rect, bg);
            }
        }

        // Border
        if let Some(bc) = self.border_color {
            ctx.stroke_rect(rect, bc, self.border_width);
        }

        // Child
        if let Some(child) = &self.child {
            let inner = self.padding.shrink(rect);
            child.paint(&mut ctx.child(inner));
        }
    }
}

/// Fill a rounded rectangle through a `PaintCtx` (used by widgets that need
/// rounded corners but aren't `Container`).
pub(super) fn draw_rounded_rect_pub(ctx: &mut PaintCtx, rect: Rect, color: Color, radius: f32) {
    ctx.fill_rrect(rect, radius, color);
}
