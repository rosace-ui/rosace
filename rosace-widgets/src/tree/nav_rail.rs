use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget, avail_h};
use super::container::draw_rounded_rect_pub;

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A single item in a [`NavRail`] — a navigation destination.
pub struct NavItem {
    pub label: String,
    pub badge: Option<u32>,
    pub active: bool,
    pub leading: Option<BoxedWidget>,
    pub height: f32,
    /// `None` = read from the active theme's `typography.label_medium`
    /// (D127 "environment" track — see `Checkbox::resolved_font_size`'s doc
    /// for the reasoning).
    pub font_size: Option<f32>,
    on_press: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl NavItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            badge: None,
            active: false,
            leading: None,
            height: 36.0,
            font_size: None,
            on_press: None,
        }
    }
    pub fn active(mut self) -> Self { self.active = true; self }
    pub fn active_if(mut self, c: bool) -> Self { self.active = c; self }
    pub fn badge(mut self, n: u32) -> Self { self.badge = Some(n); self }
    pub fn leading(mut self, w: impl Widget + 'static) -> Self { self.leading = Some(Box::new(w)); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }

    fn resolved_font_size(&self, theme: &rosace_theme::ThemeData) -> f32 {
        self.font_size.unwrap_or(theme.typography.label_medium.size)
    }
    /// Navigate on tap. Without it the item still absorbs (interactive-by-identity).
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(f)); self
    }
}

impl Widget for NavItem {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        Size { width: constraints.max_width_f32(), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // NavRail items are navigation destinations — <a> in the real-world
        // HTML shape (<nav><ul><li><a>...) a nav rail maps to (D107).
        let mut sem = super::SemanticsProps::new(rosace_core::Role::Link).label(&self.label);
        // The SELECTED destination is painted as a pill and was never
        // announced, so a screen-reader user could not tell which page they
        // were on. Selection outranks the badge count when both apply — the
        // badge is a detail, "you are here" is the primary fact.
        sem = match (self.active, self.badge) {
            (true, Some(n)) => sem.value(format!("selected, {n}")),
            (true, None) => sem.value("selected"),
            (false, Some(n)) => sem.value(n.to_string()),
            (false, None) => sem,
        };
        ctx.semantics(sem);
        let r = ctx.rect;

        // Interactive-by-identity: always own the hit region.
        match &self.on_press {
            Some(cb) => ctx.register_hit(Arc::clone(cb)),
            None => ctx.register_hit(Arc::new(|| {})),
        }
        let hovered = ctx.hovered();
        let pressed = ctx.pressed();

        let colors = ctx.theme.colors.clone();
        let accent = ctx.tc(colors.primary);
        let on_surf = ctx.tc(colors.on_surface);
        let pill = Rect { origin: Point { x: r.origin.x + 6.0, y: r.origin.y + 2.0 },
                          size: Size { width: r.size.width - 12.0, height: r.size.height - 4.0 } };

        // Active pill + accent bar; hover/press wash for the rest.
        if self.active {
            draw_rounded_rect_pub(ctx, pill, with_alpha(accent, 0.16), 8.0);
            ctx.fill_rect(Rect { origin: Point { x: r.origin.x, y: r.origin.y + 6.0 },
                                 size: Size { width: 3.0, height: r.size.height - 12.0 } }, accent);
        } else if hovered || pressed {
            draw_rounded_rect_pub(ctx, pill, with_alpha(on_surf, if pressed { 0.12 } else { 0.07 }), 8.0);
        }

        let mut lx = r.origin.x + 14.0;

        // Leading icon
        if let Some(lead) = &self.leading {
            let ls = lead.layout(&ctx.layout_ctx(Constraints::loose(20.0, r.size.height)));
            let ly = r.origin.y + (r.size.height - ls.height) / 2.0;
            lead.paint(&mut ctx.child(Rect { origin: Point { x: lx, y: ly }, size: ls }));
            lx += ls.width + 8.0;
        }

        // Label — active/hover brighten toward full on_surface.
        let label_color = if self.active { on_surf }
            else if hovered { super::lerp_color(with_alpha(on_surf, 0.6), on_surf, 0.5) }
            else { with_alpha(on_surf, 0.6) };
        let font_size = self.resolved_font_size(&ctx.theme);
        let line_h = ctx.font.line_height(font_size);
        let ty = r.origin.y + (r.size.height - line_h) / 2.0;
        ctx.draw_text_at(&self.label, Point { x: lx, y: ty }, label_color, font_size);

        if let Some(n) = self.badge {
            let ns = n.to_string();
            // Measured at the size it is drawn — see `BottomNav`'s badge for
            // why `len() * 7.0` was wrong.
            let badge_fs = 8.5;
            let bw = (ctx.font.measure_text(&ns, badge_fs) + 8.0).max(16.0);
            let bh = (ctx.font.line_height(badge_fs) + 2.0).max(16.0);
            let bx = r.origin.x + r.size.width - bw - 10.0;
            let by = r.origin.y + (r.size.height - bh) / 2.0;
            let badge_col = if self.active { accent } else { with_alpha(on_surf, 0.18) };
            draw_rounded_rect_pub(ctx, Rect { origin: Point { x: bx, y: by }, size: Size { width: bw, height: bh } }, badge_col, bh / 2.0);
            let badge_text_color = if self.active { ctx.tc(ctx.theme.colors.on_primary) } else { with_alpha(on_surf, 0.7) };
            let tx = bx + (bw - ctx.font.measure_text(&ns, badge_fs)) / 2.0;
            let ty = by + (bh - ctx.font.line_height(badge_fs)) / 2.0;
            ctx.draw_text_at(&ns, Point { x: tx, y: ty }, badge_text_color, badge_fs);
        }
    }
}

/// A vertical navigation sidebar (section headers + nav items).
pub struct NavRail {
    pub width: f32,
    /// `None` = the theme's `surface`. Every colour in this file used to be
    /// a hardcoded dark-theme literal, so the rail's chrome ignored the
    /// app's palette entirely while `NavItem`'s own colours resolved
    /// correctly — a half-themed widget.
    pub background: Option<Color>,
    /// `None` = the theme's `outline`.
    pub border_color: Option<Color>,
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
            background: None,
            border_color: None,
            items: Vec::new(),
        }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// The hairline separating the rail from the content beside it.
    pub fn border_color(mut self, c: Color) -> Self { self.border_color = Some(c); self }
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
        ctx.fill_rect(r, self.background.unwrap_or_else(|| ctx.tc(ctx.theme.colors.surface)));
        ctx.fill_rect(Rect {
            origin: Point { x: r.origin.x + r.size.width - 1.0, y: r.origin.y },
            size: Size { width: 1.0, height: r.size.height },
        }, self.border_color.unwrap_or_else(|| ctx.tc(ctx.theme.colors.outline)));

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
                        super::SemanticsProps::new(rosace_core::Role::Heading).label(label).heading_level(3),
                    );
                    ctx.draw_text_at(
                        label,
                        Point { x: r.origin.x + 14.0, y: y + 4.0 },
                        with_alpha(ctx.tc(ctx.theme.colors.on_surface), 0.55), 8.0,
                    );
                    y += 20.0;
                }
                NavRailEntry::Separator => {
                    ctx.fill_rect(Rect {
                        origin: Point { x: r.origin.x, y },
                        size: Size { width: self.width, height: 1.0 },
                    }, with_alpha(ctx.tc(ctx.theme.colors.outline), 0.5));
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
