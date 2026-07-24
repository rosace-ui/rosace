use std::sync::Arc;

use rosace_core::types::{Rect, Size};
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

type Item = (String, Arc<dyn Fn() + Send + Sync>);

/// A vertical list of pressable rows — the standard dropdown content.
///
/// Pair with [`OverlayApi::dropdown`], which anchors it below the trigger:
///
/// ```rust,ignore
/// Button::new("File")
///     .dropdown(open.clone(), move || Box::new(
///         Menu::new()
///             .item("New",  { let o = open.clone(); move || { o.set(false); /* … */ } })
///             .item("Open", { let o = open.clone(); move || { o.set(false); /* … */ } })
///     ))
/// ```
///
/// [`OverlayApi::dropdown`]: super::overlay_api::OverlayApi::dropdown
pub struct Menu {
    items: Vec<Item>,
    pub min_width: f32,
    pub row_height: f32,
    pub font_size: f32,
    pub radius: f32,
    background: Option<Color>,
    color: Option<Color>,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            min_width: 180.0,
            row_height: 34.0,
            font_size: 13.0,
            radius: 14.0,
            background: None,
            color: None,
        }
    }

    pub fn min_width(mut self, w: f32) -> Self { self.min_width = w; self }
    pub fn row_height(mut self, h: f32) -> Self { self.row_height = h; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    /// Menu surface fill color (theme's `surface` if unset).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Item label color (theme's `on_surface` if unset).
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }

    /// Append a pressable row. The callback fires on click; close the menu
    /// yourself by setting the `open` atom false inside it.
    pub fn item(mut self, label: impl Into<String>, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.items.push((label.into(), Arc::new(f)));
        self
    }
}

impl Default for Menu {
    fn default() -> Self { Self::new() }
}

const PAD_V: f32 = 6.0;
const PAD_H: f32 = 14.0;

impl Widget for Menu {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let widest = self.items.iter()
            .map(|(label, _)| ctx.font.measure_text(label, self.font_size))
            .fold(0.0_f32, f32::max);
        let width = (widest + PAD_H * 2.0).max(self.min_width);
        let height = self.items.len() as f32 * self.row_height + PAD_V * 2.0;
        ctx.constraints.constrain(Size { width, height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let (bg, fg, outline) = {
            let t = &ctx.theme.colors;
            // Default panel is TRANSLUCENT (the overlay pass alpha-blends
            // over the app) with a hairline border — a popup reads as part
            // of the scene instead of an opaque slab punched over it
            // (found live: the dropdown menu broke the liquid-glass app's
            // whole look). Real backdrop-glass popups need overlay-pass
            // shader-quad support — named, not built. An explicit
            // `.background()` opts out entirely.
            let default_bg = {
                let s = ctx.tc(t.surface);
                Color { r: s.r, g: s.g, b: s.b, a: 216 }
            };
            (self.background.unwrap_or(default_bg),
             self.color.unwrap_or_else(|| ctx.tc(t.on_surface)),
             ctx.tc(t.outline))
        };
        let r = ctx.rect;
        ctx.fill_shadow_rrect(r, self.radius, Color::rgba(0, 0, 0, 90), 10.0);
        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        ctx.stroke_rrect(r, self.radius, Color { a: 120, ..outline }, 1.0);
        let line_h = ctx.font.line_height(self.font_size);

        for (i, (label, cb)) in self.items.iter().enumerate() {
            ctx.semantics(super::Semantics::new(rosace_core::Role::MenuItem).label(label));
            let row = Rect {
                origin: rosace_core::types::Point {
                    x: r.origin.x,
                    y: r.origin.y + PAD_V + i as f32 * self.row_height,
                },
                size: Size { width: r.size.width, height: self.row_height },
            };
            let ty = row.origin.y + (self.row_height - line_h) / 2.0;
            ctx.draw_text_at(
                label,
                rosace_core::types::Point { x: row.origin.x + PAD_H, y: ty },
                fg,
                self.font_size,
            );
            // register_hit uses the ctx rect — derive a child ctx for the row
            // so the hit rect is clip-aware.
            ctx.child(row).register_hit(Arc::clone(cb));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn customization_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = Menu::new().item("Item one", || {});
        let customized = Menu::new()
            .background(Color::rgb(20, 20, 20))
            .color(Color::rgb(255, 255, 255))
            .radius(2.0)
            .item("Item one", || {});
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }
}
