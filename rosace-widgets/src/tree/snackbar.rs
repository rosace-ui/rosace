//! `Snackbar` (D115/Phase 32 Step 1) — [`Toast`]'s action-bearing
//! sibling: a bottom-anchored message WITH an action button ("UNDO",
//! "RETRY"), the Material convention for reversible operations.
//!
//! Same visibility model as `Toast`: the app owns an `Atom<bool>` and
//! conditionally includes the snackbar in its build;
//! [`Snackbar::show`] opens it with an auto-dismiss timer. Fully
//! customizable per the Phase 32 sweep (background, text/action colors,
//! radius, font size), theme-token defaults.

use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use rosace_state::Atom;

use super::container::draw_rounded_rect_pub;
use super::{LayoutCtx, PaintCtx, Widget};

const PAD_H: f32 = 16.0;
const GAP: f32 = 16.0;

pub struct Snackbar {
    message: String,
    action_label: Option<String>,
    on_action: Option<Arc<dyn Fn() + Send + Sync>>,
    height: f32,
    background: Option<Color>,
    text_color: Option<Color>,
    action_color: Option<Color>,
    radius: f32,
    font_size: f32,
}

impl Snackbar {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            action_label: None,
            on_action: None,
            height: 46.0,
            background: None,
            text_color: None,
            action_color: None,
            radius: 0.0,
            font_size: 13.0,
        }
    }
    /// The action button ("UNDO", "RETRY") and its callback.
    pub fn action(mut self, label: impl Into<String>, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.action_label = Some(label.into());
        self.on_action = Some(Arc::new(f));
        self
    }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    /// Panel fill — defaults to an inverse-surface look derived from the
    /// theme (`on_background` at high opacity, the Material convention).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Message color — defaults to the theme's `background` (inverse text).
    pub fn color(mut self, c: Color) -> Self { self.text_color = Some(c); self }
    /// Action label color — defaults to the theme's `primary`.
    pub fn action_color(mut self, c: Color) -> Self { self.action_color = Some(c); self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }

    /// Present as a floating overlay pinned bottom-center, above ALL
    /// content (the Scaffold-level surface the platform convention
    /// demands — a snackbar is never an inline child of where it was
    /// declared). Call while your open-atom is true; same per-frame
    /// re-push convention as `Drawer::emit`. Clicks outside it pass
    /// through; the action button still receives its own hits.
    pub fn emit(self) {
        use super::overlay::{push_overlay, InputBehavior, FocusBehavior, LayerPosition, OverlayEntry};
        // Android-convention docked bar (user-specified): full width,
        // flush with the Scaffold's bottom — the engine raises
        // BottomAnchored overlays above the bottom nav bar when one is
        // present (bottom-overlay-inset channel).
        push_overlay(
            OverlayEntry::new(LayerPosition::BottomAnchored, self)
                .input(InputBehavior::PassThrough)
                .focus(FocusBehavior::Inert),
        );
    }

    /// Open the snackbar and auto-dismiss after `secs` seconds — same
    /// timer model as [`super::Toast::show`].
    pub fn show(open: &Atom<bool>, secs: f32) {
        open.set(true);
        let open = open.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs_f32(secs));
            open.set(false);
        });
    }
}

impl Widget for Snackbar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // A snackbar is a BAR, not a pill (user-reported: intrinsic width
        // read as a Toast): fill the available width minus margins,
        // capped for very wide windows, never narrower than its content.
        let text_w = ctx.font.measure_text(&self.message, self.font_size);
        let action_w = self
            .action_label
            .as_ref()
            .map(|a| GAP + ctx.font.measure_text(a, self.font_size))
            .unwrap_or(0.0);
        let content_w = PAD_H * 2.0 + text_w + action_w;
        let avail = ctx.constraints.max_width_f32();
        // Android-style docked bar: edge-to-edge full width.
        let w = avail.max(content_w.min(avail));
        Size { width: w, height: self.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (borrow must end before mutable painting).
        let (bg, fg, action_fg) = {
            let t = &ctx.theme.colors;
            let inv = ctx.tc(t.on_background);
            (
                self.background.unwrap_or(Color::rgba(inv.r, inv.g, inv.b, 235)),
                self.text_color.unwrap_or_else(|| ctx.tc(t.background)),
                self.action_color.unwrap_or_else(|| ctx.tc(t.primary)),
            )
        };
        let r = ctx.rect;

        ctx.semantics(super::Semantics::new(rosace_core::Role::Alert).label(&self.message));
        draw_rounded_rect_pub(ctx, r, bg, self.radius);

        let line_h = ctx.font.line_height(self.font_size);
        let ty = r.origin.y + (r.size.height - line_h) / 2.0;
        ctx.draw_text_at(
            &self.message,
            Point { x: r.origin.x + PAD_H, y: ty },
            fg,
            self.font_size,
        );

        if let Some(label) = &self.action_label {
            let aw = ctx.font.measure_text(label, self.font_size);
            let ax = r.origin.x + r.size.width - PAD_H - aw;
            // The action gets its own hit slot (a button inside an alert).
            let hit = Rect {
                origin: Point { x: ax - 8.0, y: r.origin.y },
                size: Size { width: aw + 16.0, height: r.size.height },
            };
            let mut action_ctx = ctx.child(hit);
            action_ctx.semantics(super::Semantics::new(rosace_core::Role::Button).label(label));
            action_ctx.draw_text_at(label, Point { x: ax, y: ty }, action_fg, self.font_size);
            if let Some(cb) = &self.on_action {
                action_ctx.register_hit(Arc::clone(cb));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn snackbar_is_a_bar_filling_available_width_within_margins() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(500.0, 400.0), &font, &theme);

        // Android-convention docked bar (user-specified): edge-to-edge
        // full width regardless of content or action.
        let plain = Snackbar::new("Saved").layout(&ctx);
        let with_action = Snackbar::new("Saved").action("UNDO", || {}).layout(&ctx);
        assert_eq!(plain.width, 500.0);
        assert_eq!(with_action.width, plain.width);

        let narrow = LayoutCtx::new(Constraints::loose(120.0, 400.0), &font, &theme);
        assert!(Snackbar::new("A very long message that cannot fit").layout(&narrow).width <= 120.0);
    }
}
