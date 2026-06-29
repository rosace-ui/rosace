//! Widget tree — composable, layout-aware, paint-capable widgets.
//!
//! # Architecture
//! Every widget implements [`Widget`]:
//! - `layout(constraints) → Size` — measure pass (bottom-up)
//! - `paint(ctx)` — paint pass (top-down, rect already allocated)
//!
//! Children are stored as `Vec<Box<dyn Widget>>`. [`Column`] / [`Row`] handle
//! [`Expanded`] children by doing a two-pass measure internally.

pub mod app;
pub mod app_bar;
pub mod avatar;
pub mod badge;
pub mod button;
pub mod card;
pub mod center;
pub mod checkbox;
pub mod chip;
pub mod colored_box;
pub mod column;
pub mod container;
pub mod divider;
pub mod icon;
pub mod list_tile;
pub mod nav_rail;
pub mod padding;
pub mod progress_bar;
pub mod row;
pub mod scaffold;
pub mod scroll_view;
pub mod sized_box;
pub mod slider;
pub mod spacer;
pub mod stack;
pub mod switch;
pub mod tab;
pub mod text;
pub mod text_input;

pub use app::WidgetApp;
pub use app_bar::AppBar;
pub use avatar::Avatar;
pub use badge::Badge;
pub use button::{Button, ButtonVariant};
pub use card::Card;
pub use center::Center;
pub use checkbox::Checkbox;
pub use chip::Chip;
pub use colored_box::ColoredBox;
pub use column::Column;
pub use container::Container;
pub use divider::Divider;
pub use icon::{Icon, IconKind};
pub use list_tile::{ListTile, ListView};
pub use nav_rail::{NavItem, NavRail};
pub use padding::{EdgeInsets, Padding};
pub use progress_bar::ProgressBar;
pub use row::Row;
pub use scaffold::Scaffold;
pub use scroll_view::ScrollView;
pub use sized_box::SizedBox;
pub use slider::Slider;
pub use spacer::{Expanded, Spacer};
pub use stack::Stack;
pub use switch::Switch;
pub use tab::{Tab, TabBar};
pub use text::{Text, TextAlign, FontWeight};
pub use text_input::TextInput;

use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

use tezzera_core::types::{Point, Rect, Size};
use tezzera_core::{Element, NativeElement, WidgetPayload};
use tezzera_layout::Constraints;
use tezzera_render::{Color, FontCache, SkiaCanvas};
use tezzera_theme::ThemeData;

// ── HitTarget ────────────────────────────────────────────────────────────────

/// A clickable region registered during painting.
pub struct HitTarget {
    pub rect: Rect,
    pub callback: Arc<dyn Fn() + Send + Sync>,
}

// ── PaintCtx ─────────────────────────────────────────────────────────────────

/// Context passed to every widget's [`Widget::paint`] call.
pub struct PaintCtx<'a> {
    pub canvas: &'a mut SkiaCanvas,
    pub rect: Rect,
    pub font: &'a FontCache,
    pub theme: ThemeData,
    /// Shared hit-target registry. All child contexts share the same vec so
    /// button callbacks registered deep in the tree are visible to the root.
    pub hit_targets: Rc<RefCell<Vec<HitTarget>>>,
}

impl<'a> PaintCtx<'a> {
    /// Derive a child context with a different rect (reborrowing the canvas).
    pub fn child(&mut self, rect: Rect) -> PaintCtx<'_> {
        PaintCtx {
            canvas: self.canvas,
            rect,
            font: self.font,
            theme: self.theme.clone(),
            hit_targets: Rc::clone(&self.hit_targets),
        }
    }

    /// Register a click callback for `self.rect`. Called from Button::paint.
    pub fn register_hit(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        self.hit_targets.borrow_mut().push(HitTarget {
            rect: self.rect,
            callback,
        });
    }

    /// Convert a theme `Color` (f32 0–1) to a render `Color` (u8 0–255).
    ///
    /// Use this to pull semantic colors from the active theme:
    /// ```ignore
    /// let fill = ctx.tc(ctx.theme.colors.surface);
    /// ctx.canvas.fill_rect(ctx.rect, fill);
    /// ```
    pub fn tc(&self, c: tezzera_theme::Color) -> Color {
        Color::rgba(
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            (c.a * 255.0) as u8,
        )
    }

    /// Convenience: fill the widget's rect with a solid color.
    pub fn fill(&mut self, color: Color) {
        self.canvas.fill_rect(self.rect, color);
    }

    /// Convenience: stroke the widget's rect outline.
    pub fn stroke(&mut self, color: Color, width: f32) {
        self.canvas.stroke_rect(self.rect, color, width);
    }

    /// Convenience: draw text relative to the widget's top-left origin.
    pub fn text(&mut self, s: &str, dx: f32, dy: f32, color: Color, px: f32) {
        let p = Point { x: self.rect.origin.x + dx, y: self.rect.origin.y + dy };
        self.canvas.draw_text(s, p, color, self.font, px);
    }
}

// ── Widget trait ─────────────────────────────────────────────────────────────

/// The render layer trait. Every built-in widget implements this.
///
/// `Widget` is the render/paint concern — layout + draw. It is NOT what users
/// implement to compose UI; that's [`tezzera_core::Component`].
/// Custom widgets can implement `Widget` for low-level control.
pub trait Widget: Send + Sync {
    fn layout(&self, constraints: Constraints) -> Size;
    fn paint(&self, ctx: &mut PaintCtx);
    fn flex_factor(&self) -> f32 { 0.0 }

    /// Wrap this widget in an [`Element`] so it can be returned from
    /// `Component::build()`.
    fn into_element(self) -> Element
    where
        Self: Sized + 'static,
    {
        Element::Native(NativeElement {
            tag: std::any::type_name::<Self>(),
            payload: Some(Arc::new(WidgetBox(Box::new(self)))),
            children: vec![],
        })
    }
}

/// Heap-allocated, type-erased widget.
pub type BoxedWidget = Box<dyn Widget>;

impl Widget for Box<dyn Widget> {
    fn layout(&self, constraints: Constraints) -> Size { (**self).layout(constraints) }
    fn paint(&self, ctx: &mut PaintCtx)               { (**self).paint(ctx) }
    fn flex_factor(&self) -> f32                       { (**self).flex_factor() }
}

// ── WidgetBox — bridges Widget into the Element tree ─────────────────────────

/// Concrete wrapper that stores a `Box<dyn Widget>` inside a `NativeElement`.
///
/// The element walker in the umbrella crate downcasts `NativeElement.payload`
/// to this type to retrieve the widget for layout + paint.
pub struct WidgetBox(pub Box<dyn Widget>);

impl WidgetPayload for WidgetBox {
    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the max available width from constraints (f32::INFINITY if unbounded).
pub(crate) fn avail_w(c: Constraints) -> f32 { c.max_width_f32() }

/// Extract the max available height from constraints.
pub(crate) fn avail_h(c: Constraints) -> f32 { c.max_height_f32() }

/// Clamp a size to sit within constraints.
#[allow(dead_code)]
pub(crate) fn clamp(c: Constraints, s: Size) -> Size { c.constrain(s) }

/// Build a Rect from origin point + size.
pub(crate) fn rect_at(origin: Point, size: Size) -> Rect {
    Rect { origin, size }
}

/// Offset a point relative to a parent rect's origin.
pub(crate) fn offset(base: Point, dx: f32, dy: f32) -> Point {
    Point { x: base.x + dx, y: base.y + dy }
}
