//! [`Chip`] — a compact selection or filter chip with optional dismiss button.

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::canvas::SkiaCanvas;
use tezzera_render::FontCache;
use tezzera_theme::ThemeData;
use crate::theme_color_to_render as tc;

/// A compact chip used for tags, filters, or selections.
///
/// When `selected` is `true` the chip is filled with the primary color.
/// When `dismissible` is `true` a "×" glyph is appended to the label.
pub struct Chip {
    pub label: String,
    pub selected: bool,
    pub dismissible: bool,
}

impl Chip {
    /// Creates a new unselected, non-dismissible [`Chip`] with the given label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            selected: false,
            dismissible: false,
        }
    }

    /// Sets whether the chip is in the selected (filled) state.
    pub fn selected(mut self, s: bool) -> Self {
        self.selected = s;
        self
    }

    /// When `true`, a dismiss glyph ("×") is rendered at the right edge.
    pub fn dismissible(mut self, d: bool) -> Self {
        self.dismissible = d;
        self
    }

    /// Paints the chip onto `canvas` at `(x, y)`.
    pub fn render(
        &self,
        canvas: &mut SkiaCanvas,
        font: &FontCache,
        x: f32,
        y: f32,
        theme: &ThemeData,
    ) {
        let w = self.width();
        let h = Self::height();

        let text_color = if self.selected {
            tc(theme.colors.on_primary)
        } else {
            tc(theme.colors.on_surface)
        };

        if self.selected {
            canvas.fill_rect(
                Rect {
                    origin: Point { x, y },
                    size: Size { width: w, height: h },
                },
                tc(theme.colors.primary),
            );
        }

        canvas.stroke_rect(
            Rect {
                origin: Point { x, y },
                size: Size { width: w, height: h },
            },
            tc(theme.colors.outline),
            1.0,
        );

        canvas.draw_text(
            &self.label,
            Point { x: x + 12.0, y: y + 10.0 },
            text_color,
            font,
            13.0,
        );

        if self.dismissible {
            let label_w = self.label.len() as f32 * 7.0;
            canvas.draw_text(
                "\u{00D7}",
                Point { x: x + label_w + 16.0, y: y + 9.0 },
                text_color,
                font,
                14.0,
            );
        }
    }

    /// Total pixel width of the chip including padding and optional dismiss glyph.
    pub fn width(&self) -> f32 {
        let extra = if self.dismissible { 20.0 } else { 0.0 };
        self.label.len() as f32 * 7.0 + 24.0 + extra
    }

    /// Fixed height of the chip in pixels.
    pub fn height() -> f32 {
        32.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_default_not_selected() {
        let c = Chip::new("Tag");
        assert!(!c.selected);
    }

    #[test]
    fn chip_selected_setter() {
        let c = Chip::new("Tag").selected(true);
        assert!(c.selected);
    }

    #[test]
    fn chip_dismissible_adds_width() {
        let normal = Chip::new("Tag");
        let dismiss = Chip::new("Tag").dismissible(true);
        assert!(dismiss.width() > normal.width());
        assert!((dismiss.width() - normal.width() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn chip_height_is_constant() {
        assert_eq!(Chip::height(), 32.0);
    }

    #[test]
    fn chip_label_stored() {
        let c = Chip::new("Hello");
        assert_eq!(c.label, "Hello");
    }

    #[test]
    fn chip_not_dismissible_by_default() {
        let c = Chip::new("X");
        assert!(!c.dismissible);
    }

    #[test]
    fn chip_width_scales_with_label() {
        let short = Chip::new("Hi");
        let long = Chip::new("Hello World");
        assert!(long.width() > short.width());
    }

    #[test]
    fn chip_dismissible_false_after_true() {
        let c = Chip::new("X").dismissible(true).dismissible(false);
        assert!(!c.dismissible);
    }
}
