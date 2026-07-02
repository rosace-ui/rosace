use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::container::draw_rounded_rect_pub;
use super::padding::EdgeInsets;

/// A bottom sheet surface: full-width panel with rounded top corners and a
/// grab handle. Pair with [`OverlayApi::sheet`], which anchors it to the
/// bottom edge and supplies the scrim + tap-to-dismiss.
///
/// [`OverlayApi::sheet`]: super::overlay_api::OverlayApi::sheet
pub struct Sheet {
    pub child: BoxedWidget,
    pub radius: f32,
    pub padding: EdgeInsets,
    pub show_handle: bool,
}

impl Sheet {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            radius: 16.0,
            padding: EdgeInsets::all(20.0),
            show_handle: true,
        }
    }

    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn no_handle(mut self) -> Self { self.show_handle = false; self }

    fn handle_space(&self) -> f32 {
        if self.show_handle { 16.0 } else { 0.0 }
    }
}

impl Widget for Sheet {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let width = ctx.constraints.max_width_f32();
        let inner_c = Constraints::loose(
            (width - self.padding.total_h()).max(0.0),
            f32::INFINITY,
        );
        let child_size = self.child.layout(&ctx.with_constraints(inner_c));
        ctx.constraints.constrain(Size {
            width,
            height: child_size.height + self.padding.total_v() + self.handle_space(),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let surface = ctx.tc(ctx.theme.colors.surface);

        // Rounded surface, then square off the bottom corners — the sheet
        // sits flush against the window's bottom edge.
        draw_rounded_rect_pub(ctx, r, surface, self.radius);
        ctx.fill_rect(Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + r.size.height - self.radius },
            size: Size { width: r.size.width, height: self.radius },
        }, surface);

        if self.show_handle {
            let handle_w = 36.0;
            ctx.fill_rrect(Rect {
                origin: Point {
                    x: r.origin.x + (r.size.width - handle_w) / 2.0,
                    y: r.origin.y + 6.0,
                },
                size: Size { width: handle_w, height: 4.0 },
            }, 2.0, ctx.tc(ctx.theme.colors.outline));
        }

        let content = Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + self.handle_space() },
            size: Size { width: r.size.width, height: r.size.height - self.handle_space() },
        };
        self.child.paint(&mut ctx.child(self.padding.shrink(content)));
    }
}
