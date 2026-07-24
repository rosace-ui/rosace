use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx};
use super::overlay::{OverlayEntry, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig, push_overlay};
use super::menu::Menu;
use rosace_render::Color;

/// A select control: a trigger showing the current option; tapping opens a
/// Menu of options below it; choosing one calls `on_change(index)`.
pub struct Dropdown {
    options: Vec<String>,
    selected: usize,
    open: Atom<bool>,
    disabled: bool,
    width: f32,
    background: Option<Color>,
    color: Option<Color>,
    border_color: Option<Color>,
    border_width: f32,
    radius: f32,
    on_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl Dropdown {
    pub fn new(options: Vec<impl Into<String>>, selected: usize, open: Atom<bool>) -> Self {
        Self {
            options: options.into_iter().map(Into::into).collect(), selected, open, disabled: false, width: 200.0,
            background: None, color: None, border_color: None, border_width: 1.0, radius: 8.0,
            on_change: None,
        }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    /// Trigger fill color (theme's `surface_variant` if unset).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Label/chevron color (theme's `on_surface` if unset).
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn border(mut self, c: Color, w: f32) -> Self { self.border_color = Some(c); self.border_width = w; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn on_change(mut self, f: impl Fn(usize) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f)); self
    }
}

impl Widget for Dropdown {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        ctx.constraints.constrain(Size { width: self.width, height: 36.0 })
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let selected_label = self.options.get(self.selected).map(|s| s.as_str()).unwrap_or("");
        // The trigger is a button that opens a menu — its own MenuItem
        // children (via Menu, which already declares semantics) carry the
        // option list; this is just the current-selection summary.
        ctx.semantics(super::Semantics::new(rosace_core::Role::Button).label(selected_label));
        let (bg, fg, border) = {
            let t = &ctx.theme.colors;
            (self.background.unwrap_or_else(|| ctx.tc(t.surface_variant)),
             self.color.unwrap_or_else(|| ctx.tc(t.on_surface)),
             self.border_color.unwrap_or_else(|| ctx.tc(t.outline)))
        };
        let is_open = self.open.get();
        let focused = !self.disabled && ctx.focus_node().is_focused();
        let hovered = !self.disabled && ctx.hovered();
        let pressed = !self.disabled && ctx.pressed();
        let wash = ctx.animate_channel(0, if pressed { 0.10 } else if hovered { 0.05 } else { 0.0 }, 0.0);
        let dim = if self.disabled { 0.45 } else { 1.0 };
        let with_alpha = |c: Color, a: f32| Color::rgba(c.r, c.g, c.b, ((c.a as f32 / 255.0) * a.clamp(0.0, 1.0) * 255.0).round() as u8);

        let r = ctx.rect;
        let mut bg = bg;
        if wash > 0.001 { bg = super::lerp_color(bg, Color::rgb(255, 255, 255), wash); }
        ctx.fill_rrect(r, self.radius, with_alpha(bg, dim));
        // Focus/open border brightens toward the accent.
        let ring = if focused || is_open { ctx.tc(ctx.theme.colors.primary) } else { border };
        ctx.stroke_rrect(r, self.radius, with_alpha(ring, dim), if focused || is_open { 1.5 } else { self.border_width });
        let lh = ctx.font.line_height(13.0);
        ctx.draw_text_at(selected_label, Point { x: r.origin.x + 12.0, y: r.origin.y + (r.size.height - lh) / 2.0 }, with_alpha(fg, dim), 13.0);
        // Chevron flips ▾→▴ when open.
        let chev = if is_open { "\u{25b4}" } else { "\u{25be}" };
        let cw = ctx.font.measure_text(chev, 13.0);
        ctx.draw_text_at(chev, Point { x: r.origin.x + r.size.width - cw - 10.0, y: r.origin.y + (r.size.height - lh) / 2.0 }, with_alpha(fg, dim), 13.0);

        if !self.disabled {
            let open = self.open.clone();
            ctx.register_hit(Arc::new(move || open.set(true)));
        } else {
            ctx.register_hit(Arc::new(|| {}));
        }

        if self.open.get() {
            let pos = Point { x: r.origin.x, y: r.origin.y + r.size.height + 4.0 };
            let mut menu = Menu::new().min_width(self.width);
            for (i, opt) in self.options.iter().enumerate() {
                let open = self.open.clone();
                let cb = self.on_change.clone();
                menu = menu.item(opt.clone(), move || {
                    open.set(false);
                    if let Some(cb) = &cb { cb(i); }
                });
            }
            let open2 = self.open.clone();
            push_overlay(
                OverlayEntry::new(LayerPosition::Absolute(pos), menu)
                    .input(InputBehavior::PassThrough)
                    .focus(FocusBehavior::PassThrough)
                    .scrim(ScrimConfig { color: Color::TRANSPARENT, on_tap: Some(Arc::new(move || open2.set(false))) }),
            );
        }
    }
}

// Silence unused Rect import in some configs.
#[allow(unused_imports)]
use Rect as _RectUsed;

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn customization_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let open = Atom::new(rosace_state::next_atom_id(), false);
        let dd = Dropdown::new(vec!["A", "B"], 0, open)
            .background(Color::rgb(10, 10, 10))
            .color(Color::rgb(255, 255, 255))
            .border(Color::rgb(200, 0, 0), 2.0)
            .radius(4.0);
        let size = dd.layout(&ctx);
        assert_eq!((size.width, size.height), (200.0, 36.0));
    }
}
