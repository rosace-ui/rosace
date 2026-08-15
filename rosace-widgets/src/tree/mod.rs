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
pub mod drawer;
pub mod dropdown;
pub mod accordion;
mod hero;
pub mod hero_tag;
pub mod segmented;
pub mod tabs;
pub mod radio;
pub mod skeleton;
pub mod circular_progress;
pub mod wrap;
pub mod positioned;
pub mod grid;
pub mod aspect_ratio;
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
pub mod list_view;
pub mod menu;
pub mod nav_rail;
pub mod overlay;
pub mod overlay_api;
pub mod padding;
pub mod autocomplete;
pub mod dismissible;
pub mod pointer;
pub mod pressable;
pub mod progress_bar;
pub mod pull_to_refresh;
pub mod rect_reader;
pub mod responsive;
pub mod will_pop_scope;
pub mod bottom_nav;
pub mod search_bar;
pub mod snackbar;
pub mod fab;
pub mod table;
pub mod carousel;
pub mod stepper;
pub mod rating_bar;
pub mod interactive_viewer;
pub mod material;
pub mod selection;
pub mod shader_paint;
pub mod date_picker;
pub mod time_picker;
pub mod data_table;
pub mod render_tree;
pub mod stateful;
pub mod repaint_boundary;
pub mod semantics;
pub mod row;
pub mod scaffold;
pub mod screen_transition_view;
pub mod scroll_view;
pub mod sheet;
pub mod slider;
pub mod spacer;
pub mod stack;
pub mod switch;
pub mod tab;
pub mod text;
pub mod text_area;
pub mod text_edit;
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
pub use container::{BoxShape, Container};
pub use aspect_ratio::AspectRatio;
pub use grid::Grid;
pub use circular_progress::CircularProgress;
pub use skeleton::Skeleton;
pub use radio::Radio;
pub use segmented::SegmentedControl;
pub use tabs::{TabView, Tabs};
pub use accordion::Accordion;
pub use hero_tag::{Hero, HeroApi};
pub use dropdown::Dropdown;
pub use drawer::Drawer;
pub use positioned::Positioned;
pub use wrap::Wrap;
pub use custom_paint::CustomPaint;
pub use dialog::{Dialog, DialogPresentation};
pub use menu::Menu;
pub use sheet::Sheet;
pub use toast::{Toast, ToastKind};
pub use divider::Divider;
pub use focus_api::{FocusApi, WithFocus};
pub use icon::{register_icon, resolve_icon, Icon, IconKind};
pub use image::Image;
pub use list_tile::ListTile;
pub use list_view::ListView;
pub use nav_rail::{NavItem, NavRail};
pub use overlay::{
    LayerId, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
    OverlayEntry, push_overlay, drain_overlays, clear_overlays,
};
pub use overlay_api::{OverlayApi, OverlayKind, WithOverlay};
pub use padding::EdgeInsets;
pub use pointer::{AbsorbPointer, IgnorePointer};
pub use autocomplete::Autocomplete;
pub use dismissible::{Dismissible, DismissDirection};
pub use pressable::{LongPressable, PressApi, Pressable};
pub use progress_bar::ProgressBar;
pub use pull_to_refresh::PullToRefresh;
pub use rect_reader::RectReader;
pub use responsive::{breakpoint, Responsive};
pub use will_pop_scope::WillPopScope;
pub use bottom_nav::{BottomNavItem, BottomNavigationBar};
pub use search_bar::SearchBar;
pub use snackbar::Snackbar;
pub use fab::FloatingActionButton;
pub use table::{Table, TableColumn};
pub use carousel::{Carousel, PageView};
pub use stepper::Stepper;
pub use rating_bar::RatingBar;
pub use interactive_viewer::InteractiveViewer;
pub use material::{
    MaterialKey, resolve_material, ContainerMaterial, CardMaterial,
    DialogMaterial, SheetMaterial, DrawerMaterial, AppBarMaterial, BottomNavMaterial,
};
pub use selection::{GlassLens, SelectionKind, SelectionStyle};
pub use shader_paint::ShaderPaint;
pub use date_picker::{DatePicker, SimpleDate, SelectionMode, PageAxis};
pub use time_picker::{TimePicker, SimpleTime, TimeUnit};
pub use data_table::{DataTable, DataTableColumn, SortDirection};
pub use stateful::{Stateful, StatefulExt, StatefulWidget};
pub use render_tree::{HitHandler, InspectNode, NodeId, RenderTree, ScrollAxes, ScrollHandler, TreeNode};
pub use repaint_boundary::RepaintBoundary;
pub use semantics::Semantics;
pub use row::Row;
pub use scaffold::Scaffold;
pub use screen_transition_view::ScreenTransitionView;
pub use scroll_view::{ScrollView, ScrollAxis, MAX_TL_DIM};
pub use slider::Slider;
pub use spacer::{Expanded, Spacer};
pub use stack::Stack;
pub use switch::Switch;
pub use tab::{Tab, TabBar};
pub use text::{Text, TextAlign, FontWeight};
pub use text_area::TextArea;
pub use text_edit::{
    CursorShape, CursorStyle, EditController, EditableDecl, InputFilter, Span, SpanFn,
    TextEditState, TextLayoutSnapshot,
};
pub use text_input::TextInput;
pub use tooltip::{Tooltip, TooltipStyle, WidgetExt};
pub use transform_layer::TransformLayer;

use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_core::{Element, NativeElement, WidgetPayload};
use rosace_layout::{AxisBound, Constraints};

/// Shrink a bounded axis by `by` logical pixels (padding); unbounded and
/// shrink-to-fit axes pass through unchanged — never collapse Unbounded
/// into `Bounded(f32::INFINITY)`.
pub(crate) fn shrink_axis(b: AxisBound, by: f32) -> AxisBound {
    match b {
        AxisBound::Bounded(v) => AxisBound::Bounded((v - by).max(0.0)),
        other => other,
    }
}
use rosace_render::{Color, DrawCommand, FontCache, Picture, PictureRecorder};
use rosace_theme::ThemeData;

// ── Continuous animation request (spinners, shimmer) ─────────────────────────

use std::cell::Cell;
thread_local! {
    static ANIM_REQUEST: Cell<bool> = const { Cell::new(false) };
}

/// Ask the frame loop to schedule another frame — self-animating widgets
/// (CircularProgress spinner, Skeleton shimmer) call this each paint.
pub fn request_animation() { ANIM_REQUEST.with(|a| a.set(true)); }

thread_local! {
    static CURRENT_POINTER: Cell<(f32, f32)> = const { Cell::new((0.0, 0.0)) };
}
/// The latest pointer position (window-space logical px) — set by the engine on
/// every pointer event. Read via [`PaintCtx::pointer`]: for widgets that must
/// follow the finger CONTINUOUSLY during a drag (a clock hand sweeping to any
/// angle), not just snap between discrete values.
pub fn set_pointer(x: f32, y: f32) { CURRENT_POINTER.with(|p| p.set((x, y))); }
/// The last pointer position the engine recorded.
pub fn current_pointer() -> (f32, f32) { CURRENT_POINTER.with(|p| p.get()) }

thread_local! {
    static BOTTOM_OVERLAY_INSET: Cell<f32> = const { Cell::new(0.0) };
}

/// Height reserved at the window's bottom edge by chrome (the Scaffold's
/// bottom bar) — bottom-anchored overlays (Snackbar/Toast) float ABOVE
/// it, per platform convention. Declared fresh each frame by Scaffold's
/// paint; the engine reads it during the overlay pass and resets it.
pub fn set_bottom_overlay_inset(px: f32) { BOTTOM_OVERLAY_INSET.with(|v| v.set(px)); }

/// Engine-side read+reset (once per frame, in the overlay pass).
pub fn take_bottom_overlay_inset() -> f32 { BOTTOM_OVERLAY_INSET.with(|v| v.replace(0.0)) }

/// Frame loop: did any widget request continuous animation this frame?
pub fn take_animation_request() -> bool { ANIM_REQUEST.with(|a| a.replace(false)) }

/// Run `f` and report whether IT asked for another frame, without disturbing
/// what the surrounding paint had already asked for.
///
/// Needed because a self-animating widget requests its next frame *from
/// inside its own `paint`*. Once painting can be skipped by replaying a
/// cached picture, a spinner that replays would silently stop requesting and
/// freeze mid-spin. So a node that animates itself must never replay — and
/// the only way to know that is to watch the flag across its paint.
fn scoped_animation_request<R>(f: impl FnOnce() -> R) -> (R, bool) {
    let outer = ANIM_REQUEST.with(|a| a.replace(false));
    let out = f();
    let mine = ANIM_REQUEST.with(|a| a.replace(outer || false));
    if mine {
        ANIM_REQUEST.with(|a| a.set(true));
    }
    (out, mine)
}

thread_local! {
    static DIRTY_NODES: RefCell<Vec<render_tree::NodeId>> = const { RefCell::new(Vec::new()) };
}

/// Schedule ONE widget to re-layout, re-paint and (if it builds its children)
/// re-build, without rebuilding anything else.
///
/// This is the public seam the plan calls for. `Atom` and
/// `rosace_state::external::Subscribers` mark a whole COMPONENT dirty, which
/// re-runs `build()` and makes the frame structural — every per-node cache is
/// then ignored. This marks a single node, so the frame stays targeted.
///
/// It is public so third-party state libraries get the same precision the
/// built-ins do. `Subscribers` is component-granularity by its own
/// documentation; without this, "plugins are a core principle" would hold for
/// rebuilds and not for the fine-grained path.
pub fn mark_node_dirty(node: render_tree::NodeId) {
    DIRTY_NODES.with(|d| d.borrow_mut().push(node));
    REBUILD_REQUESTS.with(|r| { r.borrow_mut().insert(node); });
    rosace_state::request_frame();
}

thread_local! {
    static REBUILD_REQUESTS: RefCell<std::collections::HashSet<render_tree::NodeId>> =
        RefCell::new(std::collections::HashSet::new());
}

/// Whether `node` was asked to rebuild, consuming the request.
///
/// Kept separate from the node's `needs_layout`/`needs_paint` flags because
/// `Stateful::layout` is the first thing that needs the answer, and
/// `LayoutCtx` has no access to the tree — deliberately, since that absence is
/// what makes skipping layout for a hover provably safe. A thread-local keeps
/// that guarantee intact instead of widening `LayoutCtx` to get around it.
pub fn take_rebuild_request(node: render_tree::NodeId) -> bool {
    REBUILD_REQUESTS.with(|r| r.borrow_mut().remove(&node))
}

type LifecycleObserver = Arc<dyn stateful::StatefulWidget>;
thread_local! {
    static LIFECYCLE_OBSERVERS: RefCell<Vec<(render_tree::NodeId, LifecycleObserver)>> =
        const { RefCell::new(Vec::new()) };
}

/// Register a widget that overrode `on_lifecycle`. Only those are stored, so a
/// phase change costs one call per interested widget rather than a tree walk.
pub fn register_lifecycle_observer(node: render_tree::NodeId, w: LifecycleObserver) {
    LIFECYCLE_OBSERVERS.with(|o| {
        let mut o = o.borrow_mut();
        if !o.iter().any(|(n, _)| *n == node) {
            o.push((node, w));
        }
    });
}

/// Stop delivering lifecycle phases to `node` — called when it is disposed.
pub fn unregister_lifecycle_observer(node: render_tree::NodeId) {
    LIFECYCLE_OBSERVERS.with(|o| o.borrow_mut().retain(|(n, _)| *n != node));
}

/// Deliver an app lifecycle phase to every registered widget. The engine calls
/// this when the phase actually changes.
pub fn dispatch_lifecycle(phase: rosace_core::app_lifecycle::LifecycleState) {
    let observers = LIFECYCLE_OBSERVERS.with(|o| o.borrow().clone());
    for (node, w) in observers {
        let _scope = enter_widget(node);
        w.on_lifecycle(phase);
    }
}

thread_local! {
    static CURRENT_WIDGET: Cell<Option<render_tree::NodeId>> = const { Cell::new(None) };
}

/// Scope guard that makes `node` the ambient widget. Restores the previous
/// one on drop, so nesting works.
pub struct WidgetScope(Option<render_tree::NodeId>);

impl Drop for WidgetScope {
    fn drop(&mut self) {
        CURRENT_WIDGET.with(|c| c.set(self.0));
    }
}

/// Enter `node` as the ambient widget for the duration of the returned guard.
///
/// Set while a widget builds or paints, and again by the engine around a
/// handler it dispatches to that node — so a callback written inside a widget
/// still knows which widget it belongs to when it runs, long after paint
/// returned.
pub fn enter_widget(node: render_tree::NodeId) -> WidgetScope {
    let prev = CURRENT_WIDGET.with(|c| c.replace(Some(node)));
    WidgetScope(prev)
}

/// The widget currently building, painting, or handling an event.
pub fn current_widget() -> Option<render_tree::NodeId> {
    CURRENT_WIDGET.with(|c| c.get())
}

/// Schedule the widget in scope to rebuild.
///
/// Callable from anywhere without threading a handle through `build` — from a
/// button's `on_press`, from a method on your own type, from a helper three
/// calls deep. It resolves against the ambient widget set while a widget
/// builds, paints, or handles an event.
///
/// # Where there is no ambient widget
///
/// A spawned task, a timer callback, or a network response arrives with no
/// widget in scope. There is nothing to refresh and this is a no-op (with a
/// debug warning, since it is almost always a mistake rather than an
/// intention). Capture [`current_widget`] while you still have one and use
/// [`refresh`] with it:
///
/// ```ignore
/// let id = current_widget();                  // during build
/// spawn(async move { fetch().await; if let Some(id) = id { refresh(id) } });
/// ```
pub fn refresh_state() {
    match current_widget() {
        Some(node) => mark_node_dirty(node),
        None => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[ROSACE] refresh_state() called with no widget in scope — nothing was \
                 refreshed. This happens in a spawned task, timer or network callback, \
                 where the ambient widget is gone. Capture current_widget() during build \
                 and call refresh(id) instead."
            );
        }
    }
}

/// Schedule a specific widget to rebuild.
///
/// The explicit form of [`refresh_state`], for when the ambient widget is not
/// available — background work, a plugin store, another thread. `NodeId` is
/// `Copy`, so it can be captured freely.
pub fn refresh(node: render_tree::NodeId) {
    mark_node_dirty(node);
}

/// Take the nodes marked since the last frame. The engine calls this and
/// applies the flags; nothing else should.
pub fn take_dirty_nodes() -> Vec<render_tree::NodeId> {
    DIRTY_NODES.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

/// Whether any widget has asked to update — lets the engine decide to paint a
/// frame at all, since a node mark dirties no component.
pub fn has_dirty_nodes() -> bool {
    DIRTY_NODES.with(|d| !d.borrow().is_empty())
}

thread_local! {
    static STRUCTURAL_FRAME: Cell<bool> = const { Cell::new(true) };
    static FRAME_ID: Cell<u64> = const { Cell::new(0) };
}

/// Monotonic frame counter.
///
/// `Row`/`Column` memoise their child measurements so `layout` and `paint`
/// share one pass. That memo must be scoped to a FRAME: keyed only by
/// constraints, it survives across frames and hands back a stale child size
/// after that child re-laid-out — a grandchild that grew stayed at its old
/// height, silently, with no error. Stamping the frame makes the memo do its
/// real job (avoid the double measure) without pretending to be a cache.
pub fn frame_id() -> u64 { FRAME_ID.with(|f| f.get()) }

/// Advance the frame counter. The engine calls this once per painted frame.
pub fn begin_frame() { FRAME_ID.with(|f| f.set(f.get().wrapping_add(1))); }

/// Tell the widget layer whether this frame may reuse cached pictures.
///
/// **Structural** (the default, and what every frame was before per-node
/// caching): a component rebuilt, the window resized, the theme changed. The
/// widget objects are all NEW, and a node has no way to tell a fresh-but-
/// identical widget from a genuinely changed one — the only way to find out
/// what it draws is to draw it. So caches are ignored and everything
/// repaints, exactly as before.
///
/// **Targeted**: nothing rebuilt; only nodes explicitly marked dirty changed,
/// which today means hover and press. Those nodes re-record and their
/// siblings replay.
///
/// Getting this wrong in the unsafe direction — claiming targeted when
/// something rebuilt — shows a stale picture, so the default is structural
/// and the engine opts in per frame.
pub fn set_structural_frame(v: bool) { STRUCTURAL_FRAME.with(|s| s.set(v)); }
/// Whether this frame must ignore cached pictures. See [`set_structural_frame`].
pub fn is_structural_frame() -> bool { STRUCTURAL_FRAME.with(|s| s.get()) }

/// Seconds since process start — a shared clock for time-driven widgets.
pub fn anim_clock() -> f32 {
    use std::sync::OnceLock;
    use web_time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f32()
}

/// Linear blend between two colors (t in 0..1) — animation interpolation.
pub(crate) fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::rgba(l(a.r, b.r), l(a.g, b.g), l(a.b, b.b), l(a.a, b.a))
}

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

// ── SemanticsProps ────────────────────────────────────────────────────────────────

/// A declarative semantics entry (D099). Widgets push these during paint via
/// [`PaintCtx::semantics`]; the frame derives the accessibility tree from the
/// render tree. Roles come from `rosace_core::Role`.
///
/// `heading_level`/`href` (D107/Phase 25) mirror `rosace_core::SemanticNode`'s

/// Minimum interactive target, in logical pixels (Quality Bar §6).
///
/// 44 is the iOS HIG figure; Material says 48. The smaller of the two is the
/// floor a control must clear, not the size it should look.
///
/// Applied to LAYOUT, not by quietly inflating hit rects. A control reserves
/// at least this much space and paints its smaller visual centred inside it —
/// the approach Flutter takes with `MaterialTapTargetSize.padded`. Growing
/// hit regions instead would make adjacent small controls overlap, and then
/// registration order silently decides which one a tap lands on.
pub const MIN_TAP_TARGET: f32 = 44.0;

/// The height a text-bearing control should PAINT at: its designed height,
/// or the scaled line box plus padding when that is taller.
///
/// `line_height` is what applies `MediaQuery::text_scale`, so a control that
/// hardcodes its height clips its own label the moment a user raises the OS
/// text size. The designed height stays a MINIMUM, so nothing changes at
/// 100%.
pub fn text_fit_height(designed: f32, font: &rosace_render::FontCache, font_size: f32) -> f32 {
    designed.max(font.line_height(font_size) + 12.0)
}

/// The height an interactive, text-bearing control should OCCUPY:
/// [`text_fit_height`] floored at [`MIN_TAP_TARGET`].
///
/// The difference between this and `text_fit_height` is transparent padding
/// — the control paints at the smaller value via [`centered_visual`], so the
/// tap-target floor adds spacing rather than a visually fatter control. Text
/// growth, by contrast, must grow the visual itself, or the label clips.
pub fn control_height(designed: f32, font: &rosace_render::FontCache, font_size: f32) -> f32 {
    text_fit_height(designed, font, font_size).max(MIN_TAP_TARGET)
}


/// The rect a control should PAINT into, centred inside the (larger) rect it
/// was laid out with. See [`MIN_TAP_TARGET`].
pub fn centered_visual(outer: rosace_core::types::Rect, w: f32, h: f32) -> rosace_core::types::Rect {
    let w = w.min(outer.size.width);
    let h = h.min(outer.size.height);
    rosace_core::types::Rect {
        origin: rosace_core::types::Point {
            x: outer.origin.x + (outer.size.width - w) / 2.0,
            y: outer.origin.y + (outer.size.height - h) / 2.0,
        },
        size: rosace_core::types::Size { width: w, height: h },
    }
}

/// fields of the same name — carried through unchanged by `collect_semantics`.
#[derive(Clone, Debug)]
pub struct SemanticsProps {
    pub role: rosace_core::Role,
    pub label: Option<String>,
    pub value: Option<String>,
    pub heading_level: Option<u8>,
    pub href: Option<String>,
}

impl SemanticsProps {
    pub fn new(role: rosace_core::Role) -> Self {
        Self { role, label: None, value: None, heading_level: None, href: None }
    }
    pub fn label(mut self, l: impl Into<String>) -> Self { self.label = Some(l.into()); self }
    pub fn value(mut self, v: impl Into<String>) -> Self { self.value = Some(v.into()); self }
    pub fn heading_level(mut self, level: u8) -> Self { self.heading_level = Some(level); self }
    pub fn href(mut self, href: impl Into<String>) -> Self { self.href = Some(href.into()); self }
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
    /// Content magnification factor — `1.0` for ordinary scrolling (all
    /// existing consumers). `InteractiveViewer` (Phase 32) is the only
    /// consumer that varies this: the offscreen content texture is
    /// rasterized at `dpi_scale * zoom` (engine.rs), so the compositor's
    /// existing UV-window math (`uv_span = dest / tex_size`) naturally
    /// samples a smaller fraction of a bigger texture — real GPU-crisp
    /// zoom with no compositor changes. Screen<->content coordinate remap
    /// (`child_coords`/`content_to_screen`) must divide/multiply by this.
    pub zoom:          f32,
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
    /// The component that owns this paint subtree — node-created state
    /// (default scroll controllers, D101) subscribes it so writes repaint.
    pub owner: rosace_core::types::ComponentId,
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
            owner: rosace_core::types::ComponentId(0),
            clip_rect: None,
        }
    }

    /// Derive a child context with a different rect (reborrowing the recorder).
    /// Consumes the next child slot of this node — the child's previously
    /// declared regions are cleared for re-declaration. `clip_rect` propagates.
    pub fn child(&mut self, rect: Rect) -> PaintCtx<'_> {
        let node = self.tree.borrow_mut().slot(self.node, true);
        // Record every painted widget node's world-space rect (D123/O2):
        // the DevTools element picker (`RenderTree::pick`/`inspect`) reads
        // `cached_rect`, and without this only element/cache-boundary nodes
        // — not the widgets inside a component's tree — were selectable.
        // Cheap (one field write per child paint); walk_element sets its
        // OWN element nodes' `cached_rect` separately, so this can't
        // interfere with the picture-cache replay check.
        self.tree.borrow_mut().node_mut(node).cached_rect = Some(rect);
        PaintCtx {
            recorder: self.recorder,
            rect,
            font: self.font,
            theme: self.theme.clone(),
            tree: Rc::clone(&self.tree),
            node,
            owner: self.owner,
            clip_rect: self.clip_rect,
        }
    }

    /// This widget's node in the render tree.
    pub fn node_id(&self) -> render_tree::NodeId { self.node }

    /// Run `f` when this widget leaves the tree — cancel a subscription, stop
    /// a timer, release a handle.
    ///
    /// Fires from both removal paths: the widget being dropped from its
    /// parent's children, and its slot being taken over by a different widget
    /// type. Children are disposed before parents.
    ///
    /// Register it once, guarded by first-run state, or every paint will queue
    /// another copy.
    pub fn on_dispose(&self, f: impl FnOnce() + Send + 'static) {
        self.tree.borrow_mut().node_mut(self.node).dispose.push(Box::new(f));
    }

    /// Paint `child` into its own node, giving that node TYPE IDENTITY.
    ///
    /// Prefer this over `child.paint(&mut ctx.child(rect))`. Both create a
    /// node at the next slot; only this one records WHAT is in the slot, so
    /// the tree can tell a `Button` replacing a `TextField` from a `Button`
    /// repainting. See [`RenderTree::adopt_tag`] for what that prevents.
    ///
    /// [`Self::child`] stays for the other use of a node — declaring a bare
    /// region for hit-testing or semantics without a widget to paint
    /// (`ctx.child(hit).register_hit(..)`). Those nodes have no widget, so
    /// there is no type to record and nothing to cache.
    pub fn paint_child(&mut self, rect: Rect, child: &dyn Widget) {
        // `reset: false` — a node that ends up replaying must KEEP the hit
        // regions, semantics and scroll viewports it declared during its last
        // real paint. `begin` (what `reset: true` calls) clears exactly those,
        // and the widget only re-declares them by running. Resetting here and
        // then replaying would render a widget that looks right and cannot be
        // clicked. The repaint path below resets explicitly instead.
        let node = self.tree.borrow_mut().slot(self.node, false);

        // Layout and paint walk the same children through separate cursors.
        // If a widget's `layout` measures a different set than its `paint`
        // slots, the two disagree and this child would inherit a sibling's
        // cached size — plausible-looking, wrong, and invisible to any test
        // that only compares pixels. Fail loudly in dev instead.
        //
        // The fix when this fires is NOT to force the indices to line up: it
        // is to leave that widget's `layout` uncached (plain
        // `ctx.with_constraints`) and let the paint-time `measure_child` do
        // the caching, since that one peeks the slot paint will consume.
        #[cfg(debug_assertions)]
        {
            let claimed = self.tree.borrow().node(node).layout_tag;
            debug_assert!(
                claimed.is_empty() || claimed == child.type_tag(),
                "layout/paint slot misalignment: the layout walk put `{claimed}` in this \
                 slot but paint is putting `{}` there. That widget's layout measures a \
                 different set of children than its paint slots — leave its layout \
                 uncached and measure during paint instead.",
                child.type_tag(),
            );
        }
        self.tree.borrow_mut().node_mut(node).layout_tag = "";

        let retagged = self.tree.borrow_mut().adopt_tag(node, child.type_tag());

        let replay = !retagged
            && !is_structural_frame()
            && {
                let t = self.tree.borrow();
                let n = t.node(node);
                !n.needs_paint
                    && n.cached_picture.is_some()
                    // Same place as well as same content. A moved widget could
                    // in principle be re-blitted with `replay_offset`, but its
                    // hit regions are world-space and declared during paint, so
                    // moving the pixels without re-running would leave the
                    // clickable area behind. Re-record until that is handled.
                    && n.cached_rect == Some(rect)
                    // A widget that drives its own animation asks for the next
                    // frame from inside `paint`. If it replays it stops asking
                    // and freezes mid-animation.
                    && !n.self_animating
            };

        if replay {
            let pic = self.tree.borrow().node(node).cached_picture.clone()
                .expect("checked above");
            for cmd in &pic.commands {
                self.recorder.push(cmd.clone());
            }
            return;
        }

        self.tree.borrow_mut().reset(node);
        self.tree.borrow_mut().node_mut(node).cached_rect = Some(rect);

        // Record into a sub-recorder so this child's commands can be kept as
        // its own picture, then splice them into the parent's stream.
        let mut sub = PictureRecorder::new();
        let ((), animating) = scoped_animation_request(|| {
            let mut cctx = PaintCtx {
                recorder: &mut sub,
                rect,
                font: self.font,
                theme: self.theme.clone(),
                tree: Rc::clone(&self.tree),
                node,
                owner: self.owner,
                clip_rect: self.clip_rect,
            };
            child.paint(&mut cctx);
        });

        let pic = sub.finish();
        for cmd in &pic.commands {
            self.recorder.push(cmd.clone());
        }
        let mut t = self.tree.borrow_mut();
        let n = t.node_mut(node);
        n.cached_picture = Some(Arc::new(pic));
        n.needs_paint = false;
        n.self_animating = animating;
    }

    /// Like [`Self::child`], but the child's node is found by an explicit
    /// stable `key` (`RenderTree::keyed_slot`) instead of positional call
    /// order — used ONLY by `ScreenTransitionView` so a screen keeps its
    /// own scroll position/animation state across navigation instead of
    /// aliasing onto whatever screen last occupied that tree position. See
    /// `render_tree.rs`'s module doc ("Identity" section) for the full story.
    pub fn child_keyed(&mut self, rect: Rect, key: u64) -> PaintCtx<'_> {
        let node = self.tree.borrow_mut().keyed_slot(self.node, key);
        self.tree.borrow_mut().node_mut(node).cached_rect = Some(rect);
        PaintCtx {
            recorder: self.recorder,
            rect,
            font: self.font,
            theme: self.theme.clone(),
            tree: Rc::clone(&self.tree),
            node,
            owner: self.owner,
            clip_rect: self.clip_rect,
        }
    }

    /// Register a scroll viewport so the event router can dispatch wheel events
    /// to the correct `ScrollView`. Called from `ScrollView::paint`. The
    /// callback receives `(delta_x, delta_y)` in logical pixels.
    pub fn register_scroll_target(
        &self,
        rect: Rect,
        axes: render_tree::ScrollAxes,
        callback: Arc<dyn Fn(f32, f32) + Send + Sync>,
    ) {
        self.tree.borrow_mut().node_mut(self.node).scrolls.push((rect, axes, callback));
    }

    /// Register a trackpad pinch-to-zoom region (`InteractiveViewer`, Phase
    /// 32) — the callback receives the gesture's raw `delta` (see
    /// [`render_tree::ZoomRegion`]'s doc: an increment, not a multiplier).
    pub fn register_zoom_target(&self, rect: Rect, callback: Arc<dyn Fn(f32) + Send + Sync>) {
        self.tree.borrow_mut().node_mut(self.node).zooms.push((rect, callback));
    }

    /// Register a focus node for Tab-cycle inclusion (called from `WithFocus<W>::paint`).
    pub fn register_focus(&self, node: rosace_core::a11y::FocusNode) {
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
        // Bind the owning widget INTO the handler, so `refresh_state()` called
        // inside an `on_press` resolves to the widget that registered it.
        //
        // Captured here rather than restored by the engine at dispatch time
        // because here is where it is known for certain: `hit_test` returns a
        // handler with no node attached, so the engine would have to be told,
        // and any path that forgot would silently refresh the wrong widget or
        // none at all. Binding at registration makes it correct by
        // construction.
        // Bind the enclosing STATEFUL widget, not this node. A `Button` has
        // nothing to rebuild — `refresh_state()` written in its `on_press`
        // means "update the widget whose `build` produced me". Falls back to
        // this node when there is no stateful ancestor, where the mark is
        // harmless and rebuilds nothing.
        let node = current_widget().unwrap_or(self.node);
        let bound: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _scope = enter_widget(node);
            callback();
        });
        self.tree.borrow_mut().node_mut(self.node).hits.push((hit_rect, bound));
    }

    /// Declare that this widget's rect responds to left-click (D099).
    /// Sugar over [`PaintCtx::register_hit`] — clip-aware, z-order and
    /// persistence handled by the render tree.
    pub fn on_press(&self, f: impl Fn() + Send + Sync + 'static) {
        self.register_hit(Arc::new(f));
    }

    /// The implicit scroll controller for this widget's tree node (D101):
    /// created on first use, persists across rebuilds, subscribed to the
    /// owning component so scroll writes repaint. This is why
    /// `ScrollView::new(child)` scrolls with zero wiring.
    pub fn scroll_controller(&self) -> crate::scroll::ScrollController {
        let mut tree = self.tree.borrow_mut();
        let node = tree.node_mut(self.node);
        if let Some(c) = &node.scroll_ctrl {
            return c.clone();
        }
        let c = crate::scroll::ScrollController::new();
        c.offset.subscribe(self.owner);
        c.content_size.subscribe(self.owner);
        c.viewport_size.subscribe(self.owner);
        node.scroll_ctrl = Some(c.clone());
        c
    }

    /// True while the cursor is over this widget's interactive region —
    /// paint hover feedback with it. Hover changes repaint automatically.
    pub fn hovered(&self) -> bool {
        self.tree.borrow().node(self.node).hovered
    }

    /// True from MouseDown until MouseUp while this widget is the pressed
    /// target — pair with [`Self::animate_to`] for press/tap feedback
    /// (D108/Phase 26 Step 1).
    pub fn pressed(&self) -> bool {
        self.tree.borrow().node(self.node).pressed
    }

    /// Declare a hover-only region (tooltips): participates in hover
    /// tracking without swallowing clicks.
    /// True when the pointer is over this widget **or anything inside it**.
    ///
    /// [`Self::hovered`] is exact-node-only: hover marks a single topmost
    /// node, so a wrapper around an interactive child never sees it (the
    /// child wins). Wrappers whose whole purpose is to react to the pointer
    /// being *somewhere in their subtree* — `Tooltip` above all — need this
    /// instead. Kept separate from `hovered()` rather than changing it,
    /// because 19 widgets drive visual state off the exact-node meaning and
    /// would light up spuriously if hover propagated to every ancestor.
    pub fn hovered_within(&self) -> bool {
        let tree = self.tree.borrow();
        fn any(tree: &RenderTree, id: NodeId) -> bool {
            let n = tree.node(id);
            n.hovered || n.children.iter().any(|&c| any(tree, c))
        }
        any(&tree, self.node)
    }

    pub fn hoverable(&self) {
        let r = self.rect;
        self.tree.borrow_mut().node_mut(self.node).hover_regions.push(r);
    }

    /// Declare a long-press callback for this widget's rect (fires after
    /// ~500 ms of press without movement).
    pub fn on_long_press(&self, f: impl Fn() + Send + Sync + 'static) {
        let r = self.rect;
        self.tree.borrow_mut().node_mut(self.node).long_hits.push((r, Arc::new(f)));
    }

    /// Pointer interception for this subtree: `IgnorePointer` /
    /// `AbsorbPointer` widgets call this — 1 = transparent, 2 = absorb.
    pub fn set_pointer_mode(&self, mode: u8) {
        self.tree.borrow_mut().node_mut(self.node).pointer_mode = mode;
    }

    /// Declare a POSITIONAL press region for this widget's rect — the
    /// callback receives the click point in window-space logical pixels
    /// (sliders, pickers, canvases). Clip-aware like register_hit.
    pub fn on_press_at(&self, f: impl Fn(f32, f32) + Send + Sync + 'static) {
        let hit_rect = if let Some(clip) = self.clip_rect {
            match intersect_rect(self.rect, clip) {
                Some(r) => r,
                None    => return,
            }
        } else {
            self.rect
        };
        self.tree.borrow_mut().node_mut(self.node).hits_at.push((hit_rect, Arc::new(f)));
    }

    /// Declares a nested-scroll chain link over this widget's rect (see
    /// `render_tree::ScrollHandler`'s doc for the full contract) — what
    /// `ScrollView`'s own drag-to-pan registers instead of
    /// [`Self::on_press_at`], so a gesture that starts on a plain-hit
    /// child inside it (or on an inner nested `ScrollView`) can still
    /// reach it, and it in turn can hand off to whatever encloses IT once
    /// exhausted.
    pub fn register_nested_scroll(&mut self, f: impl Fn(f32, f32) -> bool + Send + Sync + 'static) {
        let hit_rect = if let Some(clip) = self.clip_rect {
            match intersect_rect(self.rect, clip) {
                Some(r) => r,
                None    => return,
            }
        } else {
            self.rect
        };
        self.tree.borrow_mut().node_mut(self.node).nested_scrolls.push((hit_rect, Arc::new(f)));
    }

    /// Declare that this widget's rect responds to scroll wheel/trackpad.
    /// The callback receives `(delta_x, delta_y)` in logical pixels.
    pub fn on_scroll(&self, f: impl Fn(f32, f32) + Send + Sync + 'static) {
        self.register_scroll_target(self.rect, render_tree::ScrollAxes::BOTH, Arc::new(f));
    }

    /// Declare semantics for this widget (D099): role, label, value.
    /// Written to the render-tree node — persists on clean frames, cleared
    /// on repaint, like every other declaration. The a11y tree is derived
    /// from the render tree each frame.
    pub fn semantics(&self, s: SemanticsProps) {
        self.tree.borrow_mut().node_mut(self.node).semantics.push(s);
    }

    /// Drops this node and everything below it from the accessibility tree.
    ///
    /// For purely decorative content that would otherwise be announced as
    /// noise — a background flourish, a duplicate icon sitting next to text
    /// that already says the same thing. Backs `Semantics::exclude()`.
    pub fn exclude_semantics(&self) {
        self.tree.borrow_mut().node_mut(self.node).semantics_excluded = true;
    }

    /// Absorbs this node's descendants into its own semantics — the node is
    /// announced, the subtree below it is not. Backs `Semantics::merge()`.
    pub fn merge_semantics(&self) {
        self.tree.borrow_mut().node_mut(self.node).semantics_merges_descendants = true;
    }

    /// The [`rosace_core::a11y::FocusNode`] for this widget's tree position —
    /// created lazily on first paint and persists across rebuilds, the
    /// same "zero wiring by default" precedent as [`Self::scroll_controller`]
    /// (D101: "this is why `ScrollView::new(child)` scrolls with zero
    /// wiring"). Powers `TextInput`'s built-in click-to-focus/Tab-cycling
    /// (D112/Phase 28) without requiring every app to construct and wire
    /// an explicit `FocusNode` for the common single-field case — apps
    /// that DO want explicit neighbor wiring can still layer
    /// `FocusApi::focus_node` on top; the two are independent focus-graph
    /// nodes if both are used on the same widget.
    pub fn focus_node(&self) -> rosace_core::a11y::FocusNode {
        self.focus_node_seeded(false)
    }

    /// Same as [`Self::focus_node`], but if this is the FIRST paint of
    /// this render-tree node (no focus node existed yet) and `seed` is
    /// true, requests focus immediately. Backs `TextInput::focused()`'s
    /// "start focused" behavior: a one-shot seed, not a per-frame
    /// re-request — a later paint with `seed == true` on an
    /// already-focus-noded position does NOT steal focus back after the
    /// user has tabbed away.
    pub fn focus_node_seeded(&self, seed: bool) -> rosace_core::a11y::FocusNode {
        let mut tree = self.tree.borrow_mut();
        let node = tree.node_mut(self.node);
        if let Some(f) = &node.focus_node {
            return f.clone();
        }
        let f = rosace_core::a11y::FocusNode::new();
        if seed {
            f.request();
        }
        node.focus_node = Some(f.clone());
        f
    }

    /// Declare this widget's rect as editable text content (D112/Phase 28
    /// Step 1). The engine's key/click dispatch (`rosace/src/engine.rs`)
    /// finds it via the render tree, not a captured closure — see
    /// [`text_edit::EditableDecl`]'s doc comment for why a plain
    /// `Arc<dyn Fn + Send + Sync>` hit callback can't do this job.
    pub fn register_editable(&self, decl: text_edit::EditableDecl) {
        self.tree.borrow_mut().node_mut(self.node).editable = Some(decl);
    }

    /// This widget's persistent cursor/selection state (D091) — read
    /// during paint to draw the caret/selection highlight. Mutated by the
    /// engine's key/click dispatch, never by the widget itself (`paint`
    /// takes `&self`) — with one deliberate exception: the VIEW-state
    /// field `scrolled_cursor`, written through [`Self::set_scrolled_cursor`].
    pub fn text_edit(&self) -> text_edit::TextEditState {
        self.tree.borrow().node(self.node).text_edit.clone()
    }

    /// Record the caret position scroll-into-view has chased (see
    /// `TextEditState::scrolled_cursor`). View state, so paint-written —
    /// the one sanctioned widget-side write into `text_edit`.
    pub fn set_scrolled_cursor(&self, cursor: Option<usize>) {
        self.tree.borrow_mut().node_mut(self.node).text_edit.scrolled_cursor = cursor;
    }

    /// Record the horizontal scroll-into-view offset (see
    /// [`TextEditState::scroll_x`]) — the single-line counterpart to
    /// `set_scrolled_cursor`. View state, so paint-written: `TextInput`
    /// computes how far the content must shift left to keep the caret
    /// visible when the value overflows the field, and stores it here so
    /// it persists across repaints instead of resetting to 0.
    pub fn set_scroll_x(&self, scroll_x: f32) {
        self.tree.borrow_mut().node_mut(self.node).text_edit.scroll_x = scroll_x;
    }

    /// Record `paint` into a standalone [`Picture`] at `rect`, returning it —
    /// used by RepaintBoundary to cache an expensive subtree. Runs on a fresh
    /// child slot so interactive regions declared inside still register.
    pub fn capture(&mut self, rect: Rect, paint: impl FnOnce(&mut PaintCtx)) -> rosace_render::Picture {
        let node = self.tree.borrow_mut().slot(self.node, true);
        self.capture_into(node, rect, paint)
    }

    /// Consume the next child slot WITHOUT resetting it — preserves the
    /// subtree's declared interactive regions across a cache-replay frame.
    pub fn keep_child_slot(&mut self) {
        self.tree.borrow_mut().slot(self.node, false);
    }

    fn capture_into(&mut self, node: NodeId, rect: Rect, paint: impl FnOnce(&mut PaintCtx)) -> rosace_render::Picture {
        let mut rec = rosace_render::PictureRecorder::new();
        {
            let mut cctx = PaintCtx {
                recorder: &mut rec,
                rect,
                font: self.font,
                theme: self.theme.clone(),
                tree: Rc::clone(&self.tree),
                node,
                owner: self.owner,
                clip_rect: self.clip_rect,
            };
            paint(&mut cctx);
        }
        rec.finish()
    }

    /// Replay an already-recorded [`Picture`] into this context, translating
    /// every command by `(dx, dy)`.
    pub fn replay_offset(&mut self, picture: &rosace_render::Picture, dx: f32, dy: f32) {
        for cmd in &picture.commands {
            self.recorder.push(cmd.offset(dx, dy));
        }
    }

    /// Replay a [`Picture`] captured at `src` instead at `dst` — translates
    /// AND scales every command's geometry, unlike [`Self::replay_offset`]'s
    /// translate-only. Backs Hero/shared-element transitions (D108/Phase 26
    /// Step 5): a widget's captured appearance on one screen re-painted at a
    /// different-sized rect on the other screen's tagged match.
    pub fn replay_morphed(&mut self, picture: &rosace_render::Picture, src: Rect, dst: Rect) {
        let sx = if src.size.width.abs() > f32::EPSILON { dst.size.width / src.size.width } else { 1.0 };
        let sy = if src.size.height.abs() > f32::EPSILON { dst.size.height / src.size.height } else { 1.0 };
        for cmd in &picture.commands {
            self.recorder.push(cmd.morph(src.origin, dst.origin, sx, sy));
        }
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
    pub fn tc(&self, c: rosace_theme::Color) -> Color {
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

    /// Frosted-glass panel (D-DEF-012): blurs and tints everything already
    /// painted beneath `rect` behind a rounded panel — real backdrop
    /// glassmorphism on GPU-composited targets (CPU fallback: translucent
    /// tint, no blur). `blur` is the Gaussian strength in logical px;
    /// `tint.a` controls how strongly the tint mixes over the blur.
    pub fn backdrop_blur(&mut self, rect: Rect, radius: f32, blur: f32, tint: Color) {
        self.recorder.push(DrawCommand::BackdropBlur { rect, radius, blur, tint });
    }

    /// Fill `rect` with a registered GPU shader pipeline (D109/Phase 27).
    ///
    /// `uniforms` come from a `#[derive(ShaderUniforms)]` struct's
    /// `to_bytes()`. The pipeline must have been registered via
    /// `rosace_shader::register_shader` (compiled eagerly at the next frame
    /// boundary). Executes on the GPU at present time — this records a
    /// command, like every other helper here, and never touches pixels.
    /// Renders on GPU-composited targets only (desktop/mobile); web and the
    /// softbuffer fallback drop it (Phase 27's documented scope).
    pub fn shader_fill(&mut self, rect: Rect, pipeline: rosace_shader::PipelineId, uniforms: Vec<u8>) {
        self.recorder.push(DrawCommand::ShaderFill {
            pipeline_id: pipeline.raw(),
            rect,
            uniforms,
            animate_time: false,
        });
    }

    /// [`Self::shader_fill`] with the D109-maturity animation flag: the
    /// PLATFORM patches the first 4 uniform bytes (the `time`-first
    /// convention) with a live clock at every present, so continuous
    /// animation costs a GPU buffer write per frame — record once, never
    /// repaint, no `request_animation` loop.
    pub fn shader_fill_animated(&mut self, rect: Rect, pipeline: rosace_shader::PipelineId, uniforms: Vec<u8>) {
        self.recorder.push(DrawCommand::ShaderFill {
            pipeline_id: pipeline.raw(),
            rect,
            uniforms,
            animate_time: true,
        });
    }

    /// Promotes `weight` one step toward bold when the OS accessibility
    /// "bold text" setting is on (`mq.bold_text`) — leaves anything already
    /// SemiBold/Bold alone, since it's already at or past that intent.
    fn bold_text_weight(mq: rosace_core::MediaQuery, weight: rosace_render::FontWeight) -> rosace_render::FontWeight {
        use rosace_render::FontWeight;
        if mq.bold_text && matches!(weight, FontWeight::Light | FontWeight::Regular | FontWeight::Medium) {
            FontWeight::SemiBold
        } else {
            weight
        }
    }

    /// Draw text at an absolute position (not relative to `self.rect`).
    pub fn draw_text_at(&mut self, text: &str, origin: Point, color: Color, px: f32) {
        let mq = rosace_core::media_query::use_media_query();
        let px = px * mq.text_scale;
        self.recorder.push(DrawCommand::DrawText {
            text: text.to_string(),
            origin,
            color,
            px,
            weight: Self::bold_text_weight(mq, rosace_render::FontWeight::Regular),
        });
    }

    /// Draw a glyph at EXACTLY `px`, ignoring `MediaQuery::text_scale`.
    ///
    /// For icon glyphs only. An icon's `size` is a design dimension — the
    /// diameter of a target, the height of a bar — not type, so it does not
    /// grow with the OS text setting (same choice Flutter and Compose make).
    ///
    /// This exists because `draw_text_at` scales the px it draws while
    /// `FontCache::glyph`/`ascender` (which every icon uses to centre
    /// itself) do NOT. Drawing an icon through the text path therefore
    /// rendered the glyph at `px * text_scale` inside a box of `px`,
    /// positioned by metrics solved at `px` — oversized AND mis-centred,
    /// with the error growing linearly in the OS setting.
    pub fn draw_glyph_unscaled(&mut self, text: &str, origin: Point, color: Color, px: f32) {
        self.recorder.push(DrawCommand::DrawText {
            text: text.to_string(),
            origin,
            color,
            px,
            // Icons are not prose: `bold_text` must not thicken them either.
            weight: rosace_render::FontWeight::Regular,
        });
    }

    /// Draw text at `(self.rect.origin + (dx, dy))`.
    pub fn text(&mut self, s: &str, dx: f32, dy: f32, color: Color, px: f32) {
        let mq = rosace_core::media_query::use_media_query();
        let px = px * mq.text_scale;
        let origin = Point { x: self.rect.origin.x + dx, y: self.rect.origin.y + dy };
        self.recorder.push(DrawCommand::DrawText {
            text: s.to_string(), origin, color, px,
            weight: Self::bold_text_weight(mq, rosace_render::FontWeight::Regular),
        });
    }

    /// Draw text at `(self.rect.origin + (dx, dy))` with an explicit weight —
    /// SemiBold/Bold route to the real bold face.
    pub fn text_styled(&mut self, s: &str, dx: f32, dy: f32, color: Color, px: f32, weight: rosace_render::FontWeight) {
        let mq = rosace_core::media_query::use_media_query();
        let px = px * mq.text_scale;
        let weight = Self::bold_text_weight(mq, weight);
        let origin = Point { x: self.rect.origin.x + dx, y: self.rect.origin.y + dy };
        self.recorder.push(DrawCommand::DrawText { text: s.to_string(), origin, color, px, weight });
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

    /// Fill a (rounded) rect with a two-stop linear gradient.
    pub fn fill_gradient(&mut self, rect: Rect, radius: f32, from: Color, to: Color, vertical: bool) {
        self.recorder.push(DrawCommand::FillGradient { rect, radius, from, to, vertical });
    }

    /// Draw a ring segment (progress arc / spinner).
    pub fn fill_arc(&mut self, center: Point, radius: f32, thickness: f32, start_deg: f32, sweep_deg: f32, color: Color) {
        self.recorder.push(DrawCommand::FillArc { center, radius, thickness, start_deg, sweep_deg, color });
    }

    /// Request another frame — self-animating widgets (spinner, shimmer) call
    /// this each paint so the frame loop keeps repainting them.
    pub fn request_animation(&self) { crate::tree::request_animation(); }

    /// The theme's [`AnimationConfig`], with `enabled` forced `false` when
    /// the OS accessibility "reduce motion" setting is on
    /// (`rosace_core::media_query().reduce_motion`) — same choke-point
    /// pattern as `text_scale`: override at the point the raw theme value is
    /// about to be used, no signature changes, no per-widget edits.
    fn reduce_motion_animation_cfg(&self) -> rosace_theme::AnimationConfig {
        let cfg = self.theme.animation;
        if rosace_core::media_query::use_media_query().reduce_motion {
            rosace_theme::AnimationConfig { enabled: false, ..cfg }
        } else {
            cfg
        }
    }

    /// Ease this node's persistent scalar toward `target` and return the
    /// current value. Honors the theme's global [`AnimationConfig`]: when
    /// disabled it snaps; otherwise it exponentially eases over the theme's
    /// duration (or `duration_ms` if > 0) and keeps requesting frames until
    /// settled. This is how Switch/Checkbox/Radio animate WITHOUT any per-
    /// widget state — the animation policy is global (theme), the value is
    /// per-node. First observation snaps (no appear-animation).
    pub fn animate_to(&self, target: f32, duration_ms: f32) -> f32 {
        let cfg = self.reduce_motion_animation_cfg();
        if !cfg.enabled {
            self.tree.borrow_mut().node_mut(self.node).anim = Some(target);
            return target;
        }
        let dur = (if duration_ms > 0.0 { duration_ms } else { cfg.duration_ms }).max(1.0);
        let (val, settled) = {
            let mut tree = self.tree.borrow_mut();
            let node = tree.node_mut(self.node);
            match node.anim {
                None => { node.anim = Some(target); (target, true) }
                Some(cur) => {
                    let dt = rosace_animate::frame_dt();
                    let alpha = 1.0 - (-dt * (1000.0 / dur)).exp();
                    let next = cur + (target - cur) * alpha;
                    let settled = (next - target).abs() < 0.001;
                    let v = if settled { target } else { next };
                    node.anim = Some(v);
                    (v, settled)
                }
            }
        };
        if !settled { crate::tree::request_animation(); }
        val
    }

    /// Seeds this node's persistent animated scalar to `value` — but ONLY
    /// if it has never been observed before (`None`). An already-set value
    /// is left untouched. Pairs with `animate_to` to opt OUT of its "first
    /// observation snaps straight to target" behavior for a genuine
    /// appear-animation: call this with the START value (e.g. `0.0`)
    /// before the first `animate_to` call on a node that should visibly
    /// ease in rather than pop in fully-formed — e.g. an image fading in
    /// from 0 opacity the first frame it has real decoded content
    /// (D108/Phase 26 Step 4), not fully-formed from frame one.
    pub fn seed_anim_if_unset(&self, value: f32) {
        let mut tree = self.tree.borrow_mut();
        let node = tree.node_mut(self.node);
        if node.anim.is_none() {
            node.anim = Some(value);
        }
    }

    /// Unconditionally sets this node's persistent animated scalar (the same
    /// one `animate_to` eases) to `value` — unlike `seed_anim_if_unset`, this
    /// always overwrites. For a widget that renders its OWN live value
    /// alongside `animate_to`'s eased one (a drag gesture's raw finger
    /// offset, summed with the eased snap-to-page position — see
    /// `Carousel`): call this right before switching `animate_to`'s target
    /// so the eased value starts from wherever the combined visual position
    /// actually was, instead of jumping from the stale pre-drag value and
    /// losing the live offset in the same frame (found live: released a
    /// carousel drag past the swipe threshold and the page visibly popped
    /// before easing, instead of continuing smoothly from the finger).
    pub fn set_anim(&self, value: f32) {
        self.tree.borrow_mut().node_mut(self.node).anim = Some(value);
    }

    /// Ease the `channel`-th independent animated scalar of this node toward
    /// `target` and return the current value. This is the multi-value sibling
    /// of [`Self::animate_to`]: a widget that must animate more than one thing
    /// at once (a Switch's thumb *position* AND its hover/press *state-layer*,
    /// a Slider's fill AND its thumb halo) gives each its own `channel`.
    ///
    /// Channels are independent persistent scalars keyed by the explicit
    /// `channel` index — no call-order coupling, so branches that skip a
    /// channel some frames don't shift the others. Identical easing policy to
    /// `animate_to`: honors the theme's global `AnimationConfig` (snaps when
    /// disabled), exponentially eases over the theme duration (or `duration_ms`
    /// if > 0), first observation snaps (no appear-pop), and keeps requesting
    /// frames until settled.
    pub fn animate_channel(&self, channel: usize, target: f32, duration_ms: f32) -> f32 {
        let cfg = self.reduce_motion_animation_cfg();
        let mut tree = self.tree.borrow_mut();
        let node = tree.node_mut(self.node);
        if node.anim_channels.len() <= channel {
            node.anim_channels.resize(channel + 1, None);
        }
        if !cfg.enabled {
            node.anim_channels[channel] = Some(target);
            return target;
        }
        let dur = (if duration_ms > 0.0 { duration_ms } else { cfg.duration_ms }).max(1.0);
        let (val, settled) = match node.anim_channels[channel] {
            None => (target, true),
            Some(cur) => {
                let dt = rosace_animate::frame_dt();
                let alpha = 1.0 - (-dt * (1000.0 / dur)).exp();
                let next = cur + (target - cur) * alpha;
                let settled = (next - target).abs() < 0.001;
                (if settled { target } else { next }, settled)
            }
        };
        node.anim_channels[channel] = Some(val);
        drop(tree);
        if !settled { crate::tree::request_animation(); }
        val
    }

    /// Seed an [`animate_channel`](Self::animate_channel) channel to `value`
    /// ONLY if it has never been observed — the multi-channel sibling of
    /// [`seed_anim_if_unset`](Self::seed_anim_if_unset). Use it to opt a
    /// channel OUT of animate_channel's "first observation snaps" behaviour so
    /// it visibly eases FROM `value` on the first frame (e.g. a clock hand
    /// starting at 12:00 and sweeping to the current time).
    pub fn seed_channel_if_unset(&self, channel: usize, value: f32) {
        let mut tree = self.tree.borrow_mut();
        let node = tree.node_mut(self.node);
        if node.anim_channels.len() <= channel {
            node.anim_channels.resize(channel + 1, None);
        }
        if node.anim_channels[channel].is_none() {
            node.anim_channels[channel] = Some(value);
        }
    }

    /// Read an animation channel's current value (see [`animate_channel`](Self::animate_channel)),
    /// without advancing it — for widgets that need the settled position to
    /// compute a shortest-path target (e.g. a clock hand crossing 12).
    pub fn anim_channel(&self, channel: usize) -> Option<f32> {
        self.tree.borrow().node(self.node).anim_channels.get(channel).copied().flatten()
    }

    /// The latest pointer position (window-space logical px) — for a widget
    /// that follows the finger during a drag (e.g. a clock hand). Combine with
    /// [`Self::pressed`]: while pressed, draw at the raw pointer angle (smooth);
    /// on release, snap to the nearest value.
    pub fn pointer(&self) -> Point {
        let (x, y) = crate::tree::current_pointer();
        Point { x, y }
    }

    /// Snap an animation channel to `value` immediately (no easing) — for
    /// obeying a live drag: the hand tracks the finger exactly instead of
    /// lagging behind an ease.
    pub fn set_anim_channel(&self, channel: usize, value: f32) {
        let mut tree = self.tree.borrow_mut();
        let node = tree.node_mut(self.node);
        if node.anim_channels.len() <= channel { node.anim_channels.resize(channel + 1, None); }
        node.anim_channels[channel] = Some(value);
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
        LayoutCtx::with_tree(
            constraints,
            self.font,
            &self.theme,
            Rc::clone(&self.tree),
            self.node,
        )
    }

    /// Measure `child` during `paint`, caching on the node that
    /// [`Self::paint_child`] will paint it into.
    ///
    /// Widgets that position children by their measured size have to measure
    /// during paint (the rect is not known until then). Doing that through
    /// `ctx.layout_ctx(c)` measured the child against the PARENT's node, so
    /// nothing was cached per child and every frame re-measured.
    ///
    /// This peeks the slot `paint_child` will consume, so the measurement and
    /// the paint address the same node.
    pub fn measure_child(&self, constraints: Constraints, child: &dyn Widget) -> Size {
        let node = self.tree.borrow_mut().peek_slot(self.node);
        self.tree.borrow_mut().adopt_tag(node, child.type_tag());
        // Recorded on both paths (cache hit and miss) so `paint_child` can
        // check alignment regardless of whether any measuring actually ran.
        self.tree.borrow_mut().node_mut(node).layout_tag = child.type_tag();

        {
            let t = self.tree.borrow();
            let n = t.node(node);
            // `!is_structural_frame()` is not optional, and the symmetry with
            // `paint_child` is the reason. A rebuild hands this node a BRAND
            // NEW widget object; the node cannot tell a fresh-but-identical
            // widget from one whose content changed, so a matching
            // `Constraints` proves nothing about the size. Reusing the cache
            // here made a rebuilt `Text` keep its old width — output that
            // looks plausible and is wrong, which is the exact failure mode
            // these caches must never produce.
            if !is_structural_frame()
                && n.last_constraints == Some(constraints)
                && !n.needs_layout
            {
                if let Some(size) = n.cached_size {
                    return size;
                }
            }
        }

        self.tree.borrow_mut().begin_layout(node);
        let size = child.layout(&LayoutCtx::with_tree(
            constraints,
            self.font,
            &self.theme,
            Rc::clone(&self.tree),
            node,
        ));

        let mut t = self.tree.borrow_mut();
        let n = t.node_mut(node);
        n.last_constraints = Some(constraints);
        n.cached_size      = Some(size);
        n.needs_layout     = false;
        size
    }
}

// ── LayoutCtx ────────────────────────────────────────────────────────────────

/// Context passed to every widget's [`Widget::layout`] call.
///
/// Carries the available constraints plus font and theme access so that widgets
/// can measure text accurately without relying on character-count heuristics.
///
/// # Why it also carries the tree
///
/// It must, to cache a measurement per node — without a node there is nowhere
/// to put `cached_size`, and layout would re-measure the whole tree on every
/// non-idle frame (which it did, until this existed).
///
/// The safety property that the tree-less version provided is preserved by the
/// API surface instead: the ONLY thing `tree`/`node` are used for is
/// [`Self::layout_child`]. `hovered()`, `pressed()` and `animate_to()` remain
/// on [`PaintCtx`] alone, so a widget's `layout` still provably cannot observe
/// interaction state. That is what makes skipping layout for a hover sound.
/// Flutter and Compose draw the line the same way.
pub struct LayoutCtx<'a> {
    pub constraints: Constraints,
    pub font: &'a FontCache,
    pub theme: &'a ThemeData,
    /// `None` in unit tests built through [`Self::new`], where there is no
    /// engine and no arena. `layout_child` then measures uncached, so tests
    /// keep working unchanged and only production paths get caching.
    tree: Option<Rc<RefCell<RenderTree>>>,
    node: render_tree::NodeId,
}

impl<'a> LayoutCtx<'a> {
    /// Tree-less constructor, for unit tests and any caller with no arena.
    /// Measurements taken through the returned context are never cached.
    pub fn new(constraints: Constraints, font: &'a FontCache, theme: &'a ThemeData) -> Self {
        Self { constraints, font, theme, tree: None, node: RenderTree::ROOT }
    }

    /// The engine/widget-side constructor: caches measurements on `node`.
    pub fn with_tree(
        constraints: Constraints,
        font: &'a FontCache,
        theme: &'a ThemeData,
        tree: Rc<RefCell<RenderTree>>,
        node: render_tree::NodeId,
    ) -> Self {
        Self { constraints, font, theme, tree: Some(tree), node }
    }

    /// This widget's node in the render tree, when there is one.
    pub fn node_id(&self) -> render_tree::NodeId { self.node }

    /// Derive a context with different constraints for the SAME widget.
    ///
    /// Use this when a widget re-measures ITSELF under a different bound. To
    /// measure a CHILD, use [`Self::layout_child`] — that is what gives the
    /// child its own node and its own cached size.
    pub fn with_constraints(&self, constraints: Constraints) -> LayoutCtx<'_> {
        LayoutCtx {
            constraints,
            font: self.font,
            theme: self.theme,
            tree: self.tree.clone(),
            node: self.node,
        }
    }

    /// Measure `child` in its own node, reusing the cached size when nothing
    /// that could change it has changed.
    ///
    /// This is the layout mirror of [`PaintCtx::paint_child`], and the early
    /// return below is Flutter's:
    ///
    /// ```dart
    /// if (!_needsLayout && constraints == _constraints) return;
    /// ```
    ///
    /// Flutter pairs that with a list of relayout-boundary roots so the walk
    /// need not start at the top. We deliberately do not: reaching a boundary
    /// directly requires the node to reach its own layout logic, and
    /// `TreeNode` is a side-table that does not hold the widget. It is also
    /// unnecessary — with this cache the top-down walk returns immediately at
    /// every clean node, so the cost is the dirty spine plus one `Constraints`
    /// comparison per sibling.
    pub fn layout_child(&self, constraints: Constraints, child: &dyn Widget) -> Size {
        self.measure_into(None, constraints, child)
    }

    /// [`Self::layout_child`], addressing the child by an EXPLICIT index
    /// instead of call order.
    ///
    /// Required by any container whose `layout` makes more than one pass and
    /// does not touch every child on each — `Row`/`Column` measure non-flex
    /// children first, then flex ones, so call order is not slot order. See
    /// [`RenderTree::layout_slot_at`] for what goes wrong without it.
    pub fn layout_child_at(&self, index: usize, constraints: Constraints, child: &dyn Widget) -> Size {
        self.measure_into(Some(index), constraints, child)
    }

    fn measure_into(&self, index: Option<usize>, constraints: Constraints, child: &dyn Widget) -> Size {
        let Some(tree) = self.tree.as_ref() else {
            // No arena (unit test): measure directly, cache nothing.
            return child.layout(&LayoutCtx {
                constraints,
                font: self.font,
                theme: self.theme,
                tree: None,
                node: self.node,
            });
        };

        let node = match index {
            Some(i) => tree.borrow_mut().layout_slot_at(self.node, i),
            None    => tree.borrow_mut().layout_slot(self.node),
        };
        tree.borrow_mut().adopt_tag(node, child.type_tag());
        // Recorded on both paths (cache hit and miss) so `paint_child` can
        // check alignment regardless of whether any measuring actually ran.
        tree.borrow_mut().node_mut(node).layout_tag = child.type_tag();

        {
            let t = tree.borrow();
            let n = t.node(node);
            // `!is_structural_frame()` is not optional, and the symmetry with
            // `paint_child` is the reason. A rebuild hands this node a BRAND
            // NEW widget object; the node cannot tell a fresh-but-identical
            // widget from one whose content changed, so a matching
            // `Constraints` proves nothing about the size. Reusing the cache
            // here made a rebuilt `Text` keep its old width — output that
            // looks plausible and is wrong, which is the exact failure mode
            // these caches must never produce.
            if !is_structural_frame()
                && n.last_constraints == Some(constraints)
                && !n.needs_layout
            {
                if let Some(size) = n.cached_size {
                    return size;
                }
            }
        }

        tree.borrow_mut().begin_layout(node);
        let size = child.layout(&LayoutCtx {
            constraints,
            font: self.font,
            theme: self.theme,
            tree: Some(Rc::clone(tree)),
            node,
        });

        let mut t = tree.borrow_mut();
        let n = t.node_mut(node);
        n.last_constraints = Some(constraints);
        n.cached_size      = Some(size);
        n.needs_layout     = false;
        n.layout_tag       = child.type_tag();
        size
    }
}

// ── Widget trait ─────────────────────────────────────────────────────────────

/// The render layer trait. Every built-in widget implements this.
///
/// `Widget` is the render/paint concern — layout + draw. It is NOT what users
/// implement to compose UI; that's [`rosace_core::Component`].
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

    /// This widget's concrete type name, usable through `dyn Widget`.
    ///
    /// The render tree keys a node's persistent state and caches on the type
    /// occupying that slot, so it needs the concrete name even when all it
    /// holds is a trait object. A default method body is instantiated per
    /// `impl`, and the vtable entry points at that instantiation — so
    /// `type_name::<Self>()` here reports the real type, not `dyn Widget`.
    ///
    /// Do not override. It exists to be called, not implemented.
    fn type_tag(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

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
                ctx.paint_child(r, &*c);
            }
            Children::Many(cs) => {
                let r = ctx.rect;
                for c in cs {
                    ctx.paint_child(r, &*c);
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
    /// Forward, or every boxed child reports `Box<dyn Widget>` as its type
    /// and the render tree cannot tell any two of them apart — which is the
    /// entire point of the tag. Containers hold `Vec<BoxedWidget>`, so
    /// without this the forwarding impl would swallow the identity of the
    /// majority of widgets in any real tree.
    fn type_tag(&self) -> &'static str        { (**self).type_tag() }
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

/// `PaintCtx::draw_text_at`'s `origin.y` is the TOP of the text line, not its
/// baseline or vertical center (`layout_glyphs` adds the font's ascender
/// internally) — an eyeballed fraction of a box's height overflows the box
/// whenever the box-height/font-size ratio changes (bit `DatePicker`'s
/// header/day-cells and `TimePicker`'s value pills/arrows: text spilled past
/// the box bottom). Center properly using the font's own line height.
pub(crate) fn vcenter_text_y(box_top: f32, box_h: f32, font: &rosace_render::FontCache, px: f32) -> f32 {
    box_top + (box_h - font.line_height(px)) / 2.0
}

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
