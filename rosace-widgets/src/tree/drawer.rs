use std::sync::Arc;
use rosace_core::types::Size;
use rosace_render::Color;
use rosace_shader::ShaderMaterial;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::material::{resolve_material, DrawerMaterial};
use super::overlay::{LayerPosition, InputBehavior, FocusBehavior, ScrimConfig};

/// A slide-in side panel. Attach to any widget's paint via `.drawer(open, ..)`
/// (see DrawerApi) or use directly: when `open`, it pushes a dimmed scrim +
/// a left-anchored panel overlay. Tapping the scrim closes it.
///
/// Customization (D115/Phase 32 Step 1): [`Drawer::full_screen`] makes the
/// panel cover the whole window (mobile nav-page style); [`Drawer::background`]
/// and [`Drawer::scrim_color`] replace the theme-derived defaults.
pub struct Drawer {
    open: bool,
    on_open_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    width: f32,
    full_screen: bool,
    background: Option<Color>,
    scrim_color: Color,
    material: Option<ShaderMaterial>,
    panel: Arc<dyn Fn() -> BoxedWidget + Send + Sync>,
}

impl Drawer {
    pub fn new(open: bool, panel: impl Fn() -> BoxedWidget + Send + Sync + 'static) -> Self {
        Self {
            open,
            on_open_change: None,
            width: 280.0,
            full_screen: false,
            background: None,
            scrim_color: Color::rgba(0, 0, 0, 120),
            material: None,
            panel: Arc::new(panel),
        }
    }
    /// Called with `false` when the drawer asks to close — a scrim tap, or
    /// Escape. Without one it can only be closed by the app changing `open`.
    pub fn on_open_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_open_change = Some(Arc::new(f));
        self
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
    /// Per-instance shader material — replaces the panel fill when
    /// resolved. Beats the theme's `DrawerMaterial` default (D124 Step 5).
    pub fn material(mut self, m: ShaderMaterial) -> Self { self.material = Some(m); self }

    /// Emit the drawer overlay if open. Call from a host widget's paint (the
    /// Scaffold does this) — the drawer has no visual of its own when closed.
    pub fn emit(&self, ctx: &mut PaintCtx) {
        if !self.open { return; }
        let panel = (self.panel)();
        let on_tap = self.on_open_change.clone()
            .map(|cb| Arc::new(move || cb(false)) as Arc<dyn Fn() + Send + Sync>);
        // Identity comes from the node the promotion occupies — the `open`
        // atom's id used to stand in for it, which is why `Drawer` could not
        // drop its `Atom` before promoted nodes existed.
        ctx.promote_at(
            LayerPosition::Fill,
            &DrawerPanel {
                width: self.width,
                full_screen: self.full_screen,
                background: self.background,
                material: self.material.clone(),
                panel,
            },
            super::PromoteOpts {
                scrim: Some(ScrimConfig {
                    color: self.scrim_color,
                    on_tap,
                    exclude_rect: None,
                }),
                input: InputBehavior::Block,
                focus: FocusBehavior::Trap,
            },
        );
    }
}

struct DrawerPanel {
    width: f32,
    full_screen: bool,
    background: Option<Color>,
    material: Option<ShaderMaterial>,
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
        // With a material, only paint a fallback it EXPLICITLY carries — an
        // unconditional base fill is what a backdrop-sampling glass material
        // would sample instead of the content behind the panel (same rule
        // as Container/Card).
        let material = resolve_material::<DrawerMaterial>(&ctx.theme, self.material.as_ref());
        match &material {
            Some(m) => {
                if let Some(fallback) = m.fallback {
                    ctx.fill_rect(r, fallback);
                }
                ctx.shader_fill(r, m.pipeline, m.uniforms.clone());
            }
            None => ctx.fill_rect(r, bg),
        }
        ctx.paint_child(r, &*self.panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::spacer::Spacer;
    use rosace_layout::Constraints;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Emit through a REAL paint context and hand back the tree, so these
    /// assert what the engine will actually see rather than a description of
    /// it. `promote_at` resolves against the window, so it needs one.
    fn emit_into_tree(drawer: &Drawer) -> std::rc::Rc<std::cell::RefCell<super::super::render_tree::RenderTree>> {
        super::super::set_window_size(400.0, 600.0);
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(
            super::super::render_tree::RenderTree::new(),
        ));
        let rect = rosace_core::types::Rect {
            origin: rosace_core::types::Point { x: 0.0, y: 0.0 },
            size: Size { width: 400.0, height: 600.0 },
        };
        {
            let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, std::rc::Rc::clone(&tree));
            drawer.emit(&mut ctx);
        }
        tree.borrow_mut().finalize();
        tree
    }

    #[test]
    fn instance_material_paints_a_shader_fill() {
        let m = ShaderMaterial::new(rosace_shader::PipelineId::user(0x4002), vec![0u8; 16]);
        let drawer = Drawer::new(true, || Arc::new(Spacer::new(0.0))).material(m);
        let tree = emit_into_tree(&drawer);

        let t = tree.borrow();
        let node = *t.promoted_nodes().first().expect("an open drawer promotes a layer");
        let picture = &t.node(node).promoted.as_ref().unwrap().picture;
        assert!(
            picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. })),
            "the panel's material must reach the promoted layer's own picture"
        );
    }

    #[test]
    fn a_closed_drawer_promotes_nothing() {
        let tree = emit_into_tree(&Drawer::new(false, || Arc::new(Spacer::new(0.0))));
        assert!(tree.borrow().promoted_nodes().is_empty());
    }

    #[test]
    fn an_open_drawer_fills_blocks_traps_and_dismisses_on_a_scrim_tap() {
        let closed = Arc::new(AtomicBool::new(false));
        let c = Arc::clone(&closed);
        let drawer = Drawer::new(true, || Arc::new(Spacer::new(0.0)))
            .on_open_change(move |open| c.store(!open, Ordering::SeqCst));
        let tree = emit_into_tree(&drawer);

        let t = tree.borrow();
        let node = *t.promoted_nodes().first().expect("an open drawer promotes a layer");
        let n = t.node(node);
        assert_eq!(n.focus_behavior, FocusBehavior::Trap, "a drawer traps focus");
        let p = n.promoted.as_ref().unwrap();
        assert_eq!(
            (p.rect.size.width, p.rect.size.height),
            (400.0, 600.0),
            "a Fill drawer's layer spans the window (it carries the scrim)"
        );
        (p.on_dismiss.as_ref().expect("scrim must dismiss on tap"))();
        assert!(closed.load(Ordering::SeqCst), "scrim tap must ask to close the drawer");
    }

    #[test]
    fn full_screen_panel_uses_the_whole_width() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 600.0), &font, &theme);
        let panel = DrawerPanel {
            width: 280.0,
            full_screen: true,
            background: None,
            material: None,
            panel: Arc::new(Spacer::new(0.0)),
        };
        assert_eq!(panel.layout(&ctx).width, 400.0);
    }
}
