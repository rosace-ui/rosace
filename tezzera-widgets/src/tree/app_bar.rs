use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// A top app bar with title, leading, and trailing action slots.
pub struct AppBar {
    pub title: String,
    pub title_size: f32,
    pub background: Color,
    pub foreground: Color,
    pub border_color: Color,
    pub height: f32,
    pub leading: Option<BoxedWidget>,
    pub actions: Vec<BoxedWidget>,
    pub show_traffic_lights: bool,
}

impl AppBar {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            title_size: 13.0,
            background: Color::rgba(0, 0, 0, 0), // sentinel: use theme.surface
            foreground: Color::rgba(0, 0, 0, 0), // sentinel: use theme.on_surface
            border_color: Color::rgba(0, 0, 0, 0), // sentinel: use theme.outline
            height: 44.0,
            leading: None,
            actions: Vec::new(),
            show_traffic_lights: true,
        }
    }

    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn foreground(mut self, c: Color) -> Self { self.foreground = c; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn leading(mut self, w: impl Widget + 'static) -> Self { self.leading = Some(Box::new(w)); self }
    pub fn action(mut self, w: impl Widget + 'static) -> Self { self.actions.push(Box::new(w)); self }
    pub fn no_traffic_lights(mut self) -> Self { self.show_traffic_lights = false; self }
    pub fn title_size(mut self, s: f32) -> Self { self.title_size = s; self }
}

impl Widget for AppBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let t = &ctx.theme.colors;
        let bg     = if self.background.a   == 0 { ctx.tc(t.surface)     } else { self.background   };
        let fg     = if self.foreground.a   == 0 { ctx.tc(t.on_surface)  } else { self.foreground   };
        let border = if self.border_color.a == 0 { ctx.tc(t.outline)     } else { self.border_color };

        let r = ctx.rect;
        ctx.fill_rect(r, bg);

        // Bottom border
        ctx.fill_rect(Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + r.size.height - 1.0 },
            size: Size { width: r.size.width, height: 1.0 },
        }, border);

        let cy = r.origin.y + r.size.height / 2.0;
        let mut lx = r.origin.x + 16.0;

        // Traffic lights (macOS window controls)
        if self.show_traffic_lights {
            let tl_y = cy;
            for (i, color) in [
                Color::rgb(235, 85, 75),
                Color::rgb(245, 185, 55),
                Color::rgb(75, 200, 85),
            ].iter().enumerate() {
                ctx.fill_circle(Point { x: lx + i as f32 * 20.0, y: tl_y }, 7.0, *color);
            }
            lx += 72.0;
        }

        // Leading widget
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(40.0, self.height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            lead.paint(&mut ctx.child(Rect { origin: Point { x: lx, y: ly }, size: ls }));
            let _ = lx + ls.width + 8.0;
        }

        // Title (centered)
        let title_w = ctx.font.measure_text(&self.title, self.title_size);
        let title_x = r.origin.x + (r.size.width - title_w) / 2.0;
        let line_h = ctx.font.line_height(self.title_size);
        let title_y = r.origin.y + (r.size.height - line_h) / 2.0;
        ctx.draw_text_at(&self.title, Point { x: title_x, y: title_y }, fg, self.title_size);

        // Actions (right side)
        let mut ax = r.origin.x + r.size.width - 12.0;
        for action in self.actions.iter().rev() {
            let as_ = action.layout(&ctx.layout_ctx(Constraints::loose(36.0, self.height)));
            ax -= as_.width + 4.0;
            let ay = r.origin.y + (r.size.height - as_.height) / 2.0;
            action.paint(&mut ctx.child(Rect { origin: Point { x: ax, y: ay }, size: as_ }));
        }
    }
}
