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
    /// When set, the container fills available space and places its child
    /// at this alignment (absorbs the old `Center` widget — D095).
    pub align: Option<super::Alignment>,
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
            align: None,
            child: None,
        }
    }

    /// Align the child within this container. Implies filling the available
    /// space (there is nothing to align within a shrink-wrapped box).
    pub fn align(mut self, a: super::Alignment) -> Self { self.align = Some(a); self }

    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
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
            // A fixed width/height bounds the CHILD too — a 240px-wide card's
            // text must wrap at 240px even when the parent offers infinity.
            let avail_w = self.width.unwrap_or_else(|| avail_w(constraints));
            let avail_h = self.height.unwrap_or_else(|| avail_h(constraints));
            let inner_c = Constraints::loose(
                (avail_w - self.padding.total_h()).max(0.0),
                (avail_h - self.padding.total_v()).max(0.0),
            );
            self.padding.grow(c.layout(&ctx.with_constraints(inner_c)))
        }).unwrap_or(Size { width: 0.0, height: 0.0 });

        // With an alignment set, fill the available (bounded) space —
        // Flutter semantics; a shrink-wrapped box has no room to align in.
        let (fill_w, fill_h) = if self.align.is_some() {
            (avail_w(constraints), avail_h(constraints))
        } else {
            (f32::INFINITY, f32::INFINITY) // sentinel: not used below
        };
        let w = self.width.unwrap_or_else(|| {
            if self.align.is_some() && fill_w.is_finite() { fill_w }
            else { child_size.width.max(self.min_width) }
        });
        let h = self.height.unwrap_or_else(|| {
            if self.align.is_some() && fill_h.is_finite() { fill_h }
            else { child_size.height.max(self.min_height) }
        });

        constraints.constrain(Size {
            width:  w.max(self.min_width),
            height: h.max(self.min_height),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let rect = ctx.rect;

        // Drop shadow — source shape matches the (possibly rounded) background.
        if self.shadow_blur > 0.5 {
            ctx.fill_shadow_rrect(rect, self.border_radius, self.shadow_color, self.shadow_blur);
        }

        // Background
        if let Some(bg) = self.background {
            if self.border_radius > 0.5 {
                ctx.fill_rrect(rect, self.border_radius, bg);
            } else {
                ctx.fill_rect(rect, bg);
            }
        }

        // Border — follows the same corner geometry as the background.
        if let Some(bc) = self.border_color {
            if self.border_radius > 0.5 {
                ctx.stroke_rrect(rect, self.border_radius, bc, self.border_width);
            } else {
                ctx.stroke_rect(rect, bc, self.border_width);
            }
        }

        // Child — aligned within the padded rect when an alignment is set,
        // otherwise given the full inner rect.
        if let Some(child) = &self.child {
            let inner = self.padding.shrink(rect);
            let child_rect = if let Some(align) = self.align {
                let inner_c = Constraints::loose(inner.size.width, inner.size.height);
                let child_size = child.layout(&ctx.layout_ctx(inner_c));
                let off = align.offset(inner.size, child_size);
                Rect {
                    origin: tezzera_core::types::Point {
                        x: inner.origin.x + off.x,
                        y: inner.origin.y + off.y,
                    },
                    size: child_size,
                }
            } else {
                inner
            };
            child.paint(&mut ctx.child(child_rect));
        }
    }
}

/// Fill a rounded rectangle through a `PaintCtx` (used by widgets that need
/// rounded corners but aren't `Container`).
pub(super) fn draw_rounded_rect_pub(ctx: &mut PaintCtx, rect: Rect, color: Color, radius: f32) {
    ctx.fill_rrect(rect, radius, color);
}
