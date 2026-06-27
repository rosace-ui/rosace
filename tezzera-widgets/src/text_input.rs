//! [`TextInput`] — a single-line text input widget with cursor rendering.

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::canvas::{Color, SkiaCanvas};
use tezzera_render::FontCache;
use tezzera_theme::ThemeData;

/// A single-line text input field.
///
/// Renders a background, a border (accent-colored when focused), the current
/// value (or placeholder), and a blinking-cursor indicator when focused.
pub struct TextInput {
    pub value: String,
    pub placeholder: String,
    pub width: f32,
    pub height: f32,
    /// When `true`, the value is displayed as bullet characters (`•`).
    pub obscure: bool,
    /// When `true`, the field draws an accent border and a cursor glyph.
    pub focused: bool,
}

impl TextInput {
    /// Creates a new [`TextInput`] with default placeholder text.
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: "Type here\u{2026}".into(),
            width: 200.0,
            height: 36.0,
            obscure: false,
            focused: false,
        }
    }

    // ── Builder methods ───────────────────────────────────────────────────────

    /// Sets the current text value.
    pub fn value(mut self, v: impl Into<String>) -> Self {
        self.value = v.into();
        self
    }

    /// Sets the placeholder string shown when the value is empty.
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }

    /// Sets the field width in pixels.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// When `true`, the value characters are replaced with `•` glyphs.
    pub fn obscure(mut self, o: bool) -> Self {
        self.obscure = o;
        self
    }

    /// When `true`, renders an accent border and a cursor indicator.
    pub fn focused(mut self, f: bool) -> Self {
        self.focused = f;
        self
    }

    // ── Layout ───────────────────────────────────────────────────────────────

    /// Returns the preferred (width, height) of this field.
    pub fn preferred_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    // ── Display helpers (pure, no canvas) ────────────────────────────────────

    /// Returns the string that should be displayed in the field (after applying
    /// `obscure`) and the color it should be drawn in.
    ///
    /// This is a pure helper used by both `render` and tests.
    pub fn display_text(&self) -> (String, Color) {
        if self.value.is_empty() {
            (self.placeholder.clone(), Color::rgba(140, 145, 175, 160))
        } else if self.obscure {
            ("\u{2022}".repeat(self.value.chars().count()), Color::rgb(240, 242, 255))
        } else {
            (self.value.clone(), Color::rgb(240, 242, 255))
        }
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    /// Draws the input field onto `canvas` at `(x, y)` using `font` for glyphs.
    pub fn render(&self, canvas: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
        let rect = Rect {
            origin: Point { x, y },
            size: Size { width: self.width, height: self.height },
        };

        // Background
        canvas.fill_rect(rect, Color::rgb(28, 30, 46));

        // Border — accent when focused
        let border = if self.focused {
            Color::rgb(100, 160, 255)
        } else {
            Color::rgb(55, 60, 90)
        };
        canvas.stroke_rect(rect, border, 1.5);

        // Text
        let font_size = 14.0_f32;
        let (text, text_color) = self.display_text();
        let ty = y + (self.height - font_size) / 2.0;
        canvas.draw_text(&text, Point { x: x + 10.0, y: ty }, text_color, font, font_size);

        // Cursor
        if self.focused {
            let cursor_x = x + 10.0 + text.chars().count() as f32 * font_size * 0.55;
            canvas.fill_rect(
                Rect {
                    origin: Point { x: cursor_x, y: y + 8.0 },
                    size: Size { width: 1.5, height: self.height - 16.0 },
                },
                Color::rgb(100, 160, 255),
            );
        }
    }

    // ── Theme-aware color helpers ─────────────────────────────────────────────

    /// Returns the border color for the current focus state, reading from `theme`.
    ///
    /// - Focused  → `theme.colors.primary`
    /// - Unfocused → `theme.colors.outline`
    pub fn border_color_themed(&self, theme: &ThemeData) -> Color {
        if self.focused {
            crate::theme_color_to_render(theme.colors.primary)
        } else {
            crate::theme_color_to_render(theme.colors.outline)
        }
    }

    /// Returns the background fill color from the theme (`surface_variant`).
    pub fn bg_color_themed(theme: &ThemeData) -> Color {
        crate::theme_color_to_render(theme.colors.surface_variant)
    }

    /// Returns the display text *and* a theme-sourced text color.
    ///
    /// - Non-empty value → `theme.colors.on_surface`
    /// - Placeholder      → `theme.colors.on_surface` at 60 % alpha
    pub fn display_text_themed(&self, theme: &ThemeData) -> (String, Color) {
        if self.value.is_empty() {
            let ph_color =
                crate::theme_color_to_render(theme.colors.on_surface.with_alpha(0.6));
            (self.placeholder.clone(), ph_color)
        } else if self.obscure {
            let text_color = crate::theme_color_to_render(theme.colors.on_surface);
            ("\u{2022}".repeat(self.value.chars().count()), text_color)
        } else {
            let text_color = crate::theme_color_to_render(theme.colors.on_surface);
            (self.value.clone(), text_color)
        }
    }

    /// Renders the input field using the supplied theme (or the built-in light
    /// theme when `theme` is `None`).
    ///
    /// Color mapping:
    /// - Background → `theme.colors.surface_variant`
    /// - Focused border → `theme.colors.primary`
    /// - Unfocused border → `theme.colors.outline`
    /// - Text color → `theme.colors.on_surface`
    /// - Placeholder color → `theme.colors.on_surface` at 60 % alpha
    /// - Cursor → `theme.colors.primary`
    pub fn render_themed(
        &self,
        canvas: &mut SkiaCanvas,
        font: &FontCache,
        x: f32,
        y: f32,
        theme: Option<&ThemeData>,
    ) {
        let default = tezzera_theme::built_in::light_theme();
        let theme = theme.unwrap_or(&default);

        let rect = Rect {
            origin: Point { x, y },
            size: Size { width: self.width, height: self.height },
        };

        // Background
        canvas.fill_rect(rect, Self::bg_color_themed(theme));

        // Border
        canvas.stroke_rect(rect, self.border_color_themed(theme), 1.5);

        // Text / placeholder
        let font_size = 14.0_f32;
        let (text, text_color) = self.display_text_themed(theme);
        let ty = y + (self.height - font_size) / 2.0;
        canvas.draw_text(&text, Point { x: x + 10.0, y: ty }, text_color, font, font_size);

        // Cursor
        if self.focused {
            let cursor_x = x + 10.0 + text.chars().count() as f32 * font_size * 0.55;
            canvas.fill_rect(
                Rect {
                    origin: Point { x: cursor_x, y: y + 8.0 },
                    size: Size { width: 1.5, height: self.height - 16.0 },
                },
                crate::theme_color_to_render(theme.colors.primary),
            );
        }
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_input_default_value_is_empty() {
        let ti = TextInput::new();
        assert!(ti.value.is_empty());
    }

    #[test]
    fn text_input_obscure_shows_bullets() {
        let ti = TextInput::new().value("abc").obscure(true);
        let (display, _) = ti.display_text();
        assert_eq!(display, "\u{2022}\u{2022}\u{2022}");
        assert!(!display.contains('a'));
    }

    #[test]
    fn text_input_no_obscure_shows_value() {
        let ti = TextInput::new().value("hello").obscure(false);
        let (display, _) = ti.display_text();
        assert_eq!(display, "hello");
    }

    #[test]
    fn text_input_empty_shows_placeholder() {
        let ti = TextInput::new().placeholder("Enter name");
        let (display, _) = ti.display_text();
        assert_eq!(display, "Enter name");
    }

    // ── Theme-aware tests ─────────────────────────────────────────────────────

    #[test]
    fn text_input_focused_border_uses_theme_primary() {
        let theme = tezzera_theme::built_in::light_theme();
        let ti = TextInput::new().focused(true);
        let border = ti.border_color_themed(&theme);
        // light theme primary is #6750A4 → r≈103
        let expected = tezzera_theme::Color::from_hex(0x6750A4);
        assert_eq!(border.r, (expected.r * 255.0) as u8);
        assert_eq!(border.g, (expected.g * 255.0) as u8);
        assert_eq!(border.b, (expected.b * 255.0) as u8);
    }

    #[test]
    fn text_input_unfocused_border_uses_theme_outline() {
        let theme = tezzera_theme::built_in::light_theme();
        let ti = TextInput::new().focused(false);
        let border = ti.border_color_themed(&theme);
        // light theme outline is #79747E
        let expected = tezzera_theme::Color::from_hex(0x79747E);
        assert_eq!(border.r, (expected.r * 255.0) as u8);
        assert_eq!(border.g, (expected.g * 255.0) as u8);
        assert_eq!(border.b, (expected.b * 255.0) as u8);
    }

    #[test]
    fn text_input_placeholder_color_has_reduced_alpha() {
        let theme = tezzera_theme::built_in::light_theme();
        let ti = TextInput::new(); // empty value → placeholder
        let (_, ph_color) = ti.display_text_themed(&theme);
        // on_surface.with_alpha(0.6) → alpha ≈ 153
        let expected_alpha = (0.6 * 255.0) as u8;
        assert_eq!(ph_color.a, expected_alpha);
    }

    #[test]
    fn text_input_render_themed_does_not_panic() {
        use tezzera_render::canvas::SkiaCanvas;
        use tezzera_render::FontCache;

        let theme = tezzera_theme::built_in::light_theme();
        let mut canvas = SkiaCanvas::new(250, 50);
        let font = FontCache::system_mono().expect("system font required for this test");
        TextInput::new()
            .value("hello")
            .focused(true)
            .render_themed(&mut canvas, &font, 5.0, 5.0, Some(&theme));
    }
}
