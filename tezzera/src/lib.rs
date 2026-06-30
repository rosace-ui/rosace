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

use std::collections::HashSet;
use std::rc::Rc;
use std::cell::RefCell;

use tezzera_theme::built_in;
use tezzera_platform::PlatformWindow;
use tezzera_widgets::tree::{HitTarget, WidgetBox, clear_overlays, drain_overlays};

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
}

impl App {
    pub fn new() -> Self {
        Self {
            title: "Tezzera".into(),
            width: 800,
            height: 600,
            theme: built_in::dark_theme(),
        }
    }

    pub fn title(mut self, t: impl Into<String>) -> Self { self.title = t.into(); self }
    pub fn size(mut self, w: u32, h: u32) -> Self { self.width = w; self.height = h; self }
    pub fn dark(mut self) -> Self { self.theme = built_in::dark_theme(); self }
    pub fn light(mut self) -> Self { self.theme = built_in::light_theme(); self }
    pub fn theme(mut self, t: ThemeData) -> Self { self.theme = t; self }

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
        #[cfg(debug_assertions)]
        {
            use std::sync::Arc;
            use tezzera_trace::TRACING_BUS;
            use tezzera_trace::subscribers::console::ConsoleSubscriber;
            TRACING_BUS.add_subscriber(Arc::new(ConsoleSubscriber::new()));
        }

        let font = tezzera_render::FontCache::system_ui()
            .or_else(tezzera_render::FontCache::system_mono)
            .expect("no system font found");
        let theme = self.theme;
        let width = self.width;
        let height = self.height;

        // ── Reconciler state — persists across frames ──────────────────────
        // ComponentIds assigned by DFS position; stable IDs mean state persists.
        let mut prev_mounted: HashSet<u64> = HashSet::new();

        // Set theme once at startup — not per-frame. Writing the theme atom
        // every frame triggers subscriber notifications and causes a render loop.
        tezzera_theme::set_theme(theme.clone());

        PlatformWindow::new()
            .title(self.title)
            .size(width, height)
            .run(move |canvas, events| {
                // ── Build root ─────────────────────────────────────────────
                let mut ctx = tezzera_core::Context::new(tezzera_core::types::ComponentId(0));
                let element = root.build(&mut ctx);

                // ── Clear overlay registry from prior frame ────────────────
                clear_overlays();

                // ── Clear background (direct canvas write — not recorded) ──
                let bg = theme_color(&theme.colors.background);
                canvas.clear(bg);

                // ── Set up main display-list recording ─────────────────────
                let mut recorder = tezzera_render::PictureRecorder::new();
                let hit_targets: Rc<RefCell<Vec<HitTarget>>> =
                    Rc::new(RefCell::new(Vec::new()));

                // Layout in logical pixels so widget sizes and font sizes are
                // display-independent. play_picture scales to physical pixels.
                let win_w = canvas.logical_width() as f32;
                let win_h = canvas.logical_height() as f32;

                let mut paint_ctx = tezzera_widgets::tree::PaintCtx {
                    recorder: &mut recorder,
                    rect: tezzera_core::types::Rect {
                        origin: tezzera_core::types::Point { x: 0.0, y: 0.0 },
                        size: tezzera_core::types::Size { width: win_w, height: win_h },
                    },
                    font: &font,
                    theme: theme.clone(),
                    hit_targets: Rc::clone(&hit_targets),
                };

                let constraints = tezzera_layout::Constraints::tight(win_w, win_h);

                // ── Walk element tree — widgets record DrawCommands ────────
                let mut position: u64 = 0;
                let mut new_mounted: HashSet<u64> = HashSet::new();
                walk_element(
                    &element,
                    constraints,
                    &mut paint_ctx,
                    &mut position,
                    &mut new_mounted,
                );

                // ── Replay the main display list onto the canvas ───────────
                let picture = recorder.finish();
                canvas.play_picture(&picture, &font);

                // ── Overlay pass — second recorder, always on top ──────────
                let entries = drain_overlays();
                if !entries.is_empty() {
                    use tezzera_core::types::{Point, Rect, Size};
                    use tezzera_widgets::tree::LayerPosition;
                    let mut ov_recorder = tezzera_render::PictureRecorder::new();
                    let ov_hit_targets: Rc<RefCell<Vec<HitTarget>>> =
                        Rc::new(RefCell::new(Vec::new()));

                    for entry in entries {
                        // Draw scrim first if configured
                        if let Some(scrim) = &entry.scrim {
                            let scrim_rect = Rect {
                                origin: Point { x: 0.0, y: 0.0 },
                                size: Size { width: win_w, height: win_h },
                            };
                            ov_recorder.push(tezzera_render::DrawCommand::FillRect {
                                rect: scrim_rect,
                                color: scrim.color,
                            });
                        }

                        // Determine overlay widget rect from LayerPosition
                        let loose_c = tezzera_layout::Constraints::loose(win_w, win_h);
                        let lctx = tezzera_widgets::tree::LayoutCtx::new(
                            loose_c, &font, &theme,
                        );
                        let widget_size = entry.widget.layout(&lctx);

                        let origin = match &entry.position {
                            LayerPosition::Absolute(p) => *p,
                            LayerPosition::Centered => Point {
                                x: ((win_w - widget_size.width) / 2.0).max(0.0),
                                y: ((win_h - widget_size.height) / 2.0).max(0.0),
                            },
                            LayerPosition::BottomAnchored => Point {
                                x: 0.0,
                                y: (win_h - widget_size.height).max(0.0),
                            },
                            LayerPosition::Fill => Point { x: 0.0, y: 0.0 },
                        };
                        let widget_rect = Rect { origin, size: widget_size };

                        let mut ov_ctx = tezzera_widgets::tree::PaintCtx {
                            recorder: &mut ov_recorder,
                            rect: widget_rect,
                            font: &font,
                            theme: theme.clone(),
                            hit_targets: Rc::clone(&ov_hit_targets),
                        };
                        entry.widget.paint(&mut ov_ctx);
                    }

                    let ov_picture = ov_recorder.finish();
                    canvas.play_picture(&ov_picture, &font);

                    // Merge overlay hit targets into the main list
                    // (overlay targets are checked first during event routing)
                    let ov_targets = ov_hit_targets.borrow();
                    let mut main_targets = hit_targets.borrow_mut();
                    for t in ov_targets.iter() {
                        main_targets.insert(0, tezzera_widgets::tree::HitTarget {
                            rect: t.rect,
                            callback: t.callback.clone(),
                        });
                    }
                }

                // ── Reconcile: fire lifecycle for mounted/unmounted components
                for &id in new_mounted.difference(&prev_mounted) {
                    let cid = tezzera_core::types::ComponentId(id);
                    root.on_mount();
                    #[cfg(debug_assertions)]
                    {
                        use tezzera_trace::{event::TezzeraTrace, location, trace};
                        trace!(TezzeraTrace::ComponentMount {
                            id: cid,
                            name: root.type_name(),
                            location: location!(),
                        });
                    }
                    let _ = cid;
                }
                for &id in prev_mounted.difference(&new_mounted) {
                    let cid = tezzera_core::types::ComponentId(id);
                    tezzera_state::cleanup_store::fire_and_clear(cid);
                    tezzera_state::clear_component(cid);
                    root.on_unmount();
                    #[cfg(debug_assertions)]
                    {
                        use tezzera_trace::{event::TezzeraTrace, trace};
                        trace!(TezzeraTrace::ComponentUnmount {
                            id: cid,
                            name: root.type_name(),
                        });
                    }
                }
                prev_mounted = new_mounted;

                // ── Route click events to hit targets ──────────────────────
                let targets = hit_targets.borrow();
                for event in events {
                    if let tezzera_platform::InputEvent::MouseDown {
                        x, y, button: tezzera_platform::MouseButton::Left
                    } = event {
                        for t in targets.iter() {
                            let r = &t.rect;
                            if x >= &r.origin.x
                                && x <= &(r.origin.x + r.size.width)
                                && y >= &r.origin.y
                                && y <= &(r.origin.y + r.size.height)
                            {
                                (t.callback)();
                                break; // one click fires exactly one target
                            }
                        }
                    }
                }
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
/// `position` is a DFS counter — each `Component` node consumes one slot so
/// the same component always gets the same ID as long as the tree shape is
/// stable across frames.
fn walk_element(
    element: &tezzera_core::Element,
    constraints: tezzera_layout::Constraints,
    ctx: &mut tezzera_widgets::tree::PaintCtx,
    position: &mut u64,
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

            let mut child_ctx = tezzera_core::Context::new(id);

            // Catch panics from Component::build so ErrorBoundary can show fallback.
            let build_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                c.component.build(&mut child_ctx)
            }));

            match build_result {
                Ok(child_element) => {
                    walk_element(&child_element, constraints, ctx, position, new_mounted)
                }
                Err(_) => {
                    #[cfg(debug_assertions)]
                    {
                        use tezzera_trace::{event::TezzeraTrace, trace};
                        trace!(TezzeraTrace::ComponentUnmount {
                            id,
                            name: "ErrorBoundary::fallback",
                        });
                    }
                    let fallback = tezzera_core::Element::text("⚠ component error");
                    walk_element(&fallback, constraints, ctx, position, new_mounted)
                }
            }
        }

        Element::Native(n) => {
            if let Some(wb) = n.payload.as_ref()
                .and_then(|p| p.as_any().downcast_ref::<WidgetBox>())
            {
                let lctx = ctx.layout_ctx(constraints);
                let size = wb.0.layout(&lctx);
                let child_rect = Rect { origin: ctx.rect.origin, size };
                let mut child_ctx = ctx.child(child_rect);
                wb.0.paint(&mut child_ctx);
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

// ── Helpers ───────────────────────────────────────────────────────────────────

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
pub use tezzera_widgets::{FocusApi, OverlayApi, OverlayKind};

// Widgets
pub use tezzera_widgets::{
    Widget, WidgetApp, PaintCtx, BoxedWidget,
    AppBar, Avatar, Badge,
    Button, ButtonVariant,
    Card, Center, Checkbox, Chip,
    ColoredBox, Column, Container, Divider,
    EdgeInsets, Expanded, Icon, IconKind,
    Image, ListTile, ListView,
    NavItem, NavRail,
    Padding, ProgressBar,
    RectReader,
    OverlayEntry, LayerId, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
    push_overlay,
    Row, Scaffold, ScrollView,
    SizedBox, Slider, Spacer, Stack, Switch,
    Tab, TabBar, Text, TextInput, Tooltip,
};

// Text styling
pub use tezzera_widgets::{TextAlign, FontWeight};

// Theme
pub use tezzera_theme::{ThemeData, ColorScheme};
pub use tezzera_theme::built_in::{dark_theme, light_theme};

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
    pub use tezzera_render::canvas::Color;
    pub use tezzera_theme::{ThemeData, ColorScheme};
    pub use tezzera_theme::built_in::{dark_theme, light_theme};
    pub use tezzera_core::types::{Point, Rect, Size};
    pub use tezzera_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
    pub use tezzera_state::Atom;
}
