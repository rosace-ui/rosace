//! [`Button`] — a pressable widget with a label and optional variant styling.

use tezzera_core::{element::Element, types::{Point, Rect, Size}};
use tezzera_render::canvas::{Color, SkiaCanvas};
use tezzera_render::FontCache;
use tezzera_theme::ThemeData;

/// Visual style variant for a [`Button`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonVariant {
    /// Solid accent-blue background — the default call-to-action style.
    Primary,
    /// Muted dark background for secondary actions.
    Secondary,
    /// Red background for destructive actions.
    Danger,
    /// Transparent background with an accent border.
    Ghost,
}

/// A rectangular button with a text label.
///
/// Supports two rendering APIs:
/// - Phase 1 (`paint`) — placeholder rectangle drawn via [`SkiaCanvas::fill_rect`].
/// - Phase 2 (`render`) — real glyph rendering using a [`FontCache`].
///
/// Pressing the button calls the closure registered with [`Button::on_press`].
/// When [`Button::disabled`] is `true`, [`Button::fire_press`] is a no-op.
pub struct Button {
    pub label: String,
    on_press: Option<Box<dyn Fn() + Send + Sync>>,
    background: Color,
    foreground: Color,
    padding: f32,
    pub disabled: bool,
    pub variant: ButtonVariant,
    pub width: f32,
    pub height: f32,
}

impl Button {
    /// Creates a new [`Button`] with the given label and default styling.
    ///
    /// Default variant is [`ButtonVariant::Primary`], width 120 px, height 40 px.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            on_press: None,
            background: Color::rgb(70, 130, 200),
            foreground: Color::WHITE,
            padding: 8.0,
            disabled: false,
            variant: ButtonVariant::Primary,
            width: 120.0,
            height: 40.0,
        }
    }

    // ── Builder methods ───────────────────────────────────────────────────────

    /// Registers a callback that is invoked when the button is pressed.
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }

    /// Sets the button's background color (used by Phase 1 `paint`).
    pub fn background(mut self, c: Color) -> Self {
        self.background = c;
        self
    }

    /// Sets the text / foreground color (used by Phase 1 `paint`).
    pub fn foreground(mut self, c: Color) -> Self {
        self.foreground = c;
        self
    }

    /// When `true`, [`fire_press`](Self::fire_press) becomes a no-op.
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    /// Sets the visual variant (affects colors in [`render`](Self::render)).
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Sets the button width in pixels.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Sets the button height in pixels.
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    // ── Size helpers ──────────────────────────────────────────────────────────

    /// Returns the button's approximate natural size (Phase 1).
    pub fn natural_size(&self) -> Size {
        let text_w = self.label.len() as f32 * 8.0;
        Size { width: text_w + self.padding * 2.0, height: 16.0 + self.padding * 2.0 }
    }

    /// Returns the button's preferred (width, height) for Phase 2 layout.
    pub fn preferred_size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    // ── Color helpers ─────────────────────────────────────────────────────────

    /// Returns the background color for the current variant / disabled state.
    pub fn bg_color(&self) -> Color {
        if self.disabled {
            return Color::rgba(100, 100, 120, 128);
        }
        match self.variant {
            ButtonVariant::Primary   => Color::rgb(100, 160, 255),
            ButtonVariant::Secondary => Color::rgb(55, 60, 90),
            ButtonVariant::Danger    => Color::rgb(220, 70, 70),
            ButtonVariant::Ghost     => Color::rgba(0, 0, 0, 0),
        }
    }

    /// Returns the label text color for the current variant / disabled state.
    pub fn text_color(&self) -> Color {
        if self.disabled {
            return Color::rgba(180, 180, 200, 128);
        }
        match self.variant {
            ButtonVariant::Ghost => Color::rgb(100, 160, 255),
            _ => Color::WHITE,
        }
    }

    // ── Theme-aware color helpers ─────────────────────────────────────────────

    /// Returns the background color for the current variant / disabled state,
    /// reading values from the provided [`ThemeData`].
    pub fn bg_color_themed(&self, theme: &ThemeData) -> Color {
        if self.disabled {
            return crate::theme_color_to_render(theme.colors.on_surface.with_alpha(0.38));
        }
        match self.variant {
            ButtonVariant::Primary   => crate::theme_color_to_render(theme.colors.primary),
            ButtonVariant::Secondary => crate::theme_color_to_render(theme.colors.secondary),
            ButtonVariant::Danger    => crate::theme_color_to_render(theme.colors.error),
            ButtonVariant::Ghost     => Color::TRANSPARENT,
        }
    }

    /// Returns the label text color for the current variant / disabled state,
    /// reading values from the provided [`ThemeData`].
    pub fn text_color_themed(&self, theme: &ThemeData) -> Color {
        if self.disabled {
            return crate::theme_color_to_render(theme.colors.on_surface.with_alpha(0.38));
        }
        match self.variant {
            ButtonVariant::Primary   => crate::theme_color_to_render(theme.colors.on_primary),
            ButtonVariant::Secondary => crate::theme_color_to_render(theme.colors.on_secondary),
            ButtonVariant::Danger    => crate::theme_color_to_render(theme.colors.on_error),
            ButtonVariant::Ghost     => crate::theme_color_to_render(theme.colors.primary),
        }
    }

    /// Renders the button using the supplied theme (or the built-in light theme
    /// when `theme` is `None`). Adds a themed border for [`ButtonVariant::Ghost`].
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
        canvas.fill_rect(rect, self.bg_color_themed(theme));

        if self.variant == ButtonVariant::Ghost {
            canvas.stroke_rect(rect, crate::theme_color_to_render(theme.colors.primary), 1.5);
        }

        let fs = self.label_font_size();
        let lw = self.label.len() as f32 * fs * 0.55;
        let tx = x + (self.width - lw) / 2.0;
        let ty = y + (self.height - fs) / 2.0;
        canvas.draw_text(
            &self.label,
            Point { x: tx, y: ty },
            self.text_color_themed(theme),
            font,
            fs,
        );
    }

    // ── Font size (internal) ──────────────────────────────────────────────────

    fn label_font_size(&self) -> f32 {
        14.0
    }

    // ── Rendering ────────────────────────────────────────────────────────────

    /// Phase 1: paints the button onto `canvas` at `origin` with the given `size`.
    pub fn paint(&self, canvas: &mut SkiaCanvas, origin: Point, size: Size) {
        let bg = if self.disabled { Color::rgb(180, 180, 180) } else { self.background };
        canvas.fill_rect(Rect { origin, size }, bg);
        let text_origin = Point {
            x: origin.x + self.padding,
            y: origin.y + self.padding,
        };
        canvas.draw_text_placeholder(&self.label, text_origin, self.foreground);
    }

    /// Phase 2: renders the button with variant colors and real glyphs at `(x, y)`.
    pub fn render(&self, canvas: &mut SkiaCanvas, font: &FontCache, x: f32, y: f32) {
        let rect = Rect {
            origin: Point { x, y },
            size: Size { width: self.width, height: self.height },
        };
        canvas.fill_rect(rect, self.bg_color());

        if self.variant == ButtonVariant::Ghost {
            canvas.stroke_rect(rect, Color::rgb(100, 160, 255), 1.5);
        }

        // Center the label horizontally and vertically.
        let fs = self.label_font_size();
        let lw = self.label.len() as f32 * fs * 0.55;
        let tx = x + (self.width - lw) / 2.0;
        let ty = y + (self.height - fs) / 2.0;
        canvas.draw_text(&self.label, Point { x: tx, y: ty }, self.text_color(), font, fs);
    }

    // ── Event handling ────────────────────────────────────────────────────────

    /// Fires the registered `on_press` callback if the button is not disabled.
    pub fn fire_press(&self) {
        if !self.disabled {
            if let Some(f) = &self.on_press {
                f();
            }
        }
    }
}

impl From<Button> for Element {
    fn from(b: Button) -> Element {
        Element::Native(tezzera_core::element::NativeElement {
            tag: "button",
            children: vec![Element::text(b.label)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_new_default_variant_is_primary() {
        let b = Button::new("OK");
        assert_eq!(b.variant, ButtonVariant::Primary);
    }

    #[test]
    fn button_bg_color_primary_is_opaque() {
        let b = Button::new("OK");
        let c = b.bg_color();
        assert_eq!(c.a, 255);
    }

    #[test]
    fn button_bg_color_disabled_is_translucent() {
        let b = Button::new("OK").disabled(true);
        let c = b.bg_color();
        assert!(c.a < 255, "disabled button should have alpha < 255, got {}", c.a);
    }

    #[test]
    fn button_bg_color_ghost_is_transparent() {
        let b = Button::new("OK").variant(ButtonVariant::Ghost);
        let c = b.bg_color();
        assert_eq!(c.a, 0);
    }

    #[test]
    fn button_fires_on_press() {
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
        let fired = Arc::new(AtomicBool::new(false));
        let fired2 = fired.clone();
        let btn = Button::new("Click").on_press(move || fired2.store(true, Ordering::SeqCst));
        btn.fire_press();
        assert!(fired.load(Ordering::SeqCst));
    }

    #[test]
    fn disabled_button_does_not_fire() {
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
        let fired = Arc::new(AtomicBool::new(false));
        let fired2 = fired.clone();
        let btn = Button::new("Click")
            .on_press(move || fired2.store(true, Ordering::SeqCst))
            .disabled(true);
        btn.fire_press();
        assert!(!fired.load(Ordering::SeqCst));
    }

    // ── Theme-aware tests ─────────────────────────────────────────────────────

    #[test]
    fn button_primary_bg_uses_theme_primary_color() {
        let theme = tezzera_theme::built_in::light_theme();
        let b = Button::new("OK").variant(ButtonVariant::Primary);
        let c = b.bg_color_themed(&theme);
        // light theme primary is #6750A4 → r=103, g=80, b=164
        let expected = tezzera_theme::Color::from_hex(0x6750A4);
        assert_eq!(c.r, (expected.r * 255.0) as u8);
        assert_eq!(c.g, (expected.g * 255.0) as u8);
        assert_eq!(c.b, (expected.b * 255.0) as u8);
        assert_eq!(c.a, 255);
    }

    #[test]
    fn button_danger_bg_uses_theme_error_color() {
        let theme = tezzera_theme::built_in::light_theme();
        let b = Button::new("Delete").variant(ButtonVariant::Danger);
        let c = b.bg_color_themed(&theme);
        // light theme error is #B3261E → r=179, g=38, b=30
        let expected = tezzera_theme::Color::from_hex(0xB3261E);
        assert_eq!(c.r, (expected.r * 255.0) as u8);
        assert_eq!(c.g, (expected.g * 255.0) as u8);
        assert_eq!(c.b, (expected.b * 255.0) as u8);
    }

    #[test]
    fn button_disabled_uses_muted_color() {
        let theme = tezzera_theme::built_in::light_theme();
        let b = Button::new("Disabled").disabled(true);
        let bg = b.bg_color_themed(&theme);
        let txt = b.text_color_themed(&theme);
        // Both disabled colors carry on_surface with 0.38 alpha → alpha ≈ 97
        let expected_alpha = (0.38 * 255.0) as u8;
        assert_eq!(bg.a, expected_alpha);
        assert_eq!(txt.a, expected_alpha);
    }
}
