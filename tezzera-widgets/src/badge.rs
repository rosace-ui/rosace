//! [`Badge`] — a notification dot or count badge for overlaying on widgets.

use tezzera_core::types::Point;
use tezzera_render::canvas::{Color, SkiaCanvas};
use tezzera_render::FontCache;
use tezzera_theme::ThemeData;
use crate::theme_color_to_render as tc;

// Rect and Size are used in render
use tezzera_core::types::{Rect, Size};

/// A small overlay indicator — either a plain dot or a numeric count chip.
pub struct Badge {
    /// `Some(n)` shows the number; `None` shows a plain dot.
    pub count: Option<u32>,
    /// Numbers above this limit display as `"<max>+"`.
    pub max_count: u32,
}

impl Badge {
    /// Creates a new dot [`Badge`] (no count) with a max of 99.
    pub fn new() -> Self {
        Self { count: None, max_count: 99 }
    }

    /// Sets a specific count to display.
    pub fn count(mut self, n: u32) -> Self {
        self.count = Some(n);
        self
    }

    /// Switches to dot mode (hides the number).
    pub fn dot(mut self) -> Self {
        self.count = None;
        self
    }

    /// Sets the overflow threshold. Counts above this are shown as `"<max>+"`.
    pub fn max_count(mut self, m: u32) -> Self {
        self.max_count = m;
        self
    }

    /// Returns the label string that will be rendered for the given count.
    pub fn label_text(&self) -> Option<String> {
        self.count.map(|n| {
            if n > self.max_count {
                format!("{}+", self.max_count)
            } else {
                format!("{}", n)
            }
        })
    }

    /// Paints the badge centered at `(x, y)` — typically the top-right corner
    /// of a parent widget.
    pub fn render(
        &self,
        canvas: &mut SkiaCanvas,
        font: &FontCache,
        x: f32,
        y: f32,
        theme: &ThemeData,
    ) {
        let color = tc(theme.colors.error);
        match self.count {
            None => {
                // Dot badge
                canvas.fill_circle(Point { x, y }, 5.0, color);
            }
            Some(n) => {
                let label = if n > self.max_count {
                    format!("{}+", self.max_count)
                } else {
                    format!("{}", n)
                };
                let w = (label.len() as f32 * 7.5 + 10.0).max(20.0);
                let h = 20.0_f32;
                canvas.fill_rect(
                    Rect {
                        origin: Point { x: x - w / 2.0, y: y - h / 2.0 },
                        size: Size { width: w, height: h },
                    },
                    color,
                );
                canvas.draw_text(
                    &label,
                    Point { x: x - label.len() as f32 * 3.5, y: y - 6.0 },
                    Color::WHITE,
                    font,
                    11.0,
                );
            }
        }
    }
}

impl Default for Badge {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_dot_badge_has_no_count() {
        let b = Badge::new();
        assert!(b.count.is_none());
    }

    #[test]
    fn badge_count_badge() {
        let b = Badge::new().count(5);
        assert_eq!(b.count, Some(5));
    }

    #[test]
    fn badge_count_exceeds_max_label() {
        let b = Badge::new().max_count(99).count(150);
        let label = b.label_text().unwrap();
        assert_eq!(label, "99+");
    }

    #[test]
    fn badge_count_within_max_shows_number() {
        let b = Badge::new().max_count(99).count(42);
        let label = b.label_text().unwrap();
        assert_eq!(label, "42");
    }

    #[test]
    fn badge_dot_after_count() {
        let b = Badge::new().count(10).dot();
        assert!(b.count.is_none());
    }

    #[test]
    fn badge_max_count_setter() {
        let b = Badge::new().max_count(9);
        assert_eq!(b.max_count, 9);
    }

    #[test]
    fn badge_default_max_count_is_99() {
        let b = Badge::default();
        assert_eq!(b.max_count, 99);
    }

    #[test]
    fn badge_label_text_none_for_dot() {
        let b = Badge::new().dot();
        assert!(b.label_text().is_none());
    }
}
