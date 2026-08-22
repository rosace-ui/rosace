use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::PictureRecorder;
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
    pub scroll_y:   f32,
    /// Horizontal scroll offset in logical pixels.
    pub scroll_x:   f32,
    /// Viewport height in logical pixels — content beyond this is clipped.
    pub viewport_h: f32,
}

/// Physical-pixel cap for TransformLayer content (D082).
///
/// The offscreen texture is allocated at `logical_size * render_scale()`, so
/// the cap is PHYSICAL — a logical-only check passes content that then
/// cannot fit its texture on a 2x/3x display.
///
/// This was declared and never enforced: `child_size` went straight into the
/// `TransformLayerEntry` unclamped, so the documented cap did not exist on
/// this path. `ScrollView` had been carrying its own private copy of the
/// same number to compensate; it now reads this one.
pub const MAX_TRANSFORM_DIM: u32 = 4096;

/// Clamp a logical size so its PHYSICAL texture stays within
/// [`MAX_TRANSFORM_DIM`] on both axes.
fn clamp_to_texture_cap(size: Size) -> Size {
    let scale = rosace_state::render_scale().max(0.01);
    let max_logical = MAX_TRANSFORM_DIM as f32 / scale;
    if size.width <= max_logical && size.height <= max_logical {
        return size;
    }
    #[cfg(debug_assertions)]
    {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            eprintln!(
                "[ROSACE] TransformLayer: content {:.0}x{:.0} exceeds the \
                 {MAX_TRANSFORM_DIM}px physical texture cap at {scale}x and \
                 was clamped. Content past the cap will not paint.",
                size.width, size.height,
            );
        });
    }
    Size {
        width:  size.width.min(max_logical),
        height: size.height.min(max_logical),
    }
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
        // Enforce the D082 texture cap. Unclamped, an over-tall child asks
        // the platform for a texture it cannot allocate.
        let child_size = clamp_to_texture_cap(self.child.layout(&child_lctx));
        Size {
            width:  child_size.width,
            height: self.viewport_h.min(child_size.height),
        }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let (scroll_y, scroll_x) = (self.scroll_y, self.scroll_x);
        let vp_rect  = ctx.rect;

        // Measure child with unconstrained height to get its natural size.
        // Claim the child's node BEFORE measuring, and measure through it —
        // see `InteractiveViewer::paint` for what goes wrong otherwise. Same
        // recipe, same hazard.
        let sub_node = ctx.tree.borrow_mut().slot(ctx.node, true);
        ctx.tree.borrow_mut().begin_layout(sub_node);
        let child_lctx = LayoutCtx::with_tree(
            Constraints::loose(vp_rect.size.width, f32::INFINITY),
            ctx.font,
            &ctx.theme,
            ctx.tree.clone(),
            sub_node,
        );
        let child_size = self.child.layout(&child_lctx);

        // Record child into a SEPARATE PictureRecorder (D087).
        // The child is painted at (0,0) — the platform positions it on screen.
        let mut sub_rec = PictureRecorder::new();
        let child_origin = Point { x: 0.0, y: 0.0 };
        let child_rect = Rect { origin: child_origin, size: child_size };

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
            zoom: 1.0,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The D082 cap is PHYSICAL, so the logical limit shrinks as the display
    /// scale rises. Declared but never applied until 2026-08-13: `child_size`
    /// went into the entry unclamped, so an over-tall child asked the
    /// platform for a texture it could not allocate.
    #[test]
    fn content_is_clamped_to_the_physical_texture_cap() {
        let cap = MAX_TRANSFORM_DIM as f32;

        rosace_state::set_render_scale(1.0);
        let ok = Size { width: 800.0, height: 2000.0 };
        assert_eq!(clamp_to_texture_cap(ok), ok, "under the cap: untouched");

        let over = Size { width: 800.0, height: 9000.0 };
        let c = clamp_to_texture_cap(over);
        assert_eq!(c.height, cap, "height clamped to the cap at 1x");
        assert_eq!(c.width, 800.0, "the axis under the cap is left alone");

        // At 2x the SAME logical size needs twice the texture, so the
        // logical limit halves. A logical-only check would have passed this.
        rosace_state::set_render_scale(2.0);
        let c2 = clamp_to_texture_cap(Size { width: 800.0, height: 3000.0 });
        assert_eq!(c2.height, cap / 2.0, "logical limit halves at 2x");

        rosace_state::set_render_scale(1.0);
    }

    /// A zero or nonsense scale must not produce an infinite or NaN limit.
    #[test]
    fn a_degenerate_scale_does_not_produce_an_infinite_limit() {
        rosace_state::set_render_scale(0.0);
        let c = clamp_to_texture_cap(Size { width: 1.0e9, height: 1.0e9 });
        assert!(c.width.is_finite() && c.height.is_finite(), "got {c:?}");
        rosace_state::set_render_scale(1.0);
    }
}
