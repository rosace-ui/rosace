//! # TEZZERA SDK
//!
//! One import. One concept. Build UI by composing [`Component`]s.
//!
//! ```rust,ignore
//! use tezzera::prelude::*;
//!
//! struct Counter;
//!
//! impl Component for Counter {
//!     fn build(&self, ctx: &mut Context) -> Element {
//!         let count = ctx.state(0i32);
//!         Column::new()
//!             .child(Text::display(&count.get().to_string()))
//!             .child(Button::new("Increment")
//!                 .on_press(move || count.set(count.get() + 1)))
//!             .into_element()
//!     }
//! }
//!
//! fn main() {
//!     App::run(Counter);
//! }
//! ```

use std::rc::Rc;
use std::sync::Arc;

use tezzera_theme::built_in;
use tezzera_platform::PlatformWindow;
use tezzera_widgets::tree::WidgetBox;

mod engine;
pub use engine::FrameEngine;

// ── App ───────────────────────────────────────────────────────────────────────

/// The TEZZERA application runner.
///
/// Pass a [`Component`] to [`App::run`]. The framework owns everything else:
/// window, event loop, font, theme, layout, painting, hit-testing.
///
/// ```rust,ignore
/// App::run(MyApp);
/// ```
pub struct App {
    title: String,
    width: u32,
    height: u32,
    theme: ThemeData,
    /// Platform-keyed theme bundle (D105 Phase 23 Step 2). When set, this
    /// takes priority over `theme` — the active theme is resolved from it
    /// once at startup, keyed by the running platform.
    themes: Option<tezzera_theme::Themes>,
    /// Forces the running platform for theme resolution (preview) instead of
    /// the real detected one (D105 Phase 23 Step 1).
    platform_override: Option<tezzera_core::Platform>,
}

impl App {
    pub fn new() -> Self {
        Self {
            title: "Tezzera".into(),
            width: 800,
            height: 600,
            theme: built_in::dark_theme(),
            themes: None,
            platform_override: None,
        }
    }

    pub fn title(mut self, t: impl Into<String>) -> Self { self.title = t.into(); self }
    pub fn size(mut self, w: u32, h: u32) -> Self { self.width = w; self.height = h; self }
    pub fn dark(mut self) -> Self { self.theme = built_in::dark_theme(); self }
    pub fn light(mut self) -> Self { self.theme = built_in::light_theme(); self }
    pub fn theme(mut self, t: ThemeData) -> Self { self.theme = t; self }

    /// Supplies a platform-keyed set of themes (D105). When set, this is
    /// resolved once at startup against the running platform and takes
    /// priority over `.theme(..)`/`.dark()`/`.light()`. Apps that don't call
    /// this are unaffected — a single `.theme(..)` keeps working exactly as
    /// before.
    pub fn themes(mut self, themes: tezzera_theme::Themes) -> Self {
        self.themes = Some(themes);
        self
    }

    /// Forces the platform used for theme resolution, regardless of the
    /// real detected one — e.g. `.platform(Platform::Ios)` to preview an iOS
    /// theme on desktop. Only affects which entry of `.themes(..)` gets
    /// picked; has no effect without a `Themes` bundle.
    pub fn platform(mut self, p: tezzera_core::Platform) -> Self {
        self.platform_override = Some(p);
        self
    }

    /// Run the app with a root [`Component`]. This is the only call needed in `main`.
    ///
    /// The framework calls `component.build(ctx)` every frame, walks the
    /// returned [`Element`] tree, lays out + paints every widget, and routes
    /// click events to the correct `on_press` callbacks.
    pub fn run<C: tezzera_core::Component>(root: C) {
        App::new().launch(root);
    }

    /// Builder variant — use when you need to configure title/size/theme first.
    pub fn launch<C: tezzera_core::Component>(self, root: C) {
        // ── Wire ConsoleSubscriber so trace events appear in the terminal ──
        //
        // Opt-in via TEZZERA_TRACE=all|state|network|perf. Printing every
        // trace event to stderr costs more than the entire render pass —
        // AtomRead fires on every atom.get() during paint — so the default
        // is no console subscriber at all.
        #[cfg(debug_assertions)]
        if let Ok(filter) = std::env::var("TEZZERA_TRACE") {
            use std::sync::Arc;
            use tezzera_trace::TRACING_BUS;
            use tezzera_trace::subscribers::console::{ConsoleFilter, ConsoleSubscriber};
            let filter = match filter.as_str() {
                "state"   => ConsoleFilter::State,
                "network" => ConsoleFilter::Network,
                "perf"    => ConsoleFilter::Performance,
                _         => ConsoleFilter::All,
            };
            TRACING_BUS.add_subscriber(Arc::new(ConsoleSubscriber::with_filter(filter)));
        }

        // Prefer a system UI/mono font; fall back to the embedded DejaVu Sans
        // when none is found (always the case on web/wasm) so text always
        // renders on every platform.
        let font = tezzera_render::FontCache::system_ui()
            .or_else(tezzera_render::FontCache::system_mono)
            .unwrap_or_else(tezzera_render::FontCache::embedded);
        // Platform resolution (D105 Phase 23 Step 1): forced override, else
        // the real detected platform. Themes::resolve (Step 2) reads this to
        // pick the active theme; widgets never see the platform directly.
        if let Some(p) = self.platform_override {
            tezzera_core::set_platform(p);
        }
        let theme = match &self.themes {
            Some(themes) => themes.resolve(tezzera_core::use_platform()),
            None => self.theme,
        };
        let width = self.width;
        let height = self.height;

        // Set theme once at startup — not per-frame. Writing the theme atom
        // every frame triggers subscriber notifications and causes a render loop.
        tezzera_theme::set_theme(theme.clone());

        // The per-frame build/paint/input logic lives in `FrameEngine`
        // (Phase 24 Step 1, D106) so it's drivable from more than just this
        // winit-backed loop — a future native-host FFI boundary reuses it
        // without duplicating this code.
        let mut engine = FrameEngine::new(Box::new(root), font);

        PlatformWindow::new()
            .title(self.title)
            .size(width, height)
            .run_layered(move |canvas, overlay_canvas, events| {
                let content_changed = engine.paint(canvas, overlay_canvas, events);
                // D107 Phase 25 Step 4 — web-only, and only when this
                // frame's build may have changed something (the module's
                // own string diff catches the rest, e.g. state that
                // changed and changed back).
                #[cfg(target_arch = "wasm32")]
                if content_changed {
                    tezzera_platform::web_seo_sync::sync(&engine.semantics());
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = content_changed;
            });
    }
}

impl Default for App {
    fn default() -> Self { Self::new() }
}

// ── Element walker ────────────────────────────────────────────────────────────

/// Walk the element tree, assigning stable position-based [`ComponentId`]s,
/// collecting mounted component IDs for the reconciler, and painting widgets.
///
/// `position` — DFS counter for Component nodes (determines ComponentId).
/// `damage` — union of world rects whose pixels change this frame.
/// `dirty_ids` — component IDs whose atoms changed this frame.
/// `global_dirty` — when true, skip cache and rebuild everything.
/// `subtree_dirty` — an ancestor component rebuilt this frame; force re-paint.
/// `element_cache` — cached build() output per ComponentId.
#[allow(clippy::too_many_arguments)]
fn walk_element(
    element: &tezzera_core::Element,
    constraints: tezzera_layout::Constraints,
    ctx: &mut tezzera_widgets::tree::PaintCtx,
    position: &mut u64,
    damage: &mut Option<Rect>,
    dirty_ids: &std::collections::HashSet<tezzera_core::types::ComponentId>,
    global_dirty: bool,
    subtree_dirty: bool,
    element_cache: &mut std::collections::HashMap<u64, tezzera_core::Element>,
    new_mounted: &mut std::collections::HashSet<u64>,
) -> tezzera_core::types::Size {
    use tezzera_core::Element;
    use tezzera_core::types::{ComponentId, Rect, Size};

    match element {
        Element::Component(c) => {
            // Assign a stable position-based ID (D001).
            let id = ComponentId(*position);
            *position += 1;
            new_mounted.insert(id.0);

            let is_dirty = global_dirty || subtree_dirty || dirty_ids.contains(&id);
            let prev_owner = ctx.owner;
            ctx.owner = id;

            let (child_element, child_subtree_dirty) = if is_dirty {
                // Build fresh and update cache.
                let mut child_ctx = tezzera_core::Context::new(id);
                let build_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    c.component.build(&mut child_ctx)
                }));
                let elem = match build_result {
                    Ok(e) => e,
                    Err(_) => {
                        #[cfg(debug_assertions)]
                        {
                            use tezzera_trace::{event::TezzeraTrace, trace};
                            trace!(TezzeraTrace::ComponentUnmount {
                                id,
                                name: "ErrorBoundary::fallback",
                            });
                        }
                        tezzera_core::Element::text("⚠ component error")
                    }
                };
                element_cache.insert(id.0, elem.clone());
                (elem, true)
            } else if let Some(cached) = element_cache.get(&id.0) {
                // Not dirty — reuse last frame's element tree, no subtree repaint.
                (cached.clone(), false)
            } else {
                // No cache yet (first frame or tree shape change).
                let mut child_ctx = tezzera_core::Context::new(id);
                let elem = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    c.component.build(&mut child_ctx)
                })).unwrap_or_else(|_| tezzera_core::Element::text("⚠ component error"));
                element_cache.insert(id.0, elem.clone());
                (elem, true)
            };

            let size = walk_element(
                &child_element,
                constraints,
                ctx,
                position,
                damage,
                dirty_ids,
                global_dirty,
                child_subtree_dirty,
                element_cache,
                new_mounted,
            );
            ctx.owner = prev_owner;
            size
        }

        Element::Native(n) => {
            if let Some(wb) = n.payload.as_ref()
                .and_then(|p| p.as_any().downcast_ref::<WidgetBox>())
            {
                // Consume this position's slot WITHOUT reset — the cache
                // state on the node decides whether we repaint (Phase 20:
                // the arena IS the render tree; the flat list is gone).
                let node_id = ctx.tree.borrow_mut().slot(ctx.node, false);

                {
                    let mut tree = ctx.tree.borrow_mut();
                    let node = tree.node_mut(node_id);
                    if node.tag != n.tag {
                        // Type mismatch — hard cache reset.
                        node.tag = n.tag;
                        node.last_constraints = None;
                        node.cached_size = None;
                        node.cached_picture = None;
                        node.cached_rect = None;
                        node.paint_dirty = true;
                    }
                    if subtree_dirty {
                        node.paint_dirty = true;
                    }
                }

                // ── Layout (skip if constraints unchanged and not dirty) ──
                let cached = {
                    let tree = ctx.tree.borrow();
                    let node = tree.node(node_id);
                    if node.last_constraints == Some(constraints)
                        && !node.paint_dirty
                        && node.cached_size.is_some()
                    {
                        node.cached_size
                    } else {
                        None
                    }
                };
                let size = match cached {
                    Some(s) => s,
                    None => {
                        let lctx = ctx.layout_ctx(constraints);
                        let s = wb.0.layout(&lctx);
                        let mut tree = ctx.tree.borrow_mut();
                        let node = tree.node_mut(node_id);
                        node.last_constraints = Some(constraints);
                        node.cached_size = Some(s);
                        node.paint_dirty = true;
                        s
                    }
                };

                let child_rect = Rect { origin: ctx.rect.origin, size };

                // ── Paint (replay cache or fresh, tracking damage) ─────────
                let (replay, old_rect) = {
                    let tree = ctx.tree.borrow();
                    let node = tree.node(node_id);
                    (
                        !node.paint_dirty
                            && node.cached_picture.is_some()
                            && node.cached_rect == Some(child_rect),
                        node.cached_rect,
                    )
                };

                if replay {
                    // Zero widget work; slot untouched so the subtree's
                    // declared regions persist (D091).
                    let pic = ctx.tree.borrow().node(node_id).cached_picture.clone().unwrap();
                    for cmd in &pic.commands {
                        ctx.recorder.push(cmd.clone());
                    }
                } else {
                    // Damage = where it was ∪ where it is.
                    *damage = union_rect(*damage, old_rect);
                    *damage = union_rect(*damage, Some(child_rect));

                    // Reset declarations; the widget re-declares during paint.
                    ctx.tree.borrow_mut().reset(node_id);
                    let mut sub_recorder = tezzera_render::PictureRecorder::new();
                    {
                        let mut child_ctx = tezzera_widgets::tree::PaintCtx {
                            recorder: &mut sub_recorder,
                            rect: child_rect,
                            font: ctx.font,
                            theme: ctx.theme.clone(),
                            tree: Rc::clone(&ctx.tree),
                            node: node_id,
                            owner: ctx.owner,
                            clip_rect: ctx.clip_rect,
                        };
                        wb.0.paint(&mut child_ctx);
                    }
                    let picture = sub_recorder.finish();
                    for cmd in &picture.commands {
                        ctx.recorder.push(cmd.clone());
                    }
                    let mut tree = ctx.tree.borrow_mut();
                    let node = tree.node_mut(node_id);
                    node.cached_picture = Some(Arc::new(picture));
                    node.cached_rect    = Some(child_rect);
                    node.paint_dirty    = false;
                }

                size
            } else {
                Size { width: 0.0, height: 0.0 }
            }
        }

        Element::Text(t) => {
            let line_h = ctx.font.line_height(16.0);
            let color = ctx.tc(ctx.theme.colors.on_surface);
            ctx.text(&t.content, 0.0, 0.0, color, 16.0);
            Size { width: constraints.max_width_f32(), height: line_h }
        }

        Element::Empty => Size { width: 0.0, height: 0.0 },
    }
}

// ── Navigation sugar (D097) ──────────────────────────────────────────────────

/// One-call back button: replaces the manual
/// `if nav.can_pop() { bar.leading(Button::new("← Back").on_press(pop)) }`
/// block every app was writing. Lives in the facade because it needs both
/// `AppBar` (widgets) and `ScreenNav` (nav).
pub trait AppBarNavExt {
    /// Add a `← Back` leading button that pops `nav` — only when there is
    /// somewhere to pop to.
    fn back_button<R: Clone + Send + Sync + 'static>(self, nav: &tezzera_nav::ScreenNav<R>) -> Self;
}

impl AppBarNavExt for AppBar {
    fn back_button<R: Clone + Send + Sync + 'static>(self, nav: &tezzera_nav::ScreenNav<R>) -> Self {
        if !nav.can_pop() {
            return self;
        }
        let nav = nav.clone();
        self.leading(
            Button::new("← Back")
                .variant(ButtonVariant::Ghost)
                .on_press(move || { nav.pop(); }),
        )
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Flattened dispatch data for one overlay entry (D092). Built by the overlay
/// pass each frame from the entry's per-entry render tree.
struct OverlayRoute {
    rect: tezzera_core::types::Rect,
    input: tezzera_widgets::tree::InputBehavior,
    on_tap: Option<Arc<dyn Fn() + Send + Sync>>,
    hits: Vec<(tezzera_core::types::Rect, Arc<dyn Fn() + Send + Sync>)>,
    scrolls: Vec<(tezzera_core::types::Rect, tezzera_widgets::tree::ScrollAxes, Arc<dyn Fn(f32, f32) + Send + Sync>)>,
}

/// Grow a rect by `m` logical pixels on every side.
fn inflate_rect(r: tezzera_core::types::Rect, m: f32) -> tezzera_core::types::Rect {
    use tezzera_core::types::{Point, Rect, Size};
    Rect {
        origin: Point { x: r.origin.x - m, y: r.origin.y - m },
        size: Size { width: r.size.width + 2.0 * m, height: r.size.height + 2.0 * m },
    }
}

/// Union of two optional rects (damage accumulation).
fn union_rect(a: Option<tezzera_core::types::Rect>, b: Option<tezzera_core::types::Rect>) -> Option<tezzera_core::types::Rect> {
    use tezzera_core::types::{Point, Rect, Size};
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => {
            let x0 = a.origin.x.min(b.origin.x);
            let y0 = a.origin.y.min(b.origin.y);
            let x1 = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
            let y1 = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
            Some(Rect { origin: Point { x: x0, y: y0 }, size: Size { width: x1 - x0, height: y1 - y0 } })
        }
    }
}

#[inline]
fn rect_contains(r: &tezzera_core::types::Rect, x: f32, y: f32) -> bool {
    x >= r.origin.x
        && x <= r.origin.x + r.size.width
        && y >= r.origin.y
        && y <= r.origin.y + r.size.height
}

fn theme_color(c: &tezzera_theme::Color) -> Color {
    Color::rgba(
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
        (c.a * 255.0) as u8,
    )
}

// ── Re-exports ────────────────────────────────────────────────────────────────

// Core
pub use tezzera_core::{Component, Context, Element};
pub use tezzera_render::canvas::Color;

// Accessibility + focus
pub use tezzera_a11y::FocusNode;
pub use tezzera_widgets::{AbsorbPointer, FocusApi, IgnorePointer, OverlayApi, OverlayKind, PressApi, Pressable};

// Widgets
pub use tezzera_widgets::{
    Alignment, Children, Semantics, Widget, WidgetApp, PaintCtx, BoxedWidget,
    AppBar, Avatar, Badge,
    Button, ButtonVariant,
    Card, Checkbox, Chip,
    AspectRatio, BoxShape, CircularProgress, Column, Container, CustomPaint, Dialog, Divider, Grid, Positioned, Skeleton, Wrap,
    Dropdown, Drawer, Expander, Radio, SegmentedControl,
    EdgeInsets, Expanded, Icon, IconKind,
    Image, ListTile, ListView,
    Menu, NavItem, NavRail,
    ProgressBar,
    RectReader,
    RepaintBoundary,
    TransformLayer,
    OverlayEntry, LayerId, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
    push_overlay,
    Row, Scaffold, ScrollView, ScrollAxis, Sheet,
    Slider, Spacer, Stack, Switch,
    Tab, TabBar, Text, TextInput, Toast, ToastKind, Tooltip,
};

// Text styling
pub use tezzera_widgets::{TextAlign, FontWeight};

// Theme
pub use tezzera_theme::{ThemeData, ColorScheme, Themes, AppBarStyle, TitleAlign};
pub use tezzera_theme::built_in::{dark_theme, light_theme, material, cupertino};

// Platform (D105)
pub use tezzera_core::Platform;

// Geometry
pub use tezzera_core::types::{Point, Rect, Size};

// Layout
pub use tezzera_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};

// Render utilities (advanced / golden tests)
pub use tezzera_render::{FontCache, SkiaCanvas};

// Namespaced sub-system access
pub mod widgets   { pub use tezzera_widgets::*; }
pub mod theme     { pub use tezzera_theme::*; }
pub mod layout    { pub use tezzera_layout::*; }
pub mod render    { pub use tezzera_render::*; }
pub mod core      { pub use tezzera_core::*; }
pub mod state     { pub use tezzera_state::*; }
pub mod animate   { pub use tezzera_animate::*; }
pub mod anim      { pub use tezzera_anim::*; }
pub mod scroll    { pub use tezzera_scroll::*; }
pub mod nav       { pub use tezzera_nav::*; }
pub mod nav_anim  { pub use tezzera_nav_anim::*; }
pub mod forms     { pub use tezzera_forms::*; }
pub mod gesture   { pub use tezzera_gesture::*; }
pub mod a11y      { pub use tezzera_a11y::*; }
pub mod text      { pub use tezzera_text::*; }
pub mod shaping   { pub use tezzera_shaping::*; }
pub mod style     { pub use tezzera_style::*; }
pub mod i18n      { pub use tezzera_i18n::*; }
pub mod net       { pub use tezzera_net::*; }
pub mod clipboard { pub use tezzera_clipboard::*; }
pub mod platform  { pub use tezzera_platform::*; }
pub mod media     { pub use tezzera_media::*; }
pub mod ime       { pub use tezzera_ime::*; }
pub mod bidi      { pub use tezzera_bidi::*; }
pub mod ws        { pub use tezzera_ws::*; }
pub mod hot_reload { pub use tezzera_hot_reload::*; }
pub mod devtools  { pub use tezzera_devtools::*; }
pub mod test_utils { pub use tezzera_test_utils::*; }

// ── Prelude ───────────────────────────────────────────────────────────────────

pub mod prelude {
    pub use crate::App;
    pub use tezzera_core::{Component, Context, Element};
    pub use tezzera_platform::{InputEvent, MouseButton, Key};
    pub use tezzera_widgets::prelude::*;
    pub use tezzera_widgets::{
        OverlayEntry, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
        push_overlay, OverlayApi, OverlayKind,
    };
    pub use tezzera_a11y::FocusNode;
    pub use tezzera_widgets::FocusApi;
    pub use tezzera_widgets::RepaintBoundary;
    pub use tezzera_widgets::TransformLayer;
    pub use tezzera_nav::ScreenNav;
    pub use crate::AppBarNavExt;
    pub use tezzera_render::canvas::Color;
    pub use tezzera_theme::{ThemeData, ColorScheme, Themes, AppBarStyle, TitleAlign};
    pub use tezzera_theme::built_in::{dark_theme, light_theme, material, cupertino};
    pub use tezzera_core::Platform;
    pub use tezzera_core::types::{Point, Rect, Size};
    pub use tezzera_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
    pub use tezzera_state::Atom;
    pub use tezzera_scroll::ScrollController;
}
