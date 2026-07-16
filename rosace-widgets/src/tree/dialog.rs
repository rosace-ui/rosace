use std::sync::Arc;

use rosace_core::types::Size;
use rosace_layout::Constraints;
use rosace_render::Color;
use rosace_state::Atom;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::button::{Button, ButtonVariant};
use super::column::Column;
use super::container::draw_rounded_rect_pub;
use super::overlay::{
    FocusBehavior, InputBehavior, LayerPosition, OverlayEntry, ScrimConfig, push_overlay,
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
    pub title: String,
    pub message: Option<String>,
    pub width: f32,
    pub radius: f32,
    pub presentation: DialogPresentation,
    actions: Vec<Action>,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: None,
            width: 340.0,
            radius: 12.0,
            presentation: DialogPresentation::default(),
            actions: Vec::new(),
        }
    }

    pub fn message(mut self, m: impl Into<String>) -> Self { self.message = Some(m.into()); self }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }

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
    pub fn overlay_entry(self, on_dismiss: impl Fn() + Send + Sync + 'static) -> OverlayEntry {
        match self.presentation {
            DialogPresentation::Modal => {
                OverlayEntry::new(LayerPosition::Centered, self)
                    .input(InputBehavior::Block)
                    .focus(FocusBehavior::Trap)
                    .scrim(ScrimConfig {
                        color: Color::rgba(0, 0, 0, 160),
                        on_tap: Some(Arc::new(on_dismiss)),
                    })
            }
            DialogPresentation::NonModal => {
                OverlayEntry::new(LayerPosition::Centered, self)
                    .input(InputBehavior::PassThrough)
                    .focus(FocusBehavior::PassThrough)
            }
            DialogPresentation::FullPage => {
                // The transparent scrim draws nothing visible and can never
                // be tapped (the page covers the window), but it carries the
                // dismisser so Escape still closes the page — same dismissal
                // key the modal presentation honors.
                OverlayEntry::new(LayerPosition::Fill, self)
                    .input(InputBehavior::Block)
                    .focus(FocusBehavior::Trap)
                    .scrim(ScrimConfig {
                        color: Color::TRANSPARENT,
                        on_tap: Some(Arc::new(on_dismiss)),
                    })
            }
        }
    }

    /// Present via the overlay stack while `open` is true — same per-frame
    /// re-push convention as [`Drawer::emit`] / [`Snackbar::emit`]: call from
    /// a host widget's paint (or the app's build) every frame the dialog
    /// should be visible. The barrier dismisser sets `open` to false.
    ///
    /// [`Drawer::emit`]: super::drawer::Drawer::emit
    /// [`Snackbar::emit`]: super::snackbar::Snackbar::emit
    pub fn emit(self, open: &Atom<bool>) {
        if !open.get() { return; }
        let close = open.clone();
        push_overlay(self.overlay_entry(move || close.set(false)));
    }

    /// Compose the inner content tree from the stored parts.
    ///
    /// Rebuilt on each layout/paint call — construction is a few allocations,
    /// far below the cost of the paint itself.
    fn build_inner(&self) -> BoxedWidget {
        let mut col = Column::new()
            .spacing(12.0)
            .child(Text::title(&self.title));

        if let Some(msg) = &self.message {
            col = col.child(Text::caption(msg));
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

        Box::new(col)
    }
}

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
        let inner_c = Constraints::loose(self.width - PADDING * 2.0, f32::INFINITY);
        let inner_size = inner.layout(&ctx.with_constraints(inner_c));
        ctx.constraints.constrain(Size {
            width: self.width,
            height: inner_size.height + PADDING * 2.0,
        })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        ctx.semantics(super::Semantics::new(rosace_core::Role::Dialog).label(&self.title));
        let surface = ctx.tc(ctx.theme.colors.surface);
        let r = ctx.rect;
        if self.presentation == DialogPresentation::FullPage {
            // A page, not a floating card: square, edge-to-edge, no shadow.
            ctx.fill_rect(r, surface);
        } else {
            ctx.fill_shadow_rrect(r, self.radius, Color::rgba(0, 0, 0, 100), 16.0);
            draw_rounded_rect_pub(ctx, r, surface, self.radius);
        }

        let inner_rect = EdgeInsets::all(PADDING).shrink(r);
        self.build_inner().paint(&mut ctx.child(inner_rect));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::overlay::{clear_overlays, drain_overlays};
    use rosace_layout::Constraints;

    #[test]
    fn modal_maps_to_centered_block_trap_with_dismissable_scrim() {
        let e = Dialog::new("t").overlay_entry(|| {});
        assert!(matches!(e.position, LayerPosition::Centered));
        assert_eq!(e.input, InputBehavior::Block);
        assert_eq!(e.focus, FocusBehavior::Trap);
        let scrim = e.scrim.expect("modal must have a barrier scrim");
        assert!(scrim.color.a > 0, "modal barrier must be visible");
        assert!(scrim.on_tap.is_some(), "modal barrier must dismiss on tap");
    }

    #[test]
    fn non_modal_maps_to_pass_through_with_no_scrim() {
        let e = Dialog::new("t").non_modal().overlay_entry(|| {});
        assert!(matches!(e.position, LayerPosition::Centered));
        assert_eq!(e.input, InputBehavior::PassThrough);
        assert_eq!(e.focus, FocusBehavior::PassThrough);
        assert!(e.scrim.is_none(), "non-modal must leave the background interactive");
    }

    #[test]
    fn full_page_maps_to_fill_block_trap_with_invisible_escape_scrim() {
        let e = Dialog::new("t").full_page().overlay_entry(|| {});
        assert!(matches!(e.position, LayerPosition::Fill));
        assert_eq!(e.input, InputBehavior::Block);
        assert_eq!(e.focus, FocusBehavior::Trap);
        let scrim = e.scrim.expect("full-page carries the Escape dismisser");
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
    fn emit_respects_the_open_atom_and_wires_dismiss_to_it() {
        clear_overlays();
        let open = rosace_state::use_atom(false);
        Dialog::new("t").emit(&open);
        assert!(drain_overlays().is_empty(), "closed dialog must push nothing");

        open.set(true);
        Dialog::new("t").emit(&open);
        let entries = drain_overlays();
        assert_eq!(entries.len(), 1);
        let on_tap = entries[0].scrim.as_ref().unwrap().on_tap.as_ref().unwrap().clone();
        on_tap();
        assert!(!open.get(), "barrier tap must close the dialog");
    }
}
