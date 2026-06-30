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

#[derive(Debug, Clone, Copy, Default)]
pub enum FontWeight {
    Light,
    #[default]
    Regular,
    Medium,
    SemiBold,
    Bold,
}

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

impl Widget for Text {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let text_w = ctx.font.measure_text(&self.text, self.size);
        let line_h = ctx.font.line_height(self.size);
        ctx.constraints.constrain(Size { width: text_w, height: line_h })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        if self.text.is_empty() { return; }

        // Fall back to theme on_surface when no explicit color is set.
        let color = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.on_surface));

        let line_h = ctx.font.line_height(self.size);
        let text_w = ctx.font.measure_text(&self.text, self.size);

        let x_off = match self.align {
            TextAlign::Left   => 0.0,
            TextAlign::Center => ((ctx.rect.size.width - text_w) / 2.0).max(0.0),
            TextAlign::Right  => (ctx.rect.size.width - text_w).max(0.0),
        };

        let y_off = ((ctx.rect.size.height - line_h) / 2.0).max(0.0);
        ctx.text(&self.text, x_off, y_off, color, self.size);
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
