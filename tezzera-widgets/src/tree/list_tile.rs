use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_w};

/// A standard list row: leading widget + title + subtitle + trailing widget.
pub struct ListTile {
    pub title: String,
    pub subtitle: Option<String>,
    pub leading: Option<BoxedWidget>,
    pub trailing: Option<BoxedWidget>,
    pub selected: bool,
    pub height: f32,
    pub padding_h: f32,
    pub title_size: f32,
    pub subtitle_size: f32,
    pub title_color: Color,
    pub subtitle_color: Color,
    pub bg: Color,
    pub selected_bg: Color,
    pub selected_accent: Color,
    pub divider: bool,
}

impl ListTile {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            leading: None,
            trailing: None,
            selected: false,
            height: 48.0,
            padding_h: 14.0,
            title_size: 11.0,
            subtitle_size: 9.0,
            title_color: Color::rgb(220, 222, 240),
            subtitle_color: Color::rgb(140, 144, 175),
            bg: Color::rgba(0, 0, 0, 0),
            selected_bg: Color::rgb(26, 29, 50),
            selected_accent: Color::rgb(110, 75, 210),
            divider: true,
        }
    }
    pub fn subtitle(mut self, s: impl Into<String>) -> Self { self.subtitle = Some(s.into()); self }
    pub fn leading(mut self, w: impl Widget + 'static) -> Self { self.leading = Some(Box::new(w)); self }
    pub fn trailing(mut self, w: impl Widget + 'static) -> Self { self.trailing = Some(Box::new(w)); self }
    pub fn selected(mut self) -> Self { self.selected = true; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn no_divider(mut self) -> Self { self.divider = false; self }
    pub fn title_color(mut self, c: Color) -> Self { self.title_color = c; self }
    pub fn background(mut self, c: Color) -> Self { self.bg = c; self }
}

impl Widget for ListTile {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let bg = if self.selected { self.selected_bg } else { self.bg };
        if bg.a > 0 { ctx.fill_rect(r, bg); }

        if self.selected {
            ctx.fill_rect(Rect {
                origin: r.origin,
                size: Size { width: 2.5, height: r.size.height },
            }, self.selected_accent);
        }

        let mut x = r.origin.x + self.padding_h;

        // Leading
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(32.0, r.size.height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            lead.paint(&mut ctx.child(Rect {
                origin: Point { x, y: ly },
                size: ls,
            }));
            x += ls.width + 10.0;
        }

        // Trailing
        let trailing_w = if let Some(trail) = &self.trailing {
            let ts = trail.layout(&ctx.layout_ctx(Constraints::loose(60.0, r.size.height)));
            let ty = r.origin.y + (r.size.height - ts.height) / 2.0;
            let tx = r.origin.x + r.size.width - self.padding_h - ts.width;
            trail.paint(&mut ctx.child(Rect { origin: Point { x: tx, y: ty }, size: ts }));
            ts.width + self.padding_h
        } else { self.padding_h };

        // Title + subtitle
        let _text_w = r.size.width - x + r.origin.x - trailing_w;
        let line_h_title = ctx.font.line_height(self.title_size);
        let has_sub = self.subtitle.is_some();
        let total_text_h = if has_sub {
            line_h_title + ctx.font.line_height(self.subtitle_size) + 2.0
        } else {
            line_h_title
        };
        let text_y = r.origin.y + (r.size.height - total_text_h) / 2.0;

        ctx.draw_text_at(&self.title, Point { x, y: text_y }, self.title_color, self.title_size);

        if let Some(sub) = &self.subtitle {
            let sub_y = text_y + line_h_title + 2.0;
            ctx.draw_text_at(sub, Point { x, y: sub_y }, self.subtitle_color, self.subtitle_size);
        }

        if self.divider {
            ctx.fill_rect(Rect {
                origin: Point { x: r.origin.x + self.padding_h, y: r.origin.y + r.size.height - 1.0 },
                size: Size { width: r.size.width - self.padding_h, height: 1.0 },
            }, Color::rgb(24, 26, 44));
        }
    }
}

