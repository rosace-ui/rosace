//! `Dismissible` — swipe a list item left/right to reveal an action or
//! dismiss it (Mail/Messages-style), the other very common mobile list
//! pattern missing from the widget set. Single-child wrapper, typically
//! around a [`super::ListTile`].
//!
//! Drag model mirrors [`super::Carousel`]'s (same `ctx.on_press_at` +
//! per-node [`crate::scroll::ScrollController`] idiom, D101/D108): the
//! controller's otherwise-unused `offset[0]` slot stores the live drag
//! distance (this widget never scrolls anything, so there's no real
//! offset meaning to conflict with). On release past `threshold` (a
//! fraction of the width), the content eases fully off-screen and
//! `on_dismissed` fires once; short of that, it springs back to rest.

use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};

use super::{avail_w, BoxedWidget, Children, LayoutCtx, PaintCtx, Widget};

/// Which swipe directions dismiss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissDirection {
    /// Swipe either way.
    Horizontal,
    /// Only swiping left (content moves left, revealing the background on
    /// the right) dismisses.
    EndToStart,
    /// Only swiping right dismisses.
    StartToEnd,
}

/// Drag distance, as a fraction of width, past which a release commits to
/// dismissing instead of springing back.
const DEFAULT_THRESHOLD: f32 = 0.35;

pub struct Dismissible {
    child: BoxedWidget,
    background: Option<BoxedWidget>,
    direction: DismissDirection,
    threshold: f32,
    on_dismissed: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Accessible name for the row ("Archive message from Ana"). Defaults to
    /// a generic "Dismissible item", which is better than silence but says
    /// nothing about WHAT is being dismissed.
    semantic_label: Option<String>,
}

impl Dismissible {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            background: None,
            direction: DismissDirection::Horizontal,
            threshold: DEFAULT_THRESHOLD,
            semantic_label: None,
            on_dismissed: None,
        }
    }

    /// Custom content shown behind, revealed as the item is dragged
    /// (defaults to a red panel with a trash icon on the revealed side).
    pub fn background(mut self, w: impl Widget + 'static) -> Self {
        self.background = Some(Box::new(w));
        self
    }
    pub fn direction(mut self, d: DismissDirection) -> Self {
        self.direction = d;
        self
    }
    /// Drag distance (fraction of width, default 0.35) past which release
    /// commits to dismissing instead of springing back.
    pub fn threshold(mut self, t: f32) -> Self {
        self.threshold = t.clamp(0.05, 0.95);
        self
    }
    /// Fired once when the dismiss commits. The app is expected to remove
    /// this item from its list in response — this widget doesn't know
    /// about the list it's in, it only reports the gesture.
    pub fn on_dismissed(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismissed = Some(Arc::new(f));
        self
    }

    /// Name this row for assistive tech — what is being dismissed.
    pub fn semantic_label(mut self, l: impl Into<String>) -> Self {
        self.semantic_label = Some(l.into());
        self
    }

    fn allows(&self, dx: f32) -> bool {
        match self.direction {
            DismissDirection::Horizontal => true,
            DismissDirection::EndToStart => dx < 0.0,
            DismissDirection::StartToEnd => dx > 0.0,
        }
    }
}

impl Widget for Dismissible {
    fn children(&self) -> Children<'_> {
        Children::One(&*self.child)
    }

    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // Measured DETACHED — no node, no slot, no cache.
        //
        // `paint` slots a background BEFORE the child, but only while a swipe
        // is in progress. So the child is paint slot 0 or slot 1 depending on
        // drag state, which is decided at paint time and cannot be known here.
        // No fixed slot is correct, so this must claim none.
        // Found by `wrapper_nesting.rs`, not by anyone clicking.
        let child_size = self.child.layout(&ctx.detached());
        Size { width: avail_w(ctx.constraints), height: child_size.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Quality Bar §5. A swipe-to-delete row announced nothing, so the
        // affordance was invisible AND unreachable without a pointer.
        //
        // Same known gap as `PullToRefresh`: the dismiss ACTION cannot yet be
        // offered non-gesturally (WIDGET_FINDINGS L2). Declaring the role and
        // label at least tells a screen-reader user the row is dismissible
        // and lets the app supply a meaningful name.
        ctx.semantics(
            super::SemanticsProps::new(rosace_core::Role::Button)
                .label(self.semantic_label.as_deref().unwrap_or("Dismissible item")),
        );

        let r = ctx.rect;
        let ctrl = ctx.scroll_controller();

        let drag_ctrl = ctrl.clone();
        ctx.on_press_at(move |x, y| {
            let (dx, _) = drag_ctrl.drag_delta(x, y);
            if dx != 0.0 {
                let o = drag_ctrl.offset.get();
                drag_ctrl.offset.set([o[0] + dx, o[1]]);
            }
        });

        let is_pressed = ctx.pressed();
        let was_pressed = ctrl.was_pressed();
        let mut dx = ctrl.offset.get()[0];

        if !is_pressed && was_pressed {
            let commit = dx.abs() >= r.size.width * self.threshold && self.allows(dx);
            if commit {
                let target = if dx < 0.0 { -r.size.width } else { r.size.width };
                ctrl.offset.set([target, 0.0]);
                dx = target;
                if let Some(cb) = &self.on_dismissed {
                    cb();
                }
            } else {
                ctrl.offset.set([0.0, 0.0]);
                dx = 0.0;
            }
            ctrl.end_drag();
        }
        ctrl.set_was_pressed(is_pressed);

        if dx.abs() > 0.001 {
            ctx.request_animation();
        }

        // Background revealed behind the sliding content.
        if dx.abs() > 0.001 {
            match &self.background {
                Some(bg) => ctx.paint_child(r, &**bg),
                None => draw_default_background(ctx, r, dx),
            }
        }

        let child_rect = Rect { origin: Point { x: r.origin.x + dx, y: r.origin.y }, size: r.size };
        ctx.record(rosace_render::DrawCommand::PushClip { rect: r });
        ctx.paint_child(child_rect, &*self.child);
        ctx.record(rosace_render::DrawCommand::PopClip);
    }
}

/// Default background: a red panel with a trash icon anchored to the side
/// being revealed (the side opposite the drag direction).
fn draw_default_background(ctx: &mut PaintCtx, r: Rect, dx: f32) {
    // The destructive affordance follows the theme's error tokens, so an
    // app with its own palette gets its own red rather than this one.
    let red = ctx.tc(ctx.theme.colors.error);
    ctx.fill_rect(r, red);
    const ICON: f32 = 22.0;
    let cy = r.origin.y + (r.size.height - ICON) / 2.0;
    let cx = if dx < 0.0 {
        r.origin.x + r.size.width - ICON - 18.0 // revealed on the right
    } else {
        r.origin.x + 18.0 // revealed on the left
    };
    let on_red = ctx.tc(ctx.theme.colors.on_error);
    let icon = super::Icon::new(super::IconKind::Trash).size(ICON).color(on_red);
    ctx.paint_child(Rect { origin: Point { x: cx, y: cy }, size: Size { width: ICON, height: ICON } }, &icon);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    struct Row;
    impl Widget for Row {
        fn layout(&self, _ctx: &LayoutCtx) -> Size {
            Size { width: 300.0, height: 56.0 }
        }
        fn paint(&self, _ctx: &mut PaintCtx) {}
    }

    fn test_env() -> (rosace_render::FontCache, rosace_theme::ThemeData) {
        (rosace_render::FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    #[test]
    fn height_matches_child_width_fills_parent() {
        let d = Dismissible::new(Row);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(390.0, 800.0), &font, &theme);
        let size = d.layout(&ctx);
        assert_eq!((size.width, size.height), (390.0, 56.0));
    }

    #[test]
    fn allows_respects_direction() {
        let both = Dismissible::new(Row);
        assert!(both.allows(-50.0) && both.allows(50.0));

        let end_to_start = Dismissible::new(Row).direction(DismissDirection::EndToStart);
        assert!(end_to_start.allows(-50.0) && !end_to_start.allows(50.0));

        let start_to_end = Dismissible::new(Row).direction(DismissDirection::StartToEnd);
        assert!(!start_to_end.allows(-50.0) && start_to_end.allows(50.0));
    }

    #[test]
    fn threshold_clamps_to_sane_range() {
        let d = Dismissible::new(Row).threshold(5.0);
        assert!(d.threshold <= 0.95);
        let d2 = Dismissible::new(Row).threshold(-1.0);
        assert!(d2.threshold >= 0.05);
    }
}
