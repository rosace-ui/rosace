use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::{Color, DrawCommand};
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h, intersect_rect, ScrollAxes};

/// A virtualized vertical list (RecyclerView / FlatList model).
///
/// Rows are built ON DEMAND by the `builder` closure — for a 1,000-item list
/// only the rows intersecting the viewport are built, laid out, and painted
/// each frame (typically 10–20). Memory and paint cost are O(visible), not
/// O(count).
///
/// v1 uses a fixed `item_extent` (like Flutter's `itemExtent` /
/// RecyclerView's fixed row height): the scroll geometry is pure arithmetic,
/// no measurement of off-screen rows ever happens. Variable-extent rows are
/// a future extension.
///
/// Prefer this over [`super::ScrollView`] for long lists — `ScrollView`'s
/// GPU-composited layer path (D090) composites its ENTIRE content as one
/// texture, capped at [`super::MAX_TL_DIM`] (4096 logical px); content past
/// that silently falls back to a plain (still correct, non-GPU) paint.
/// `ListView` never materializes off-screen content at all, so it has no
/// such limit regardless of `count`.
///
/// ```rust,ignore
/// let scroll = ctx.state(0.0f32);
/// ListView::builder(1_000, 48.0, scroll, |i| {
///     Box::new(ListTile::new(format!("Row {i}")))
/// })
/// ```
pub struct ListView {
    /// `None` = rows flush to the viewport edges.
    pub padding: Option<super::EdgeInsets>,
    count: usize,
    item_extent: f32,
    builder: Arc<dyn Fn(usize) -> BoxedWidget + Send + Sync>,
    pub show_scrollbar: bool,
    /// `None` = the theme's `outline`. This used to be a hardcoded
    /// dark-theme blue-grey that was wrong in a light theme.
    pub scrollbar_color: Option<Color>,
}

impl ListView {
    /// A virtualized list of `count` rows, each `item_extent` logical pixels
    /// tall. Scroll position is implicit per-node state (D101).
    pub fn builder(
        count: usize,
        item_extent: f32,
        builder: impl Fn(usize) -> BoxedWidget + Send + Sync + 'static,
    ) -> Self {
        Self {
            count,
            item_extent: item_extent.max(1.0),
            builder: Arc::new(builder),
            show_scrollbar: true,
            scrollbar_color: None,
            padding: None,
        }
    }

    pub fn no_scrollbar(mut self) -> Self { self.show_scrollbar = false; self }
    /// Override the scrollbar thumb colour (defaults to the theme's
    /// `outline`). Was reachable only by assigning the public field, which
    /// breaks the builder chain.
    pub fn scrollbar_color(mut self, c: Color) -> Self { self.scrollbar_color = Some(c); self }
    /// Inset the rows from the viewport edges. Rows were laid out flush, so
    /// a list could not be padded without wrapping every row.
    pub fn padding(mut self, p: super::EdgeInsets) -> Self { self.padding = Some(p); self }
}

/// The half-open range of row indices that intersect the viewport.
///
/// Extracted from `paint` so it can be tested directly: this arithmetic is
/// the entire point of a virtualized list, and getting it wrong either drops
/// visible rows or quietly builds thousands of invisible ones.
///
/// `scroll` may be NEGATIVE during an overscroll bounce, which is why `first`
/// clamps before the cast — an `as usize` on a negative float saturates to 0
/// on some paths and is a trap worth not relying on.
fn visible_window(scroll: f32, viewport_h: f32, item_extent: f32, count: usize) -> (usize, usize) {
    if item_extent <= 0.0 || count == 0 {
        return (0, 0);
    }
    let first = (scroll / item_extent).floor().max(0.0) as usize;
    let last = (((scroll + viewport_h) / item_extent).ceil().max(0.0) as usize).min(count);
    (first.min(last), last)
}

impl Widget for ListView {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // The list is a viewport: it fills the available space; content
        // height is virtual (count × extent) and never materialized.
        let c = ctx.constraints;
        let h = avail_h(c);
        Size {
            width: avail_w(c),
            height: if h.is_finite() { h } else { self.count as f32 * self.item_extent },
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp = ctx.rect;
        let ctrl = ctx.scroll_controller();
        let content_h = self.count as f32 * self.item_extent;
        let max_scroll = (content_h - vp.size.height).max(0.0);
        let scroll = ctrl.offset.get()[1].clamp(0.0, max_scroll);
        // Publish extents (guarded) so programmatic scroll_to can clamp.
        let vp_s = [vp.size.width, vp.size.height];
        if ctrl.viewport_size.get() != vp_s { ctrl.viewport_size.set(vp_s); }
        let cs = [vp.size.width, content_h];
        if ctrl.content_size.get() != cs { ctrl.content_size.set(cs); }

        // Quality Bar §5. Rows outside the viewport are never built (that is
        // the whole point of this widget), so assistive tech has no way to
        // discover how long the list is by walking it — the count has to be
        // stated here or it is unavailable.
        ctx.semantics(
            super::SemanticsProps::new(rosace_core::Role::List)
                .value(format!("{} items", self.count)),
        );

        ctx.record(DrawCommand::PushClip { rect: vp });
        let effective_clip = ctx.clip_rect
            .and_then(|parent| intersect_rect(parent, vp))
            .unwrap_or(vp);

        // The visible window — the only rows that ever exist this frame.
        let (first, last) = visible_window(scroll, vp.size.height, self.item_extent, self.count);

        for i in first..last {
            let row = (self.builder)(i);
            let pad = self.padding.unwrap_or_default();
            let row_rect = Rect {
                origin: Point {
                    x: vp.origin.x + pad.left,
                    y: vp.origin.y + i as f32 * self.item_extent - scroll,
                },
                size: Size { width: (vp.size.width - pad.total_h()).max(0.0), height: self.item_extent },
            };
            let lctx = ctx.layout_ctx(Constraints::tight((vp.size.width - pad.total_h()).max(0.0), self.item_extent));
            let _ = row.layout(&lctx);
            let mut row_ctx = ctx.child(row_rect);
            row_ctx.clip_rect = Some(effective_clip);
            row.paint(&mut row_ctx);
        }

        ctx.record(DrawCommand::PopClip);

        // Wheel/trackpad drives the node controller (vertical only).
        let wheel = ctrl.clone();
        ctx.register_scroll_target(vp, ScrollAxes::Y, Arc::new(move |_dx, dy| {
            wheel.scroll_by(0.0, -dy);
        }));

        if self.show_scrollbar && content_h > vp.size.height {
            let ratio = vp.size.height / content_h;
            let bar_h = (vp.size.height * ratio).max(16.0);
            // Clamped to the track: unclamped, at max scroll the thumb ran
            // past the bottom by up to its own height. `ScrollView` clamps
            // exactly this (see its `draw_scrollbars`).
            let track_top = vp.origin.y;
            let track_h = (vp.size.height - bar_h).max(0.0);
            let bar_y = (track_top + (scroll / content_h) * vp.size.height)
                .clamp(track_top, track_top + track_h);
            let bar_col = self.scrollbar_color
                .unwrap_or_else(|| ctx.tc(ctx.theme.colors.outline));
            ctx.fill_rect(Rect {
                origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                size: Size { width: 3.0, height: bar_h },
            }, bar_col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_render::{FontCache, PictureRecorder};
    use crate::tree::RenderTree;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn the_window_covers_exactly_the_rows_touching_the_viewport() {
        // 300px viewport, 50px rows: 6 full rows, so 0..6 at rest.
        assert_eq!(visible_window(0.0, 300.0, 50.0, 1000), (0, 6));
        // Scrolled by two whole rows.
        assert_eq!(visible_window(100.0, 300.0, 50.0, 1000), (2, 8));
        // Scrolled by half a row: the partially visible row at each end
        // must be built, or the list shows a gap while dragging.
        assert_eq!(visible_window(25.0, 300.0, 50.0, 1000), (0, 7));
    }

    #[test]
    fn the_window_is_clamped_to_the_row_count() {
        // Near the end, `last` must not run past `count`.
        assert_eq!(visible_window(400.0, 300.0, 50.0, 10), (8, 10));
        // A viewport taller than all the content.
        assert_eq!(visible_window(0.0, 5000.0, 50.0, 10), (0, 10));
    }

    /// `scroll` goes negative during an overscroll bounce.
    #[test]
    fn an_overscroll_bounce_never_produces_an_inverted_range() {
        let (first, last) = visible_window(-80.0, 300.0, 50.0, 1000);
        assert!(first <= last, "inverted range {first}..{last}");
        assert_eq!(first, 0, "cannot scroll above the first row");
    }

    /// Degenerate inputs must not panic or produce a huge range.
    #[test]
    fn a_zero_extent_or_empty_list_yields_an_empty_window() {
        assert_eq!(visible_window(0.0, 300.0, 0.0, 100), (0, 0));
        assert_eq!(visible_window(0.0, 300.0, 50.0, 0), (0, 0));
    }

    /// The actual guarantee, measured rather than reasoned about: a list of
    /// 10,000 rows must BUILD only the visible handful. This is the property
    /// the whole widget exists for, and it had no test.
    #[test]
    fn ten_thousand_rows_build_only_the_visible_ones() {
        let built = StdArc::new(AtomicUsize::new(0));
        let counter = built.clone();

        let font = FontCache::embedded();
        let tree = Rc::new(RefCell::new(RenderTree::new()));
        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 },
                       size: Size { width: 300.0, height: 300.0 } },
                &font,
                rosace_theme::built_in::dark_theme(),
                tree,
            );
            ListView::builder(10_000, 50.0, move |_i| {
                counter.fetch_add(1, Ordering::Relaxed);
                Box::new(super::super::Spacer::new(1.0))
            })
            .paint(&mut ctx);
        }

        let n = built.load(Ordering::Relaxed);
        assert!(n > 0, "nothing was built at all");
        assert!(n <= 8, "built {n} rows for a 300px viewport — virtualization is not working");
    }
}
