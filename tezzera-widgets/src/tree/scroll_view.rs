use std::sync::Arc;
use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::{Color, DrawCommand};
use tezzera_state::Atom;
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
/// is painted at `offset` and clipped to the viewport bounds.
///
/// Two modes:
/// - Static: `ScrollView::new(child).offset(n)` — fixed offset, for snapshot demos.
/// - Live: `ScrollView::live(child, scroll_atom)` — reactive, driven by an `Atom<f32>` (D084).
pub struct ScrollView {
    child: BoxedWidget,
    /// Static offset (used when `live_offset` is None).
    pub offset: f32,
    /// Reactive scroll position atom (D084). If set, overrides `offset` at paint time.
    live_offset: Option<Atom<f32>>,
    /// Second axis live offset (horizontal in `Both` mode).
    live_offset_x: Option<Atom<f32>>,
    pub axis: ScrollAxis,
    pub show_scrollbar: bool,
    pub scrollbar_color: Color,
}

impl ScrollView {
    /// A live vertical scroll view (D097): the wheel/trackpad drives
    /// `scroll_y`. The atom must come from `ctx.state` so the position
    /// survives rebuilds.
    ///
    /// This is THE constructor — a ScrollView that cannot scroll was the
    /// trap that shipped a broken demo. For golden tests / snapshots use
    /// [`ScrollView::fixed`].
    pub fn new(child: impl Widget + 'static, scroll_y: Atom<f32>) -> Self {
        Self {
            child: Box::new(child),
            offset: 0.0,
            live_offset: Some(scroll_y),
            live_offset_x: None,
            axis: ScrollAxis::Vertical,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
        }
    }

    /// A live horizontal scroll view — carousels, chip rows, code blocks.
    pub fn horizontal(child: impl Widget + 'static, scroll_x: Atom<f32>) -> Self {
        Self {
            child: Box::new(child),
            offset: 0.0,
            live_offset: None,
            live_offset_x: Some(scroll_x),
            axis: ScrollAxis::Horizontal,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
        }
    }

    /// A snapshot viewport — never responds to input. Set the offset with
    /// `.offset(px)`. For golden tests and static mockups (honest name, D097).
    pub fn fixed(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            offset: 0.0,
            live_offset: None,
            live_offset_x: None,
            axis: ScrollAxis::Vertical,
            show_scrollbar: true,
            scrollbar_color: Color::rgb(50, 55, 85),
        }
    }

    pub fn offset(mut self, o: f32) -> Self { self.offset = o; self }

    /// Drive horizontal scrolling from an `Atom<f32>`. Use together with
    /// [`ScrollAxis::Horizontal`] or [`ScrollAxis::Both`].
    pub fn live_x(mut self, scroll_x: Atom<f32>) -> Self {
        self.live_offset_x = Some(scroll_x);
        self
    }

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

        // Resolve scroll offset — atom value takes precedence over static field.
        let scroll_y = self.live_offset.as_ref().map_or(self.offset, |a| a.get());
        let scroll_x = self.live_offset_x.as_ref().map_or(0.0_f32, |a| a.get());

        // Content constraints (unbounded-axis doctrine, API_DESIGN §6):
        // on the scroll axis min = viewport, max = Unbounded — short content
        // can center/space itself against the full viewport (no Flutter-style
        // LayoutBuilder + ConstrainedBox boilerplate); long content scrolls.
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

        // Clip child paint output to the viewport so scrolled-off content
        // is not visible and does not receive hit events in other panels.
        ctx.record(DrawCommand::PushClip { rect: vp });

        // Compute effective clip for hit-testing: intersect parent clip (if any)
        // with our viewport so nested ScrollViews clip correctly.
        let effective_clip = ctx.clip_rect
            .and_then(|parent| intersect_rect(parent, vp))
            .unwrap_or(vp);

        let mut child_ctx = ctx.child(child_rect);
        child_ctx.clip_rect = Some(effective_clip);
        self.child.paint(&mut child_ctx);

        ctx.record(DrawCommand::PopClip);

        // Register a scroll target so the event router can dispatch wheel events
        // to this viewport. Only live-scrolling ScrollViews respond to wheel input.
        if self.live_offset.is_some() || self.live_offset_x.is_some() {
            let atom_y = self.live_offset.clone();
            let atom_x = self.live_offset_x.clone();
            let max_scroll_y = (child_size.height - vp.size.height).max(0.0);
            let max_scroll_x = (child_size.width - vp.size.width).max(0.0);
            let axes = super::ScrollAxes {
                x: self.live_offset_x.is_some(),
                y: self.live_offset.is_some(),
            };
            ctx.register_scroll_target(vp, axes, Arc::new(move |delta_x, delta_y| {
                if let Some(a) = &atom_y {
                    a.set((a.get() - delta_y).clamp(0.0, max_scroll_y));
                }
                if let Some(a) = &atom_x {
                    a.set((a.get() - delta_x).clamp(0.0, max_scroll_x));
                }
            }));
        }

        // Scrollbars drawn AFTER PopClip so they are not clipped by the viewport.
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
