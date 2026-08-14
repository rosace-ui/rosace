use rosace_core::types::{Point, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

/// Circular avatar with initials or a colored fill.
pub struct Avatar {
    pub initials: String,
    /// `None` = the active theme's `colors.primary`. This used to be a
    /// hardcoded violet, so the widget ignored the app's palette entirely.
    pub color: Option<Color>,
    /// `None` = the active theme's `colors.on_primary`.
    pub text_color: Option<Color>,
    pub size: f32,
    pub font_size: f32,
}

impl Avatar {
    pub fn new(initials: impl Into<String>) -> Self {
        let s = initials.into();
        Self {
            initials: s,
            color: None,
            text_color: None,
            size: 32.0,
            font_size: 12.0,
        }
    }
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn text_color(mut self, c: Color) -> Self { self.text_color = Some(c); self }
    /// Diameter. Also rescales the initials proportionally — call
    /// [`Avatar::font_size`] AFTER this to override that.
    pub fn size(mut self, s: f32) -> Self { self.size = s; self.font_size = s * 0.38; self }

    /// Initials size, independent of the diameter.
    ///
    /// Previously derivable only through `size`, so an avatar could not
    /// carry larger or smaller initials than the 0.38 ratio — and the
    /// diameter's growth for scaled text had no way to be exercised.
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
}

impl Widget for Avatar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // An avatar is a circle of a DESIGNED diameter, but the initials
        // inside it are real text that scales with the OS setting. Kept
        // square (a stretched avatar reads as broken) and grown only when
        // the scaled initials would not fit, so ordinary avatars are
        // untouched at 100%.
        let d = self.size.max(ctx.font.line_height(self.font_size) + 4.0);
        Size { width: d, height: d }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Image).label(&self.initials));
        let cx = ctx.rect.origin.x + self.size / 2.0;
        let cy = ctx.rect.origin.y + self.size / 2.0;
        let bg = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        ctx.fill_circle(Point { x: cx, y: cy }, self.size / 2.0, bg);

        // Centered initials
        let text_w = ctx.font.measure_text(&self.initials, self.font_size);
        let line_h = ctx.font.line_height(self.font_size);
        ctx.text(&self.initials,
            (self.size - text_w) / 2.0,
            (self.size - line_h) / 2.0,
            self.text_color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.on_primary)), self.font_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The circle is a designed diameter, but the initials inside it are
    /// real text that scales with the OS setting — a fixed diameter clipped
    /// them. Must stay SQUARE: a stretched avatar reads as broken.
    #[test]
    fn the_circle_grows_for_large_initials_and_stays_square() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 400.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let normal = Avatar::new("GJ").layout(&ctx);
        assert_eq!(normal.width, normal.height, "always square");
        assert_eq!(normal.width, 32.0, "the designed diameter at 100%");

        let big = Avatar::new("GJ").font_size(48.0).layout(&ctx);
        assert_eq!(big.width, big.height, "still square when grown");
        assert!(big.width > normal.width,
            "48px initials do not fit a 32px circle: {} vs {}", normal.width, big.width);
    }
}
