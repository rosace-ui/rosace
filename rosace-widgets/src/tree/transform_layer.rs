use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use super::{Widget, LayoutCtx, PaintCtx};

/// Clips a child to a viewport and offsets it — a scroll position applied to
/// a subtree, with no scrollbar, physics or gesture handling.
///
/// This used to record the child into its own Picture and hand it to the
/// platform as an independent GPU layer whose sample offset moved (D080/D087
/// /D088). That bought a repaint-free scroll at the cost of a second
/// coordinate space beneath it, a 4096px content cap, and a whole second
/// scrolling mechanism to keep correct. Replay-on-move makes the ordinary
/// paint path just as cheap when only the offset changes, so this now records
/// into its parent's picture like any other widget: no texture, no cap, and
/// the enclosing clips apply by construction.
pub struct TransformLayer<W: Widget + Send + Sync + 'static> {
    pub child:      W,
    /// Scroll offset in **logical** pixels, positive = scroll down.
    pub scroll_y:   f32,
    /// Horizontal scroll offset in logical pixels.
    pub scroll_x:   f32,
    /// Viewport height in logical pixels — content beyond this is clipped.
    pub viewport_h: f32,
}

impl<W: Widget + Send + Sync + 'static> TransformLayer<W> {
    pub fn new(child: W, viewport_h: f32, scroll_y: f32) -> Self {
        Self { child, scroll_y, scroll_x: 0.0, viewport_h }
    }

    /// Horizontal offset, for a layer that scrolls on both axes.
    pub fn scroll_x(mut self, x: f32) -> Self { self.scroll_x = x; self }
}

impl<W: Widget + Send + Sync + 'static> Widget for TransformLayer<W> {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // Viewport size is what we occupy in the parent layout.
        let unconstrained = Constraints::loose(ctx.constraints.max_width_f32(), f32::INFINITY);
        let child_lctx = LayoutCtx::new(unconstrained, ctx.font, ctx.theme);
        let child_size = self.child.layout(&child_lctx);
        Size {
            width:  child_size.width,
            height: self.viewport_h.min(child_size.height),
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp_rect = ctx.rect;
        let viewport = Rect {
            origin: vp_rect.origin,
            size: Size {
                width: vp_rect.size.width,
                height: self.viewport_h.min(vp_rect.size.height),
            },
        };

        // Measure the child against an unbounded height to get its natural
        // size, exactly as before.
        let child_size = {
            let lctx = LayoutCtx::new(
                Constraints::loose(vp_rect.size.width, f32::INFINITY),
                ctx.font,
                &ctx.theme,
            );
            self.child.layout(&lctx)
        };

        // Clip to the viewport, then paint the child shifted by the offset.
        // `paint_child` re-blits its cached picture translated when only the
        // origin moved, so a changed offset costs a translation rather than a
        // re-record — the property the texture existed for.
        ctx.record(rosace_render::DrawCommand::PushClip { rect: viewport });
        let prev_clip = ctx.clip_rect;
        let clip = ctx.clip_rect
            .and_then(|parent| super::intersect_rect(parent, viewport))
            .unwrap_or(viewport);
        ctx.set_clip(Some(clip));

        // The app-supplied offset PLUS whatever this layer scrolled itself.
        //
        // The wheel handler below writes into the non-reactive channel. On the
        // old GPU path the compositor read that channel and shifted the
        // texture's sample origin, so wheel scrolling worked without this
        // widget knowing. Painting directly means nothing reads it unless this
        // does — the registration would write to a channel with no consumer
        // and a TransformLayer would silently ignore the wheel.
        let own = rosace_state::scroll_offset(ctx.node as u64);
        let child_rect = Rect {
            origin: Point {
                x: viewport.origin.x - self.scroll_x - own[0],
                y: viewport.origin.y - self.scroll_y - own[1],
            },
            size: child_size,
        };
        ctx.paint_child(child_rect, &self.child);

        ctx.set_clip(prev_clip);
        ctx.record(rosace_render::DrawCommand::PopClip);

        // Wheel scrolling still drives the same non-reactive offset channel.
        let node_id = ctx.node as u64;
        let max_x = (child_size.width - viewport.size.width).max(0.0);
        let max_y = (child_size.height - self.viewport_h).max(0.0);
        // Marking the node dirty is the other half. The channel is
        // non-reactive by design — on the GPU path a wheel tick moved the
        // compositor's sample origin and deliberately repainted NOTHING. With
        // the content painted directly, a write that schedules no repaint
        // leaves the new offset sitting in the channel, read by nobody until
        // something else happens to redraw.
        let node = ctx.node;
        ctx.register_scroll_target(
            viewport,
            super::render_tree::ScrollAxes::BOTH,
            std::sync::Arc::new(move |dx, dy| {
                rosace_state::scroll_offset_by(node_id, -dx, -dy, max_x, max_y);
                super::mark_node_dirty(node);
            }),
        );

        ctx.rect = viewport;
    }
}
