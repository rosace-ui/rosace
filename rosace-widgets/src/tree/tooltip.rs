use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;

use super::{Widget, Children, LayoutCtx, PaintCtx, BoxedWidget};
use super::overlay::{OverlayEntry, LayerPosition, InputBehavior, FocusBehavior, push_overlay};

/// Wraps a child and shows a floating label while the pointer hovers it.
///
/// Built on the hover infrastructure: the child registers a hover region;
/// when hovered, a label overlay floats just above it (clamped on-screen by
/// the overlay pass).
pub struct Tooltip {
    label:     String,
    font_size: f32,
    child:     BoxedWidget,
}

impl Tooltip {
    pub fn new(label: impl Into<String>, child: impl Widget + 'static) -> Self {
        Self { label: label.into(), font_size: 12.0, child: Box::new(child) }
    }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
}

impl Widget for Tooltip {
    fn children(&self) -> Children<'_> { Children::One(&*self.child) }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        self.child.paint(&mut ctx.child(r));
        ctx.hoverable();
        if ctx.hovered() {
            let px = self.font_size;
            let w = ctx.font.measure_text(&self.label, px) + 16.0;
            let h = px * 1.7;
            // Anchor just above the hovered child.
            let pos = Point { x: r.origin.x, y: (r.origin.y - h - 4.0).max(0.0) };
            let label = self.label.clone();
            push_overlay(
                OverlayEntry::new(LayerPosition::Absolute(pos), TooltipLabel { label, w, h, px })
                    .input(InputBehavior::PassThrough)
                    .focus(FocusBehavior::Inert),
            );
        }
    }
    // layout: default delegates to the child.
}

struct TooltipLabel { label: String, w: f32, h: f32, px: f32 }

impl Widget for TooltipLabel {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.w, height: self.h })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        ctx.fill_shadow_rrect(r, 6.0, Color::rgba(0, 0, 0, 90), 8.0);
        ctx.fill_rrect(r, 6.0, Color::rgba(40, 42, 58, 245));
        let ty = r.origin.y + (self.h - ctx.font.line_height(self.px)) / 2.0;
        ctx.draw_text_at(&self.label, Point { x: r.origin.x + 8.0, y: ty },
            Color::rgb(228, 230, 244), self.px);
    }
}
