use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::PictureRecorder;
use rosace_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, TransformLayerEntry};

/// Captures a child widget into an independent Picture and applies a 2D scroll
/// offset on the GPU without re-rendering the child (D080, Phase 17/19).
///
/// Phase 17: CPU shift in paint() — UV offset uniform wired in compositor.
/// Phase 19: child is recorded into a separate PictureRecorder and pushed into
/// PaintCtx.transform_entries (D087) for the platform to replay into its own
/// SkiaCanvas and present as an extra GPU compositor layer (D088).
pub struct TransformLayer<W: Widget + Send + Sync + 'static> {
    pub child:      W,
    /// Scroll offset in **logical** pixels, positive = scroll down.
    pub scroll_y:   Atom<f32>,
    /// Horizontal scroll offset in logical pixels.
    pub scroll_x:   Atom<f32>,
    /// Viewport height in logical pixels — content beyond this is clipped.
    pub viewport_h: f32,
}

/// Physical-pixel cap for TransformLayer content (D082).
pub const MAX_TRANSFORM_DIM: u32 = 4096;

impl<W: Widget + Send + Sync + 'static> TransformLayer<W> {
    pub fn new(child: W, viewport_h: f32, scroll_y: Atom<f32>) -> Self {
        Self {
            child,
            scroll_y,
            scroll_x: rosace_state::use_atom(0.0_f32),
            viewport_h,
        }
    }
}

impl<W: Widget + Send + Sync + 'static> Widget for TransformLayer<W> {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // Viewport size is what we occupy in the parent layout.
        let unconstrained = Constraints::loose(ctx.constraints.max_width_f32(), f32::INFINITY);
        let child_lctx = LayoutCtx::new(unconstrained, ctx.font, &ctx.theme);
        let child_size = self.child.layout(&child_lctx);
        Size {
            width:  child_size.width,
            height: self.viewport_h.min(child_size.height),
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let scroll_y = self.scroll_y.get();
        let scroll_x = self.scroll_x.get();
        let vp_rect  = ctx.rect;

        // Measure child with unconstrained height to get its natural size.
        let child_lctx = ctx.layout_ctx(Constraints::loose(vp_rect.size.width, f32::INFINITY));
        let child_size = self.child.layout(&child_lctx);

        // Record child into a SEPARATE PictureRecorder (D087).
        // The child is painted at (0,0) — the platform positions it on screen.
        let mut sub_rec = PictureRecorder::new();
        let child_origin = Point { x: 0.0, y: 0.0 };
        let child_rect = Rect { origin: child_origin, size: child_size };

        let sub_node = ctx.tree.borrow_mut().slot(ctx.node, true);
        let mut sub_ctx = PaintCtx {
            recorder: &mut sub_rec,
            rect: child_rect,
            font: ctx.font,
            theme: ctx.theme.clone(),
            tree: ctx.tree.clone(),
            node: sub_node,
            owner: ctx.owner,
            clip_rect: None,
        };
        self.child.paint(&mut sub_ctx);
        let picture = sub_rec.finish();

        // Attach the entry to this node — the platform replays it into a
        // dedicated canvas (D088); it persists across clean frames (D091).
        ctx.attach_transform(TransformLayerEntry {
            picture,
            child_size,
            viewport_rect: vp_rect,
            scroll_x,
            scroll_y,
        });

        // Register wheel scrolling straight into the non-reactive offset
        // channel, keyed by this node id (D090). A scroll tick updates the
        // channel + requests a present-only frame — it dirties NO component,
        // so the content texture is reused and only the compositor UV offset
        // changes. Zero CPU paint on scroll.
        let node_id = ctx.node as u64;
        let max_x = (child_size.width  - vp_rect.size.width).max(0.0);
        let max_y = (child_size.height - self.viewport_h).max(0.0);
        ctx.register_scroll_target(
            vp_rect,
            super::render_tree::ScrollAxes::BOTH,
            std::sync::Arc::new(move |dx, dy| {
                rosace_state::scroll_offset_by(node_id, -dx, -dy, max_x, max_y);
            }),
        );

        // Update ctx.rect to the viewport size for sibling layout correctness.
        ctx.rect = Rect {
            origin: vp_rect.origin,
            size: Size {
                width:  vp_rect.size.width,
                height: self.viewport_h.min(vp_rect.size.height),
            },
        };
    }
}
