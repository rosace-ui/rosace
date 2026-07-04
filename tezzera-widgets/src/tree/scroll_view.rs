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

/// A scrollable viewport. The child can exceed the available size; content
/// is painted at the scroll offset and clipped to the viewport bounds.
///
/// Scrolls by default (D101): the position lives on the widget's render-tree
/// node and survives rebuilds — no wiring needed. Pass a
/// [`ScrollController`] (`::controlled` / `.controller()`) only when the app
/// needs programmatic control.
pub struct ScrollView {
    child: BoxedWidget,
    /// Fixed offset for [`ScrollView::fixed`] snapshot mode.
    fixed_offset: Option<f32>,
    /// Explicit controller override (D101). `None` = implicit node controller.
    controller: Option<ScrollController>,
    pub axis: ScrollAxis,
    pub show_scrollbar: bool,
    pub scrollbar_color: Color,
}

impl ScrollView {
    /// A vertical scroll view. Just scrolls — position is implicit per-node
    /// state (D101).
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            fixed_offset: None,
            controller: None,
            axis: ScrollAxis::Vertical,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
        }
    }

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
}

impl Widget for ScrollView {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
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

        // Content constraints (unbounded-axis doctrine, API_DESIGN §6):
        // on the scroll axis min = viewport, max = Unbounded.
        use tezzera_layout::AxisBound;
        let child_constraints = match self.axis {
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
        };
        let child_size = self.child.layout(&ctx.layout_ctx(child_constraints));

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
