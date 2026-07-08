use std::sync::Arc;
use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::{Color, DrawCommand};
use tezzera_scroll::ScrollController;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h, intersect_rect};

/// Scroll direction.
#[derive(Debug, Clone, Copy, Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

/// Maximum content extent (logical px) on the scroll axis that the GPU-layer
/// path (D090) can composite as a single placed texture. Content taller than
/// this automatically uses the base (CPU-painted) path instead — correct,
/// just without the zero-repaint scroll optimization — via the
/// automatic-default heuristic (`should_auto_gpu`), so it never silently
/// mis-renders.
///
/// This is intentionally NOT solved with GPU-layer re-render windowing (a
/// moving texture window re-rendered as scroll approaches its edge). For
/// content that's actually large because it's a LONG LIST, [`super::ListView`]
/// already solves this the better way: real virtualization — only the rows
/// intersecting the viewport are ever built, laid out, or painted (O(visible)
/// cost, no texture-size limit to hit at all, since the full content is never
/// materialized). Reach for `ListView::builder` for that case rather than
/// wrapping a huge item list in a `ScrollView`. The base-path fallback here
/// exists for the much narrower remaining case — one large *non-virtualized*
/// widget subtree (e.g. a single big `Image`) — where it's correct but not
/// GPU-accelerated.
pub const MAX_TL_DIM: f32 = 4096.0;

/// A scrollable viewport. The child can exceed the available size; content
/// is painted at the scroll offset and clipped to the viewport bounds.
///
/// Scrolls by default (D101): the position lives on the widget's render-tree
/// node and survives rebuilds — no wiring needed. Pass a
/// [`ScrollController`] (`::controlled` / `.controller()`) only when the app
/// needs programmatic control.
///
/// The GPU-composited layer path (D090) is now the TRANSPARENT DEFAULT for
/// plain [`ScrollView::new`] scroll views: once content is measured, a scroll
/// view whose content actually overflows the viewport on the scroll axis and
/// stays within [`MAX_TL_DIM`] automatically composites as a placed GPU layer
/// (scrolling becomes a compositor UV shift, zero component repaint) — no
/// `.gpu_layer()` call needed. Content that doesn't overflow, or that exceeds
/// `MAX_TL_DIM`, uses the base (CPU-painted) path automatically. `::fixed`
/// and `::controlled` always use the base path — programmatic control and
/// snapshot modes need exact, un-composited semantics.
pub struct ScrollView {
    child: BoxedWidget,
    /// Fixed offset for [`ScrollView::fixed`] snapshot mode.
    fixed_offset: Option<f32>,
    /// Explicit controller override (D101). `None` = implicit node controller.
    controller: Option<ScrollController>,
    pub axis: ScrollAxis,
    pub show_scrollbar: bool,
    pub scrollbar_color: Color,
    /// Force the GPU-layer path on even when the automatic heuristic
    /// (`should_auto_gpu`) would not have chosen it (e.g. content smaller
    /// than the viewport that the app still wants pre-composited). The
    /// automatic default (see struct docs) already enables it when it helps;
    /// this flag is now an override for the exceptional case, not the only
    /// way to get the GPU path.
    gpu_layer: bool,
}

impl ScrollView {
    /// A vertical scroll view. Just scrolls — position is implicit per-node
    /// state (D101). Automatically GPU-composited once content overflows the
    /// viewport and fits within [`MAX_TL_DIM`] (see struct docs) — no
    /// `.gpu_layer()` call needed for the common case.
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            fixed_offset: None,
            controller: None,
            axis: ScrollAxis::Vertical,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
            gpu_layer: false,
        }
    }

    /// Force the GPU-layer path on regardless of the automatic size
    /// heuristic (see struct docs — [`ScrollView::new`] already auto-detects
    /// the common case). Content is capped at [`MAX_TL_DIM`]; taller content
    /// silently falls back to the base path (windowing is not yet built).
    pub fn gpu(child: impl Widget + 'static) -> Self {
        Self { gpu_layer: true, ..Self::new(child) }
    }

    /// Force GPU-layer compositing on (see [`ScrollView::gpu`]).
    pub fn gpu_layer(mut self) -> Self { self.gpu_layer = true; self }

    /// A horizontal scroll view — carousels, chip rows, code blocks.
    pub fn horizontal(child: impl Widget + 'static) -> Self {
        Self { axis: ScrollAxis::Horizontal, ..Self::new(child) }
    }

    /// A snapshot viewport — never responds to input. Set the offset with
    /// `.offset(px)`. For golden tests and static mockups.
    pub fn fixed(child: impl Widget + 'static) -> Self {
        Self { fixed_offset: Some(0.0), ..Self::new(child) }
    }

    /// A scroll view driven by an explicit [`ScrollController`] —
    /// programmatic scroll_to / scroll_by / scroll_to_top / scroll_to_bottom.
    /// Create the controller with `ScrollController::for_ctx(ctx)`.
    pub fn controlled(child: impl Widget + 'static, controller: ScrollController) -> Self {
        Self { controller: Some(controller), ..Self::new(child) }
    }

    /// Attach an explicit controller (same as [`ScrollView::controlled`]).
    pub fn controller(mut self, c: ScrollController) -> Self {
        self.controller = Some(c);
        self
    }

    /// Fixed-mode offset in logical pixels (only meaningful with `fixed`).
    pub fn offset(mut self, o: f32) -> Self { self.fixed_offset = Some(o); self }

    pub fn axis(mut self, a: ScrollAxis) -> Self { self.axis = a; self }
    pub fn no_scrollbar(mut self) -> Self { self.show_scrollbar = false; self }
    pub fn scrollbar_color(mut self, c: Color) -> Self { self.scrollbar_color = c; self }

    /// Content constraints (unbounded-axis doctrine, API_DESIGN §6): on the
    /// scroll axis min = viewport, max = Unbounded. Shared by both the GPU
    /// and base paint paths so content is measured identically either way.
    fn child_constraints(&self, vp: Rect) -> Constraints {
        use tezzera_layout::AxisBound;
        match self.axis {
            ScrollAxis::Vertical => Constraints {
                min_width: vp.size.width,
                max_width: AxisBound::Bounded(vp.size.width),
                min_height: vp.size.height,
                max_height: AxisBound::Unbounded,
            },
            ScrollAxis::Horizontal => Constraints {
                min_width: vp.size.width,
                max_width: AxisBound::Unbounded,
                min_height: vp.size.height,
                max_height: AxisBound::Bounded(vp.size.height),
            },
            ScrollAxis::Both => Constraints {
                min_width: vp.size.width,
                max_width: AxisBound::Unbounded,
                min_height: vp.size.height,
                max_height: AxisBound::Unbounded,
            },
        }
    }

    /// The automatic-default heuristic (D090 transparent default): GPU-layer
    /// compositing helps only when there is actually something to scroll
    /// (content overflows the viewport on the scroll axis) and only when the
    /// content fits in a single placed texture ([`MAX_TL_DIM`] — taller
    /// content needs re-render windowing, not yet built, so it must stay on
    /// the base path rather than silently mis-render).
    fn should_auto_gpu(&self, vp: Size, child_size: Size) -> bool {
        let (overflow, extent) = match self.axis {
            ScrollAxis::Vertical => (child_size.height > vp.height, child_size.height),
            ScrollAxis::Horizontal => (child_size.width > vp.width, child_size.width),
            ScrollAxis::Both => (
                child_size.height > vp.height || child_size.width > vp.width,
                child_size.height.max(child_size.width),
            ),
        };
        overflow && extent <= MAX_TL_DIM
    }

    /// GPU-layer paint path (D090). Records the content once into its own
    /// sub-tree/picture at content-local `(0,0)`, attaches it as a
    /// TransformLayer entry (the platform composites it as a placed layer), and
    /// registers wheel scrolling straight into the non-reactive offset channel
    /// so a scroll tick is a compositor UV shift with no component repaint.
    /// `child_size` is measured once by the caller ([`Widget::paint`]) and
    /// passed in — this never re-measures.
    fn paint_gpu(&self, ctx: &mut PaintCtx, child_size: Size) {
        use super::TransformLayerEntry;
        let vp = ctx.rect;
        let node_id = ctx.node as u64;
        let off = tezzera_state::scroll_offset(node_id);

        // Record the content at (0,0) into its own node/picture (D090).
        let sub_node = ctx.tree.borrow_mut().slot(ctx.node, true);
        let mut sub_rec = tezzera_render::PictureRecorder::new();
        let child_rect = Rect { origin: Point { x: 0.0, y: 0.0 }, size: child_size };
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

        ctx.attach_transform(TransformLayerEntry {
            picture,
            child_size,
            viewport_rect: vp,
            scroll_x: off[0],
            scroll_y: off[1],
        });

        // Wheel scrolling → offset channel (no repaint). Axis-clamped.
        let max_x = match self.axis {
            ScrollAxis::Horizontal | ScrollAxis::Both => (child_size.width - vp.size.width).max(0.0),
            ScrollAxis::Vertical => 0.0,
        };
        let max_y = match self.axis {
            ScrollAxis::Vertical | ScrollAxis::Both => (child_size.height - vp.size.height).max(0.0),
            ScrollAxis::Horizontal => 0.0,
        };
        let axes = match self.axis {
            ScrollAxis::Vertical   => super::ScrollAxes::Y,
            ScrollAxis::Horizontal => super::ScrollAxes::X,
            ScrollAxis::Both       => super::ScrollAxes::BOTH,
        };
        ctx.register_scroll_target(vp, axes, Arc::new(move |dx, dy| {
            tezzera_state::scroll_offset_by(node_id, -dx, -dy, max_x, max_y);
        }));

        // Scrollbar drawn into the base canvas from the live channel offset.
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both)
            && child_size.height > vp.size.height
        {
            let ratio = (vp.size.height / child_size.height).min(1.0);
            let bar_h = vp.size.height * ratio;
            let bar_y = vp.origin.y + (off[1] / child_size.height) * vp.size.height;
            ctx.fill_rect(Rect {
                origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                size: Size { width: 3.0, height: bar_h },
            }, self.scrollbar_color);
        }
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both)
            && child_size.width > vp.size.width
        {
            let ratio = (vp.size.width / child_size.width).min(1.0);
            let bar_w = vp.size.width * ratio;
            let bar_x = vp.origin.x + (off[0] / child_size.width) * vp.size.width;
            ctx.fill_rect(Rect {
                origin: Point { x: bar_x, y: vp.origin.y + vp.size.height - 4.0 },
                size: Size { width: bar_w, height: 3.0 },
            }, self.scrollbar_color);
        }
    }

    /// Base (CPU-painted) path: content painted directly into the main
    /// canvas at the scroll offset, clipped to the viewport. `child_size` is
    /// measured once by the caller ([`Widget::paint`]) and passed in.
    fn paint_base(&self, ctx: &mut PaintCtx, child_size: Size) {
        let vp = ctx.rect;

        // Resolve the controller: explicit override, or the node's implicit
        // one (D101). Fixed mode has no controller and never handles input.
        let ctrl = if self.fixed_offset.is_some() {
            None
        } else {
            Some(self.controller.clone().unwrap_or_else(|| ctx.scroll_controller()))
        };

        let (scroll_x, scroll_y) = match (&ctrl, self.fixed_offset) {
            (Some(c), _) => {
                let [x, y] = c.offset.get();
                (x, y)
            }
            (None, Some(o)) => match self.axis {
                ScrollAxis::Horizontal => (o, 0.0),
                _ => (0.0, o),
            },
            (None, None) => (0.0, 0.0),
        };

        let (ox, oy) = match self.axis {
            ScrollAxis::Vertical   => (0.0, -scroll_y),
            ScrollAxis::Horizontal => (-scroll_x, 0.0),
            ScrollAxis::Both       => (-scroll_x, -scroll_y),
        };

        let child_rect = Rect {
            origin: Point { x: vp.origin.x + ox, y: vp.origin.y + oy },
            size: child_size,
        };

        // Clip child paint output to the viewport.
        ctx.record(DrawCommand::PushClip { rect: vp });
        let effective_clip = ctx.clip_rect
            .and_then(|parent| intersect_rect(parent, vp))
            .unwrap_or(vp);
        let mut child_ctx = ctx.child(child_rect);
        child_ctx.clip_rect = Some(effective_clip);
        self.child.paint(&mut child_ctx);
        ctx.record(DrawCommand::PopClip);

        // Publish extents (guarded — unconditional atom writes during paint
        // would dirty the component every frame) and route wheel input.
        if let Some(ctrl) = &ctrl {
            let vp_s = [vp.size.width, vp.size.height];
            if ctrl.viewport_size.get() != vp_s { ctrl.viewport_size.set(vp_s); }
            let cs = [child_size.width, child_size.height];
            if ctrl.content_size.get() != cs { ctrl.content_size.set(cs); }

            let axes = match self.axis {
                ScrollAxis::Vertical   => super::ScrollAxes::Y,
                ScrollAxis::Horizontal => super::ScrollAxes::X,
                ScrollAxis::Both       => super::ScrollAxes::BOTH,
            };
            let ctrl = ctrl.clone();
            let (ax, ay) = (axes.x, axes.y);
            ctx.register_scroll_target(vp, axes, Arc::new(move |dx, dy| {
                ctrl.scroll_by(
                    if ax { -dx } else { 0.0 },
                    if ay { -dy } else { 0.0 },
                );
            }));
        }

        // Scrollbars drawn AFTER PopClip so they are not clipped.
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Vertical | ScrollAxis::Both) {
            let ratio = (vp.size.height / child_size.height.max(1.0)).min(1.0);
            if ratio < 1.0 {
                let bar_h = vp.size.height * ratio;
                let bar_y = vp.origin.y + (scroll_y / child_size.height) * vp.size.height;
                ctx.fill_rect(Rect {
                    origin: Point { x: vp.origin.x + vp.size.width - 4.0, y: bar_y },
                    size: Size { width: 3.0, height: bar_h },
                }, self.scrollbar_color);
            }
        }
        if self.show_scrollbar && matches!(self.axis, ScrollAxis::Horizontal | ScrollAxis::Both) {
            let ratio = (vp.size.width / child_size.width.max(1.0)).min(1.0);
            if ratio < 1.0 {
                let bar_w = vp.size.width * ratio;
                let bar_x = vp.origin.x + (scroll_x / child_size.width) * vp.size.width;
                ctx.fill_rect(Rect {
                    origin: Point { x: bar_x, y: vp.origin.y + vp.size.height - 4.0 },
                    size: Size { width: bar_w, height: 3.0 },
                }, self.scrollbar_color);
            }
        }
    }
}

impl Widget for ScrollView {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let vp = ctx.rect;
        let child_size = self.child.layout(&ctx.layout_ctx(self.child_constraints(vp)));

        // `::fixed` and `::controlled` always use the base path — exact,
        // un-composited semantics for programmatic control and snapshots.
        // Otherwise: explicit `.gpu_layer()` forces the GPU path on; plain
        // `ScrollView::new` auto-detects it via the size heuristic (D090
        // transparent default).
        let eligible = self.fixed_offset.is_none() && self.controller.is_none();
        let use_gpu = eligible
            && (self.gpu_layer || self.should_auto_gpu(vp.size, child_size));

        if use_gpu {
            self.paint_gpu(ctx, child_size);
        } else {
            self.paint_base(ctx, child_size);
        }
    }
}
