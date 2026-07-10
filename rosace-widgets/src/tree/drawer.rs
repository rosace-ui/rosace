use std::sync::Arc;
use rosace_core::types::Size;
use rosace_state::Atom;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::overlay::{OverlayEntry, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig, push_overlay};

/// A slide-in side panel. Attach to any widget's paint via `.drawer(open, ..)`
/// (see DrawerApi) or use directly: when `open`, it pushes a dimmed scrim +
/// a left-anchored panel overlay. Tapping the scrim closes it.
pub struct Drawer {
    open: Atom<bool>,
    width: f32,
    panel: Arc<dyn Fn() -> BoxedWidget + Send + Sync>,
}

impl Drawer {
    pub fn new(open: Atom<bool>, panel: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        Self { open, width: 280.0, panel: Arc::new(panel) }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }

    /// Emit the drawer overlay if open. Call from a host widget's paint (the
    /// Scaffold does this) — the drawer has no visual of its own when closed.
    pub fn emit(&self) {
        if !self.open.get() { return; }
        let close = self.open.clone();
        let panel = (self.panel)();
        push_overlay(
            OverlayEntry::new(LayerPosition::Fill, DrawerPanel { width: self.width, panel })
                .input(InputBehavior::Block)
                .focus(FocusBehavior::Trap)
                .scrim(ScrimConfig { color: Color::rgba(0, 0, 0, 120), on_tap: Some(Arc::new(move || close.set(false))) }),
        );
    }
}

struct DrawerPanel { width: f32, panel: BoxedWidget }

impl Widget for DrawerPanel {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        Size { width: super::avail_w(ctx.constraints), height: super::avail_h(ctx.constraints) }
    }
    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let panel_rect = rosace_core::types::Rect {
            origin: r.origin,
            size: Size { width: self.width.min(r.size.width), height: r.size.height },
        };
        ctx.fill_rect(panel_rect, ctx.tc(ctx.theme.colors.surface));
        self.panel.paint(&mut ctx.child(panel_rect));
    }
}
