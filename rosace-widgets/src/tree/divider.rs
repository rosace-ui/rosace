use rosace_core::types::Size;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w, avail_h};

/// A thin separator line — horizontal or vertical.
pub struct Divider {
    pub vertical: bool,
    pub thickness: f32,
    pub color: Color,
    pub indent: f32,
}

impl Divider {
    /// A horizontal divider — the common case (D093: `new()` must exist
    /// wherever named constructors exist).
    pub fn new() -> Self {
        Self::horizontal()
    }

    pub fn horizontal() -> Self {
        Self { vertical: false, thickness: 1.0, color: Color::rgba(0, 0, 0, 0), indent: 0.0 }
    }
    pub fn vertical() -> Self {
        Self { vertical: true, thickness: 1.0, color: Color::rgba(0, 0, 0, 0), indent: 0.0 }
    }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn indent(mut self, i: f32) -> Self { self.indent = i; self }
}

impl Default for Divider {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Divider {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        if self.vertical {
            Size { width: self.thickness, height: avail_h(constraints) }
        } else {
            Size { width: avail_w(constraints), height: self.thickness }
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        use rosace_core::types::{Point, Rect};
        let color = if self.color.a == 0 { ctx.tc(ctx.theme.colors.outline) } else { self.color };
        let r = ctx.rect;
        let rect = if self.vertical {
            Rect { origin: Point { x: r.origin.x, y: r.origin.y + self.indent }, size: Size { width: self.thickness, height: (r.size.height - self.indent).max(0.0) } }
        } else {
            Rect { origin: Point { x: r.origin.x + self.indent, y: r.origin.y }, size: Size { width: (r.size.width - self.indent).max(0.0), height: self.thickness } }
        };
        ctx.fill_rect(rect, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect};
    use rosace_render::{DrawCommand, FontCache, PictureRecorder};
    use crate::tree::RenderTree;
    use std::{cell::RefCell, rc::Rc};

    fn env() -> (FontCache, rosace_theme::ThemeData) {
        (FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    fn drawn(d: Divider, w: f32, h: f32) -> (Rect, rosace_render::Color) {
        let (font, theme) = env();
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: w, height: h } },
                &font, theme, Rc::new(RefCell::new(RenderTree::new())),
            );
            d.paint(&mut ctx);
        }
        rec.finish().commands.iter().find_map(|c| match c {
            DrawCommand::FillRect { rect, color } => Some((*rect, *color)),
            _ => None,
        }).expect("a divider must draw a rule")
    }

    /// A horizontal rule spans the width and is `thickness` tall; a vertical
    /// one is the transpose. Getting the axis backwards draws a rule across
    /// the wrong dimension, which reads as a missing divider rather than a
    /// rotated one.
    #[test]
    fn orientation_decides_which_axis_the_rule_spans() {
        let (font, theme) = env();
        let c = rosace_layout::Constraints::loose(300.0, 200.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let h = Divider::new().layout(&ctx);
        assert_eq!((h.width, h.height), (300.0, 1.0), "horizontal fills the width");

        let v = Divider::vertical().layout(&ctx);
        assert_eq!((v.width, v.height), (1.0, 200.0), "vertical fills the height");
    }

    #[test]
    fn indent_insets_the_rule_from_the_leading_edge_only() {
        let (rect, _) = drawn(Divider::new().indent(24.0), 300.0, 20.0);
        assert_eq!(rect.origin.x, 24.0, "the rule starts after the indent");
        assert_eq!(rect.size.width, 300.0 - 24.0, "and gives that width back");
    }

    /// An indent wider than the rect must collapse the rule, not invert it —
    /// ordinary the moment a window is dragged narrow.
    #[test]
    fn an_indent_wider_than_the_rect_collapses_to_zero() {
        let (rect, _) = drawn(Divider::new().indent(500.0), 300.0, 20.0);
        assert_eq!(rect.size.width, 0.0);
        assert!(rect.size.width >= 0.0, "never negative");
    }

    /// The "unset colour" sentinel is `alpha == 0`, so a DELIBERATELY
    /// transparent divider silently becomes the theme outline instead.
    ///
    /// Documented here rather than fixed: `Option<Color>` is the library's
    /// own pattern elsewhere (`dropdown`, `fab`) and would remove the
    /// collision, but changing it is a public API break. This test pins the
    /// CURRENT behaviour so the sharp edge is at least known and the fix is
    /// a deliberate choice rather than a surprise.
    #[test]
    fn a_fully_transparent_colour_is_treated_as_unset_and_falls_back_to_the_theme() {
        let (_, c) = drawn(Divider::new().color(rosace_render::Color::rgba(255, 0, 0, 0)), 300.0, 20.0);
        assert_ne!((c.r, c.g, c.b), (255, 0, 0),
            "alpha-0 is the unset sentinel, so the red is discarded");
        assert!(c.a > 0, "and the theme outline is opaque");
    }

    #[test]
    fn an_explicit_opaque_colour_wins_over_the_theme() {
        let (_, c) = drawn(Divider::new().color(rosace_render::Color::rgb(10, 200, 30)), 300.0, 20.0);
        assert_eq!((c.r, c.g, c.b), (10, 200, 30));
    }
}
