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
pub mod focus_api;
pub mod icon;
pub mod image;
pub mod list_tile;
pub mod nav_rail;
pub mod overlay;
pub mod overlay_api;
pub mod padding;
pub mod progress_bar;
pub mod rect_reader;
pub mod repaint_boundary;
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
pub mod tooltip;
pub mod transform_layer;

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
pub use focus_api::{FocusApi, WithFocus};
pub use icon::{Icon, IconKind};
pub use image::Image;
pub use list_tile::{ListTile, ListView};
pub use nav_rail::{NavItem, NavRail};
pub use overlay::{
    LayerId, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
    OverlayEntry, push_overlay, drain_overlays, clear_overlays,
};
pub use overlay_api::{OverlayApi, OverlayKind, WithOverlay};
pub use padding::{EdgeInsets, Padding};
pub use progress_bar::ProgressBar;
pub use rect_reader::RectReader;
pub use repaint_boundary::RepaintBoundary;
pub use row::Row;
pub use scaffold::Scaffold;
pub use scroll_view::{ScrollView, ScrollAxis};
pub use sized_box::SizedBox;
pub use slider::Slider;
pub use spacer::{Expanded, Spacer};
pub use stack::Stack;
pub use switch::Switch;
pub use tab::{Tab, TabBar};
pub use text::{Text, TextAlign, FontWeight};
pub use text_input::TextInput;
pub use tooltip::Tooltip;
pub use transform_layer::TransformLayer;

use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

use tezzera_core::types::{Point, Rect, Size};
use tezzera_core::{Element, NativeElement, WidgetPayload};
use tezzera_layout::Constraints;
use tezzera_render::{Color, DrawCommand, FontCache, Picture, PictureRecorder};
use tezzera_theme::ThemeData;

// ── HitTarget ────────────────────────────────────────────────────────────────

/// A clickable region registered during painting.
pub struct HitTarget {
    pub rect: Rect,
    pub callback: Arc<dyn Fn() + Send + Sync>,
}

// ── ScrollTarget ──────────────────────────────────────────────────────────────

/// A scrollable viewport region registered during painting.
///
/// `ScrollView::paint` registers one per live scroll region. The event router
/// dispatches `InputEvent::Scroll` to the target whose rect contains the cursor.
/// The callback receives the delta in logical pixels (positive = content scrolls down).
pub struct ScrollTarget {
    pub rect: Rect,
    pub callback: Arc<dyn Fn(f32) + Send + Sync>,
}

// ── TransformLayerEntry ──────────────────────────────────────────────────────

/// A captured TransformLayer — child content recorded into a separate Picture
/// (D087) that the platform replays into its own SkiaCanvas and presents as an
/// additional GPU compositor layer (D088).
#[derive(Clone)]
pub struct TransformLayerEntry {
    /// Recorded child draw commands — replay-able independently of the main pass.
    pub picture:       Picture,
    /// Natural (unconstrained) size of the child content in logical pixels.
    pub child_size:    Size,
    /// Viewport rect in screen-space logical pixels.
    pub viewport_rect: Rect,
    /// Current horizontal scroll in logical pixels.
    pub scroll_x:      f32,
    /// Current vertical scroll in logical pixels.
    pub scroll_y:      f32,
}

// ── PaintCtx ─────────────────────────────────────────────────────────────────

/// Context passed to every widget's [`Widget::paint`] call.
///
/// Widgets push [`DrawCommand`]s via the helper methods here. Nothing writes
/// pixels during paint — the commands are replayed by the compositor after
/// the full tree has been walked.
pub struct PaintCtx<'a> {
    pub recorder: &'a mut PictureRecorder,
    pub rect: Rect,
    pub font: &'a FontCache,
    pub theme: ThemeData,
    pub hit_targets: Rc<RefCell<Vec<HitTarget>>>,
    /// Scroll targets registered by `ScrollView` widgets during this paint pass.
    pub scroll_targets: Rc<RefCell<Vec<ScrollTarget>>>,
    /// Focus nodes registered by `WithFocus<W>` during this paint pass.
    /// Collected in DFS order; used by `FocusManager` to build the Tab cycle.
    pub focus_nodes: Rc<RefCell<Vec<tezzera_a11y::FocusNode>>>,
    /// TransformLayer entries collected during this paint pass (D087).
    /// The platform replays each into a separate canvas and presents as an
    /// additional GPU compositor layer.
    pub transform_entries: Rc<RefCell<Vec<TransformLayerEntry>>>,
    /// Current clip viewport in world-space logical pixels. `None` means no clip.
    /// Set by `ScrollView` so that `register_hit` ignores targets outside the
    /// visible area, preventing phantom clicks in other panels below the fold.
    pub clip_rect: Option<Rect>,
}

impl<'a> PaintCtx<'a> {
    /// Derive a child context with a different rect (reborrowing the recorder).
    /// `clip_rect` and `scroll_targets` are propagated unchanged.
    pub fn child(&mut self, rect: Rect) -> PaintCtx<'_> {
        PaintCtx {
            recorder: self.recorder,
            rect,
            font: self.font,
            theme: self.theme.clone(),
            hit_targets: Rc::clone(&self.hit_targets),
            scroll_targets: Rc::clone(&self.scroll_targets),
            focus_nodes: Rc::clone(&self.focus_nodes),
            transform_entries: Rc::clone(&self.transform_entries),
            clip_rect: self.clip_rect,
        }
    }

    /// Register a scroll viewport so the event router can dispatch wheel events
    /// to the correct `ScrollView`. Called from `ScrollView::paint`.
    pub fn register_scroll_target(&self, rect: Rect, callback: Arc<dyn Fn(f32) + Send + Sync>) {
        self.scroll_targets.borrow_mut().push(ScrollTarget { rect, callback });
    }

    /// Register a focus node for Tab-cycle inclusion (called from `WithFocus<W>::paint`).
    pub fn register_focus(&self, node: tezzera_a11y::FocusNode) {
        self.focus_nodes.borrow_mut().push(node);
    }

    /// Register a click callback for `self.rect`.
    ///
    /// If a `clip_rect` is active (set by `ScrollView`), the hit target is
    /// intersected with it. Targets fully outside the clip are silently dropped
    /// so they cannot intercept clicks in other panels below the fold.
    pub fn register_hit(&self, callback: Arc<dyn Fn() + Send + Sync>) {
        let hit_rect = if let Some(clip) = self.clip_rect {
            match intersect_rect(self.rect, clip) {
                Some(r) => r,
                None    => return, // widget is outside the visible scroll viewport
            }
        } else {
            self.rect
        };
        self.hit_targets.borrow_mut().push(HitTarget { rect: hit_rect, callback });
    }

    /// Convert a theme color (f32 0.0–1.0) to a render color (u8 0–255).
    pub fn tc(&self, c: tezzera_theme::Color) -> Color {
        Color::rgba(
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8,
            (c.a * 255.0) as u8,
        )
    }

    // ── Draw helpers — all push DrawCommands, no pixel writes ────────────────

    /// Fill `self.rect` with a solid color.
    pub fn fill(&mut self, color: Color) {
        let rect = self.rect;
        self.recorder.push(DrawCommand::FillRect { rect, color });
    }

    /// Stroke the outline of `self.rect`.
    pub fn stroke(&mut self, color: Color, width: f32) {
        let rect = self.rect;
        self.recorder.push(DrawCommand::StrokeRect { rect, color, width });
    }

    /// Fill an arbitrary rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.recorder.push(DrawCommand::FillRect { rect, color });
    }

    /// Stroke an arbitrary rectangle.
    pub fn stroke_rect(&mut self, rect: Rect, color: Color, width: f32) {
        self.recorder.push(DrawCommand::StrokeRect { rect, color, width });
    }

    /// Fill a rounded rectangle with corner radius `radius`.
    pub fn fill_rrect(&mut self, rect: Rect, radius: f32, color: Color) {
        self.recorder.push(DrawCommand::FillRRect { rect, radius, color });
    }

    /// Fill a circle.
    pub fn fill_circle(&mut self, center: Point, radius: f32, color: Color) {
        self.recorder.push(DrawCommand::FillCircle { center, radius, color });
    }

    /// Draw text at an absolute position (not relative to `self.rect`).
    pub fn draw_text_at(&mut self, text: &str, origin: Point, color: Color, px: f32) {
        self.recorder.push(DrawCommand::DrawText {
            text: text.to_string(),
            origin,
            color,
            px,
        });
    }

    /// Draw text at `(self.rect.origin + (dx, dy))`.
    pub fn text(&mut self, s: &str, dx: f32, dy: f32, color: Color, px: f32) {
        let origin = Point { x: self.rect.origin.x + dx, y: self.rect.origin.y + dy };
        self.recorder.push(DrawCommand::DrawText { text: s.to_string(), origin, color, px });
    }

    /// Emit a multi-step drop shadow behind `rect`.
    pub fn fill_shadow(&mut self, rect: Rect, color: Color, blur: f32) {
        self.recorder.push(DrawCommand::DrawShadow { rect, color, blur });
    }

    /// Push a raw [`DrawCommand`] for advanced use.
    pub fn record(&mut self, cmd: DrawCommand) {
        self.recorder.push(cmd);
    }

    /// Create a [`LayoutCtx`] from this paint context.
    ///
    /// Needed when a widget measures children inside `paint()` (e.g. to position
    /// them). Uses the available rect as tight constraints.
    pub fn layout_ctx(&self, constraints: Constraints) -> LayoutCtx<'_> {
        LayoutCtx::new(constraints, self.font, &self.theme)
    }
}

// ── LayoutCtx ────────────────────────────────────────────────────────────────

/// Context passed to every widget's [`Widget::layout`] call.
///
/// Carries the available constraints plus font and theme access so that widgets
/// can measure text accurately without relying on character-count heuristics.
pub struct LayoutCtx<'a> {
    pub constraints: Constraints,
    pub font: &'a FontCache,
    pub theme: &'a ThemeData,
}

impl<'a> LayoutCtx<'a> {
    pub fn new(constraints: Constraints, font: &'a FontCache, theme: &'a ThemeData) -> Self {
        Self { constraints, font, theme }
    }

    /// Derive a child context with tighter constraints (font/theme are shared).
    pub fn with_constraints(&self, constraints: Constraints) -> LayoutCtx<'_> {
        LayoutCtx { constraints, font: self.font, theme: self.theme }
    }
}

// ── Widget trait ─────────────────────────────────────────────────────────────

/// The render layer trait. Every built-in widget implements this.
///
/// `Widget` is the render/paint concern — layout + draw. It is NOT what users
/// implement to compose UI; that's [`tezzera_core::Component`].
/// Custom widgets can implement `Widget` for low-level control.
pub trait Widget: Send + Sync {
    fn layout(&self, ctx: &LayoutCtx) -> Size;
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
            key: None,
        })
    }
}

/// Heap-allocated, type-erased widget.
pub type BoxedWidget = Box<dyn Widget>;

impl Widget for Box<dyn Widget> {
    fn layout(&self, ctx: &LayoutCtx) -> Size { (**self).layout(ctx) }
    fn paint(&self, ctx: &mut PaintCtx)       { (**self).paint(ctx) }
    fn flex_factor(&self) -> f32              { (**self).flex_factor() }
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

/// Intersect two world-space rects. Returns `None` if they do not overlap.
pub(crate) fn intersect_rect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.origin.x.max(b.origin.x);
    let y0 = a.origin.y.max(b.origin.y);
    let x1 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    if x1 > x0 && y1 > y0 {
        Some(Rect { origin: Point { x: x0, y: y0 }, size: Size { width: x1 - x0, height: y1 - y0 } })
    } else {
        None
    }
}
