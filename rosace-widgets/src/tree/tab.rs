use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};

/// A single tab descriptor.
pub struct Tab {
    pub label: String,
}

impl Tab {
    pub fn new(label: impl Into<String>) -> Self { Self { label: label.into() } }
}

/// A horizontal tab bar. The selected tab is underlined with an accent.
pub struct TabBar {
    tabs: Vec<Tab>,
    pub selected: usize,
    pub background: Color,
    pub active_color: Color,
    pub inactive_color: Color,
    pub indicator_color: Color,
    pub height: f32,
    pub font_size: f32,
    pub border_color: Color,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            selected: 0,
            background: Color::rgb(9, 10, 18),
            active_color: Color::rgb(220, 222, 240),
            inactive_color: Color::rgb(100, 104, 140),
            indicator_color: Color::rgb(110, 75, 210),
            height: 40.0,
            font_size: 10.5,
            border_color: Color::rgb(32, 35, 58),
        }
    }
    pub fn tab(mut self, t: Tab) -> Self { self.tabs.push(t); self }
    pub fn selected(mut self, i: usize) -> Self { self.selected = i; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn indicator_color(mut self, c: Color) -> Self { self.indicator_color = c; self }
}

impl Default for TabBar {
    fn default() -> Self { Self::new() }
}

impl Widget for TabBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: avail_w(constraints), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        ctx.fill_rect(r, self.background);
        ctx.fill_rect(Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + r.size.height - 1.0 },
            size: Size { width: r.size.width, height: 1.0 },
        }, self.border_color);

        if self.tabs.is_empty() { return; }
        let tab_w = r.size.width / self.tabs.len() as f32;

        for (i, tab) in self.tabs.iter().enumerate() {
            let tab_x = r.origin.x + i as f32 * tab_w;
            let active = i == self.selected;
            let label_color = if active { self.active_color } else { self.inactive_color };
            let text_w = ctx.font.measure_text(&tab.label, self.font_size);
            let line_h = ctx.font.line_height(self.font_size);
            let tx = tab_x + (tab_w - text_w) / 2.0;
            let ty = r.origin.y + (r.size.height - line_h) / 2.0;
            ctx.draw_text_at(&tab.label, Point { x: tx, y: ty }, label_color, self.font_size);
            let tab_rect = Rect { origin: Point { x: tab_x, y: r.origin.y }, size: Size { width: tab_w, height: r.size.height } };
            ctx.child(tab_rect).semantics(
                super::Semantics::new(rosace_core::Role::Tab)
                    .label(&tab.label)
                    .value(if active { "selected" } else { "not selected" }),
            );
            if active {
                ctx.fill_rect(Rect {
                    origin: Point { x: tab_x + tab_w * 0.1, y: r.origin.y + r.size.height - 2.0 },
                    size: Size { width: tab_w * 0.8, height: 2.0 },
                }, self.indicator_color);
            }
        }
    }
}
