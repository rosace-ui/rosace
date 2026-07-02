use tezzera_core::types::{Point, Size};
use tezzera_render::Color;
use tezzera_state::Atom;
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
}

impl Toast {
    pub fn info(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ToastKind::Info, font_size: 13.0 }
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ToastKind::Success, font_size: 13.0 }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { message: message.into(), kind: ToastKind::Error, font_size: 13.0 }
    }

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

    fn accent(&self, ctx: &PaintCtx) -> Color {
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
        let r = ctx.rect;
        ctx.fill_shadow(r, Color::rgba(0, 0, 0, 90), 10.0);
        draw_rounded_rect_pub(ctx, r, ctx.tc(ctx.theme.colors.surface_variant), r.size.height / 2.0);

        let accent = self.accent(ctx);
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
            ctx.tc(ctx.theme.colors.on_surface),
            self.font_size,
        );
    }
}
