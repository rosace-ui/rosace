use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, SkiaCanvas};

use crate::physics::ScrollDirection;

/// Fractional position and size of a scrollbar thumb within its track.
pub struct ScrollbarMetrics {
    /// Thumb position as a fraction of the available track space (0.0–1.0).
    pub thumb_pos: f32,
    /// Thumb size as a fraction of the total track length (0.0–1.0).
    pub thumb_size: f32,
}

impl ScrollbarMetrics {
    /// Compute thumb metrics for a single axis.
    pub fn compute(offset: f32, content_size: f32, viewport_size: f32) -> Self {
        if content_size <= viewport_size {
            return Self {
                thumb_pos: 0.0,
                thumb_size: 1.0,
            };
        }
        let ratio = viewport_size / content_size;
        let max_offset = content_size - viewport_size;
        Self {
            thumb_pos: (offset / max_offset) * (1.0 - ratio),
            thumb_size: ratio.clamp(0.05, 1.0),
        }
    }
}

/// Render a scrollbar thumb onto `canvas` for the given viewport and scroll state.
///
/// `opacity` in `0.0–1.0` controls the thumb alpha (used for the fade-out effect).
#[allow(clippy::too_many_arguments)]
pub fn render_scrollbar(
    canvas: &mut SkiaCanvas,
    direction: ScrollDirection,
    viewport_x: f32,
    viewport_y: f32,
    viewport_w: f32,
    viewport_h: f32,
    offset: f32,
    content_size: f32,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    let alpha = (opacity * 120.0) as u8;
    let thumb_color = Color::rgba(180, 185, 210, alpha);
    const TRACK_W: f32 = 6.0;
    const MARGIN: f32 = 3.0;

    match direction {
        ScrollDirection::Vertical | ScrollDirection::Both => {
            let metrics =
                ScrollbarMetrics::compute(offset, content_size, viewport_h);
            let track_h = viewport_h - MARGIN * 2.0;
            let thumb_h = (track_h * metrics.thumb_size).max(20.0);
            let thumb_y = viewport_y + MARGIN + track_h * metrics.thumb_pos;
            let thumb_x = viewport_x + viewport_w - TRACK_W - MARGIN;
            canvas.fill_rect(
                Rect {
                    origin: Point {
                        x: thumb_x,
                        y: thumb_y,
                    },
                    size: Size {
                        width: TRACK_W,
                        height: thumb_h,
                    },
                },
                thumb_color,
            );
        }
        ScrollDirection::Horizontal => {
            let metrics =
                ScrollbarMetrics::compute(offset, content_size, viewport_w);
            let track_w = viewport_w - MARGIN * 2.0;
            let thumb_w = (track_w * metrics.thumb_size).max(20.0);
            let thumb_x = viewport_x + MARGIN + track_w * metrics.thumb_pos;
            let thumb_y = viewport_y + viewport_h - TRACK_W - MARGIN;
            canvas.fill_rect(
                Rect {
                    origin: Point {
                        x: thumb_x,
                        y: thumb_y,
                    },
                    size: Size {
                        width: thumb_w,
                        height: TRACK_W,
                    },
                },
                thumb_color,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_metrics_thumb_size_equals_ratio_when_content_exceeds_viewport() {
        let m = ScrollbarMetrics::compute(0.0, 400.0, 200.0);
        // ratio = 200/400 = 0.5
        assert!((m.thumb_size - 0.5).abs() < 1e-6);
    }

    #[test]
    fn scrollbar_metrics_thumb_size_is_one_when_content_fits() {
        let m = ScrollbarMetrics::compute(0.0, 100.0, 200.0);
        assert_eq!(m.thumb_size, 1.0);
        assert_eq!(m.thumb_pos, 0.0);
    }

    #[test]
    fn scrollbar_metrics_thumb_pos_at_max_offset() {
        // offset = max = 200 (400-200), ratio=0.5, pos should be 0.5*(1-0.5)=0.25
        let m = ScrollbarMetrics::compute(200.0, 400.0, 200.0);
        assert!((m.thumb_pos - 0.5).abs() < 1e-5);
    }

    #[test]
    fn render_scrollbar_does_not_panic_with_zero_opacity() {
        let mut canvas = SkiaCanvas::new(200, 400);
        canvas.clear(Color::WHITE);
        // Should return early without drawing anything or panicking.
        render_scrollbar(
            &mut canvas,
            ScrollDirection::Vertical,
            0.0, 0.0, 200.0, 400.0,
            50.0, 800.0, 0.0,
        );
    }

    #[test]
    fn render_scrollbar_vertical_does_not_panic() {
        let mut canvas = SkiaCanvas::new(200, 400);
        canvas.clear(Color::WHITE);
        render_scrollbar(
            &mut canvas,
            ScrollDirection::Vertical,
            0.0, 0.0, 200.0, 400.0,
            100.0, 800.0, 1.0,
        );
    }

    #[test]
    fn render_scrollbar_horizontal_does_not_panic() {
        let mut canvas = SkiaCanvas::new(400, 200);
        canvas.clear(Color::WHITE);
        render_scrollbar(
            &mut canvas,
            ScrollDirection::Horizontal,
            0.0, 0.0, 400.0, 200.0,
            50.0, 800.0, 0.8,
        );
    }
}
