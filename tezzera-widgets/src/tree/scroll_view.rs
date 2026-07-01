use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use tezzera_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w, avail_h};

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
    pub fn new(child: impl Widget + 'static) -> Self {
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

    /// Live-scrolling constructor: offset is driven by an `Atom<f32>` (D084).
    /// The component that owns the atom subscribes automatically; when the atom
    /// changes the component rebuilds and ScrollView reads the new value in `paint`.
    pub fn live(child: impl Widget + 'static, scroll_y: Atom<f32>) -> Self {
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

    pub fn offset(mut self, o: f32) -> Self { self.offset = o; self }
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

        let child_constraints = match self.axis {
            ScrollAxis::Vertical   => Constraints::loose(vp.size.width,  f32::INFINITY),
            ScrollAxis::Horizontal => Constraints::loose(f32::INFINITY, vp.size.height),
            ScrollAxis::Both       => Constraints::loose(f32::INFINITY, f32::INFINITY),
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
        self.child.paint(&mut ctx.child(child_rect));

        // Scrollbar (vertical)
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
    }
}
