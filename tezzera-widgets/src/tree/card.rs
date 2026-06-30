use tezzera_core::types::Size;
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::padding::EdgeInsets;
use super::container::draw_rounded_rect_pub;

/// An elevated surface — background + rounded corners + optional shadow.
///
/// The most common surface for grouping content (task card, profile card, etc.).
pub struct Card {
    pub background: Color,
    pub border_color: Option<Color>,
    pub radius: f32,
    pub elevation: f32,
    pub padding: EdgeInsets,
    pub width: Option<f32>,
    pub child: BoxedWidget,
}

impl Card {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            background: Color::rgba(0, 0, 0, 0), // sentinel: use theme.surface_variant
            border_color: Some(Color::rgba(0, 0, 0, 0)), // sentinel: use theme.outline
            radius: 8.0,
            elevation: 4.0,
            padding: EdgeInsets::all(12.0),
            width: None,
            child: Box::new(child),
        }
    }

    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn border(mut self, c: Color) -> Self { self.border_color = Some(c); self }
    pub fn no_border(mut self) -> Self { self.border_color = None; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn elevation(mut self, e: f32) -> Self { self.elevation = e; self }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
}

impl Widget for Card {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let inner_c = Constraints::loose(
            (constraints.max_width_f32() - self.padding.total_h()).max(0.0),
            (constraints.max_height_f32() - self.padding.total_v()).max(0.0),
        );
        let child_size = self.child.layout(&ctx.with_constraints(inner_c));
        let total = self.padding.grow(child_size);
        constraints.constrain(Size {
            width:  self.width.unwrap_or(total.width),
            height: total.height,
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;

        if self.elevation > 0.5 {
            ctx.fill_shadow(r, Color::rgba(0, 0, 0, 80), self.elevation);
        }

        let bg = if self.background.a == 0 {
            ctx.tc(ctx.theme.colors.surface_variant)
        } else {
            self.background
        };
        draw_rounded_rect_pub(ctx, r, bg, self.radius);

        if let Some(bc) = self.border_color {
            let bc = if bc.a == 0 { ctx.tc(ctx.theme.colors.outline) } else { bc };
            ctx.stroke_rect(r, bc, 1.0);
        }

        // Child
        let inner = self.padding.shrink(r);
        self.child.paint(&mut ctx.child(inner));
    }
}
