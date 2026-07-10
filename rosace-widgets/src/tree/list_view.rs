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
    count: usize,
    item_extent: f32,
    builder: Arc<dyn Fn(usize) -> BoxedWidget + Send + Sync>,
    pub show_scrollbar: bool,
    pub scrollbar_color: Color,
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
            scrollbar_color: Color::rgb(50, 55, 85),
        }
    }

    pub fn no_scrollbar(mut self) -> Self { self.show_scrollbar = false; self }
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

        ctx.record(DrawCommand::PushClip { rect: vp });
        let effective_clip = ctx.clip_rect
            .and_then(|parent| intersect_rect(parent, vp))
            .unwrap_or(vp);

        // The visible window — the only rows that ever exist this frame.
        let first = (scroll / self.item_extent).floor().max(0.0) as usize;
        let last = (((scroll + vp.size.height) / self.item_extent).ceil() as usize)
            .min(self.count);

        for i in first..last {
            let row = (self.builder)(i);
            let row_rect = Rect {
                origin: Point {
                    x: vp.origin.x,
                    y: vp.origin.y + i as f32 * self.item_extent - scroll,
                },
                size: Size { width: vp.size.width, height: self.item_extent },
            };
            let lctx = ctx.layout_ctx(Constraints::tight(vp.size.width, self.item_extent));
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
            let bar_y = vp.origin.y + (scroll / content_h) * vp.size.height;
            ctx.fill_rect(Rect {
                origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                size: Size { width: 3.0, height: bar_h },
            }, self.scrollbar_color);
        }
    }
}
