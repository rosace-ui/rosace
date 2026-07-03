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
pub mod checkbox;
pub mod chip;
pub mod column;
pub mod container;
pub mod custom_paint;
pub mod dialog;
pub mod divider;
pub mod focus_api;
pub mod icon;
pub mod image;
pub mod list_tile;
pub mod menu;
pub mod nav_rail;
pub mod overlay;
pub mod overlay_api;
pub mod padding;
pub mod progress_bar;
pub mod rect_reader;
pub mod render_tree;
pub mod repaint_boundary;
pub mod row;
pub mod scaffold;
pub mod scroll_view;
pub mod sheet;
pub mod slider;
pub mod spacer;
pub mod stack;
pub mod switch;
pub mod tab;
pub mod text;
pub mod text_input;
pub mod toast;
pub mod tooltip;
pub mod transform_layer;

pub use app::WidgetApp;
pub use app_bar::AppBar;
pub use avatar::Avatar;
pub use badge::Badge;
pub use button::{Button, ButtonVariant};
pub use card::Card;
pub use checkbox::Checkbox;
pub use chip::Chip;
pub use column::Column;
pub use container::Container;
pub use custom_paint::CustomPaint;
pub use dialog::Dialog;
pub use menu::Menu;
pub use sheet::Sheet;
pub use toast::{Toast, ToastKind};
pub use divider::Divider;
pub use focus_api::{FocusApi, WithFocus};
pub use icon::{Icon, IconKind};
pub use image::Image;
pub use list_tile::ListTile;
pub use nav_rail::{NavItem, NavRail};
pub use overlay::{
    LayerId, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
    OverlayEntry, push_overlay, drain_overlays, clear_overlays,
};
pub use overlay_api::{OverlayApi, OverlayKind, WithOverlay};
pub use padding::EdgeInsets;
pub use progress_bar::ProgressBar;
pub use rect_reader::RectReader;
pub use render_tree::{NodeId, RenderTree, TreeNode};
pub use repaint_boundary::RepaintBoundary;
pub use row::Row;
pub use scaffold::Scaffold;
pub use scroll_view::{ScrollView, ScrollAxis};
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
use tezzera_layout::{AxisBound, Constraints};

/// Shrink a bounded axis by `by` logical pixels (padding); unbounded and
/// shrink-to-fit axes pass through unchanged — never collapse Unbounded
/// into `Bounded(f32::INFINITY)`.
pub(crate) fn shrink_axis(b: AxisBound, by: f32) -> AxisBound {
    match b {
        AxisBound::Bounded(v) => AxisBound::Bounded((v - by).max(0.0)),
        other => other,
    }
}
use tezzera_render::{Color, DrawCommand, FontCache, Picture, PictureRecorder};
use tezzera_theme::ThemeData;

// ── Alignment ────────────────────────────────────────────────────────────────

/// Where a single child sits inside its parent's rect (D095).
/// Setting an alignment on [`Container`] makes it fill the available space
/// (Flutter semantics) — otherwise there is nothing to align within.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Alignment {
    TopLeft,    TopCenter,    TopRight,
    CenterLeft, #[default] Center, CenterRight,
    BottomLeft, BottomCenter, BottomRight,
}

impl Alignment {
    /// Child offset within a container for this alignment.
    pub fn offset(&self, container: Size, child: Size) -> Point {
        let fx = match self {
            Alignment::TopLeft | Alignment::CenterLeft | Alignment::BottomLeft => 0.0,
            Alignment::TopCenter | Alignment::Center | Alignment::BottomCenter => 0.5,
            Alignment::TopRight | Alignment::CenterRight | Alignment::BottomRight => 1.0,
        };
        let fy = match self {
            Alignment::TopLeft | Alignment::TopCenter | Alignment::TopRight => 0.0,
            Alignment::CenterLeft | Alignment::Center | Alignment::CenterRight => 0.5,
            Alignment::BottomLeft | Alignment::BottomCenter | Alignment::BottomRight => 1.0,
        };
        Point {
            x: ((container.width - child.width) * fx).max(0.0),
            y: ((container.height - child.height) * fy).max(0.0),
        }
    }
}

// ── Semantics ────────────────────────────────────────────────────────────────

/// A declarative semantics entry (D099). Widgets push these during paint via
/// [`PaintCtx::semantics`]; the frame derives the accessibility tree from the
/// render tree. Roles come from `tezzera_core::Role`.
#[derive(Clone, Debug)]
pub struct Semantics {
    pub role: tezzera_core::Role,
    pub label: Option<String>,
    pub value: Option<String>,
}

impl Semantics {
    pub fn new(role: tezzera_core::Role) -> Self {
        Self { role, label: None, value: None }
    }
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = Some(v.into()); self }
}

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
/// The callback receives `(delta_x, delta_y)` in logical pixels
/// (positive = content scrolls right / down).
pub struct ScrollTarget {
    pub rect: Rect,
    pub callback: Arc<dyn Fn(f32, f32) + Send + Sync>,
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
    /// The persistent render tree (D091) — sole owner of retained per-node
    /// state. Widgets *declare* hit/scroll regions, focus nodes, overlays,
    /// and transform layers onto `node`; the frame pipeline derives dispatch
    /// order and the overlay stack from the tree.
    pub tree: Rc<RefCell<RenderTree>>,
    /// The tree node this widget declares onto.
    pub node: NodeId,
    /// Current clip viewport in world-space logical pixels. `None` means no clip.
    /// Set by `ScrollView` so that `register_hit` ignores targets outside the
    /// visible area, preventing phantom clicks in other panels below the fold.
    pub clip_rect: Option<Rect>,
}

impl<'a> PaintCtx<'a> {
    /// Root context for a standalone paint pass (golden tests, overlay pass).
    /// Starts a frame on `tree` and paints into the root node. Windowed frame
    /// loops that interleave cached subtrees manage the tree explicitly instead.
    pub fn root(
        recorder: &'a mut PictureRecorder,
        rect: Rect,
        font: &'a FontCache,
        theme: ThemeData,
        tree: Rc<RefCell<RenderTree>>,
    ) -> PaintCtx<'a> {
        tree.borrow_mut().start_frame();
        PaintCtx {
            recorder,
            rect,
            font,
            theme,
            tree,
            node: RenderTree::ROOT,
            clip_rect: None,
        }
    }

    /// Derive a child context with a different rect (reborrowing the recorder).
    /// Consumes the next child slot of this node — the child's previously
    /// declared regions are cleared for re-declaration. `clip_rect` propagates.
    pub fn child(&mut self, rect: Rect) -> PaintCtx<'_> {
        let node = self.tree.borrow_mut().slot(self.node, true);
        PaintCtx {
            recorder: self.recorder,
            rect,
            font: self.font,
            theme: self.theme.clone(),
            tree: Rc::clone(&self.tree),
            node,
            clip_rect: self.clip_rect,
        }
    }

    /// Register a scroll viewport so the event router can dispatch wheel events
    /// to the correct `ScrollView`. Called from `ScrollView::paint`. The
    /// callback receives `(delta_x, delta_y)` in logical pixels.
    pub fn register_scroll_target(&self, rect: Rect, callback: Arc<dyn Fn(f32, f32) + Send + Sync>) {
        self.tree.borrow_mut().node_mut(self.node).scrolls.push((rect, callback));
    }

    /// Register a focus node for Tab-cycle inclusion (called from `WithFocus<W>::paint`).
    pub fn register_focus(&self, node: tezzera_a11y::FocusNode) {
        self.tree.borrow_mut().node_mut(self.node).focus.push(node);
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
        self.tree.borrow_mut().node_mut(self.node).hits.push((hit_rect, callback));
    }

    /// Declare that this widget's rect responds to left-click (D099).
    /// Sugar over [`PaintCtx::register_hit`] — clip-aware, z-order and
    /// persistence handled by the render tree.
    pub fn on_press(&self, f: impl Fn() + Send + Sync + 'static) {
        self.register_hit(Arc::new(f));
    }

    /// Declare that this widget's rect responds to scroll wheel/trackpad.
    /// The callback receives `(delta_x, delta_y)` in logical pixels.
    pub fn on_scroll(&self, f: impl Fn(f32, f32) + Send + Sync + 'static) {
        self.register_scroll_target(self.rect, Arc::new(f));
    }

    /// Declare semantics for this widget (D099): role, label, value.
    /// Written to the render-tree node — persists on clean frames, cleared
    /// on repaint, like every other declaration. The a11y tree is derived
    /// from the render tree each frame.
    pub fn semantics(&self, s: Semantics) {
        self.tree.borrow_mut().node_mut(self.node).semantics.push(s);
    }

    /// Attach an overlay entry to this node (called from `WithOverlay::paint`).
    /// The entry persists on the node across cache-hit frames and is cleared
    /// when the node repaints — open overlays cannot vanish on clean frames.
    pub fn attach_overlay(&self, entry: OverlayEntry) {
        self.tree.borrow_mut().node_mut(self.node).overlays.push(entry);
    }

    /// Attach a transform-layer entry to this node (called from
    /// `TransformLayer::paint`). Persists like overlays (D087/D091).
    pub fn attach_transform(&self, entry: TransformLayerEntry) {
        self.tree.borrow_mut().node_mut(self.node).transforms.push(entry);
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

    /// Emit a blurred drop shadow behind a square-cornered `rect`.
    pub fn fill_shadow(&mut self, rect: Rect, color: Color, blur: f32) {
        self.recorder.push(DrawCommand::DrawShadow { rect, radius: 0.0, color, blur });
    }

    /// Emit a blurred drop shadow behind a rounded rect. `radius` must match
    /// the widget's corner radius so the shadow hugs the rounded shape.
    pub fn fill_shadow_rrect(&mut self, rect: Rect, radius: f32, color: Color, blur: f32) {
        self.recorder.push(DrawCommand::DrawShadow { rect, radius, color, blur });
    }

    /// Stroke a rounded-rect outline matching [`PaintCtx::fill_rrect`] geometry.
    pub fn stroke_rrect(&mut self, rect: Rect, radius: f32, color: Color, width: f32) {
        self.recorder.push(DrawCommand::StrokeRRect { rect, radius, color, width });
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
/// How a widget exposes its structure to the framework (D098).
///
/// This is the taxonomy: a leaf keeps the default (`None`), a single-child
/// wrapper returns `One`, a container returns `Many`. Every [`Widget`]
/// default below keys off this, so a wrapper only implements the one
/// method it actually changes.
pub enum Children<'a> {
    /// Leaf — draws content, has no children.
    None,
    /// Single-child wrapper — decorates or constrains one child.
    One(&'a dyn Widget),
    /// Multi-child container — arranges several children.
    Many(&'a [BoxedWidget]),
}

pub trait Widget: Send + Sync {
    /// Declare this widget's children. Drives every default below.
    fn children(&self) -> Children<'_> { Children::None }

    /// Measure under `ctx.constraints` and return a size within them.
    ///
    /// Defaults: leaf → smallest allowed size; `One` → the child's size;
    /// `Many` → stack-like (max of children) — real containers override.
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        match self.children() {
            Children::None => ctx.constraints.constrain(Size { width: 0.0, height: 0.0 }),
            Children::One(c) => c.layout(ctx),
            Children::Many(cs) => {
                let mut s = Size { width: 0.0, height: 0.0 };
                for c in cs {
                    let cz = c.layout(ctx);
                    s.width = s.width.max(cz.width);
                    s.height = s.height.max(cz.height);
                }
                ctx.constraints.constrain(s)
            }
        }
    }

    /// Record draw commands for `ctx.rect`.
    ///
    /// Defaults: leaf → nothing; `One` → paint the child in this rect;
    /// `Many` → stack-like (all children in this rect) — containers that
    /// position children override.
    fn paint(&self, ctx: &mut PaintCtx) {
        match self.children() {
            Children::None => {}
            Children::One(c) => {
                let r = ctx.rect;
                c.paint(&mut ctx.child(r));
            }
            Children::Many(cs) => {
                let r = ctx.rect;
                for c in cs {
                    c.paint(&mut ctx.child(r));
                }
            }
        }
    }

    /// Flex weight inside Row/Column. Wrappers are transparent by default.
    fn flex_factor(&self) -> f32 {
        match self.children() {
            Children::One(c) => c.flex_factor(),
            _ => 0.0,
        }
    }

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

/// `Box<dyn Widget>` is itself a Widget (D093) — builders accepting
/// `impl Widget` take boxed children without adapter structs. Fully
/// transparent delegation: no extra tree node, no behavior change.
impl Widget for Box<dyn Widget> {
    fn children(&self) -> Children<'_>        { (**self).children() }
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
