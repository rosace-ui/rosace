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
    width: f32,
    on_change: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl Dropdown {
    pub fn new(options: Vec<impl Into<String>>, selected: usize, open: Atom<bool>) -> Self {
        Self { options: options.into_iter().map(Into::into).collect(), selected, open, width: 200.0, on_change: None }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
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
        let r = ctx.rect;
        ctx.fill_rrect(r, 8.0, ctx.tc(ctx.theme.colors.surface_variant));
        ctx.stroke_rrect(r, 8.0, ctx.tc(ctx.theme.colors.outline), 1.0);
        let fg = ctx.tc(ctx.theme.colors.on_surface);
        let lh = ctx.font.line_height(13.0);
        ctx.draw_text_at(selected_label, Point { x: r.origin.x + 12.0, y: r.origin.y + (r.size.height - lh) / 2.0 }, fg, 13.0);
        let chev = "\u{25be}";
        let cw = ctx.font.measure_text(chev, 13.0);
        ctx.draw_text_at(chev, Point { x: r.origin.x + r.size.width - cw - 10.0, y: r.origin.y + (r.size.height - lh) / 2.0 }, fg, 13.0);

        let open = self.open.clone();
        ctx.register_hit(Arc::new(move || open.set(true)));

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
