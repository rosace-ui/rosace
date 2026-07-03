use tezzera_core::types::Size;
use tezzera_render::Color;

use super::{Widget, LayoutCtx, PaintCtx};

/// A widget that wraps a child and renders a floating label adjacent to it.
///
/// For Phase 4 the label is rendered directly below the child (no hover
/// detection — hover is Phase 6 gesture work). The tooltip is always visible
/// when `visible` is `true`.
pub struct Tooltip {
    label:      String,
    visible:    bool,
    font_size:  f32,
    child:      Box<dyn Widget>,
}

impl Tooltip {
    pub fn new(label: impl Into<String>, child: impl Widget + 'static) -> Self {
        Self {
            label:     label.into(),
            visible:   false,
            font_size: 11.0,
            child:     Box::new(child),
        }
    }

    pub fn visible(mut self, v: bool) -> Self { self.visible = v; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
}

impl Widget for Tooltip {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let child_size = self.child.layout(ctx);
        if self.visible {
            let label_h = self.font_size * 1.6;
            Size { width: child_size.width, height: child_size.height + label_h }
        } else {
            child_size
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let child_size = self.child.layout(&ctx.layout_ctx(tezzera_layout::Constraints::loose(
            ctx.rect.size.width,
            ctx.rect.size.height,
        )));

        // Paint child in its own rect
        let child_rect = tezzera_core::types::Rect {
            origin: ctx.rect.origin,
            size: child_size,
        };
        let mut child_ctx = ctx.child(child_rect);
        self.child.paint(&mut child_ctx);

        // Paint tooltip label below the child
        if self.visible {
            let lx = ctx.rect.origin.x + 4.0;
            let ly = ctx.rect.origin.y + child_size.height + 2.0;
            let bg = Color::rgba(40, 40, 60, 220);
            let label_w = self.label.len() as f32 * self.font_size * 0.6 + 8.0;
            let label_h = self.font_size * 1.6;
            ctx.fill_rect(
                tezzera_core::types::Rect {
                    origin: tezzera_core::types::Point { x: lx - 4.0, y: ly - 2.0 },
                    size: tezzera_core::types::Size { width: label_w, height: label_h },
                },
                bg,
            );
            ctx.draw_text_at(
                &self.label,
                tezzera_core::types::Point { x: lx, y: ly + self.font_size * 0.9 },
                Color::rgb(220, 220, 240),
                self.font_size,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{SizedBox, LayoutCtx};
    use tezzera_render::FontCache;
    use tezzera_theme::built_in;

    fn test_ctx(_c: tezzera_layout::Constraints) -> (FontCache, tezzera_theme::ThemeData) {
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("no system font");
        let theme = built_in::dark_theme();
        (font, theme)
    }

    #[test]
    fn tooltip_hidden_has_child_size() {
        let tip = Tooltip::new("hint", Spacer::gap(100.0, 50.0));
        let (font, theme) = test_ctx(tezzera_layout::Constraints::loose(800.0, 600.0));
        let ctx = LayoutCtx::new(tezzera_layout::Constraints::loose(800.0, 600.0), &font, &theme);
        let size = tip.layout(&ctx);
        assert_eq!(size.width, 100.0);
        assert_eq!(size.height, 50.0);
    }

    #[test]
    fn tooltip_visible_adds_label_height() {
        let tip = Tooltip::new("hint", Spacer::gap(100.0, 50.0))
            .visible(true)
            .font_size(12.0);
        let (font, theme) = test_ctx(tezzera_layout::Constraints::loose(800.0, 600.0));
        let ctx = LayoutCtx::new(tezzera_layout::Constraints::loose(800.0, 600.0), &font, &theme);
        let size = tip.layout(&ctx);
        assert_eq!(size.width, 100.0);
        assert!(size.height > 50.0, "tooltip label should add height");
    }
}
