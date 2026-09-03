//! `PullToRefresh` — wraps content with the pull-down-to-refresh gesture,
//! the standard mobile pattern for reloading a list/feed.
//!
//! Doesn't reimplement a scrollable itself: it's a
//! [`super::register_nested_scroll`] LINK (D-NESTED-SCROLL, the same chain
//! mechanism `ScrollView` composes with) around whatever `child` is. If
//! `child` is itself scrollable (a `ListView`/`ScrollView`), it only hands
//! this node the leftover drag once it's exhausted at its own top; if
//! `child` is plain content, this node gets the whole gesture directly —
//! either way `PullToRefresh` only ever owns a one-sided "pulled past the
//! top" offset, using the exact same `ScrollController` physics
//! (`try_apply_delta`/`coast`/`settle_bounce`) `ScrollView` itself runs.
//! `viewport_size`/`content_size` are deliberately left unpublished (their
//! default `[0, 0]`), which makes that bound math always resolve to
//! "spring back to exactly 0" — there is no real scroll extent here, just
//! a pull distance.

use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand};
use crate::scroll::ScrollPhysics;

use super::{avail_h, avail_w, intersect_rect, BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

/// Pull distance (logical px) past which a release triggers `on_refresh`.
const TRIGGER_DISTANCE: f32 = 70.0;

/// How much of the finger's travel becomes pull. Below 1.0 so the sheet feels
/// weighted rather than glued to the cursor, but nothing like Bounce's
/// distance-proportional resistance, which made the trigger unreachable.
const PULL_DAMPING: f32 = 0.5;
/// Indicator diameter (logical px).
const INDICATOR_SIZE: f32 = 32.0;
/// Gap between the indicator and the top edge once it's fully revealed.
const INDICATOR_TOP_MARGIN: f32 = 16.0;
/// Same rubber-band shape `ScrollView` uses under `Bounce`.
const PHYSICS: ScrollPhysics = ScrollPhysics::Bounce { friction: 0.88, spring_stiffness: 12.0 };

/// Wraps `child` (typically a `ListView`/`Column`) with pull-to-refresh.
pub struct PullToRefresh {
    child: BoxedWidget,
    on_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
    refreshing: bool,
    color: Option<Color>,
}

impl PullToRefresh {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self { child: Arc::new(child), on_refresh: None, refreshing: false, color: None }
    }

    /// Fired once when the user releases past the trigger distance. Typical
    /// use: flip an `Atom<bool>` (fed back via `.refreshing(..)`) and kick
    /// off async work that flips it back when done.
    pub fn on_refresh(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_refresh = Some(Arc::new(f));
        self
    }

    /// Whether a refresh is in flight — shows a spinning (indeterminate)
    /// indicator instead of the pull-progress ring while `true`.
    pub fn refreshing(mut self, v: bool) -> Self {
        self.refreshing = v;
        self
    }

    /// Indicator tint — defaults to the theme's `primary`.
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
}

impl Widget for PullToRefresh {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.child)
    }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let (w, h) = (avail_w(ctx.constraints), avail_h(ctx.constraints));
        // On an axis nobody bounded, "fill everything available" is infinity and
        // has no finite answer — the same case `ScrollView::layout` handles.
        // Size to the content instead.
        let h = if h.is_finite() {
            h
        } else {
            ctx.layout_child_uncached(
                rosace_layout::Constraints::loose(w, f32::INFINITY),
                &*self.child,
            ).height
        };
        Size { width: w, height: h }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Quality Bar §5. This announced nothing at all: neither that the
        // surface can be refreshed, nor that a refresh is currently running.
        //
        // `refreshing` is exposed as a value so a screen reader can say
        // "busy" while the spinner is up — that state was visible only as a
        // spinning arc, which conveys nothing non-visually.
        //
        // KNOWN GAP: there is still no non-gestural way to TRIGGER a
        // refresh. A real a11y action needs the action plumbing that is
        // currently a no-op on every platform (WIDGET_FINDINGS L2); until
        // that lands, this announces the affordance without being able to
        // offer it. Named rather than papered over.
        let mut sem = super::SemanticsProps::new(rosace_core::Role::Button)
            .label("Refresh");
        if self.refreshing {
            sem = sem.value("refreshing");
        }
        ctx.semantics(sem);

        let r = ctx.rect;
        let color = self.color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.primary));
        let ctrl = ctx.scroll_controller();

        let drag_ctrl = ctrl.clone();
        // A pull IS overscroll — the offset goes negative, past the top. So
        // this declines the first pass entirely: anything that can scroll
        // normally should do so before a drag becomes a refresh gesture.
        ctx.register_nested_scroll(move |_dx, dy, allow_overscroll| {
            if !allow_overscroll || dy == 0.0 {
                return false;
            }
            // Applied DIRECTLY, not through `try_apply_delta`.
            //
            // That routes the delta through Bounce's rubber-band resistance,
            // which is right for a scroll view being dragged past its end and
            // wrong here: the resistance is proportional to how far out you
            // already are, so the pull asymptotes. Measured before this: a
            // 220px drag produced 20px of pull against a 70px trigger, i.e.
            // the gesture could not physically fire.
            //
            // A pull follows the finger, lightly damped so it still feels
            // weighted. Only downward-at-the-top pulls; upward past zero is
            // just a scroll and belongs to the child.
            let prev = drag_ctrl.offset()[1];
            let next = (prev - dy * PULL_DAMPING).min(0.0);
            if next == prev {
                return false;
            }
            drag_ctrl.scroll_to_raw([drag_ctrl.offset()[0], next]);
            true
        });

        let dt = rosace_animate::frame_dt().max(0.0001);
        let is_pressed = ctx.pressed();
        let was_pressed = ctrl.was_pressed();
        if is_pressed {
            // A press that begins after a wheel scroll, `scroll_to` or
            // `reveal` must not measure that movement as this gesture's
            // speed — see `begin_velocity_sample`.
            if !was_pressed { ctrl.begin_velocity_sample(); }
            ctrl.track_velocity(dt);
        } else {
            if was_pressed { ctrl.end_drag(); }
            if ctrl.coast(PHYSICS, dt) {
                ctx.request_animation();
            }
        }
        let released_this_frame = was_pressed && !is_pressed;
        ctrl.set_was_pressed(is_pressed);

        let pull = (-ctrl.offset()[1]).max(0.0);

        if released_this_frame && !self.refreshing && pull >= TRIGGER_DISTANCE {
            if let Some(cb) = &self.on_refresh {
                cb();
            }
        }

        // Content translates down by the pull distance, revealing the
        // indicator above it — the standard mobile pull-to-refresh visual.
        let child_rect = Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + pull },
            size: r.size,
        };
        ctx.record(DrawCommand::PushClip { rect: r });
        let effective_clip = ctx.clip_rect.and_then(|p| intersect_rect(p, r)).unwrap_or(r);
        let mut child_ctx = ctx.child(child_rect);
        child_ctx.set_clip(Some(effective_clip));
        self.child.paint(&mut child_ctx);

        // The indicator, INSIDE the clip and after the content so it sits on
        // top of it.
        //
        // It is positioned above the widget's own origin and slides down into
        // view as the pull grows — which only reads as "sliding in" if the
        // part still above the top is hidden. Drawn after `PopClip` it was
        // simply unclipped, so at small pull distances it painted over
        // whatever sat above: reported as the refresh spinner appearing on
        // top of the AppBar.

        if self.refreshing {
            let cx = r.origin.x + r.size.width / 2.0;
            let cy = r.origin.y + INDICATOR_TOP_MARGIN + INDICATOR_SIZE / 2.0;
            draw_indicator(ctx, Point { x: cx, y: cy }, None, color);
            ctx.request_animation();
        } else if pull > 0.0 {
            let progress = (pull / TRIGGER_DISTANCE).min(1.0);
            let travel = pull.min(TRIGGER_DISTANCE + INDICATOR_TOP_MARGIN);
            let cx = r.origin.x + r.size.width / 2.0;
            let cy = r.origin.y - INDICATOR_SIZE / 2.0 + travel;
            draw_indicator(ctx, Point { x: cx, y: cy }, Some(progress), color);
        }
        ctx.record(DrawCommand::PopClip);
    }
}

/// Draws the indicator ring directly (rather than delegating to a real
/// `CircularProgress` child widget) since its center is computed from the
/// live pull distance every frame, not a layout slot.
fn draw_indicator(ctx: &mut PaintCtx, center: Point, progress: Option<f32>, color: Color) {
    const THICKNESS: f32 = 3.0;
    let radius = (INDICATOR_SIZE - THICKNESS) / 2.0;
    let track = Color::rgba(color.r, color.g, color.b, 40);
    ctx.fill_arc(center, radius, THICKNESS, 0.0, 360.0, track);
    match progress {
        Some(p) if p > 0.0 => {
            ctx.fill_arc(center, radius, THICKNESS, -90.0, 360.0 * p, color);
        }
        Some(_) => {}
        None => {
            let t = super::anim_clock();
            let start = (t * 360.0) % 360.0;
            ctx.fill_arc(center, radius, THICKNESS, start, 270.0, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    struct Filler;
    impl Widget for Filler {
        fn layout(&self, ctx: &LayoutCtx) -> Size {
            Size { width: avail_w(ctx.constraints), height: 2000.0 }
        }
        fn paint(&self, _ctx: &mut PaintCtx) {}
    }

    fn test_env() -> (rosace_render::FontCache, rosace_theme::ThemeData) {
        (rosace_render::FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    #[test]
    fn fills_available_space() {
        let w = PullToRefresh::new(Filler);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::tight(390.0, 800.0), &font, &theme);
        let size = w.layout(&ctx);
        assert_eq!((size.width, size.height), (390.0, 800.0));
    }

    #[test]
    fn builders_set_state() {
        let w = PullToRefresh::new(Filler).refreshing(true).color(Color::rgb(1, 2, 3));
        assert!(w.refreshing);
        assert_eq!(w.color, Some(Color::rgb(1, 2, 3)));
    }
}
