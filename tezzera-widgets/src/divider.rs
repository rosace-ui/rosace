//! [`Divider`] — a horizontal or vertical line separator.

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::canvas::{Color, SkiaCanvas};
use tezzera_theme::ThemeData;

/// Whether the divider is laid out horizontally or vertically.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DividerDirection {
    Horizontal,
    Vertical,
}

/// A thin line used to visually separate sections of a layout.
///
/// For theme-aware rendering use [`Divider::render_themed`]. Set `color_opt`
/// via [`Divider::color_opt`] to override the theme's `outline` token.
pub struct Divider {
    pub direction: DividerDirection,
    pub color: Color,
    /// Explicit color override for [`Divider::render_themed`].
    /// When `None`, `render_themed` uses the theme's `outline` token.
    pub color_opt: Option<Color>,
    pub thickness: f32,
    pub length: f32,
}

impl Divider {
    /// Creates a horizontal divider of the given `length`.
    pub fn horizontal(length: f32) -> Self {
        Self {
            direction: DividerDirection::Horizontal,
            color: Color::rgb(55, 60, 90),
            color_opt: None,
            thickness: 1.0,
            length,
        }
    }

    /// Creates a vertical divider of the given `length`.
    pub fn vertical(length: f32) -> Self {
        Self {
            direction: DividerDirection::Vertical,
            color: Color::rgb(55, 60, 90),
            color_opt: None,
            thickness: 1.0,
            length,
        }
    }

    /// Sets the divider color (used by the legacy `render` path).
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }

    /// Sets an explicit color override used by [`Divider::render_themed`].
    ///
    /// When set, this color takes precedence over the theme's `outline` token.
    pub fn color_opt(mut self, c: Color) -> Self {
        self.color_opt = Some(c);
        self
    }

    /// Sets the divider thickness in pixels.
    pub fn thickness(mut self, t: f32) -> Self {
        self.thickness = t;
        self
    }

    /// Returns the preferred (width, height) of this divider.
    pub fn preferred_size(&self) -> (f32, f32) {
        match self.direction {
            DividerDirection::Horizontal => (self.length, self.thickness),
            DividerDirection::Vertical   => (self.thickness, self.length),
        }
    }

    /// Draws the divider onto `canvas` at `(x, y)`.
    pub fn render(&self, canvas: &mut SkiaCanvas, x: f32, y: f32) {
        let (w, h) = self.preferred_size();
        canvas.fill_rect(
            Rect {
                origin: Point { x, y },
                size: Size { width: w, height: h },
            },
            self.color,
        );
    }

    /// Draws the divider using the supplied theme (or the built-in light theme
    /// when `theme` is `None`).
    ///
    /// Color resolution order:
    /// 1. `color_opt` if set via [`Divider::color_opt`].
    /// 2. The theme's `outline` semantic color.
    pub fn render_themed(&self, canvas: &mut SkiaCanvas, x: f32, y: f32, theme: Option<&ThemeData>) {
        let default = tezzera_theme::built_in::light_theme();
        let theme = theme.unwrap_or(&default);
        let color = self
            .color_opt
            .unwrap_or_else(|| crate::theme_color_to_render(theme.colors.outline));
        let (w, h) = self.preferred_size();
        canvas.fill_rect(
            Rect {
                origin: Point { x, y },
                size: Size { width: w, height: h },
            },
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_horizontal_preferred_size_height_equals_thickness() {
        let d = Divider::horizontal(200.0).thickness(2.0);
        let (_w, h) = d.preferred_size();
        assert_eq!(h, 2.0);
    }

    #[test]
    fn divider_horizontal_preferred_size_width_equals_length() {
        let d = Divider::horizontal(200.0);
        let (w, _h) = d.preferred_size();
        assert_eq!(w, 200.0);
    }

    #[test]
    fn divider_vertical_preferred_size_width_equals_thickness() {
        let d = Divider::vertical(100.0).thickness(3.0);
        let (w, _h) = d.preferred_size();
        assert_eq!(w, 3.0);
    }

    #[test]
    fn divider_vertical_preferred_size_height_equals_length() {
        let d = Divider::vertical(100.0);
        let (_w, h) = d.preferred_size();
        assert_eq!(h, 100.0);
    }

    // ── Theme-aware tests ─────────────────────────────────────────────────────

    #[test]
    fn divider_themed_uses_outline_color() {
        let theme = tezzera_theme::built_in::light_theme();
        // light theme outline is #79747E
        let expected = crate::theme_color_to_render(theme.colors.outline);

        // A divider with no color_opt should resolve to theme outline.
        let d = Divider::horizontal(200.0);
        let resolved = d
            .color_opt
            .unwrap_or_else(|| crate::theme_color_to_render(theme.colors.outline));
        assert_eq!(resolved.r, expected.r);
        assert_eq!(resolved.g, expected.g);
        assert_eq!(resolved.b, expected.b);
    }

    #[test]
    fn divider_color_opt_overrides_theme_outline() {
        let override_color = Color::rgb(255, 0, 0);
        let d = Divider::horizontal(100.0).color_opt(override_color);
        let theme = tezzera_theme::built_in::light_theme();
        let resolved = d
            .color_opt
            .unwrap_or_else(|| crate::theme_color_to_render(theme.colors.outline));
        assert_eq!(resolved.r, 255);
        assert_eq!(resolved.g, 0);
        assert_eq!(resolved.b, 0);
    }

    #[test]
    fn divider_render_themed_does_not_panic() {
        use tezzera_render::canvas::SkiaCanvas;

        let theme = tezzera_theme::built_in::light_theme();
        let mut canvas = SkiaCanvas::new(300, 10);
        Divider::horizontal(200.0).render_themed(&mut canvas, 0.0, 0.0, Some(&theme));
    }
}
