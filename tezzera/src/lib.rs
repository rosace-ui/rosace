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
use std::sync::Arc;

use tezzera_theme::built_in;
use tezzera_platform::PlatformWindow;
use tezzera_widgets::tree::{WidgetBox, clear_overlays, drain_overlays};

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

        // ── Reconciler state — persists across frames ──────────────────────
        // ComponentIds assigned by DFS position; stable IDs mean state persists.
        let mut prev_mounted: HashSet<u64> = HashSet::new();

        // ── Phase 14: focus manager ────────────────────────────────────────
        let mut focus_manager = tezzera_a11y::FocusManager::new();
        let mut shift_held = false;
        // Active drag grab: a POSITIONAL hit (on_press_at) captured on
        // MouseDown receives streamed MouseMove positions until MouseUp —
        // slider thumbs, pickers. Plain hits never drag.
        let mut active_drag: Option<Arc<dyn Fn(f32, f32) + Send + Sync>> = None;
        // Set when a hover change (or other non-atom event) needs a repaint on
        // the next frame; consumed by `needs_paint`.
        let mut forced_repaint = false;
        // Long-press: cancel token for the in-flight press timer + press origin.
        let mut lp_cancel: Option<Arc<std::sync::atomic::AtomicBool>> = None;
        let mut press_origin: Option<(f32, f32)> = None;

        // ── Phase 13: persistent render cache ─────────────────────────────
        // Cached build output per component ID — skips build() when the
        // component's atoms haven't changed.
        let mut element_cache: std::collections::HashMap<u64, tezzera_core::Element> =
            std::collections::HashMap::new();
        // ── Phase 20: persistent render tree (D091) ────────────────────────
        // Single owner of per-node retained state: hit/scroll regions, focus
        // nodes, overlay attachments, transform layers. State declared during
        // paint persists on cache-hit frames by construction — this replaces
        // the per-concern caches (hit_handlers, cached_transform_entries,
        // cached_overlay_entries) that each fixed one instance of the same
        // vanishing-state bug.
        let render_tree: Rc<RefCell<tezzera_widgets::tree::RenderTree>> =
            Rc::new(RefCell::new(tezzera_widgets::tree::RenderTree::new()));
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

                // Layout in logical pixels so widget sizes and font sizes are
                // display-independent. play_picture scales to physical pixels.
                let win_w = canvas.logical_width() as f32;
                let win_h = canvas.logical_height() as f32;

                // ── Frame-skip (Phase 20 Step 5, first slice) ──────────────
                // On a clean frame — nothing dirty, canvas not recreated by a
                // resize — the base canvas already holds the correct pixels
                // and the render tree holds all dispatch state: skip build,
                // walk, and rasterization entirely. Overlay pass, focus sync,
                // and event dispatch still run below.
                let window_resized = events.iter().any(|e| matches!(
                    e, tezzera_platform::InputEvent::WindowResized { .. }
                ));
                // A hover change repaints the widget subtree (the picture cache
                // unit is the top-level native node, so localized damage needs
                // the widgets to actually re-run paint — force it this frame).
                let hover_frame = forced_repaint;
                forced_repaint = false;
                let needs_paint = global_dirty
                    || !dirty_ids.is_empty()
                    || !canvas.has_drawn()   // fresh canvas after resize/scale change
                    || window_resized
                    || hover_frame;

                if needs_paint {
                // A full repaint clears the whole canvas; otherwise we clear
                // and replay only the damaged region (computed by the walk).
                let full_repaint = global_dirty || window_resized || !canvas.has_drawn();
                let bg = theme_color(&current_theme.colors.background);

                // ── Set up main display-list recording ─────────────────────
                let mut recorder = tezzera_render::PictureRecorder::new();

                // Begin the persistent render tree frame (D091). Repainted
                // nodes re-declare their regions; skipped subtrees keep theirs.
                render_tree.borrow_mut().start_frame();
                let mut paint_ctx = tezzera_widgets::tree::PaintCtx {
                    recorder: &mut recorder,
                    rect: tezzera_core::types::Rect {
                        origin: tezzera_core::types::Point { x: 0.0, y: 0.0 },
                        size: tezzera_core::types::Size { width: win_w, height: win_h },
                    },
                    font: &font,
                    theme: current_theme.clone(),
                    tree: Rc::clone(&render_tree),
                    node: tezzera_widgets::tree::RenderTree::ROOT,
                    owner: root_component_id,
                    clip_rect: None,
                };

                let constraints = tezzera_layout::Constraints::tight(win_w, win_h);

                // ── Walk element tree — widgets record DrawCommands ────────
                let mut position: u64 = 0;
                let mut damage: Option<Rect> = None;
                let mut new_mounted: HashSet<u64> = HashSet::new();
                walk_element(
                    &element,
                    constraints,
                    &mut paint_ctx,
                    &mut position,
                    &mut damage,
                    &dirty_ids,
                    global_dirty,
                    root_is_dirty || hover_frame,  // subtree_dirty (+ hover forces repaint)
                    &mut element_cache,
                    &mut new_mounted,
                );
                render_tree.borrow_mut().finalize();

                // Self-animating widgets (spinner, shimmer) asked to keep going.
                if tezzera_widgets::tree::take_animation_request() {
                    forced_repaint = true;
                    tezzera_state::request_frame();
                }

                // ── Damage-scoped clear + replay (Phase 20 Step 5, slice 2) ─
                // Full repaint (first frame, resize, theme swap) clears the
                // whole canvas; otherwise clear + replay only the union of
                // changed rects, culling every fill/blit/text outside it.
                let picture = recorder.finish();
                // Inflate damage to cover pixels a widget paints OUTSIDE its
                // rect: shadow blur (≤16px), focus rings, rounded-corner AA.
                let damage_clip = if full_repaint {
                    None
                } else {
                    damage.map(|d| inflate_rect(d, 24.0))
                };
                match damage_clip {
                    None => canvas.clear(bg),
                    Some(d) => {
                        canvas.set_logical_clip(Some(d));
                        canvas.fill_logical_rect(d, bg);
                    }
                }
                canvas.play_picture(&picture, &font);
                canvas.set_logical_clip(None);

                // The base canvas changed this frame — tell the platform to
                // re-upload its GPU texture (D089). Clean/hover frames skip
                // this block, leaving frame_dirty false so no upload happens.
                canvas.mark_frame_dirty();

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
                } // needs_paint

                // ── Overlay pass — second recorder into overlay_canvas (D076) ─
                // Entries come from the render tree (D091 — they persist on
                // clean frames and clear when their owner repaints), plus the
                // legacy thread-local registry for direct push_overlay users.
                // Overlay widgets repaint every frame; each gets a throwaway
                // per-entry tree whose regions become an OverlayRoute for
                // structural input routing (D092) — no scrim hit strips.
                let legacy_overlays = drain_overlays();
                let mut overlay_routes: Vec<OverlayRoute> = Vec::new();
                {
                    use tezzera_core::types::{Point, Rect, Size};
                    use tezzera_widgets::tree::LayerPosition;

                    let tree_ref = render_tree.borrow();
                    let overlay_ids = tree_ref.overlay_ids();

                    if !overlay_ids.is_empty() || !legacy_overlays.is_empty() {
                        let mut ov_recorder = tezzera_render::PictureRecorder::new();

                        let entries = overlay_ids.iter()
                            .map(|&(n, i)| &tree_ref.node(n).overlays[i])
                            .chain(legacy_overlays.iter());

                        for entry in entries {
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
                            // Window-aware: clamp the overlay inside the
                            // window with an 8px margin so anchored menus
                            // never render off-screen.
                            let origin = Point {
                                x: origin.x.min((win_w - widget_size.width - 8.0).max(0.0)).max(0.0),
                                y: origin.y.min((win_h - widget_size.height - 8.0).max(0.0)).max(0.0),
                            };
                            let widget_rect = Rect { origin, size: widget_size };

                            // Paint into a per-entry throwaway tree; its regions
                            // are flattened into the entry's dispatch route.
                            let ov_tree = Rc::new(RefCell::new(
                                tezzera_widgets::tree::RenderTree::new(),
                            ));
                            let mut ov_ctx = tezzera_widgets::tree::PaintCtx::root(
                                &mut ov_recorder,
                                widget_rect,
                                &font,
                                current_theme.clone(),
                                Rc::clone(&ov_tree),
                            );
                            entry.widget.paint(&mut ov_ctx);
                            drop(ov_ctx);

                            let ov_tree = ov_tree.borrow();
                            overlay_routes.push(OverlayRoute {
                                rect: widget_rect,
                                input: entry.input,
                                on_tap: entry.scrim.as_ref().and_then(|s| s.on_tap.clone()),
                                hits: ov_tree.collect_hits(),
                                scrolls: ov_tree.collect_scrolls(),
                            });
                        }

                        // Play overlay picture into the dedicated overlay canvas (D078).
                        let ov_picture = ov_recorder.finish();
                        overlay_canvas.play_picture(&ov_picture, &font);
                    }
                }

                // ── TransformLayer pass (D088/D090) ────────────────────────
                // Each entry's content is rendered ONCE into its own content-
                // sized canvas and published as a placed GPU compositor layer
                // (D090) — the compositor samples it at the scroll offset, so
                // scrolling is a UV shift rather than a base-canvas re-raster.
                // Published only on repaint frames; the platform retains the
                // set across clean frames (persists through frame-skip).
                if needs_paint {
                    // Physical-pixel cap for a content texture (D082).
                    const MAX_TL_DIM: u32 = 4096;
                    let scale = canvas.scale();
                    let mut scroll_layers: Vec<tezzera_platform::ScrollLayer> = Vec::new();
                    let tree_ref = render_tree.borrow();
                    for (n, i) in tree_ref.transform_ids() {
                        let entry = &tree_ref.node(n).transforms[i];
                        let vp = entry.viewport_rect;

                        // Content texture = child natural size at physical
                        // resolution, capped. Pixmap starts transparent, so
                        // areas the content does not cover reveal the base.
                        let cw = (((entry.child_size.width  * scale).ceil() as u32)).clamp(1, MAX_TL_DIM);
                        let ch = (((entry.child_size.height * scale).ceil() as u32)).clamp(1, MAX_TL_DIM);
                        let mut content = tezzera_render::SkiaCanvas::new_hidpi(cw, ch, scale);
                        content.play_picture(&entry.picture, &font);

                        scroll_layers.push(tezzera_platform::ScrollLayer {
                            id: n as u64,
                            pixels: content.pixels().to_vec(),
                            width:  cw,
                            height: ch,
                            dest: (
                                vp.origin.x * scale, vp.origin.y * scale,
                                vp.size.width * scale, vp.size.height * scale,
                            ),
                        });
                    }
                    drop(tree_ref);
                    tezzera_platform::publish_scroll_layers(scroll_layers);
                }


                // ── Sync focus manager from the render tree ────────────────
                // Collected from persistent nodes, so the Tab cycle survives
                // cache-hit frames where no widget repainted.
                focus_manager.sync_from_nodes(render_tree.borrow().collect_focus());

                // ── Route events — structural z-order (D092) ───────────────
                // Overlay routes first (topmost entry first): the entry's own
                // regions win; its surface absorbs; outside taps fire the scrim
                // dismiss or are swallowed by Block; PassThrough falls through.
                // Anything unclaimed goes to the render-tree walk, where later
                // siblings (painted on top) win structurally.
                for event in events {
                    match event {
                        tezzera_platform::InputEvent::MouseDown {
                            x, y, button: tezzera_platform::MouseButton::Left
                        } => {
                            let mut handled = false;
                            for route in overlay_routes.iter().rev() {
                                if let Some((_, cb)) = route.hits.iter().rev()
                                    .find(|(r, _)| rect_contains(r, *x, *y))
                                {
                                    cb();
                                    handled = true;
                                    break;
                                }
                                if rect_contains(&route.rect, *x, *y) {
                                    handled = true; // overlay surface absorbs
                                    break;
                                }
                                if let Some(on_tap) = &route.on_tap {
                                    on_tap();
                                    handled = true;
                                    break;
                                }
                                if route.input == tezzera_widgets::tree::InputBehavior::Block {
                                    handled = true;
                                    break;
                                }
                            }
                            if !handled {
                                let hit = render_tree.borrow().hit_test(*x, *y);
                                if let Some((cb, positional)) = hit {
                                    cb(*x, *y);
                                    if positional {
                                        active_drag = Some(cb);
                                    }
                                }
                            }
                            // Arm a long-press timer if a region wants one.
                            press_origin = Some((*x, *y));
                            let lp = render_tree.borrow().long_press_test(*x, *y);
                            if let Some(cb) = lp {
                                use std::sync::atomic::{AtomicBool, Ordering};
                                let cancel = Arc::new(AtomicBool::new(false));
                                lp_cancel = Some(cancel.clone());
                                std::thread::spawn(move || {
                                    std::thread::sleep(std::time::Duration::from_millis(500));
                                    if !cancel.load(Ordering::Relaxed) {
                                        cb();
                                        tezzera_state::request_frame();
                                    }
                                });
                            }
                        }
                        tezzera_platform::InputEvent::MouseMove { x, y } => {
                            use std::sync::atomic::Ordering;
                            if let Some(cb) = &active_drag {
                                cb(*x, *y);
                            }
                            // Hover tracking — repaints only the changed nodes.
                            let target = render_tree.borrow().hover_test(*x, *y);
                            let changed = render_tree.borrow_mut().set_hover(target);
                            if changed {
                                forced_repaint = true;
                                tezzera_state::request_frame();
                            }
                            // Movement past the slop cancels a pending long-press.
                            if let Some((ox, oy)) = press_origin {
                                if (x - ox).abs() > 8.0 || (y - oy).abs() > 8.0 {
                                    if let Some(c) = &lp_cancel { c.store(true, Ordering::Relaxed); }
                                    lp_cancel = None;
                                    press_origin = None;
                                }
                            }
                        }
                        tezzera_platform::InputEvent::MouseUp { .. } => {
                            use std::sync::atomic::Ordering;
                            active_drag = None;
                            if let Some(c) = &lp_cancel { c.store(true, Ordering::Relaxed); }
                            lp_cancel = None;
                            press_origin = None;
                        }
                        tezzera_platform::InputEvent::Scroll { x, y, delta_x, delta_y } => {
                            let mut handled = false;
                            for route in overlay_routes.iter().rev() {
                                let candidates: Vec<_> = route.scrolls.iter().rev()
                                    .filter(|(r, _, _)| rect_contains(r, *x, *y))
                                    .map(|(_, a, cb)| (*a, cb.clone()))
                                    .collect();
                                if let Some(cb) = tezzera_widgets::tree::render_tree::select_scroll_handler(
                                    &candidates, *delta_x, *delta_y,
                                ) {
                                    cb(*delta_x, *delta_y);
                                    handled = true;
                                    break;
                                }
                                if rect_contains(&route.rect, *x, *y)
                                    && route.input == tezzera_widgets::tree::InputBehavior::Block
                                {
                                    handled = true;
                                    break;
                                }
                            }
                            if !handled {
                                let cb = render_tree.borrow().scroll_test(*x, *y, *delta_x, *delta_y);
                                if let Some(cb) = cb {
                                    cb(*delta_x, *delta_y);
                                }
                            }
                        }
                        tezzera_platform::InputEvent::KeyDown {
                            key: tezzera_platform::Key::Escape
                        } => {
                            // Dismiss the topmost overlay that has a scrim
                            // dismisser (dialog, sheet, dropdown).
                            if let Some(on_tap) = overlay_routes.iter().rev()
                                .find_map(|r| r.on_tap.clone())
                            {
                                on_tap();
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
