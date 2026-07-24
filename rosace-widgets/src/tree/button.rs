use std::sync::Arc;

use rosace_core::types::Size;
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
    pub font_size: f32,
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
            font_size: 11.0,
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
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
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
        let text_w = self.label.len() as f32 * self.font_size * 0.6;
        let w = self.width.unwrap_or(text_w + 32.0);
        constraints.constrain(Size { width: w, height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::Button).label(&self.label));
        let t = &ctx.theme.colors;
        let variant = if self.disabled { ButtonVariant::Secondary } else { self.variant };

        let (bg, fg, border) = match variant {
            ButtonVariant::Primary   => (ctx.tc(t.primary),    ctx.tc(t.on_primary),   None),
            ButtonVariant::Secondary => (ctx.tc(t.secondary),  ctx.tc(t.on_secondary), None),
            ButtonVariant::Ghost     => (Color::rgba(0,0,0,0), ctx.tc(t.primary),      Some(ctx.tc(t.outline))),
            ButtonVariant::Link      => (Color::rgba(0,0,0,0), ctx.tc(t.primary),      None),
            ButtonVariant::Danger    => (Color::rgb(180, 50,  50), Color::rgb(255, 230, 230), None),
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

        let r = ctx.rect;
        super::container::draw_rounded_rect_pub(ctx, r, bg, self.radius);

        if let Some(bc) = border {
            ctx.stroke_rrect(r, self.radius, bc, 1.0);
        }

        let text_w = ctx.font.measure_text(&self.label, self.font_size);
        let tx = ((r.size.width - text_w) / 2.0).max(4.0);
        let line_h = ctx.font.line_height(self.font_size);
        let ty = ((r.size.height - line_h) / 2.0).max(0.0);
        ctx.text(&self.label, tx, ty, fg, self.font_size);

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
}
