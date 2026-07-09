use tezzera_core::types::{Point, Rect, Size};
use tezzera_layout::Constraints;
use tezzera_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_h};
use super::container::draw_rounded_rect_pub;

/// A single item in a [`NavRail`].
pub struct NavItem {
    pub label: String,
    pub badge: Option<u32>,
    pub active: bool,
    pub leading: Option<BoxedWidget>,
    pub height: f32,
    pub font_size: f32,
}

impl NavItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            badge: None,
            active: false,
            leading: None,
            height: 32.0,
            font_size: 10.5,
        }
    }
    pub fn active(mut self) -> Self { self.active = true; self }
    pub fn badge(mut self, n: u32) -> Self { self.badge = Some(n); self }
    pub fn leading(mut self, w: impl Widget + 'static) -> Self { self.leading = Some(Box::new(w)); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
}

impl Widget for NavItem {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: constraints.max_width_f32(), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // NavRail items are navigation destinations — <a> in the real-world
        // HTML shape (<nav><ul><li><a>...) a nav rail maps to (D107).
        let mut sem = super::Semantics::new(tezzera_core::Role::Link).label(&self.label);
        if let Some(n) = self.badge { sem = sem.value(n.to_string()); }
        ctx.semantics(sem);
        let r = ctx.rect;

        if self.active {
            ctx.fill_rect(
                Rect { origin: Point { x: r.origin.x + 6.0, y: r.origin.y },
                       size: Size { width: r.size.width - 12.0, height: r.size.height } },
                Color::rgb(26, 29, 50),
            );
            ctx.fill_rect(
                Rect { origin: r.origin, size: Size { width: 3.0, height: r.size.height } },
                Color::rgb(110, 75, 210),
            );
        }

        let mut lx = r.origin.x + 14.0;

        // Leading icon
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(20.0, r.size.height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            lead.paint(&mut ctx.child(Rect { origin: Point { x: lx, y: ly }, size: ls }));
            lx += ls.width + 8.0;
        }

        // Label
        let label_color = if self.active { Color::rgb(220, 222, 240) } else { Color::rgb(140, 144, 175) };
        let line_h = ctx.font.line_height(self.font_size);
        let ty = r.origin.y + (r.size.height - line_h) / 2.0;
        ctx.draw_text_at(&self.label, Point { x: lx, y: ty }, label_color, self.font_size);

        if let Some(n) = self.badge {
            let ns = n.to_string();
            let bw = ns.len() as f32 * 7.0 + 8.0;
            let bx = r.origin.x + r.size.width - bw - 10.0;
            let by = r.origin.y + (r.size.height - 16.0) / 2.0;
            let badge_col = if self.active { Color::rgb(110, 75, 210) } else { Color::rgb(50, 55, 90) };
            draw_rounded_rect_pub(ctx, Rect { origin: Point { x: bx, y: by }, size: Size { width: bw, height: 16.0 } }, badge_col, 8.0);
            let badge_text_color = if self.active { Color::rgb(230, 232, 245) } else { Color::rgb(140, 144, 175) };
            ctx.draw_text_at(&ns, Point { x: bx + 4.0, y: by + 3.0 }, badge_text_color, 8.5);
        }
    }
}

/// A vertical navigation sidebar (section headers + nav items).
pub struct NavRail {
    pub width: f32,
    pub background: Color,
    pub border_color: Color,
    items: Vec<NavRailEntry>,
}

enum NavRailEntry {
    Item(NavItem),
    Section(String),
    Separator,
    Custom(Box<dyn Widget>),
}

impl NavRail {
    pub fn new() -> Self {
        Self {
            width: 232.0,
            background: Color::rgb(11, 12, 22),
            border_color: Color::rgb(32, 35, 58),
            items: Vec::new(),
        }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn background(mut self, c: Color) -> Self { self.background = c; self }
    pub fn item(mut self, i: NavItem) -> Self { self.items.push(NavRailEntry::Item(i)); self }
    pub fn section(mut self, label: impl Into<String>) -> Self {
        self.items.push(NavRailEntry::Section(label.into())); self
    }
    pub fn separator(mut self) -> Self { self.items.push(NavRailEntry::Separator); self }
    pub fn widget(mut self, w: impl Widget + 'static) -> Self {
        self.items.push(NavRailEntry::Custom(Box::new(w))); self
    }
}

impl Default for NavRail {
    fn default() -> Self { Self::new() }
}

impl Widget for NavRail {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: self.width, height: avail_h(constraints) }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        ctx.fill_rect(r, self.background);
        ctx.fill_rect(Rect {
            origin: Point { x: r.origin.x + r.size.width - 1.0, y: r.origin.y },
            size: Size { width: 1.0, height: r.size.height },
        }, self.border_color);

        let mut y = r.origin.y;
        let _item_c = Constraints::tight(self.width, 32.0);

        for entry in &self.items {
            match entry {
                NavRailEntry::Item(item) => {
                    let h = item.height;
                    item.paint(&mut ctx.child(Rect {
                        origin: Point { x: r.origin.x, y },
                        size: Size { width: self.width, height: h },
                    }));
                    y += h;
                }
                NavRailEntry::Section(label) => {
                    let section_rect = Rect { origin: Point { x: r.origin.x, y }, size: Size { width: self.width, height: 20.0 } };
                    ctx.child(section_rect).semantics(
                        super::Semantics::new(tezzera_core::Role::Heading).label(label).heading_level(3),
                    );
                    ctx.draw_text_at(
                        label,
                        Point { x: r.origin.x + 14.0, y: y + 4.0 },
                        Color::rgb(80, 85, 118), 8.0,
                    );
                    y += 20.0;
                }
                NavRailEntry::Separator => {
                    ctx.fill_rect(Rect {
                        origin: Point { x: r.origin.x, y },
                        size: Size { width: self.width, height: 1.0 },
                    }, Color::rgb(24, 26, 44));
                    y += 10.0;
                }
                NavRailEntry::Custom(w) => {
                    let size = w.layout(&ctx.layout_ctx(Constraints::loose(self.width, r.size.height - (y - r.origin.y))));
                    w.paint(&mut ctx.child(Rect { origin: Point { x: r.origin.x, y }, size }));
                    y += size.height;
                }
            }
        }
    }
}
