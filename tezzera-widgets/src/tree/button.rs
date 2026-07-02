use std::sync::Arc;

use tezzera_core::types::Size;
use tezzera_render::Color;
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
            on_press: None,
        }
    }

    pub fn variant(mut self, v: ButtonVariant) -> Self { self.variant = v; self }
    pub fn disabled(mut self) -> Self { self.disabled = true; self }
    pub fn width(mut self, w: f32) -> Self { self.width = Some(w); self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
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

        let fg = if self.disabled { ctx.tc(t.outline) } else { fg };

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

        // Register hit target so clicks fire the callback.
        if let Some(cb) = &self.on_press {
            if !self.disabled {
                ctx.register_hit(Arc::clone(cb));
            }
        }
    }
}
