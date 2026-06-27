//! `tezzera-widgets` — built-in widgets for the TEZZERA UI framework.
//!
//! This crate is the "glue" layer: it implements concrete widgets by combining
//! `tezzera-core` (traits and element tree), `tezzera-layout` (constraint-based
//! layout), `tezzera-render` (pixel painting), and `tezzera-state` (atoms).
//!
//! # Phase 2 widget set
//!
//! | Widget | Description |
//! |---|---|
//! | [`Text`] | Plain text leaf — color and size |
//! | [`Button`] | Clickable rectangle with label and variant styling |
//! | [`TextInput`] | Single-line input with cursor and obscure mode |
//! | [`Divider`] | Horizontal or vertical separator line |
//! | [`Padding`] | Adds insets around a child widget |
//! | [`Center`] | Centers a child inside its container |
//!
//! # Quick start
//!
//! ```rust,ignore
//! use tezzera_widgets::prelude::*;
//! let btn = Button::new("Save").variant(ButtonVariant::Primary);
//! ```

pub mod button;
pub mod center;
pub mod counter_app;
pub mod divider;
pub mod padding;
pub mod prelude;
pub mod text;
pub mod text_input;

pub use prelude::*;

/// Converts a `tezzera_theme::Color` (f32 RGBA 0.0–1.0) to the
/// `tezzera_render::canvas::Color` (u8 RGBA 0–255) used by drawing methods.
pub(crate) fn theme_color_to_render(c: tezzera_theme::Color) -> tezzera_render::canvas::Color {
    tezzera_render::canvas::Color::rgba(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

// Phase 1 integration demo — kept for backward compatibility.
pub use counter_app::{render_counter_frame, run_counter_simulation};

#[cfg(test)]
mod tests {
    use super::*;

    // ── counter_app integration tests (Phase 1) ───────────────────────────────

    #[test]
    fn counter_simulation_increments_correctly() {
        let result = run_counter_simulation(5);
        assert_eq!(result, 5);
    }

    #[test]
    fn counter_simulation_starts_at_zero() {
        let result = run_counter_simulation(0);
        assert_eq!(result, 0);
    }

    #[test]
    fn render_counter_frame_returns_correct_pixel_count() {
        let pixels = render_counter_frame(0, 400, 300);
        // RGBA = 4 bytes per pixel
        assert_eq!(pixels.len(), 400 * 300 * 4);
    }

    #[test]
    fn render_counter_frame_is_not_all_zeros() {
        let pixels = render_counter_frame(3, 200, 200);
        assert!(pixels.iter().any(|&b| b != 0));
    }

    #[test]
    fn render_counter_frame_different_counts_produce_different_pixels() {
        let frame_0 = render_counter_frame(0, 200, 100);
        let frame_99 = render_counter_frame(99, 200, 100);
        // "Count: 0" vs "Count: 99" — pixels must differ
        assert_ne!(frame_0, frame_99);
    }
}
