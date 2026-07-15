//! `FloatingActionButton` (D115/Phase 32 Step 1) — the circular primary
//! action button, made for `Scaffold::fab` (which already positions its
//! slot bottom-trailing above the bottom bar).
//!
//! Fully themeable per the Phase 32 customization sweep: color, shape
//! (circle by default, any radius via `.radius()`), size, elevation
//! shadow — all D094 builders with live-theme defaults.

use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;

use super::button::lighten;
use super::container::draw_rounded_rect_pub;
use super::{LayoutCtx, PaintCtx, Widget};

/// A floating action button. Content is an icon widget, a text label, or
/// the default "+" glyph.
pub struct FloatingActionButton {
    icon: Option<super::BoxedWidget>,
    label: Option<String>,
    size: f32,
    background: Option<Color>,
    foreground: Option<Color>,
    /// `None` = a perfect circle (`size / 2`); any explicit value makes a
    /// rounded square (Material's "large FAB" look at ~16).
    radius: Option<f32>,
    /// Shadow strength; `0.0` disables it.
    elevation: f32,
    disabled: bool,
    on_press: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl FloatingActionButton {
    pub fn new() -> Self {
        Self {
            icon: None,
            label: None,
            size: 56.0,
            background: None,
            foreground: None,
            radius: None,
            elevation: 1.0,
            disabled: false,
            on_press: None,
        }
    }
    /// Icon widget centered in the button (usually [`super::Icon`]).
    pub fn icon(mut self, w: impl Widget + 'static) -> Self {
        self.icon = Some(Box::new(w));
        self
    }
    /// Text content instead of an icon (e.g. "+" or a short label).
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    pub fn size(mut self, s: f32) -> Self { self.size = s; self }
    /// Fill — defaults to the theme's `primary`.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Content tint — defaults to the theme's `on_primary`.
    pub fn color(mut self, c: Color) -> Self { self.foreground = Some(c); self }
    /// Rounded-square shape instead of the default circle.
    pub fn radius(mut self, r: f32) -> Self { self.radius = Some(r); self }
    pub fn elevation(mut self, e: f32) -> Self { self.elevation = e; self }
    pub fn disabled(mut self, d: bool) -> Self { self.disabled = d; self }
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(f));
        self
    }
}

impl Default for FloatingActionButton {
    fn default() -> Self { Self::new() }
}

impl Widget for FloatingActionButton {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: self.size, height: self.size }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (the borrow must end before mutable painting).
        let (bg, fg, shadow) = {
            let t = &ctx.theme.colors;
            (
                self.background.unwrap_or_else(|| ctx.tc(t.primary)),
                self.foreground.unwrap_or_else(|| ctx.tc(t.on_primary)),
                ctx.tc(t.shadow),
            )
        };
        let radius = self.radius.unwrap_or(self.size / 2.0);
        let r = ctx.rect;

        let sem_label = self.label.clone().unwrap_or_else(|| "action".to_string());
        ctx.semantics(super::Semantics::new(rosace_core::Role::Button).label(&sem_label));

        // Soft drop shadow (cheap two-layer wash — real elevation shadows
        // are tracked later work, same note as AppBarStyle::elevation).
        if self.elevation > 0.0 && !self.disabled {
            let spread = 3.0 * self.elevation;
            draw_rounded_rect_pub(
                ctx,
                Rect {
                    origin: Point { x: r.origin.x - spread / 2.0, y: r.origin.y + spread },
                    size: Size { width: r.size.width + spread, height: r.size.height },
                },
                Color::rgba(shadow.r, shadow.g, shadow.b, 60),
                radius + spread / 2.0,
            );
        }

        // Hover/press lift, the Button convention (D108 Step 1).
        let target = if self.disabled { 0.0 } else if ctx.pressed() { 1.0 } else if ctx.hovered() { 0.5 } else { 0.0 };
        let emphasis = ctx.animate_to(target, 0.0);
        let bg = if self.disabled {
            Color::rgba(bg.r, bg.g, bg.b, 110)
        } else if emphasis > 0.0 {
            lighten(bg, (0.12 * emphasis * 2.0).min(1.0))
        } else {
            bg
        };
        draw_rounded_rect_pub(ctx, r, bg, radius);

        if let Some(icon) = &self.icon {
            let inner = self.size * 0.45;
            let is = icon.layout(&ctx.layout_ctx(Constraints::loose(inner, inner)));
            icon.paint(&mut ctx.child(Rect {
                origin: Point {
                    x: r.origin.x + (r.size.width - is.width) / 2.0,
                    y: r.origin.y + (r.size.height - is.height) / 2.0,
                },
                size: is,
            }));
        } else {
            let text = self.label.as_deref().unwrap_or("+");
            let px = self.size * 0.4;
            let tw = ctx.font.measure_text(text, px);
            let lh = ctx.font.line_height(px);
            ctx.draw_text_at(
                text,
                Point {
                    x: r.origin.x + (r.size.width - tw) / 2.0,
                    y: r.origin.y + (r.size.height - lh) / 2.0,
                },
                fg,
                px,
            );
        }

        if let Some(cb) = &self.on_press {
            if !self.disabled {
                ctx.register_hit(Arc::clone(cb));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fab_is_square_at_its_configured_size() {
        let fab = FloatingActionButton::new().size(64.0);
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = fab.layout(&ctx);
        assert_eq!((size.width, size.height), (64.0, 64.0));
    }

    #[test]
    fn default_size_is_the_material_convention() {
        let fab = FloatingActionButton::new();
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        assert_eq!(fab.layout(&ctx).width, 56.0);
    }
}
