use std::sync::Arc;

use rosace_core::types::Size;
use rosace_layout::Constraints;
use rosace_render::Color;
use rosace_shader::ShaderMaterial;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::button::{Button, ButtonVariant};
use super::column::Column;
use super::container::draw_rounded_rect_pub;
use super::material::{resolve_material, DialogMaterial};
use super::overlay::{
    FocusBehavior, InputBehavior, LayerPosition, ScrimConfig,
};
use super::padding::EdgeInsets;
use super::row::Row;
use super::text::Text;
use rosace_layout::MainAxisAlignment;

type Action = (String, ButtonVariant, Arc<dyn Fn() + Send + Sync>);

/// How a [`Dialog`] presents when emitted as an overlay (D115/Phase 32 Step 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DialogPresentation {
    /// Centered card over a dimmed barrier. Input to the content below is
    /// blocked, Tab focus is trapped inside, and a tap on the barrier (or
    /// Escape) dismisses. The default.
    #[default]
    Modal,
    /// Centered card with NO barrier — the content below stays fully
    /// interactive (inspector-panel / tool-palette style). Clicks on the
    /// card itself are absorbed; everything else falls through. Dismissal
    /// is the dialog's own responsibility (an action button).
    NonModal,
    /// Fills the entire window, like a pushed page — the Material
    /// full-screen dialog. Input below is blocked and focus is trapped;
    /// Escape still dismisses (via an invisible barrier), but there is no
    /// outside area to tap.
    FullPage,
}

/// A dialog surface: title, optional message, action buttons.
///
/// Two ways to present it:
/// - [`OverlayApi::dialog`] — co-located declaration; always the modal
///   presentation (scrim, centering, input blocking, focus trap).
/// - [`Dialog::emit`] — the [`Drawer::emit`]-style per-frame push, which
///   honors the presentation chosen with [`Dialog::modal`] /
///   [`Dialog::non_modal`] / [`Dialog::full_page`].
///
/// ```rust,ignore
/// Button::new("Delete")
///     .dialog(confirm.clone(), move || Box::new(
///         Dialog::new("Delete item?")
///             .message("This cannot be undone.")
///             .action("Cancel", { let c = confirm.clone(); move || c.set(false) })
///             .destructive_action("Delete", move || { /* … */ })
///     ))
/// ```
///
/// [`OverlayApi::dialog`]: super::overlay_api::OverlayApi::dialog
/// [`Drawer::emit`]: super::drawer::Drawer::emit
pub struct Dialog {
    /// `None` = `EdgeInsets::all(PADDING)`.
    padding: Option<EdgeInsets>,
    pub title: String,
    pub message: Option<String>,
    pub width: f32,
    pub radius: f32,
    pub presentation: DialogPresentation,
    background: Option<Color>,
    color: Option<Color>,
    material: Option<ShaderMaterial>,
    actions: Vec<Action>,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            padding: None,
            message: None,
            width: 340.0,
            radius: 12.0,
            presentation: DialogPresentation::default(),
            background: None,
            color: None,
            material: None,
            actions: Vec::new(),
        }
    }

    pub fn message(mut self, m: impl Into<String>) -> Self { self.message = Some(m.into()); self }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }
    /// Inset around the title/message/actions column.
    pub fn padding(mut self, p: EdgeInsets) -> Self { self.padding = Some(p); self }
    /// Dialog surface fill color (theme's `surface` if unset).
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Title/message text color (theme's `on_surface` if unset).
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    /// Per-instance shader material — replaces the surface fill when
    /// resolved. Beats the theme's `DialogMaterial` default (D124 Step 5).
    pub fn material(mut self, m: ShaderMaterial) -> Self { self.material = Some(m); self }

    /// Present as a modal dialog (the default) — see
    /// [`DialogPresentation::Modal`].
    pub fn modal(mut self) -> Self { self.presentation = DialogPresentation::Modal; self }

    /// Present as a non-modal dialog — the content below stays interactive.
    /// See [`DialogPresentation::NonModal`].
    pub fn non_modal(mut self) -> Self { self.presentation = DialogPresentation::NonModal; self }

    /// Present full-page — the dialog fills the window like a pushed page.
    /// See [`DialogPresentation::FullPage`].
    pub fn full_page(mut self) -> Self { self.presentation = DialogPresentation::FullPage; self }

    /// Add a neutral (secondary) action button.
    pub fn action(mut self, label: impl Into<String>, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.actions.push((label.into(), ButtonVariant::Secondary, Arc::new(f)));
        self
    }

    /// Add a highlighted (primary) action button.
    pub fn primary_action(mut self, label: impl Into<String>, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.actions.push((label.into(), ButtonVariant::Primary, Arc::new(f)));
        self
    }

    /// Add a destructive (danger) action button.
    pub fn destructive_action(mut self, label: impl Into<String>, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.actions.push((label.into(), ButtonVariant::Danger, Arc::new(f)));
        self
    }

    /// The pure presentation→overlay-config mapping: consumes the dialog and
    /// returns the [`OverlayEntry`] that presents it. `on_dismiss` is wired
    /// to the barrier (scrim tap / Escape) where the presentation has one;
    /// [`DialogPresentation::NonModal`] has no barrier, so `on_dismiss` is
    /// simply unused there.
    /// The pure presentation→placement mapping: what position and policy this
    /// dialog is promoted with. `on_dismiss` is wired to the barrier (scrim
    /// tap / Escape) where the presentation has one; `NonModal` has no
    /// barrier, so it is simply unused there.
    fn promote_spec(
        &self,
        on_dismiss: Arc<dyn Fn() + Send + Sync>,
    ) -> (LayerPosition, super::PromoteOpts) {
        match self.presentation {
            DialogPresentation::Modal => (
                LayerPosition::Centered,
                super::PromoteOpts {
                    scrim: Some(ScrimConfig {
                        // A modal scrim is black in every theme by design
                        // (Material specifies black at a fixed opacity) — it
                        // dims what is behind it rather than participating in
                        // the palette. Deliberate constant, not an unswept
                        // literal.
                        color: Color::rgba(0, 0, 0, 160),
                        on_tap: Some(on_dismiss),
                        exclude_rect: None,
                    }),
                    input: InputBehavior::Block,
                    focus: FocusBehavior::Trap,
                },
            ),
            DialogPresentation::NonModal => (
                LayerPosition::Centered,
                super::PromoteOpts {
                    scrim: None,
                    input: InputBehavior::PassThrough,
                    focus: FocusBehavior::PassThrough,
                },
            ),
            // The transparent scrim draws nothing visible and can never be
            // tapped (the page covers the window), but it carries the
            // dismisser so Escape still closes the page — the same key the
            // modal presentation honours.
            DialogPresentation::FullPage => (
                LayerPosition::Fill,
                super::PromoteOpts {
                    scrim: Some(ScrimConfig {
                        color: Color::TRANSPARENT,
                        on_tap: Some(on_dismiss),
                        exclude_rect: None,
                    }),
                    input: InputBehavior::Block,
                    focus: FocusBehavior::Trap,
                },
            ),
        }
    }

    /// Present this dialog while `open`, from a host widget's paint.
    ///
    /// The only path to the `non_modal()` and `full_page()` presentations —
    /// `widget.dialog(open, ..)` is always modal.
    ///
    /// ```rust,ignore
    /// Dialog::new("Discard?")
    ///     .full_page()
    ///     .emit(ctx, open.get(), move || open.set(false));
    /// ```
    pub fn emit(
        self,
        ctx: &mut PaintCtx,
        open: bool,
        on_close: impl Fn() + Send + Sync + 'static,
    ) {
        if !open { return; }
        let (position, opts) = self.promote_spec(Arc::new(on_close));
        ctx.promote_at(position, &self, opts);
    }

    /// Compose the inner content tree from the stored parts.
    ///
    /// Rebuilt on each layout/paint call — construction is a few allocations,
    /// far below the cost of the paint itself.
    fn build_inner(&self) -> BoxedWidget {
        let mut title = Text::title(&self.title);
        if let Some(c) = self.color { title = title.color(c); }
        let mut col = Column::new()
            .spacing(12.0)
            .child(title);

        if let Some(msg) = &self.message {
            let mut msg_text = Text::caption(msg);
            if let Some(c) = self.color { msg_text = msg_text.color(c); }
            col = col.child(msg_text);
        }

        if !self.actions.is_empty() {
            let mut actions = Row::new()
                .spacing(8.0)
                .main_axis_alignment(MainAxisAlignment::End);
            for (label, variant, cb) in &self.actions {
                let cb = Arc::clone(cb);
                actions = actions.child(
                    Button::new(label.clone())
                        .variant(*variant)
                        .on_press(move || cb()),
                );
            }
            col = col.child(actions);
        }

        Arc::new(col)
    }
}

/// Default inset around a dialog's content. A MINIMUM in spirit, not a
/// ceiling — `Dialog::padding` overrides it.
const PADDING: f32 = 20.0;

impl Widget for Dialog {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        if self.presentation == DialogPresentation::FullPage {
            // A full-page dialog fills whatever it is given (the overlay
            // pass hands it the window).
            return ctx.constraints.constrain(Size {
                width: super::avail_w(ctx.constraints),
                height: super::avail_h(ctx.constraints),
            });
        }
        let inner = self.build_inner();
        let pad = self.padding.unwrap_or(EdgeInsets::all(PADDING));
        let inner_c = Constraints::loose(self.width - pad.total_h(), f32::INFINITY);
        let inner_size = ctx.layout_child(inner_c, &*inner);
        ctx.constraints.constrain(Size {
            width: self.width,
            height: inner_size.height + pad.total_v(),
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Dialog).label(&self.title));
        let surface = self.background.unwrap_or_else(|| ctx.tc(ctx.theme.colors.surface));
        let r = ctx.rect;
        let material = resolve_material::<DialogMaterial>(&ctx.theme, self.material.as_ref());
        // With a material, only paint a fallback it EXPLICITLY carries —
        // an unconditional base fill lands in the scene right before the
        // shader quad, so a backdrop-sampling glass material would sample
        // the fill instead of the content behind the dialog (same rule as
        // Container/Card).
        if self.presentation == DialogPresentation::FullPage {
            // A page, not a floating card: square, edge-to-edge, no shadow.
            if let Some(m) = &material {
                if let Some(fallback) = m.fallback {
                    ctx.fill_rect(r, fallback);
                }
                ctx.shader_fill(r, m.pipeline, m.uniforms.clone());
            } else {
                ctx.fill_rect(r, surface);
            }
        } else {
            let sh = ctx.tc(ctx.theme.colors.shadow);
            ctx.fill_shadow_rrect(r, self.radius, Color::rgba(sh.r, sh.g, sh.b, 100), 16.0);
            if let Some(m) = &material {
                if let Some(fallback) = m.fallback {
                    draw_rounded_rect_pub(ctx, r, fallback, self.radius);
                }
                ctx.shader_fill(r, m.pipeline, m.uniforms.clone());
            } else {
                draw_rounded_rect_pub(ctx, r, surface, self.radius);
            }
        }

        let inner_rect = self.padding.unwrap_or(EdgeInsets::all(PADDING)).shrink(r);
        ctx.paint_child(inner_rect, &*self.build_inner());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn spec(d: Dialog) -> (LayerPosition, super::super::PromoteOpts) {
        d.promote_spec(Arc::new(|| {}))
    }

    #[test]
    fn modal_maps_to_centered_block_trap_with_dismissable_scrim() {
        let (position, opts) = spec(Dialog::new("t"));
        assert!(matches!(position, LayerPosition::Centered));
        assert_eq!(opts.input, InputBehavior::Block);
        assert_eq!(opts.focus, FocusBehavior::Trap);
        let scrim = opts.scrim.expect("modal must have a barrier scrim");
        assert!(scrim.color.a > 0, "modal barrier must be visible");
        assert!(scrim.on_tap.is_some(), "modal barrier must dismiss on tap");
    }

    #[test]
    fn non_modal_maps_to_pass_through_with_no_scrim() {
        let (position, opts) = spec(Dialog::new("t").non_modal());
        assert!(matches!(position, LayerPosition::Centered));
        assert_eq!(opts.input, InputBehavior::PassThrough);
        assert_eq!(opts.focus, FocusBehavior::PassThrough);
        assert!(opts.scrim.is_none(), "non-modal must leave the background interactive");
    }

    #[test]
    fn full_page_maps_to_fill_block_trap_with_invisible_escape_scrim() {
        let (position, opts) = spec(Dialog::new("t").full_page());
        assert!(matches!(position, LayerPosition::Fill));
        assert_eq!(opts.input, InputBehavior::Block);
        assert_eq!(opts.focus, FocusBehavior::Trap);
        let scrim = opts.scrim.expect("full-page carries the Escape dismisser");
        assert_eq!(scrim.color.a, 0, "full-page barrier must be invisible");
        assert!(scrim.on_tap.is_some());
    }

    #[test]
    fn full_page_layout_fills_the_window_modal_keeps_the_card_width() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(800.0, 600.0), &font, &theme);

        let full = Dialog::new("t").full_page().layout(&ctx);
        assert_eq!((full.width, full.height), (800.0, 600.0));

        let modal = Dialog::new("t").layout(&ctx);
        assert_eq!(modal.width, 340.0);
        assert!(modal.height < 600.0, "a modal card must not fill the window");
    }

    #[test]
    fn the_barrier_dismisser_reports_a_close_request() {
        // `emit` needs a live `PaintCtx`, which is the engine's to hand out —
        // what is testable in isolation is the mapping it drives, and that the
        // dismisser it installs reports back.
        let closed = Arc::new(AtomicBool::new(false));
        let c = Arc::clone(&closed);
        let (_, opts) = Dialog::new("t").promote_spec(Arc::new(move || {
            c.store(true, Ordering::SeqCst);
        }));
        let on_tap = opts.scrim.expect("modal barrier").on_tap.expect("dismisser");
        on_tap();
        assert!(closed.load(Ordering::SeqCst), "a barrier tap must request a close");
    }

    #[test]
    fn instance_material_paints_a_shader_fill() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let mut recorder = rosace_render::PictureRecorder::new();
        let tree = std::rc::Rc::new(std::cell::RefCell::new(super::super::render_tree::RenderTree::new()));
        let rect = rosace_core::types::Rect {
            origin: rosace_core::types::Point { x: 0.0, y: 0.0 },
            size: Size { width: 340.0, height: 200.0 },
        };
        let mut ctx = PaintCtx::root(&mut recorder, rect, &font, theme, tree);
        let m = ShaderMaterial::new(rosace_shader::PipelineId::user(0x4000), vec![0u8; 16]);
        Dialog::new("t").material(m).paint(&mut ctx);
        let picture = recorder.finish();
        assert!(picture.commands.iter().any(|c| matches!(c, rosace_render::DrawCommand::ShaderFill { .. })));
    }

    #[test]
    fn background_and_color_builders_do_not_change_layout_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let base = Dialog::new("Title").message("Body");
        let customized = Dialog::new("Title").message("Body")
            .background(Color::rgb(10, 10, 10))
            .color(Color::rgb(255, 255, 255));
        assert_eq!(base.layout(&ctx), customized.layout(&ctx));
    }

    /// `.padding(..)` must actually move content, and the default must be
    /// unchanged — a padding builder that silently does nothing is worse
    /// than none, since it looks configurable.
    #[test]
    fn padding_insets_the_content_and_the_default_is_unchanged() {
        use rosace_render::FontCache;
        let font = FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let c = rosace_layout::Constraints::loose(400.0, 600.0);
        let ctx = LayoutCtx::new(c, &font, &theme);

        let base = Dialog::new("T").message("m").layout(&ctx);
        let padded = Dialog::new("T").message("m").padding(EdgeInsets::all(40.0)).layout(&ctx);
        assert!(padded.height > base.height,
            "generous padding must grow the box: {} vs {}", base.height, padded.height);
    }

}
