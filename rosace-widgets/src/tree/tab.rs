use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, avail_w};

/// A single tab descriptor (its label).
pub struct Tab {
    pub label: String,
}

impl Tab {
    pub fn new(label: impl Into<String>) -> Self { Self { label: label.into() } }
}

/// A horizontal tab bar with an **animated** sliding underline under the
/// selected tab. Interactive (mouse + touch) when `.on_change` is wired.
///
/// Fully customizable (D094): every color/size has a builder; unset colors
/// resolve from the active theme (`surface`/`on_surface`/`primary`/`outline`),
/// so the bar adapts to light/dark and custom themes out of the box. The
/// underline slides between tabs with the theme's animation curve by default.
pub struct TabBar {
    tabs: Vec<Tab>,
    selected: usize,
    // Colors are `Option` → `None` means "use the theme token" (resolved in
    // paint), so the widget is theme-aware unless explicitly overridden.
    background: Option<Color>,
    active_color: Option<Color>,
    inactive_color: Option<Color>,
    indicator_color: Option<Color>,
    border_color: Option<Color>,
    height: f32,
    font_size: f32,
    animated: bool,
    on_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            selected: 0,
            background: None,
            active_color: None,
            inactive_color: None,
            indicator_color: None,
            border_color: None,
            height: 40.0,
            font_size: 13.0,
            animated: true,
            on_change: None,
        }
    }
    pub fn tab(mut self, t: Tab) -> Self { self.tabs.push(t); self }
    pub fn selected(mut self, i: usize) -> Self { self.selected = i; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
    /// Bar background — theme `surface` if unset.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Selected-tab label color — theme `on_surface` if unset.
    pub fn active_color(mut self, c: Color) -> Self { self.active_color = Some(c); self }
    /// Unselected-tab label color — a muted `on_surface` if unset.
    pub fn inactive_color(mut self, c: Color) -> Self { self.inactive_color = Some(c); self }
    /// Sliding-underline color — theme `primary` if unset.
    pub fn indicator_color(mut self, c: Color) -> Self { self.indicator_color = Some(c); self }
    /// Bottom-divider color — theme `outline` if unset.
    pub fn border_color(mut self, c: Color) -> Self { self.border_color = Some(c); self }
    /// Turn the sliding-underline animation off (on by default).
    pub fn animated(mut self, on: bool) -> Self { self.animated = on; self }
    /// Make the bar interactive: `f(index)` fires on tap of tab `index`.
    pub fn on_change(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
}

impl Default for TabBar {
    fn default() -> Self { Self::new() }
}

impl Widget for TabBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        Size { width: avail_w(ctx.constraints), height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Resolve theme-defaulted colors up front (borrow-hoist: theme read must
        // end before the mutable ctx paint calls).
        let (bg, active, inactive, indicator, border) = {
            let t = &ctx.theme.colors;
            let on_surface = ctx.tc(t.on_surface);
            let surface = ctx.tc(t.surface);
            (
                self.background.unwrap_or(surface),
                self.active_color.unwrap_or(on_surface),
                // Muted default: blend on_surface toward surface.
                self.inactive_color.unwrap_or_else(|| super::lerp_color(on_surface, surface, 0.5)),
                self.indicator_color.unwrap_or_else(|| ctx.tc(t.primary)),
                self.border_color.unwrap_or_else(|| ctx.tc(t.outline)),
            )
        };

        let r = ctx.rect;
        ctx.fill_rect(r, bg);
        // Bottom divider.
        ctx.fill_rect(
            Rect { origin: Point { x: r.origin.x, y: r.origin.y + r.size.height - 1.0 },
                   size: Size { width: r.size.width, height: 1.0 } },
            border,
        );

        if self.tabs.is_empty() { return; }
        let tab_w = r.size.width / self.tabs.len() as f32;

        // Animated indicator position: eased toward the selected index, so the
        // underline slides. `animate_to` uses the theme's animation curve.
        let pos = if self.animated {
            ctx.animate_to(self.selected as f32, 0.0)
        } else {
            self.selected as f32
        };
        let underline = Rect {
            origin: Point { x: r.origin.x + pos * tab_w + tab_w * 0.15, y: r.origin.y + r.size.height - 2.5 },
            size: Size { width: tab_w * 0.7, height: 2.5 },
        };
        ctx.fill_rect(underline, indicator);

        let with_alpha = |c: Color, a: f32| Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8);
        for (i, tab) in self.tabs.iter().enumerate() {
            let tab_x = r.origin.x + i as f32 * tab_w;
            let tab_rect = Rect { origin: Point { x: tab_x, y: r.origin.y }, size: Size { width: tab_w, height: r.size.height } };
            let mut child = ctx.child(tab_rect);
            let hov = child.hovered();
            let prs = child.pressed();
            // Hover/press wash behind the tab (an inactive tab under the pointer
            // reads as reachable — Material's tab state layer).
            if hov || prs {
                child.fill_rect(Rect {
                    origin: Point { x: tab_x + 2.0, y: r.origin.y + 2.0 },
                    size: Size { width: tab_w - 4.0, height: r.size.height - 4.0 },
                }, with_alpha(active, if prs { 0.10 } else { 0.06 }));
            }
            // Label color crossfades toward `active` as the indicator nears it,
            // and brightens further while hovered.
            let nearness = (1.0 - (pos - i as f32).abs()).clamp(0.0, 1.0);
            let mut label_color = super::lerp_color(inactive, active, nearness);
            if hov { label_color = super::lerp_color(label_color, active, 0.6); }
            let text_w = child.font.measure_text(&tab.label, self.font_size);
            let line_h = child.font.line_height(self.font_size);
            let tx = tab_x + (tab_w - text_w) / 2.0;
            let ty = r.origin.y + (r.size.height - line_h) / 2.0;
            child.draw_text_at(&tab.label, Point { x: tx, y: ty }, label_color, self.font_size);

            child.semantics(
                super::Semantics::new(rosace_core::Role::Tab)
                    .label(&tab.label)
                    .value(if i == self.selected { "selected" } else { "not selected" }),
            );
            // Interactive-by-identity: ALWAYS register a hit so a tap on the bar
            // never falls through to drag-to-pan behind it; fires on_change when wired.
            match &self.on_change {
                Some(cb) => { let cb = cb.clone(); child.register_hit(Arc::new(move || cb(i))); }
                None => child.register_hit(Arc::new(|| {})),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn fills_width_and_fixed_height() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let bar = TabBar::new().tab(Tab::new("A")).tab(Tab::new("B")).height(44.0);
        let size = bar.layout(&ctx);
        assert_eq!(size.width, 400.0);
        assert_eq!(size.height, 44.0);
    }
}
