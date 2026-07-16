use std::sync::Arc;
use rosace_core::types::Size;
use rosace_state::Atom;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::overlay::{OverlayEntry, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig, push_overlay};

/// A slide-in side panel. Attach to any widget's paint via `.drawer(open, ..)`
/// (see DrawerApi) or use directly: when `open`, it pushes a dimmed scrim +
/// a left-anchored panel overlay. Tapping the scrim closes it.
///
/// Customization (D115/Phase 32 Step 1): [`Drawer::full_screen`] makes the
/// panel cover the whole window (mobile nav-page style); [`Drawer::background`]
/// and [`Drawer::scrim_color`] replace the theme-derived defaults.
pub struct Drawer {
    open: Atom<bool>,
    width: f32,
    full_screen: bool,
    background: Option<Color>,
    scrim_color: Color,
    panel: Arc<dyn Fn() -> BoxedWidget + Send + Sync>,
}

impl Drawer {
    pub fn new(open: Atom<bool>, panel: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        Self {
            open,
            width: 280.0,
            full_screen: false,
            background: None,
            scrim_color: Color::rgba(0, 0, 0, 120),
            panel: Arc::new(panel),
        }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }

    /// Cover the entire window instead of a fixed-width side panel — the
    /// full-screen navigation-page presentation. There is no scrim area
    /// left to tap, so dismissal is the panel content's job (or Escape).
    pub fn full_screen(mut self) -> Self { self.full_screen = true; self }

    /// Panel fill — defaults to the theme's `surface`.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }

    /// Scrim (barrier) color over the content behind the panel — defaults
    /// to black at ~47% opacity.
    pub fn scrim_color(mut self, c: Color) -> Self { self.scrim_color = c; self }

    /// Emit the drawer overlay if open. Call from a host widget's paint (the
    /// Scaffold does this) — the drawer has no visual of its own when closed.
    pub fn emit(&self) {
        if !self.open.get() { return; }
        let close = self.open.clone();
        let panel = (self.panel)();
        push_overlay(
            OverlayEntry::new(LayerPosition::Fill, DrawerPanel {
                width: self.width,
                full_screen: self.full_screen,
                background: self.background,
                panel,
            })
                .input(InputBehavior::Block)
                .focus(FocusBehavior::Trap)
                .scrim(ScrimConfig { color: self.scrim_color, on_tap: Some(Arc::new(move || close.set(false))) }),
        );
    }
}

struct DrawerPanel {
    width: f32,
    full_screen: bool,
    background: Option<Color>,
    panel: BoxedWidget,
}

impl Widget for DrawerPanel {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        // The panel sizes itself to its REAL width (full window height),
        // not the whole window: the overlay dispatch treats the widget's
        // rect as "the surface" — taps inside it are absorbed, taps outside
        // it reach the scrim's tap-to-dismiss. Sizing the panel to the full
        // window (the original version) made every tap land "inside" and
        // the documented scrim tap-to-close unreachable.
        let avail_w = super::avail_w(ctx.constraints);
        let w = if self.full_screen { avail_w } else { self.width.min(avail_w) };
        Size { width: w, height: super::avail_h(ctx.constraints) }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let bg = self.background.unwrap_or_else(|| ctx.tc(ctx.theme.colors.surface));
        let r = ctx.rect;
        ctx.fill_rect(r, bg);
        self.panel.paint(&mut ctx.child(r));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::overlay::{clear_overlays, drain_overlays};
    use super::super::spacer::Spacer;
    use rosace_layout::Constraints;

    fn emit_one(drawer: &Drawer) -> OverlayEntry {
        clear_overlays();
        drawer.emit();
        let mut entries = drain_overlays();
        assert_eq!(entries.len(), 1);
        entries.pop().unwrap()
    }

    #[test]
    fn emit_pushes_nothing_while_closed() {
        clear_overlays();
        let open = rosace_state::use_atom(false);
        Drawer::new(open, || Box::new(Spacer::new(0.0))).emit();
        assert!(drain_overlays().is_empty());
    }

    #[test]
    fn emit_maps_to_fill_block_trap_with_dismissable_scrim() {
        let open = rosace_state::use_atom(true);
        let drawer = Drawer::new(open.clone(), || Box::new(Spacer::new(0.0)));
        let e = emit_one(&drawer);
        assert!(matches!(e.position, LayerPosition::Fill));
        assert_eq!(e.input, InputBehavior::Block);
        assert_eq!(e.focus, FocusBehavior::Trap);
        let scrim = e.scrim.expect("drawer must have a scrim");
        let on_tap = scrim.on_tap.expect("scrim must dismiss on tap");
        on_tap();
        assert!(!open.get(), "scrim tap must close the drawer");
    }

    #[test]
    fn panel_is_side_width_by_default_and_window_width_when_full_screen() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(800.0, 600.0), &font, &theme);

        let open = rosace_state::use_atom(true);
        let side = emit_one(&Drawer::new(open.clone(), || Box::new(Spacer::new(0.0))));
        let size = side.widget.layout(&ctx);
        assert_eq!((size.width, size.height), (280.0, 600.0));

        let full = emit_one(&Drawer::new(open, || Box::new(Spacer::new(0.0))).full_screen());
        let size = full.widget.layout(&ctx);
        assert_eq!((size.width, size.height), (800.0, 600.0));
    }
}
