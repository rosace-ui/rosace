use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use rosace_text::{RichText, TextLayout};
use super::{Widget, LayoutCtx, PaintCtx};

/// `rosace-text`'s `TextSpan`/`RichText` use `rosace_theme::Color` (0..1
/// float channels — the theme-token color type); painting APIs here want
/// `rosace_render::Color` (0-255 u8 channels) — same two-type split every
/// other widget already converts via `PaintCtx::tc`, just without a
/// `ThemeData` available at this conversion site (a plain color value, not
/// a token lookup), so a direct field-by-field convert instead.
fn theme_to_render_color(c: rosace_theme::Color) -> Color {
    Color::rgba(
        (c.r * 255.0).round() as u8,
        (c.g * 255.0).round() as u8,
        (c.b * 255.0).round() as u8,
        (c.a * 255.0).round() as u8,
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

pub use rosace_render::FontWeight;

/// A plain text leaf widget.
///
/// Color defaults to the theme's `on_surface` — no explicit color needed.
pub struct Text {
    pub text: String,
    /// `None` = use `theme.colors.on_surface`. `Some(c)` = explicit override.
    pub color: Option<Color>,
    pub size: f32,
    pub align: TextAlign,
    pub weight: FontWeight,
    pub max_lines: Option<usize>,
    /// Mixed-style spans (Phase 32 Step 3, D115) — when set, this widget
    /// renders THESE styled runs instead of `text`/`color`/`size`/`weight`
    /// (which stay at their defaults and are ignored). Real integration
    /// with `rosace-text`'s existing `RichText`/`TextSpan`/`TextLayout` —
    /// this widget does the wrapping via `TextLayout::layout_with_measure`
    /// (real font metrics, not the crate's own heuristic fallback) and
    /// paints each span with `PaintCtx`'s ordinary text primitives; no
    /// rewrite of those types.
    spans: Option<RichText>,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            size: 18.0,
            align: TextAlign::Left,
            weight: FontWeight::Regular,
            max_lines: None,
            spans: None,
        }
    }

    /// A paragraph of mixed-style spans (bold/italic/color/underline changes
    /// mid-paragraph) — see `rosace_text::RichText`'s builder
    /// (`.text(...)`/`.bold(...)`/`.push(...)`). Wrapping/alignment/color
    /// still apply; `.size()`/`.weight()`/`.color()` on this widget do NOT
    /// (each span carries its own).
    pub fn rich(spans: RichText) -> Self {
        Self { spans: Some(spans), ..Self::new(String::new()) }
    }

    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self }
    pub fn align(mut self, a: TextAlign) -> Self { self.align = a; self }
    pub fn weight(mut self, w: FontWeight) -> Self { self.weight = w; self }
    pub fn max_lines(mut self, n: usize) -> Self { self.max_lines = Some(n); self }
}

impl Text {
    /// Break the text into lines that fit `max_w` pixels.
    ///
    /// Explicit `\n` breaks are honored first, then each paragraph is
    /// greedily word-wrapped via [`rosace_text::word_wrap`] with real font
    /// metrics. `max_lines` truncates the result. A text that fits on one
    /// line (the common case) skips the per-word measuring entirely.
    fn wrap_lines(&self, font: &rosace_render::FontCache, max_w: f32) -> Vec<String> {
        let single_paragraph = !self.text.contains('\n');
        if single_paragraph
            && (!max_w.is_finite() || font.measure_text_weighted(&self.text, self.size, self.weight) <= max_w)
        {
            return vec![self.text.clone()];
        }

        let mut lines = Vec::new();
        for paragraph in self.text.split('\n') {
            if paragraph.is_empty() {
                lines.push(String::new());
            } else {
                lines.extend(rosace_text::word_wrap(paragraph, max_w, |s| {
                    font.measure_text_weighted(s, self.size, self.weight)
                }));
            }
        }
        if let Some(n) = self.max_lines {
            lines.truncate(n);
        }
        lines
    }
}

impl Widget for Text {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        if let Some(rt) = &self.spans {
            let max_w = ctx.constraints.max_width_f32();
            let layout = rich_layout(rt, ctx.font, max_w);
            let text_w = layout.lines.iter().map(|l| l.width).fold(0.0_f32, f32::max);
            return ctx.constraints.constrain(Size { width: text_w, height: layout.total_height() });
        }
        let line_h = ctx.font.line_height(self.size);
        let lines = self.wrap_lines(ctx.font, ctx.constraints.max_width_f32());
        let text_w = lines
            .iter()
            .map(|l| ctx.font.measure_text_weighted(l, self.size, self.weight))
            .fold(0.0_f32, f32::max);
        let text_h = line_h * lines.len().max(1) as f32;
        ctx.constraints.constrain(Size { width: text_w, height: text_h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if let Some(rt) = &self.spans {
            paint_rich(rt, ctx, self.align);
            return;
        }
        if self.text.is_empty() { return; }
        ctx.semantics(super::Semantics::new(rosace_core::Role::Text).label(&self.text));

        // Fall back to theme on_surface when no explicit color is set.
        let color = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.on_surface));

        let line_h = ctx.font.line_height(self.size);
        let mut lines = self.wrap_lines(ctx.font, ctx.rect.size.width);

        // Honor the allocated rect: paint only the lines that fit. layout()
        // reports the wrapped height, but a parent may allot less (constrained
        // container) — painting past the rect would bleed into siblings.
        let fit = ((ctx.rect.size.height / line_h).floor() as usize).max(1);
        lines.truncate(fit);

        let total_h = line_h * lines.len() as f32;
        let y_base = ((ctx.rect.size.height - total_h) / 2.0).max(0.0);

        for (i, line) in lines.iter().enumerate() {
            if line.is_empty() { continue; }
            let line_w = ctx.font.measure_text_weighted(line, self.size, self.weight);
            let x_off = match self.align {
                TextAlign::Left   => 0.0,
                TextAlign::Center => ((ctx.rect.size.width - line_w) / 2.0).max(0.0),
                TextAlign::Right  => (ctx.rect.size.width - line_w).max(0.0),
            };
            ctx.text_styled(line, x_off, y_base + i as f32 * line_h, color, self.size, self.weight);
        }
    }
}

/// `TextLayout::layout_with_measure`'s measure closure is `Fn(&str, f32)`
/// (text + size only) — it has no slot for per-span weight, so wrapping
/// decisions measure every span as Regular. A long run of Bold text can
/// therefore wrap very slightly early (Bold is wider). Documented
/// approximation, not silently accepted: extending the closure's signature
/// would ripple into every other `layout_with_measure` caller/test in
/// `rosace-text`, which is exactly the "rewrite those types" this
/// integration is deliberately avoiding. Actual PAINTED weight (below) is
/// always correct — only the wrap point is approximated.
fn rich_layout(rt: &RichText, font: &rosace_render::FontCache, max_w: f32) -> TextLayout {
    TextLayout::layout_with_measure(&rt.spans, max_w, |s, size| {
        font.measure_text_weighted(s, size, FontWeight::Regular)
    })
}

fn paint_rich(rt: &RichText, ctx: &mut PaintCtx, align: TextAlign) {
    if rt.is_empty() { return; }
    ctx.semantics(super::Semantics::new(rosace_core::Role::Text).label(rt.plain_text()));

    let layout = rich_layout(rt, ctx.font, ctx.rect.size.width);
    let mut cy = 0.0_f32;
    for line in &layout.lines {
        let mut cx = match align {
            TextAlign::Left   => 0.0,
            TextAlign::Center => ((ctx.rect.size.width - line.width) / 2.0).max(0.0),
            TextAlign::Right  => (ctx.rect.size.width - line.width).max(0.0),
        };
        for span in &line.spans {
            // Italic isn't rendered yet — a real italic face/synthetic-oblique
            // path doesn't exist in FontCache today (named, separate deferral
            // in PHASE_32.md: "italic axis not started"). Bold and color and
            // underline all apply for real.
            let weight = if span.style.bold { FontWeight::Bold } else { FontWeight::Regular };
            let color = theme_to_render_color(span.style.color);
            let w = ctx.font.measure_text_weighted(&span.text, span.style.font_size, weight);
            ctx.text_styled(&span.text, cx, cy, color, span.style.font_size, weight);
            if span.style.underline {
                let underline_y = cy + ctx.font.line_height(span.style.font_size) * 0.92;
                ctx.fill_rect(Rect {
                    origin: Point { x: ctx.rect.origin.x + cx, y: ctx.rect.origin.y + underline_y },
                    size: Size { width: w, height: 1.0 },
                }, color);
            }
            cx += w;
        }
        cy += line.height * layout.line_spacing;
    }
}

// ── Named text styles (all use theme colors unless overridden) ────────────────

impl Text {
    pub fn label(text: impl Into<String>) -> Self {
        Self::new(text).size(16.0)
    }

    pub fn caption(text: impl Into<String>) -> Self {
        Self::new(text).size(14.0)
    }

    pub fn heading(text: impl Into<String>) -> Self {
        Self::new(text).size(22.0).weight(FontWeight::SemiBold)
    }

    pub fn title(text: impl Into<String>) -> Self {
        Self::new(text).size(20.0).weight(FontWeight::Medium)
    }

    pub fn display(text: impl Into<String>) -> Self {
        Self::new(text).size(40.0).weight(FontWeight::Bold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;
    use rosace_theme::Color as ThemeColor;

    fn ctx_font() -> rosace_render::FontCache { rosace_render::FontCache::embedded() }
    fn ctx_theme() -> rosace_theme::ThemeData { rosace_theme::built_in::dark_theme() }

    #[test]
    fn rich_text_renders_a_real_paragraph_with_mixed_styles() {
        // The concrete exit bar from PHASE_32.md Step 3: a real running app
        // renders a paragraph with at least two different inline styles in
        // a single Text widget.
        let font = ctx_font();
        let theme = ctx_theme();
        let rt = RichText::new()
            .text("Plain ", 16.0, ThemeColor::WHITE)
            .bold("bold", 16.0, ThemeColor::WHITE)
            .text(" and ", 16.0, ThemeColor::WHITE)
            .push("colored", rosace_text::TextStyle::new(16.0, ThemeColor { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }));
        let text = Text::rich(rt);

        let lctx = LayoutCtx::new(Constraints::loose(400.0, 200.0), &font, &theme);
        let size = text.layout(&lctx);
        assert!(size.width > 0.0 && size.height > 0.0);

        // paint() must not panic and must not fall into the plain-text
        // early-return (spans mode ignores the empty `self.text`).
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(super::super::render_tree::RenderTree::new()));
        let mut pctx = PaintCtx::root(&mut recorder, Rect { origin: Point { x: 0.0, y: 0.0 }, size }, &font, theme, tree);
        text.paint(&mut pctx);
        let picture = recorder.finish();
        assert!(!picture.commands.is_empty(), "rich text must actually record draw commands");
    }

    #[test]
    fn rich_text_wraps_across_multiple_lines_when_narrow() {
        let font = ctx_font();
        let theme = ctx_theme();
        let rt = RichText::new().text("one two three four five six seven", 16.0, ThemeColor::WHITE);
        let text = Text::rich(rt);
        let lctx = LayoutCtx::new(Constraints::loose(80.0, 400.0), &font, &theme);
        let size = text.layout(&lctx);
        // At 80px wide, a 7-word sentence at 16px must wrap to more than one line.
        let line_h = font.line_height(16.0);
        assert!(size.height > line_h, "expected wrapping, got single-line height {}", size.height);
    }

    #[test]
    fn plain_text_path_is_unaffected_by_spans_field_existing() {
        let font = ctx_font();
        let theme = ctx_theme();
        let text = Text::new("hello world");
        let lctx = LayoutCtx::new(Constraints::loose(400.0, 200.0), &font, &theme);
        let size = text.layout(&lctx);
        assert!(size.width > 0.0);
    }
}
