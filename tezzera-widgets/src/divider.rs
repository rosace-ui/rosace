//! [`Divider`] — a horizontal or vertical line separator.

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::canvas::{Color, SkiaCanvas};

/// Whether the divider is laid out horizontally or vertically.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DividerDirection {
    Horizontal,
    Vertical,
}

/// A thin line used to visually separate sections of a layout.
pub struct Divider {
    pub direction: DividerDirection,
    pub color: Color,
    pub thickness: f32,
    pub length: f32,
}

impl Divider {
    /// Creates a horizontal divider of the given `length`.
    pub fn horizontal(length: f32) -> Self {
        Self {
            direction: DividerDirection::Horizontal,
            color: Color::rgb(55, 60, 90),
            thickness: 1.0,
            length,
        }
    }

    /// Creates a vertical divider of the given `length`.
    pub fn vertical(length: f32) -> Self {
        Self {
            direction: DividerDirection::Vertical,
            color: Color::rgb(55, 60, 90),
            thickness: 1.0,
            length,
        }
    }

    /// Sets the divider color.
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
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
}
