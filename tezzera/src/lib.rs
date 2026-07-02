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

mod render_node;
mod reconcile;

use std::collections::HashSet;
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

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

        let font = tezzera_render::FontCache::system_ui()
            .or_else(tezzera_render::FontCache::system_mono)
            .expect("no system font found");
        let theme = self.theme;
        let width = self.width;
        let height = self.height;

        // ── Reconciler state — persists across frames ──────────────────────
        // ComponentIds assigned by DFS position; stable IDs mean state persists.
        let mut prev_mounted: HashSet<u64> = HashSet::new();

        // ── Phase 14: focus manager ────────────────────────────────────────
        let mut focus_manager = tezzera_a11y::FocusManager::new();
        let mut shift_held = false;

        // ── Phase 13: persistent render cache ─────────────────────────────
        // Cached build output per component ID — skips build() when the
        // component's atoms haven't changed.
        let mut element_cache: std::collections::HashMap<u64, tezzera_core::Element> =
            std::collections::HashMap::new();
        // Persistent RenderNode list (DFS order of native elements). Caches
        // last layout size and painted Picture so unchanged widgets skip
        // re-layout and re-paint.
        let mut render_nodes: Vec<render_node::RenderNode> = Vec::new();
        // Cached TransformLayer entries from the last fresh paint. Replayed on
        // every frame (including render-cache frames where TransformLayer::paint
        // is not called) so the scroll list remains visible when nothing is dirty.
        let mut cached_transform_entries: Vec<tezzera_widgets::tree::TransformLayerEntry> =
            Vec::new();
        // Cached overlay entries — same problem as transform entries (D088):
        // WithOverlay::paint only runs on dirty frames, so on cache-hit frames
        // drain_overlays() is empty even though an overlay's open atom is still
        // true. Without this cache the dialog vanishes (and loses its input
        // blocking) on the very next clean frame — e.g. the MouseUp that
        // follows every click — letting taps reach the buttons underneath.
        let mut cached_overlay_entries: Vec<tezzera_widgets::tree::OverlayEntry> =
            Vec::new();
        // First frame — all components are dirty.
        tezzera_state::reset_to_global_dirty();

        // Set theme once at startup — not per-frame. Writing the theme atom
        // every frame triggers subscriber notifications and causes a render loop.
        tezzera_theme::set_theme(theme.clone());

        PlatformWindow::new()
            .title(self.title)
            .size(width, height)
            .run_layered(move |canvas, overlay_canvas, events| {
                // ── Drain dirty-component set for this frame ───────────────
                let global_dirty = tezzera_state::is_global_dirty();
                let dirty_ids = tezzera_state::take_dirty_components();

                // ── Build root (only when dirty) ───────────────────────────
                //
                // The root component (ComponentId(0)) owns all atoms created via
                // ctx.state(). When any of those atoms change, ComponentId(0) lands
                // in dirty_ids. We rebuild ONLY then; on clean frames the cached
                // element is reused, keeping `build()` side-effects out of the
                // render loop (e.g. an atom.set() inside build() would otherwise
                // cause an infinite loop).
                let root_component_id = tezzera_core::types::ComponentId(0);
                let root_is_dirty = global_dirty || dirty_ids.contains(&root_component_id);

                let element = if root_is_dirty || !element_cache.contains_key(&0) {
                    let mut ctx = tezzera_core::Context::new(root_component_id);
                    let el = root.build(&mut ctx);
                    element_cache.insert(0, el.clone());
                    el
                } else {
                    element_cache.get(&0).unwrap().clone()
                };

                // ── Clear overlay registry from prior frame ────────────────
                clear_overlays();

                // ── Read active theme each frame so set_theme() takes effect ──
                // Widgets call set_theme() from button callbacks; the change is
                // picked up here on the very next frame.
                let current_theme = tezzera_theme::use_theme();

                // ── Clear background (direct canvas write — not recorded) ──
                let bg = theme_color(&current_theme.colors.background);
                canvas.clear(bg);

                // ── Set up main display-list recording ─────────────────────
                let mut recorder = tezzera_render::PictureRecorder::new();
                let hit_targets: Rc<RefCell<Vec<HitTarget>>> =
                    Rc::new(RefCell::new(Vec::new()));
                let focus_nodes: Rc<RefCell<Vec<tezzera_a11y::FocusNode>>> =
                    Rc::new(RefCell::new(Vec::new()));

                // Layout in logical pixels so widget sizes and font sizes are
                // display-independent. play_picture scales to physical pixels.
                let win_w = canvas.logical_width() as f32;
                let win_h = canvas.logical_height() as f32;

                let transform_entries: Rc<RefCell<Vec<tezzera_widgets::tree::TransformLayerEntry>>> =
                    Rc::new(RefCell::new(Vec::new()));
                let scroll_targets: Rc<RefCell<Vec<tezzera_widgets::tree::ScrollTarget>>> =
                    Rc::new(RefCell::new(Vec::new()));
                let mut paint_ctx = tezzera_widgets::tree::PaintCtx {
                    recorder: &mut recorder,
                    rect: tezzera_core::types::Rect {
                        origin: tezzera_core::types::Point { x: 0.0, y: 0.0 },
                        size: tezzera_core::types::Size { width: win_w, height: win_h },
                    },
                    font: &font,
                    theme: current_theme.clone(),
                    hit_targets: Rc::clone(&hit_targets),
                    scroll_targets: Rc::clone(&scroll_targets),
                    focus_nodes: Rc::clone(&focus_nodes),
                    transform_entries: Rc::clone(&transform_entries),
                    clip_rect: None,
                };

                let constraints = tezzera_layout::Constraints::tight(win_w, win_h);

                // ── Walk element tree — widgets record DrawCommands ────────
                let mut position: u64 = 0;
                let mut native_idx: usize = 0;
                let mut new_mounted: HashSet<u64> = HashSet::new();
                walk_element(
                    &element,
                    constraints,
                    &mut paint_ctx,
                    &mut position,
                    &mut native_idx,
                    &mut render_nodes,
                    &dirty_ids,
                    global_dirty,
                    root_is_dirty,  // subtree_dirty: dirty if root component's atoms changed
                    &mut element_cache,
                    &mut new_mounted,
                );

                // ── Replay the main display list onto the canvas ───────────
                let picture = recorder.finish();
                canvas.play_picture(&picture, &font);

                // ── Overlay pass — second recorder into overlay_canvas (D076) ─
                // The platform already cleared overlay_canvas to transparent
                // before calling this closure (D078). We record overlay widgets
                // into a separate PictureRecorder and play into overlay_canvas,
                // which the platform uploads as a second GPU texture layer.
                let fresh_overlays = drain_overlays();
                // A rebuild happened → fresh_overlays is authoritative (it is
                // empty when every overlay closed). On clean frames keep the
                // cache so open overlays survive cache-hit repaints.
                let frame_rebuilt = global_dirty || !dirty_ids.is_empty();
                if !fresh_overlays.is_empty() {
                    cached_overlay_entries = fresh_overlays;
                } else if frame_rebuilt {
                    cached_overlay_entries.clear();
                }

                let ov_hit_targets: Rc<RefCell<Vec<HitTarget>>> =
                    Rc::new(RefCell::new(Vec::new()));

                if !cached_overlay_entries.is_empty() {
                    use tezzera_core::types::{Point, Rect, Size};
                    use tezzera_widgets::tree::LayerPosition;
                    let mut ov_recorder = tezzera_render::PictureRecorder::new();

                    for entry in &cached_overlay_entries {
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

                        let loose_c = tezzera_layout::Constraints::loose(win_w, win_h);
                        let lctx = tezzera_widgets::tree::LayoutCtx::new(
                            loose_c, &font, &current_theme,
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
                            LayerPosition::BottomCenter => Point {
                                x: ((win_w - widget_size.width) / 2.0).max(0.0),
                                y: (win_h - widget_size.height - 24.0).max(0.0),
                            },
                            LayerPosition::Fill => Point { x: 0.0, y: 0.0 },
                        };
                        let widget_rect = Rect { origin, size: widget_size };

                        // ── Input routing for this entry (D058) ────────────
                        //
                        // Hit targets are pushed in reverse-priority order here
                        // because the merge below inserts each at index 0,
                        // reversing the list: the widget's own targets (pushed
                        // last, during paint) end up checked first, then the
                        // scrim strips, then the Block absorber.
                        {
                            let mut ht = ov_hit_targets.borrow_mut();
                            let full = Rect {
                                origin: Point { x: 0.0, y: 0.0 },
                                size: Size { width: win_w, height: win_h },
                            };
                            if entry.input == tezzera_widgets::tree::InputBehavior::Block {
                                // Absorb any tap not claimed by scrim or widget.
                                ht.push(HitTarget { rect: full, callback: std::sync::Arc::new(|| {}) });
                            }
                            if let Some(on_tap) = entry.scrim.as_ref().and_then(|s| s.on_tap.clone()) {
                                // Four strips around widget_rect: fires only for
                                // taps outside the widget (dialog itself is safe).
                                let r = widget_rect;
                                let strips = [
                                    Rect { origin: Point { x: 0.0, y: 0.0 },
                                           size: Size { width: win_w, height: r.origin.y } },
                                    Rect { origin: Point { x: 0.0, y: r.origin.y + r.size.height },
                                           size: Size { width: win_w, height: (win_h - r.origin.y - r.size.height).max(0.0) } },
                                    Rect { origin: Point { x: 0.0, y: r.origin.y },
                                           size: Size { width: r.origin.x, height: r.size.height } },
                                    Rect { origin: Point { x: r.origin.x + r.size.width, y: r.origin.y },
                                           size: Size { width: (win_w - r.origin.x - r.size.width).max(0.0), height: r.size.height } },
                                ];
                                for s in strips {
                                    if s.size.width > 0.0 && s.size.height > 0.0 {
                                        ht.push(HitTarget { rect: s, callback: on_tap.clone() });
                                    }
                                }
                            }
                        }

                        let mut ov_ctx = tezzera_widgets::tree::PaintCtx {
                            recorder: &mut ov_recorder,
                            rect: widget_rect,
                            font: &font,
                            theme: current_theme.clone(),
                            hit_targets: Rc::clone(&ov_hit_targets),
                            scroll_targets: Rc::clone(&scroll_targets),
                            focus_nodes: Rc::clone(&focus_nodes),
                            transform_entries: Rc::clone(&transform_entries),
                            clip_rect: None,
                        };
                        entry.widget.paint(&mut ov_ctx);
                    }

                    // Play overlay picture into the dedicated overlay canvas (D078).
                    let ov_picture = ov_recorder.finish();
                    overlay_canvas.play_picture(&ov_picture, &font);

                    // Merge overlay hit targets — overlay checked first (D079).
                    let ov_targets = ov_hit_targets.borrow();
                    let mut main_targets = hit_targets.borrow_mut();
                    for t in ov_targets.iter() {
                        main_targets.insert(0, tezzera_widgets::tree::HitTarget {
                            rect: t.rect,
                            callback: t.callback.clone(),
                        });
                    }
                }

                // ── TransformLayer pass (D088) ─────────────────────────────
                // Each TransformLayerEntry's child was recorded into a separate
                // Picture. Replay each picture into the base canvas at the
                // viewport position with the scroll offset applied.
                //
                // On cache-hit frames (nothing dirty) TransformLayer::paint is
                // not called, so transform_entries is empty. We fall back to
                // cached_transform_entries from the last fresh paint so the list
                // remains visible even when nothing has changed.
                {
                    let fresh = transform_entries.borrow();
                    if !fresh.is_empty() {
                        cached_transform_entries = fresh.clone();
                    }
                    let entries_to_draw = if fresh.is_empty() {
                        &cached_transform_entries[..]
                    } else {
                        &fresh[..]
                    };
                    for entry in entries_to_draw {
                        let vp = entry.viewport_rect;
                        let dx = vp.origin.x - entry.scroll_x;
                        let dy = vp.origin.y - entry.scroll_y;

                        let mut tl_recorder = tezzera_render::PictureRecorder::new();
                        for cmd in &entry.picture.commands {
                            tl_recorder.push(cmd.offset(dx, dy));
                        }
                        let tl_picture = tl_recorder.finish();
                        canvas.play_picture(&tl_picture, &font);
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

                // ── Sync focus manager with this frame's focusable nodes ───
                {
                    let collected = focus_nodes.borrow();
                    focus_manager.sync_from_nodes(collected.clone());
                }

                // ── Route events to hit targets and focus ──────────────────
                let targets = hit_targets.borrow();
                let scrolls = scroll_targets.borrow();
                for event in events {
                    match event {
                        tezzera_platform::InputEvent::MouseDown {
                            x, y, button: tezzera_platform::MouseButton::Left
                        } => {
                            for t in targets.iter() {
                                let r = &t.rect;
                                if x >= &r.origin.x
                                    && x <= &(r.origin.x + r.size.width)
                                    && y >= &r.origin.y
                                    && y <= &(r.origin.y + r.size.height)
                                {
                                    (t.callback)();
                                    break;
                                }
                            }
                        }
                        tezzera_platform::InputEvent::Scroll { x, y, delta_x, delta_y } => {
                            for s in scrolls.iter() {
                                let r = &s.rect;
                                if x >= &r.origin.x
                                    && x <= &(r.origin.x + r.size.width)
                                    && y >= &r.origin.y
                                    && y <= &(r.origin.y + r.size.height)
                                {
                                    (s.callback)(*delta_x, *delta_y);
                                    break;
                                }
                            }
                        }
                        tezzera_platform::InputEvent::KeyDown {
                            key: tezzera_platform::Key::Tab
                        } => {
                            if shift_held {
                                focus_manager.focus_prev_node();
                            } else {
                                focus_manager.focus_next_node();
                            }
                        }
                        tezzera_platform::InputEvent::KeyDown {
                            key: tezzera_platform::Key::Shift
                        } => { shift_held = true; }
                        tezzera_platform::InputEvent::KeyUp {
                            key: tezzera_platform::Key::Shift
                        } => { shift_held = false; }
                        _ => {}
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
/// `position` — DFS counter for Component nodes (determines ComponentId).
/// `native_idx` — DFS counter for Native nodes (indexes into `render_nodes`).
/// `render_nodes` — persistent RenderNode list; caches layout size + Picture.
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
    native_idx: &mut usize,
    render_nodes: &mut Vec<render_node::RenderNode>,
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

            walk_element(
                &child_element,
                constraints,
                ctx,
                position,
                native_idx,
                render_nodes,
                dirty_ids,
                global_dirty,
                child_subtree_dirty,
                element_cache,
                new_mounted,
            )
        }

        Element::Native(n) => {
            if let Some(wb) = n.payload.as_ref()
                .and_then(|p| p.as_any().downcast_ref::<WidgetBox>())
            {
                // ── Ensure RenderNode exists at this DFS position ──────────
                let idx = *native_idx;
                *native_idx += 1;
                if render_nodes.len() <= idx {
                    render_nodes.push(render_node::RenderNode::new(n.tag, n.key.clone()));
                } else if render_nodes[idx].tag != n.tag {
                    // Type mismatch — replace with a fresh dirty node.
                    render_nodes[idx] = render_node::RenderNode::new(n.tag, n.key.clone());
                }
                let node = &mut render_nodes[idx];

                // When the containing component rebuilt, force re-layout + re-paint.
                if subtree_dirty {
                    node.paint_dirty = true;
                }

                // ── Layout (skip if constraints unchanged and not dirty) ────
                let size = if node.last_constraints == Some(constraints) && !node.paint_dirty
                    && node.cached_size.is_some()
                {
                    node.cached_size.unwrap()
                } else {
                    let lctx = ctx.layout_ctx(constraints);
                    let s = wb.0.layout(&lctx);
                    node.last_constraints = Some(constraints);
                    node.cached_size = Some(s);
                    node.paint_dirty = true;
                    s
                };

                let child_rect = Rect { origin: ctx.rect.origin, size };

                // ── Paint (skip if rect unchanged and not dirty) ───────────
                if !node.paint_dirty
                    && node.cached_picture.is_some()
                    && node.cached_rect == Some(child_rect)
                {
                    // Replay cached display list — zero widget work.
                    let pic = node.cached_picture.as_ref().unwrap();
                    for cmd in &pic.commands {
                        ctx.recorder.push(cmd.clone());
                    }
                    // Re-register cached hit handlers with their original rects.
                    // Each entry stores (rect, callback) so we don't fall back
                    // to child_rect (the root widget's full-window rect), which
                    // would make every click fire the first-registered button.
                    let mut ht = ctx.hit_targets.borrow_mut();
                    for (rect, cb) in &node.hit_handlers {
                        ht.push(HitTarget { rect: *rect, callback: cb.clone() });
                    }
                    drop(ht);
                    // Re-register cached scroll targets similarly.
                    let mut st = ctx.scroll_targets.borrow_mut();
                    for (rect, cb) in &node.scroll_handlers {
                        st.push(tezzera_widgets::tree::ScrollTarget { rect: *rect, callback: cb.clone() });
                    }
                } else {
                    // Fresh paint — record into a sub-recorder, then merge.
                    let sub_hit: Rc<RefCell<Vec<HitTarget>>> =
                        Rc::new(RefCell::new(Vec::new()));
                    let sub_scroll: Rc<RefCell<Vec<tezzera_widgets::tree::ScrollTarget>>> =
                        Rc::new(RefCell::new(Vec::new()));
                    let mut sub_recorder = tezzera_render::PictureRecorder::new();
                    {
                        let mut child_ctx = tezzera_widgets::tree::PaintCtx {
                            recorder: &mut sub_recorder,
                            rect: child_rect,
                            font: ctx.font,
                            theme: ctx.theme.clone(),
                            hit_targets: Rc::clone(&sub_hit),
                            scroll_targets: Rc::clone(&sub_scroll),
                            focus_nodes: Rc::clone(&ctx.focus_nodes),
                            transform_entries: Rc::clone(&ctx.transform_entries),
                            clip_rect: ctx.clip_rect,
                        };
                        wb.0.paint(&mut child_ctx);
                    }
                    let picture = sub_recorder.finish();

                    // Merge sub-picture commands into main recorder.
                    for cmd in &picture.commands {
                        ctx.recorder.push(cmd.clone());
                    }
                    // Collect and cache hit handlers with their rects.
                    let targets = sub_hit.borrow();
                    node.hit_handlers = targets.iter()
                        .map(|t| (t.rect, t.callback.clone()))
                        .collect();
                    let mut ht = ctx.hit_targets.borrow_mut();
                    for t in targets.iter() {
                        ht.push(HitTarget { rect: t.rect, callback: t.callback.clone() });
                    }
                    drop(targets);
                    // Collect and cache scroll handlers.
                    let scrolls = sub_scroll.borrow();
                    node.scroll_handlers = scrolls.iter()
                        .map(|s| (s.rect, s.callback.clone()))
                        .collect();
                    let mut st = ctx.scroll_targets.borrow_mut();
                    for s in scrolls.iter() {
                        st.push(tezzera_widgets::tree::ScrollTarget { rect: s.rect, callback: s.callback.clone() });
                    }
                    drop(scrolls);

                    // Cache the picture and clear dirty flag.
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
    ColoredBox, Column, Container, Dialog, Divider,
    EdgeInsets, Expanded, Icon, IconKind,
    Image, ListTile, ListView,
    Menu, NavItem, NavRail,
    Padding, ProgressBar,
    RectReader,
    RepaintBoundary,
    TransformLayer,
    OverlayEntry, LayerId, LayerPosition, InputBehavior, FocusBehavior, ScrimConfig,
    push_overlay,
    Row, Scaffold, ScrollView, ScrollAxis, Sheet,
    SizedBox, Slider, Spacer, Stack, Switch,
    Tab, TabBar, Text, TextInput, Toast, ToastKind, Tooltip,
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
    pub use tezzera_widgets::RepaintBoundary;
    pub use tezzera_widgets::TransformLayer;
    pub use tezzera_nav::ScreenNav;
    pub use tezzera_render::canvas::Color;
    pub use tezzera_theme::{ThemeData, ColorScheme};
    pub use tezzera_theme::built_in::{dark_theme, light_theme};
    pub use tezzera_core::types::{Point, Rect, Size};
    pub use tezzera_layout::{Constraints, CrossAxisAlignment, MainAxisAlignment};
    pub use tezzera_state::Atom;
}
