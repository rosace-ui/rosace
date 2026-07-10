use rosace_render::SkiaCanvas;

use crate::{
    controller::ScrollController,
    physics::{MomentumState, ScrollDirection, ScrollPhysics},
    scrollbar::render_scrollbar,
};

/// A scrollable viewport widget.
///
/// The caller is responsible for rendering content at an origin offset by
/// `scroll_offset()`. `ScrollView` manages momentum physics, clamping,
/// scrollbar visibility, and fade-out.
pub struct ScrollView {
    pub direction: ScrollDirection,
    pub physics: ScrollPhysics,
    pub show_scrollbar: bool,
    pub controller: ScrollController,
    pub content_width: f32,
    pub content_height: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    momentum: MomentumState,
    scrollbar_opacity: f32,
    scrollbar_fade_timer: f32,
}

impl ScrollView {
    /// Create a new `ScrollView` sized to the given viewport dimensions.
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        let controller = ScrollController::new();
        controller.viewport_size.set([viewport_w, viewport_h]);
        Self {
            direction: ScrollDirection::Vertical,
            physics: ScrollPhysics::default(),
            show_scrollbar: true,
            controller,
            content_width: viewport_w,
            content_height: viewport_h,
            viewport_width: viewport_w,
            viewport_height: viewport_h,
            momentum: MomentumState::new(),
            scrollbar_opacity: 0.0,
            scrollbar_fade_timer: 0.0,
        }
    }

    // Builder-style setters -----------------------------------------------

    pub fn direction(mut self, d: ScrollDirection) -> Self {
        self.direction = d;
        self
    }

    pub fn physics(mut self, p: ScrollPhysics) -> Self {
        self.physics = p;
        self
    }

    pub fn show_scrollbar(mut self, s: bool) -> Self {
        self.show_scrollbar = s;
        self
    }

    pub fn content_size(mut self, w: f32, h: f32) -> Self {
        self.content_width = w;
        self.content_height = h;
        self.controller.content_size.set([w, h]);
        self
    }

    // Frame update ---------------------------------------------------------

    /// Call each frame (with elapsed seconds `dt`) to advance momentum and fade
    /// the scrollbar out after a period of inactivity.
    pub fn tick(&mut self, dt: f32) {
        let (dx, dy) = self.momentum.tick(self.physics);
        if dx.abs() > 0.0 || dy.abs() > 0.0 {
            self.controller.scroll_by(dx, dy);
            self.scrollbar_opacity = 1.0;
            self.scrollbar_fade_timer = 1.5;
        }
        // Fade scrollbar after 1.5 s of inactivity.
        if self.scrollbar_fade_timer > 0.0 {
            self.scrollbar_fade_timer -= dt;
            if self.scrollbar_fade_timer <= 0.0 {
                self.scrollbar_opacity = (self.scrollbar_opacity - dt * 2.0).max(0.0);
            }
        }
    }

    // Input ----------------------------------------------------------------

    /// Call on a pointer/mouse drag event with the delta since the last event.
    pub fn on_scroll(&mut self, dx: f32, dy: f32) {
        self.momentum.push(dx, dy);
        self.controller.scroll_by(dx, dy);
        self.scrollbar_opacity = 1.0;
        self.scrollbar_fade_timer = 1.5;
    }

    // Rendering ------------------------------------------------------------

    /// Render the scroll chrome (scrollbar only).
    ///
    /// Content must be rendered by the caller, translated by `-scroll_offset()`.
    pub fn render_chrome(&self, canvas: &mut SkiaCanvas, x: f32, y: f32) {
        if !self.show_scrollbar {
            return;
        }
        let [ox, oy] = self.controller.offset();
        match self.direction {
            ScrollDirection::Vertical | ScrollDirection::Both => {
                render_scrollbar(
                    canvas,
                    ScrollDirection::Vertical,
                    x,
                    y,
                    self.viewport_width,
                    self.viewport_height,
                    oy,
                    self.content_height,
                    self.scrollbar_opacity,
                );
            }
            ScrollDirection::Horizontal => {
                render_scrollbar(
                    canvas,
                    ScrollDirection::Horizontal,
                    x,
                    y,
                    self.viewport_width,
                    self.viewport_height,
                    ox,
                    self.content_width,
                    self.scrollbar_opacity,
                );
            }
        }
    }

    // Accessors ------------------------------------------------------------

    /// Returns the current `[offset_x, offset_y]` to subtract from the content
    /// drawing origin.
    pub fn scroll_offset(&self) -> [f32; 2] {
        self.controller.offset()
    }

    pub fn controller(&self) -> &ScrollController {
        &self.controller
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_view_on_scroll_updates_offset() {
        let mut sv = ScrollView::new(300.0, 400.0).content_size(300.0, 1200.0);
        sv.on_scroll(0.0, 50.0);
        let [_x, y] = sv.scroll_offset();
        assert_eq!(y, 50.0);
    }

    #[test]
    fn scroll_view_on_scroll_clamps_to_content_bounds() {
        let mut sv = ScrollView::new(300.0, 400.0).content_size(300.0, 600.0);
        // max_y = 600 - 400 = 200
        sv.on_scroll(0.0, 9999.0);
        let [_x, y] = sv.scroll_offset();
        assert_eq!(y, 200.0);
    }

    #[test]
    fn scroll_view_tick_advances_momentum() {
        let mut sv = ScrollView::new(300.0, 400.0)
            .content_size(300.0, 2000.0)
            .physics(ScrollPhysics::Momentum { friction: 0.92 });
        sv.on_scroll(0.0, 100.0);
        // tick consumes the velocity
        let before = sv.scroll_offset()[1];
        sv.tick(0.016);
        let after = sv.scroll_offset()[1];
        // After tick the position should have moved further.
        assert!(after >= before);
    }

    #[test]
    fn scroll_view_render_chrome_does_not_panic() {
        let mut canvas = SkiaCanvas::new(300, 400);
        canvas.clear(rosace_render::Color::WHITE);
        let sv = ScrollView::new(300.0, 400.0).content_size(300.0, 800.0);
        sv.render_chrome(&mut canvas, 0.0, 0.0);
    }

    #[test]
    fn scroll_view_controller_accessible() {
        let sv = ScrollView::new(300.0, 400.0).content_size(300.0, 800.0);
        let c = sv.controller();
        // Default offset is zero.
        assert_eq!(c.offset(), [0.0, 0.0]);
    }
}
