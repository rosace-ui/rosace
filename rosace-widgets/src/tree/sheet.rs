use rosace_core::types::{Point, Rect, Size};
use rosace_layout::Constraints;
use rosace_render::Color;
use rosace_scroll::ScrollController;
use rosace_shader::ShaderMaterial;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::container::draw_rounded_rect_pub;
use super::material::{resolve_material, SheetMaterial};
use super::padding::EdgeInsets;
use super::scroll_view::ScrollView;
use super::spacer::Spacer;

/// How a [`Sheet`] resolves its height (D115/Phase 32 Step 1).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SheetHeight {
    /// Natural content height (the default, the original behavior),
    /// capped at the available height.
    Content,
    /// A fixed height in logical pixels, capped at the available height.
    Fixed(f32),
    /// A fraction (0..=1) of the available height — a detent.
    Detent(f32),
    /// The full available height — the full-screen presentation.
    Full,
}

/// A bottom sheet surface: full-width panel with rounded top corners and a
/// grab handle. Pair with [`OverlayApi::sheet`], which anchors it to the
/// bottom edge and supplies the scrim + tap-to-dismiss.
///
/// Height (D115/Phase 32 Step 1): natural content height by default;
/// [`Sheet::height`] fixes it, [`Sheet::detent`] takes a fraction of the
/// window, [`Sheet::full_screen`] takes all of it. [`Sheet::scrollable`]
/// wraps the content in a [`ScrollView`] so it scrolls when it overflows
/// the sheet. [`Sheet::background`] / [`Sheet::handle_color`] replace the
/// theme-derived defaults.
///
/// [`OverlayApi::sheet`]: super::overlay_api::OverlayApi::sheet
pub struct Sheet {
    pub child: BoxedWidget,
    pub radius: f32,
    pub padding: EdgeInsets,
    pub show_handle: bool,
    height_mode: SheetHeight,
    background: Option<Color>,
    handle_color: Option<Color>,
    material: Option<ShaderMaterial>,
    scrollable: bool,
}

impl Sheet {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            child: Box::new(child),
            radius: 16.0,
            padding: EdgeInsets::all(20.0),
            show_handle: true,
            height_mode: SheetHeight::Content,
            background: None,
            handle_color: None,
            material: None,
            scrollable: false,
        }
    }

    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = p; self }
    pub fn no_handle(mut self) -> Self { self.show_handle = false; self }

    /// A fixed sheet height in logical pixels (capped at the window height).
    pub fn height(mut self, h: f32) -> Self { self.height_mode = SheetHeight::Fixed(h); self }

    /// A detent: the sheet takes this fraction (0..=1) of the available
    /// height — `.detent(0.5)` is the half-open sheet.
    pub fn detent(mut self, fraction: f32) -> Self {
        self.height_mode = SheetHeight::Detent(fraction);
        self
    }

    /// Take the full available height — the full-screen presentation
    /// (still bottom-anchored, still rounded at the top).
    pub fn full_screen(mut self) -> Self { self.height_mode = SheetHeight::Full; self }

    /// Sheet fill — defaults to the theme's `surface`.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }

    /// Grab-handle color — defaults to the theme's `outline`.
    pub fn handle_color(mut self, c: Color) -> Self { self.handle_color = Some(c); self }
    /// Per-instance shader material — replaces the surface fill when
    /// resolved. Beats the theme's `SheetMaterial` default (D124 Step 5).
    pub fn material(mut self, m: ShaderMaterial) -> Self { self.material = Some(m); self }

    /// Wrap the content in a [`ScrollView`] so it scrolls when it overflows
    /// the sheet's height. Without an explicit [`Sheet::height`] /
    /// [`Sheet::detent`], a scrollable sheet has no natural content height
    /// and takes the full available height.
    ///
    /// The scroll position lives in a controller created here — it persists
    /// as long as this `Sheet` instance does (an overlay entry's widget
    /// survives until its owner repaints). For a position that survives
    /// owner rebuilds too, keep a [`ScrollController`] in app state and pass
    /// it to [`Sheet::scrollable_with`].
    pub fn scrollable(self) -> Self {
        self.scrollable_with(ScrollController::new())
    }

    /// [`Sheet::scrollable`] with an app-owned [`ScrollController`] — the
    /// scroll position (and programmatic scrolling) survives rebuilds.
    /// The explicit controller also keeps the scroll view on the base
    /// (CPU-painted) path, which is what overlay content requires.
    pub fn scrollable_with(mut self, controller: ScrollController) -> Self {
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(0.0)));
        self.child = Box::new(ScrollView::new(child).controller(controller));
        self.scrollable = true;
        self
    }

    fn handle_space(&self) -> f32 {
        if self.show_handle { 16.0 } else { 0.0 }
    }
}

impl Widget for Sheet {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let width = ctx.constraints.max_width_f32();
        let height = match self.height_mode {
            SheetHeight::Content => {
                // Natural content height. A scrollable child is a
                // ScrollView, which fills whatever it is given — an
                // unbounded measure comes back infinite and the constrain
                // below caps it at the available height (the documented
                // "scrollable with no explicit height = full height").
                let inner_c = Constraints::loose(
                    (width - self.padding.total_h()).max(0.0),
                    f32::INFINITY,
                );
                let child_size = self.child.layout(&ctx.with_constraints(inner_c));
                child_size.height + self.padding.total_v() + self.handle_space()
            }
            SheetHeight::Fixed(h) => h,
            SheetHeight::Detent(f) => {
                ctx.constraints.max_height_f32() * f.clamp(0.0, 1.0)
            }
            SheetHeight::Full => ctx.constraints.max_height_f32(),
        };
        ctx.constraints.constrain(Size { width, height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // No title field to label itself with (unlike Dialog) — still worth
        // marking as a modal region boundary, unlabeled, so assistive tech
        // knows it's entered one.
        ctx.semantics(super::Semantics::new(rosace_core::Role::Dialog));
        // Hoisted theme reads (borrow must end before mutable painting).
        let (surface, handle_color) = {
            let t = &ctx.theme.colors;
            (
                self.background.unwrap_or_else(|| ctx.tc(t.surface)),
                self.handle_color.unwrap_or_else(|| ctx.tc(t.outline)),
            )
        };
        let r = ctx.rect;

        // Rounded surface, then square off the bottom corners — the sheet
        // sits flush against the window's bottom edge. With a material,
        // only paint a fallback it EXPLICITLY carries — an unconditional
        // base fill would be what a backdrop-sampling glass material sees
        // behind itself, instead of the real content (same rule as
        // Container/Card).
        let material = resolve_material::<SheetMaterial>(&ctx.theme, self.material.as_ref());
        let fill = match &material {
            Some(m) => m.fallback,
            None => Some(surface),
        };
        if let Some(fill) = fill {
            draw_rounded_rect_pub(ctx, r, fill, self.radius);
            ctx.fill_rect(Rect {
                origin: Point { x: r.origin.x, y: r.origin.y + r.size.height - self.radius },
                size: Size { width: r.size.width, height: self.radius },
            }, fill);
        }
        if let Some(m) = &material {
            ctx.shader_fill(r, m.pipeline, m.uniforms.clone());
        }

        if self.show_handle {
            let handle_w = 36.0;
            ctx.fill_rrect(Rect {
                origin: Point {
                    x: r.origin.x + (r.size.width - handle_w) / 2.0,
                    y: r.origin.y + 6.0,
                },
                size: Size { width: handle_w, height: 4.0 },
            }, 2.0, handle_color);
        }

        let content = Rect {
            origin: Point { x: r.origin.x, y: r.origin.y + self.handle_space() },
            size: Size { width: r.size.width, height: r.size.height - self.handle_space() },
        };
        self.child.paint(&mut ctx.child(self.padding.shrink(content)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::spacer::Spacer;
    use rosace_layout::Constraints;

    #[test]
    fn instance_material_paints_a_shader_fill() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(super::super::render_tree::RenderTree::new()));
        let rect = Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 400.0, height: 300.0 } };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        let m = ShaderMaterial::new(rosace_shader::PipelineId::user(0x4001), vec![0u8; 16]);
        Sheet::new(Spacer::new(0.0)).material(m).paint(&mut ctx);
        let picture = recorder.finish();
        assert!(picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. })));
    }

    fn ctx_800x600<'a>(
        font: &'a rosace_render::FontCache,
        theme: &'a rosace_theme::ThemeData,
    ) -> LayoutCtx<'a> {
        LayoutCtx::new(Constraints::loose(800.0, 600.0), font, theme)
    }

    #[test]
    fn content_mode_takes_the_natural_child_height_plus_chrome() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = ctx_800x600(&font, &theme);
        let size = Sheet::new(Spacer::gap(0.0, 100.0)).layout(&ctx);
        assert_eq!(size.width, 800.0);
        // 100 content + 40 padding (20 all sides) + 16 handle space.
        assert_eq!(size.height, 156.0);
    }

    #[test]
    fn fixed_detent_and_full_screen_heights_resolve_against_the_window() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = ctx_800x600(&font, &theme);

        let fixed = Sheet::new(Spacer::gap(0.0, 100.0)).height(220.0).layout(&ctx);
        assert_eq!(fixed.height, 220.0);

        let detent = Sheet::new(Spacer::gap(0.0, 100.0)).detent(0.5).layout(&ctx);
        assert_eq!(detent.height, 300.0);

        let full = Sheet::new(Spacer::gap(0.0, 100.0)).full_screen().layout(&ctx);
        assert_eq!(full.height, 600.0);
    }

    #[test]
    fn fixed_height_is_capped_at_the_available_height() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = ctx_800x600(&font, &theme);
        let size = Sheet::new(Spacer::gap(0.0, 100.0)).height(10_000.0).layout(&ctx);
        assert_eq!(size.height, 600.0);
    }

    #[test]
    fn scrollable_without_an_explicit_height_takes_the_full_height() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = ctx_800x600(&font, &theme);
        let size = Sheet::new(Spacer::gap(0.0, 5_000.0)).scrollable().layout(&ctx);
        assert_eq!(size.height, 600.0, "a scrollable sheet has no natural height");
    }

    #[test]
    fn scrollable_with_a_detent_keeps_the_detent_height() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = ctx_800x600(&font, &theme);
        let size = Sheet::new(Spacer::gap(0.0, 5_000.0))
            .scrollable()
            .detent(0.5)
            .layout(&ctx);
        assert_eq!(size.height, 300.0);
    }
}
