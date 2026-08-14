use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx};

#[derive(Debug, Clone, Copy, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
    Danger,
    Success,
    Link,
}

/// A clickable labeled button.
///
/// Attach a callback with `.on_press(|| ...)`. The callback fires when the
/// button is clicked — no boilerplate needed.
pub struct Button {
    pub label: String,
    pub variant: ButtonVariant,
    pub disabled: bool,
    pub icon: Option<Box<dyn Widget>>,
    pub width: Option<f32>,
    pub height: f32,
    /// `None` = read from the active theme's `typography.label_large`
    /// (D127 "environment" track — see `Checkbox::resolved_font_size`'s doc
    /// for the reasoning).
    pub font_size: Option<f32>,
    pub radius: f32,
    background: Option<Color>,
    color: Option<Color>,
    on_press: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::Primary,
            disabled: false,
            icon: None,
            width: None,
            height: 34.0,
            font_size: None,
            radius: 6.0,
            background: None,
            color: None,
            on_press: None,
        }
    }

    pub fn variant(mut self, v: ButtonVariant) -> Self { self.variant = v; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    /// Conditional form of [`Self::disabled`] (D116 Phase 28 Step 8) — the
    /// natural way to gate a submit button on `form.is_valid()` without an
    /// `if`/`else` at every call site: `Button::new("Submit").disabled_if(!form.is_valid())`.
    pub fn disabled_if(mut self, condition: bool) -> Self {
        if condition { self.disabled = true; }
        self
    }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = Some(s); self }

    fn resolved_font_size(&self, theme: &rosace_theme::ThemeData) -> f32 {
        self.font_size.unwrap_or(theme.typography.label_large.size)
    }
    /// Overrides the variant's own fill color — for a one-off custom color
    /// outside the Primary/Secondary/Ghost/Danger/Success/Link palette.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Overrides the variant's own label/icon color.
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn icon(mut self, w: impl Widget + 'static) -> Self { self.icon = Some(Box::new(w)); self }

    /// Set the click handler. The closure is called on every left-click
    /// inside the button's bounds.
    pub fn on_press(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(f));
        self
    }
}

impl Widget for Button {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let constraints = ctx.constraints;
        let font_size = self.resolved_font_size(ctx.theme);
        // Measure for real rather than estimating `len() * size * 0.6`.
        // The estimate is wrong twice over: it ignores per-glyph advances
        // (so "IIII" and "WWWW" size identically), and — the reason it was
        // caught — it never sees the OS text-size multiplier, because
        // `measure_text` is what applies `text_scale`. At 150% Dynamic Type
        // the label rendered larger while the pill kept its 100% width, so
        // the text spilled out of the button (reported live on iOS,
        // 2026-08-09).
        let text_w = ctx.font.measure_text(&self.label, font_size);
        // Same rough per-char estimate `layout()` already used for the
        // label — an icon's own reported size is available at `paint()`
        // time via a real `LayoutCtx`, unavailable here without one; the
        // fixed `font_size + 4.0` box matches what `paint()` lays it out
        // at, plus the same gap.
        let icon_w = if self.icon.is_some() { font_size + 4.0 + 6.0 } else { 0.0 };
        let w = self.width.unwrap_or(text_w + icon_w + 32.0);
        // Height must grow with the text too, for the same reason — a fixed
        // pill height clips a scaled-up label vertically. Keep the designed
        // height as a MINIMUM so ordinary buttons are unchanged at 100%.
        // ...and the layout adds the tap-target floor on top (Quality Bar
        // §6) — 34px was under it. That last part is transparent padding:
        // `paint` still draws the pill at `text_fit_height`, so the button
        // does not visually fatten, it just stops being cramped against its
        // neighbours.
        constraints.constrain(Size {
            width: w.max(super::MIN_TAP_TARGET),
            height: super::control_height(self.height, ctx.font, font_size),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Button).label(&self.label));
        let t = &ctx.theme.colors;
        let variant = if self.disabled { ButtonVariant::Secondary } else { self.variant };

        let (bg, fg, border) = match variant {
            ButtonVariant::Primary   => (ctx.tc(t.primary),    ctx.tc(t.on_primary),   None),
            ButtonVariant::Secondary => (ctx.tc(t.secondary),  ctx.tc(t.on_secondary), None),
            ButtonVariant::Ghost     => (Color::rgba(0,0,0,0), ctx.tc(t.primary),      Some(ctx.tc(t.outline))),
            ButtonVariant::Link      => (Color::rgba(0,0,0,0), ctx.tc(t.primary),      None),
            ButtonVariant::Danger    => (ctx.tc(t.error), ctx.tc(t.on_error), None),
            // No `success` token exists in `ColorScheme` (Material's own
            // scheme has none either), so this stays a documented semantic
            // constant rather than being forced onto an unrelated token.
            ButtonVariant::Success   => (Color::rgb( 40, 160, 80), Color::rgb(220, 255, 230), None),
        };

        let bg = if self.disabled { bg } else { self.background.unwrap_or(bg) };
        let fg = if self.disabled { ctx.tc(t.outline) } else { self.color.unwrap_or(fg) };

        // Hover/press feedback: lift the fill toward white (opaque variants)
        // or add a faint wash (ghost/link), eased between three levels (D108
        // Phase 26 Step 1) — idle, hover (matches the old flat lift), press
        // (double it, so a tap reads as visually distinct from a hover).
        let target = if self.disabled { 0.0 } else if ctx.pressed() { 1.0 } else if ctx.hovered() { 0.5 } else { 0.0 };
        let emphasis = ctx.animate_to(target, 0.0);
        let bg = if emphasis > 0.0 {
            if bg.a == 0 {
                Color::rgba(255, 255, 255, (22.0 * emphasis * 2.0).min(255.0) as u8)
            } else {
                lighten(bg, (0.12 * emphasis * 2.0).min(1.0))
            }
        } else {
            bg
        };

        // The VISUAL pill, centred in the (taller) tap target.
        let font_size_h = self.resolved_font_size(&ctx.theme);
        let r = super::centered_visual(ctx.rect, ctx.rect.size.width,
            super::text_fit_height(self.height, ctx.font, font_size_h));
        super::container::draw_rounded_rect_pub(ctx, r, bg, self.radius);

        if let Some(bc) = border {
            ctx.stroke_rrect(r, self.radius, bc, 1.0);
        }

        let font_size = self.resolved_font_size(&ctx.theme);
        let text_w = ctx.font.measure_text(&self.label, font_size);
        let line_h = ctx.font.line_height(font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);

        // `.icon()` was settable but never actually painted — the field
        // existed, `paint()` just never read it (found live: a showcase
        // AppBar button set one and nothing showed).
        const ICON_GAP: f32 = 6.0;
        if let Some(icon) = &self.icon {
            let is = icon.layout(&ctx.layout_ctx(Constraints::loose(font_size + 4.0, font_size + 4.0)));
            let content_w = is.width + ICON_GAP + text_w;
            let start_x = ((r.size.width - content_w) / 2.0).max(4.0);
            let iy = r.origin.y + (r.size.height - is.height) / 2.0;
            ctx.paint_child(Rect {
                origin: Point { x: r.origin.x + start_x, y: iy },
                size: is,
            }, &*icon);
            ctx.text(&self.label, start_x + is.width + ICON_GAP, ty, fg, font_size);
        } else {
            let tx = ((r.size.width - text_w) / 2.0).max(4.0);
            ctx.text(&self.label, tx, ty, fg, font_size);
        }

        // Interactive-by-identity (Phase 32, user directive): a Button
        // ALWAYS owns its hit region, wired or not — a click on it must
        // never fall through to whatever positional region (drag-to-pan)
        // sits behind it. Unwired = absorb, do nothing.
        if !self.disabled {
            match &self.on_press {
                Some(cb) => ctx.register_hit(Arc::clone(cb)),
                None => ctx.register_hit(Arc::new(|| {})),
            }
        }
    }
}

/// Blend a color toward white by `t` (0..1) — hover/pressed lift.
pub(super) fn lighten(c: Color, t: f32) -> Color {
    let mix = |v: u8| (v as f32 + (255.0 - v as f32) * t).round() as u8;
    Color::rgba(mix(c.r), mix(c.g), mix(c.b), c.a)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn background_and_color_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = Button::new("Save").width(90.0);
        let customized = Button::new("Save").width(90.0)
            .background(Color::rgb(20, 20, 20))
            .color(Color::rgb(255, 255, 255));
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }

    /// The tap-target floor must be transparent PADDING, not a fatter pill.
    ///
    /// Asserted against the emitted draw command rather than the layout size,
    /// because only the draw command can tell "reserved 44 and painted 34"
    /// apart from "grew the button to 44" — and the second would silently
    /// restyle every button in every app.
    #[test]
    fn the_tap_target_floor_pads_the_button_it_does_not_fatten_the_pill() {
        use rosace_render::{DrawCommand, PictureRecorder};
        use crate::tree::{PaintCtx, RenderTree};
        use rosace_core::types::{Point, Rect, Size};
        use std::{cell::RefCell, rc::Rc};

        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx_l = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let b = Button::new("Save").width(90.0);

        let laid_out = b.layout(&ctx_l);
        assert_eq!(laid_out.height, crate::tree::MIN_TAP_TARGET, "must reserve the target");

        let mut rec = PictureRecorder::new();
        {
            let mut ctx = PaintCtx::root(
                &mut rec,
                Rect { origin: Point { x: 0.0, y: 0.0 },
                       size: Size { width: 90.0, height: laid_out.height } },
                &font,
                theme.clone(),
                Rc::new(RefCell::new(RenderTree::new())),
            );
            b.paint(&mut ctx);
        }
        let pill = rec.finish().commands.iter().find_map(|c| match c {
            DrawCommand::FillRRect { rect, .. } => Some(*rect),
            _ => None,
        }).expect("the pill is drawn as a rounded rect");

        let designed = crate::tree::text_fit_height(34.0, &font, b.resolved_font_size(&theme));
        assert_eq!(pill.size.height, designed, "pill must keep its designed height");
        assert!(pill.size.height < laid_out.height, "the extra must be padding");
        // ...and the pill must be centred in it, not top-aligned.
        assert!((pill.origin.y - (laid_out.height - designed) / 2.0).abs() < 0.01);
    }

}
