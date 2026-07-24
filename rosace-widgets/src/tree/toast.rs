use rosace_core::types::{Point, Size};
use rosace_render::Color;
use rosace_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx};
use super::container::draw_rounded_rect_pub;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// A transient notification pill. Pair with [`OverlayApi::toast`], which
/// floats it above the bottom edge, and [`Toast::show`] for auto-dismiss:
///
/// ```rust,ignore
/// let saved = ctx.state(false);
/// Button::new("Save")
///     .on_press({ let s = saved.clone(); move || { /* save */ Toast::show(&s, 2.5); } })
///     .toast(saved.clone(), || Box::new(Toast::success("Saved!")))
/// ```
///
/// [`OverlayApi::toast`]: super::overlay_api::OverlayApi::toast
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub font_size: f32,
    background: Option<Color>,
    color: Option<Color>,
    accent: Option<Color>,
    radius: Option<f32>,
}

impl Toast {
    pub fn info(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ToastKind::Info, font_size: 13.0, background: None, color: None, accent: None, radius: None }
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ToastKind::Success, font_size: 13.0, background: None, color: None, accent: None, radius: None }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ToastKind::Error, font_size: 13.0, background: None, color: None, accent: None, radius: None }
    }

    /// Pill fill color (theme's `surface_variant` if unset).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Message text color (theme's `on_surface` if unset).
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    /// Leading dot color (overrides the info/success/error default).
    pub fn accent(mut self, c: Color) -> Self { self.accent = Some(c); self }
    /// Corner radius (half the pill's height — a full pill shape — if unset).
    pub fn radius(mut self, r: f32) -> Self { self.radius = Some(r); self }

    /// Open the toast and auto-dismiss after `secs` seconds.
    ///
    /// Spawns a timer thread; the closing `atom.set(false)` wakes the event
    /// loop via the registered frame-request hook, so the toast disappears
    /// without any user input.
    pub fn show(open: &Atom<bool>, secs: f32) {
        open.set(true);
        let open = open.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs_f32(secs));
            open.set(false);
        });
    }

    fn resolve_accent(&self, ctx: &PaintCtx) -> Color {
        if let Some(c) = self.accent { return c; }
        let t = &ctx.theme.colors;
        match self.kind {
            ToastKind::Info    => ctx.tc(t.primary),
            ToastKind::Success => Color::rgb(72, 199, 116),
            ToastKind::Error   => ctx.tc(t.error),
        }
    }
}

const PAD_H: f32 = 16.0;
const PAD_V: f32 = 10.0;
const DOT: f32 = 8.0;
const GAP: f32 = 10.0;

impl Widget for Toast {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let text_w = ctx.font.measure_text(&self.message, self.font_size);
        let line_h = ctx.font.line_height(self.font_size);
        ctx.constraints.constrain(Size {
            width: PAD_H * 2.0 + DOT + GAP + text_w,
            height: PAD_V * 2.0 + line_h.max(DOT),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::Alert).label(&self.message));
        let r = ctx.rect;
        let radius = self.radius.unwrap_or(r.size.height / 2.0);
        let (bg, fg) = {
            let t = &ctx.theme.colors;
            (self.background.unwrap_or_else(|| ctx.tc(t.surface_variant)),
             self.color.unwrap_or_else(|| ctx.tc(t.on_surface)))
        };
        ctx.fill_shadow_rrect(r, radius, Color::rgba(0, 0, 0, 90), 10.0);
        draw_rounded_rect_pub(ctx, r, bg, radius);

        let accent = self.resolve_accent(ctx);
        ctx.fill_circle(Point {
            x: r.origin.x + PAD_H + DOT / 2.0,
            y: r.origin.y + r.size.height / 2.0,
        }, DOT / 2.0, accent);

        let line_h = ctx.font.line_height(self.font_size);
        ctx.draw_text_at(
            &self.message,
            Point {
                x: r.origin.x + PAD_H + DOT + GAP,
                y: r.origin.y + (r.size.height - line_h) / 2.0,
            },
            fg,
            self.font_size,
        );
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
        let base = Toast::info("Saved");
        let customized = Toast::info("Saved")
            .background(Color::rgb(10, 10, 10))
            .color(Color::rgb(255, 255, 255))
            .accent(Color::rgb(0, 200, 0))
            .radius(4.0);
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }
}
