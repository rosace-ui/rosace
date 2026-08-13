use rosace_core::types::{Point, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

/// A small count badge or status dot.
pub struct Badge {
    pub label: String,
    pub dot: bool,
    /// `None` = the active theme's `colors.error` for a status dot,
    /// `colors.primary` for a labelled badge. Both used to be hardcoded, so
    /// the widget ignored the app's palette entirely.
    pub color: Option<Color>,
    /// `None` = the active theme's `colors.on_primary`.
    pub text_color: Option<Color>,
    /// `None` = read from the active theme's `typography.label_small`
    /// (D127 "environment" track — see `Checkbox::resolved_font_size`'s doc
    /// for the reasoning).
    pub font_size: Option<f32>,
}

impl Badge {
    /// D093: new() must exist wherever named constructors exist.
    pub fn new(text: impl Into<String>) -> Self {
        Self::label(text)
    }

    pub fn count(n: u32) -> Self {
        Self::label(n.to_string())
    }

    pub fn label(text: impl Into<String>) -> Self {
        Self {
            label: text.into(),
            dot: false,
            color: None,
            text_color: None,
            font_size: None,
        }
    }

    pub fn dot() -> Self {
        Self { dot: true, label: String::new(), color: None,
               text_color: None, font_size: None }
    }

    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn text_color(mut self, c: Color) -> Self { self.text_color = Some(c); self }

    fn resolved_font_size(&self, theme: &rosace_theme::ThemeData) -> f32 {
        self.font_size.unwrap_or(theme.typography.label_small.size)
    }
}

impl Widget for Badge {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        if self.dot {
            return Size { width: 8.0, height: 8.0 };
        }
        let font_size = self.resolved_font_size(ctx.theme);
        // `paint` has always measured this properly; `layout` estimated it as
        // `len() * size * 0.6`, so a badge whose glyphs are wider than the
        // guess (or any label at a raised OS text scale) got centred inside a
        // box too small for it. Both axes are floored, never fixed, so the
        // designed 16px pill is a MINIMUM.
        let w = ctx.font.measure_text(&self.label, font_size) + 12.0;
        let h = ctx.font.line_height(font_size) + 4.0;
        Size { width: w.max(16.0), height: h.max(16.0) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if self.dot {
            // A bare status dot carries no text — nothing to announce.
            let cx = ctx.rect.origin.x + 4.0;
            let cy = ctx.rect.origin.y + 4.0;
            // A bare dot is a status indicator, so it defaults to `error`.
            let dot_col = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.error));
            ctx.fill_circle(Point { x: cx, y: cy }, 4.0, dot_col);
            return;
        }
        ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Text).label(&self.label));
        let font_size = self.resolved_font_size(&ctx.theme);
        let r = ctx.rect;
        let bg = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        draw_rounded_rect_pub(ctx, r, bg, r.size.height / 2.0);
        let text_w = ctx.font.measure_text(&self.label, font_size);
        let tx = ((r.size.width - text_w) / 2.0).max(0.0);
        let line_h = ctx.font.line_height(font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);
        let fg = self.text_color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.on_primary));
        ctx.text(&self.label, tx, ty, fg, font_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::FontCache;
    use rosace_theme::built_in;

    fn layout_of(b: &Badge) -> Size {
        let font = FontCache::system_ui().or_else(FontCache::system_mono).expect("no system font");
        let theme = built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 400.0);
        b.layout(&LayoutCtx::new(c, &font, &theme))
    }

    /// `layout` used to estimate `len() * size * 0.6` while `paint` measured
    /// the same string properly, so the two disagreed and the text was
    /// centred inside a box too small for it.
    #[test]
    fn layout_width_matches_what_paint_will_actually_measure() {
        // "WWW" and "iii" have the same len() and very different widths — the
        // estimate could not tell them apart, a real measurement must.
        let wide = layout_of(&Badge::new("WWW")).width;
        let narrow = layout_of(&Badge::new("iii")).width;
        assert!(wide > narrow, "proportional glyphs must produce different widths ({wide} vs {narrow})");
    }

    /// A designed size is a MINIMUM, not a ceiling (pipeline check X2).
    #[test]
    fn a_short_label_still_gets_the_designed_minimum_pill() {
        let s = layout_of(&Badge::new("1"));
        assert!(s.width >= 16.0 && s.height >= 16.0, "got {s:?}");
    }

    /// Raising the OS text size must grow the BOX, not just the glyphs.
    ///
    /// Asserted structurally rather than by setting `text_scale`: that is a
    /// process-global, and the suite runs in parallel, so mutating it here
    /// would corrupt any other test measuring text at the same moment.
    /// `line_height`/`measure_text` are the two functions that apply the
    /// scale, so deriving both axes from them IS the guarantee — and the
    /// fixed `height: 16.0` this replaced could not have satisfied it.
    #[test]
    fn both_axes_derive_from_scaled_font_metrics_not_constants() {
        let font = FontCache::system_ui().or_else(FontCache::system_mono).expect("no system font");
        let theme = built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 400.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let b = Badge::new("notifications pending");
        let fs = b.resolved_font_size(&theme);
        let s = b.layout(&ctx);

        assert!(s.width >= font.measure_text("notifications pending", fs),
            "width must clear the measured text, got {}", s.width);
        assert!(s.height >= font.line_height(fs),
            "height must clear the scaled line box, got {}", s.height);
    }
}
