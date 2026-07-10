use std::sync::Arc;

use rosace_core::types::Size;
use rosace_layout::Constraints;
use rosace_render::Color;
use super::{Widget, LayoutCtx, PaintCtx, BoxedWidget};
use super::button::{Button, ButtonVariant};
use super::column::Column;
use super::container::draw_rounded_rect_pub;
use super::padding::EdgeInsets;
use super::row::Row;
use super::text::Text;
use rosace_layout::MainAxisAlignment;

type Action = (String, ButtonVariant, Arc<dyn Fn() + Send + Sync>);

/// A modal dialog surface: title, optional message, action buttons.
///
/// Pair with [`OverlayApi::dialog`] — the overlay layer supplies the scrim,
/// centering, input blocking, and focus trap; `Dialog` is just the card.
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
pub struct Dialog {
    pub title: String,
    pub message: Option<String>,
    pub width: f32,
    pub radius: f32,
    actions: Vec<Action>,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: None,
            width: 340.0,
            radius: 12.0,
            actions: Vec::new(),
        }
    }

    pub fn message(mut self, m: impl Into<String>) -> Self { self.message = Some(m.into()); self }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn radius(mut self, r: f32) -> Self { self.radius = r; self }

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
        let r = ctx.rect;
        ctx.fill_shadow_rrect(r, self.radius, Color::rgba(0, 0, 0, 100), 16.0);
        draw_rounded_rect_pub(ctx, r, ctx.tc(ctx.theme.colors.surface), self.radius);

        let inner_rect = EdgeInsets::all(PADDING).shrink(r);
        self.build_inner().paint(&mut ctx.child(inner_rect));
    }
}
