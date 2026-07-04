use tezzera_core::types::Size;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

pub use tezzera_render::FontWeight;

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
        }
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
    /// greedily word-wrapped via [`tezzera_text::word_wrap`] with real font
    /// metrics. `max_lines` truncates the result. A text that fits on one
    /// line (the common case) skips the per-word measuring entirely.
    fn wrap_lines(&self, font: &tezzera_render::FontCache, max_w: f32) -> Vec<String> {
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
                lines.extend(tezzera_text::word_wrap(paragraph, max_w, |s| {
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
        if self.text.is_empty() { return; }
        ctx.semantics(super::Semantics::new(tezzera_core::Role::Text).label(&self.text));

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
