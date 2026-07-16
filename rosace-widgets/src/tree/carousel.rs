//! `Carousel` / `PageView` (D115/Phase 32 Step 1) — full-width swipeable
//! pages, one visible at a time, with eased snap transitions and an
//! indicator-dot row.
//!
//! Gesture model (reuses the ScrollView machinery, D101/D108): the widget's
//! per-node [`rosace_scroll::ScrollController`] accumulates the horizontal
//! drag streamed through `ctx.on_press_at`; on release (`pressed()`
//! true→false, the same transition `ScrollView` keys momentum off) the
//! accumulated distance either snaps to the neighboring page (past
//! [`SWIPE_THRESHOLD`]) or springs back. Page position eases via
//! `ctx.animate_to`, the theme-global animation policy.
//!
//! Controlled or uncontrolled: pass `.page(Atom<usize>)` to own the current
//! page in app state (swipes write it back); without it the controller's
//! otherwise-unused `offset[1]` slot stores the page per render-tree node.

use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand};
use rosace_state::Atom;

use super::{avail_w, BoxedWidget, Children, LayoutCtx, PaintCtx, Widget, intersect_rect};

/// Horizontal drag distance (logical px) past which a release snaps to the
/// neighboring page instead of springing back.
const SWIPE_THRESHOLD: f32 = 60.0;

/// Indicator dot radius (logical px).
const DOT_RADIUS: f32 = 3.0;
/// Center-to-center spacing between indicator dots (logical px).
const DOT_SPACING: f32 = 14.0;
/// Gap between the dot row and the bottom edge (logical px).
const DOT_BOTTOM_MARGIN: f32 = 10.0;

/// Pure snap decision: which page a drag of `drag_dx` px releases onto.
/// Dragging left (negative dx) advances; dragging right goes back; anything
/// within `threshold` stays put. Always clamped to `0..page_count`.
fn snap_page(current: usize, drag_dx: f32, page_count: usize, threshold: f32) -> usize {
    if page_count == 0 {
        return 0;
    }
    let last = page_count - 1;
    if drag_dx <= -threshold && current < last {
        current + 1
    } else if drag_dx >= threshold && current > 0 {
        current - 1
    } else {
        current.min(last)
    }
}

/// A swipeable page container: every child is one full-width page.
pub struct Carousel {
    children: Vec<BoxedWidget>,
    /// Controlled current page; `None` = per-node internal state.
    page: Option<Atom<usize>>,
    height: f32,
    indicator: bool,
    indicator_color: Option<Color>,
}

/// Flutter-familiar alias — a `PageView` IS a [`Carousel`].
pub type PageView = Carousel;

impl Carousel {
    /// An empty carousel — add pages with [`Carousel::child`].
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            page: None,
            height: 200.0,
            indicator: true,
            indicator_color: None,
        }
    }
    /// Append one page.
    pub fn child(mut self, w: impl Widget + 'static) -> Self {
        self.children.push(Box::new(w));
        self
    }
    /// Append several pages.
    pub fn children(mut self, ws: Vec<BoxedWidget>) -> Self {
        self.children.extend(ws);
        self
    }
    /// Control the current page from app state: swipes write the new index
    /// back to the atom; external writes ease the carousel to that page.
    pub fn page(mut self, page: Atom<usize>) -> Self { self.page = Some(page); self }
    /// Fixed height in logical px (default `200.0`); width fills the parent.
    pub fn height(mut self, h: f32) -> Self { self.height = h.max(0.0); self }
    /// Hide the indicator dots.
    pub fn no_indicator(mut self) -> Self { self.indicator = false; self }
    /// Indicator dot tint — defaults to the theme's `primary` (active dot);
    /// inactive dots are the same color dimmed.
    pub fn indicator_color(mut self, c: Color) -> Self { self.indicator_color = Some(c); self }

    /// Current page index (controlled atom, or the controller's spare
    /// `offset[1]` slot when uncontrolled), clamped to the page count.
    fn current_page(&self, ctrl: &rosace_scroll::ScrollController, n: usize) -> usize {
        let raw = match &self.page {
            Some(a) => a.get(),
            None => ctrl.offset.get()[1].max(0.0) as usize,
        };
        raw.min(n.saturating_sub(1))
    }

    /// Write the page (atom or internal slot) and reset the drag distance.
    fn set_page(&self, ctrl: &rosace_scroll::ScrollController, p: usize) {
        if let Some(a) = &self.page {
            if a.get() != p { a.set(p); }
        }
        ctrl.offset.set([0.0, p as f32]);
    }
}

impl Default for Carousel {
    fn default() -> Self { Self::new() }
}

impl Widget for Carousel {
    fn children(&self) -> Children<'_> { Children::Many(&self.children) }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: avail_w(ctx.constraints), height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (the borrow must end before mutable painting).
        let dot_active = self
            .indicator_color
            .unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        let dot_inactive = Color::rgba(dot_active.r, dot_active.g, dot_active.b, 80);

        let r = ctx.rect;
        let n = self.children.len();
        let ctrl = ctx.scroll_controller();

        // Swipe input — always registered (interactive-by-identity): a drag
        // over the carousel must never fall through to pan a scroll view
        // behind it, wired pages or not.
        let drag_ctrl = ctrl.clone();
        ctx.on_press_at(move |x, y| {
            let (dx, _) = drag_ctrl.drag_delta(x, y);
            if dx != 0.0 {
                let o = drag_ctrl.offset.get();
                drag_ctrl.offset.set([o[0] + dx, o[1]]);
            }
        });

        if n == 0 {
            return;
        }

        // Release detection: pressed() true→false is the drag's end (the
        // same transition ScrollView keys its momentum hand-off on).
        let is_pressed = ctx.pressed();
        let was_pressed = ctrl.was_pressed();
        let mut cur = self.current_page(&ctrl, n);
        if !is_pressed && was_pressed {
            let dx = ctrl.offset.get()[0];
            cur = snap_page(cur, dx, n, SWIPE_THRESHOLD);
            self.set_page(&ctrl, cur);
            ctrl.end_drag();
        }
        ctrl.set_was_pressed(is_pressed);

        ctx.semantics(
            super::Semantics::new(rosace_core::Role::List)
                .label("carousel")
                .value(format!("page {} of {}", cur + 1, n)),
        );

        // Eased page position + live finger offset while dragging.
        let eased = ctx.animate_to(cur as f32, 0.0);
        let drag_dx = ctrl.offset.get()[0];
        let pw = r.size.width;

        // Pages, clipped to the viewport (only near-visible ones painted).
        ctx.record(DrawCommand::PushClip { rect: r });
        let effective_clip = ctx.clip_rect
            .and_then(|parent| intersect_rect(parent, r))
            .unwrap_or(r);
        for (i, child) in self.children.iter().enumerate() {
            let x = r.origin.x + (i as f32 - eased) * pw + drag_dx;
            if x + pw <= r.origin.x || x >= r.origin.x + pw {
                continue; // fully off-screen
            }
            let page_rect = Rect { origin: Point { x, y: r.origin.y }, size: r.size };
            let mut child_ctx = ctx.child(page_rect);
            child_ctx.clip_rect = Some(effective_clip);
            child.paint(&mut child_ctx);
        }
        ctx.record(DrawCommand::PopClip);

        // Indicator dots, bottom-center over the content.
        if self.indicator && n > 1 {
            let total_w = DOT_SPACING * (n - 1) as f32;
            let x0 = r.origin.x + (r.size.width - total_w) / 2.0;
            let cy = r.origin.y + r.size.height - DOT_BOTTOM_MARGIN - DOT_RADIUS;
            for i in 0..n {
                let color = if i == cur { dot_active } else { dot_inactive };
                ctx.fill_circle(
                    Point { x: x0 + i as f32 * DOT_SPACING, y: cy },
                    DOT_RADIUS,
                    color,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    /// A page reporting a fixed size regardless of constraints.
    struct Page;
    impl Widget for Page {
        fn layout(&self, _ctx: &LayoutCtx) -> Size { Size { width: 10.0, height: 10.0 } }
        fn paint(&self, _ctx: &mut PaintCtx) {}
    }

    fn test_env() -> (rosace_render::FontCache, rosace_theme::ThemeData) {
        (rosace_render::FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    #[test]
    fn carousel_fills_the_width_at_its_configured_height() {
        let c = Carousel::new().height(240.0).child(Page).child(Page);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(390.0, 800.0), &font, &theme);
        let size = c.layout(&ctx);
        assert_eq!((size.width, size.height), (390.0, 240.0));
    }

    #[test]
    fn default_height_is_200() {
        let c = Carousel::new().child(Page);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(320.0, 800.0), &font, &theme);
        assert_eq!(c.layout(&ctx).height, 200.0);
    }

    #[test]
    fn snap_advances_past_the_threshold_and_springs_back_within_it() {
        // Left drag past threshold advances.
        assert_eq!(snap_page(0, -80.0, 3, 60.0), 1);
        // Right drag past threshold goes back.
        assert_eq!(snap_page(2, 80.0, 3, 60.0), 1);
        // Within the threshold: stays put.
        assert_eq!(snap_page(1, -40.0, 3, 60.0), 1);
        assert_eq!(snap_page(1, 40.0, 3, 60.0), 1);
    }

    #[test]
    fn snap_clamps_at_both_ends() {
        assert_eq!(snap_page(0, 200.0, 3, 60.0), 0);   // no page before first
        assert_eq!(snap_page(2, -200.0, 3, 60.0), 2);  // no page after last
        assert_eq!(snap_page(0, -200.0, 0, 60.0), 0);  // empty carousel
        assert_eq!(snap_page(9, -10.0, 3, 60.0), 2);   // out-of-range current clamps
    }
}
