//! `BottomNavigationBar` (D115/Phase 32 Step 1) — the horizontal
//! counterpart to [`NavRail`]: 3-5 top-level destinations pinned to the
//! bottom edge, the mobile-first navigation convention. Drop it in
//! `Scaffold::bottom_bar`.
//!
//! Controlled, like `NavRail`/`TabBar`: the app owns the selected index
//! (mark one item `.active()`, flip your atom in `.on_press`). Fully
//! themeable per the Phase 32 customization sweep — every color/metric
//! has a D094 builder; defaults come from the live theme's tokens.

use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;

use super::container::draw_rounded_rect_pub;
use super::{avail_w, LayoutCtx, PaintCtx, Widget};

/// One destination in a [`BottomNavigationBar`].
pub struct BottomNavItem {
    label: String,
    icon: Option<super::BoxedWidget>,
    badge: Option<u32>,
    active: bool,
    on_press: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl BottomNavItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), icon: None, badge: None, active: false, on_press: None }
    }
    /// Icon shown above the label (any widget — usually [`super::Icon`]).
    pub fn icon(mut self, w: impl Widget + 'static) -> Self {
        self.icon = Some(Box::new(w));
        self
    }
    pub fn badge(mut self, n: u32) -> Self { self.badge = Some(n); self }
    pub fn active(mut self) -> Self { self.active = true; self }
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(f));
        self
    }
}

/// The bar itself — a multi-destination leaf (paints its items directly,
/// the `NavRail` pattern).
pub struct BottomNavigationBar {
    items: Vec<BottomNavItem>,
    height: f32,
    background: Option<Color>,
    active_color: Option<Color>,
    inactive_color: Option<Color>,
    /// Corner radius for the bar's TOP corners (a floating/inset bar look);
    /// `0.0` = the classic edge-to-edge flat bar.
    radius: f32,
    font_size: f32,
    /// `0.0` hides the top hairline divider.
    divider_width: f32,
}

impl BottomNavigationBar {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            height: 56.0,
            background: None,
            active_color: None,
            inactive_color: None,
            radius: 0.0,
            font_size: 10.5,
            divider_width: 1.0,
        }
    }
    pub fn item(mut self, i: BottomNavItem) -> Self { self.items.push(i); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    /// Bar fill — defaults to the theme's `surface`.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Selected label/icon tint — defaults to the theme's `primary`.
    pub fn active_color(mut self, c: Color) -> Self { self.active_color = Some(c); self }
    /// Unselected tint — defaults to the theme's `on_surface` dimmed.
    pub fn inactive_color(mut self, c: Color) -> Self { self.inactive_color = Some(c); self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
    pub fn no_divider(mut self) -> Self { self.divider_width = 0.0; self }
}

impl Default for BottomNavigationBar {
    fn default() -> Self { Self::new() }
}

impl Widget for BottomNavigationBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        Size { width: avail_w(ctx.constraints), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (the borrow must end before mutable painting).
        let (background, active, inactive, outline, err_bg, err_fg) = {
            let t = &ctx.theme.colors;
            let on_surface = ctx.tc(t.on_surface);
            (
                self.background.unwrap_or_else(|| ctx.tc(t.surface)),
                self.active_color.unwrap_or_else(|| ctx.tc(t.primary)),
                self.inactive_color.unwrap_or(Color::rgba(
                    on_surface.r, on_surface.g, on_surface.b, 150,
                )),
                ctx.tc(t.outline),
                ctx.tc(t.error),
                ctx.tc(t.on_error),
            )
        };

        let r = ctx.rect;
        if self.radius > 0.0 {
            draw_rounded_rect_pub(ctx, r, background, self.radius);
        } else {
            ctx.fill_rect(r, background);
        }
        if self.divider_width > 0.0 && self.radius == 0.0 {
            ctx.fill_rect(
                Rect { origin: r.origin, size: Size { width: r.size.width, height: self.divider_width } },
                outline,
            );
        }

        if self.items.is_empty() {
            return;
        }
        // Equal spread — the universal bottom-nav convention.
        let slot_w = r.size.width / self.items.len() as f32;

        for (i, item) in self.items.iter().enumerate() {
            let slot = Rect {
                origin: Point { x: r.origin.x + slot_w * i as f32, y: r.origin.y },
                size: Size { width: slot_w, height: r.size.height },
            };
            let mut slot_ctx = ctx.child(slot);
            // Destinations are links (the D107 <nav><a> shape, same as NavRail).
            let mut sem = super::Semantics::new(rosace_core::Role::Link).label(&item.label);
            if let Some(n) = item.badge { sem = sem.value(n.to_string()); }
            slot_ctx.semantics(sem);

            let tint = if item.active { active } else { inactive };
            let line_h = slot_ctx.font.line_height(self.font_size);

            // Icon above label when present; label alone centers vertically.
            let mut label_y = slot.origin.y + (slot.size.height - line_h) / 2.0;
            if let Some(icon) = &item.icon {
                let icon_box = 22.0f32;
                let content_h = icon_box + 3.0 + line_h;
                let top = slot.origin.y + (slot.size.height - content_h) / 2.0;
                let is = icon.layout(&slot_ctx.layout_ctx(Constraints::loose(icon_box, icon_box)));
                icon.paint(&mut slot_ctx.child(Rect {
                    origin: Point { x: slot.origin.x + (slot.size.width - is.width) / 2.0, y: top },
                    size: is,
                }));
                label_y = top + icon_box + 3.0;
            }

            let text_w = slot_ctx.font.measure_text(&item.label, self.font_size);
            let label_x = slot.origin.x + (slot.size.width - text_w) / 2.0;
            slot_ctx.draw_text_at(
                &item.label,
                Point { x: label_x, y: label_y },
                tint,
                self.font_size,
            );

            if let Some(n) = item.badge {
                let ns = n.to_string();
                let bw = ns.len() as f32 * 7.0 + 8.0;
                let bx = slot.origin.x + slot.size.width / 2.0 + 6.0;
                let by = slot.origin.y + 6.0;
                draw_rounded_rect_pub(
                    &mut slot_ctx,
                    Rect { origin: Point { x: bx, y: by }, size: Size { width: bw, height: 15.0 } },
                    err_bg,
                    7.5,
                );
                slot_ctx.draw_text_at(&ns, Point { x: bx + 4.0, y: by + 2.5 }, err_fg, 8.5);
            }

            if let Some(cb) = &item.on_press {
                slot_ctx.register_hit(Arc::clone(cb));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_takes_full_width_and_its_configured_height() {
        let bar = BottomNavigationBar::new()
            .height(64.0)
            .item(BottomNavItem::new("Home"))
            .item(BottomNavItem::new("Search"));
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(390.0, 800.0), &font, &theme);
        let size = bar.layout(&ctx);
        assert_eq!(size.width, 390.0);
        assert_eq!(size.height, 64.0);
    }

    #[test]
    fn default_height_matches_the_platform_convention() {
        let bar = BottomNavigationBar::new();
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(320.0, 600.0), &font, &theme);
        assert_eq!(bar.layout(&ctx).height, 56.0);
    }
}
