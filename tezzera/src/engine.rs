//! `FrameEngine` — the per-frame build/paint/input logic, extracted from
//! `App::launch`'s `run_layered` closure (Phase 24 Step 1, D106).
//!
//! This is a behavior-preserving extraction: the desktop/web path
//! (`App::launch` → `PlatformWindow::run_layered`) drives it exactly as
//! before. The point is to make the same logic drivable from a second place
//! — a native-host FFI boundary (`tezzera-ffi`) that has no winit event loop
//! at all — without duplicating ~450 lines of reconciler/paint/input code.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use tezzera_core::Component;
use tezzera_core::types::Rect;
use tezzera_render::SkiaCanvas;
use tezzera_widgets::tree::{clear_overlays, drain_overlays};

use crate::{inflate_rect, rect_contains, theme_color, walk_element, OverlayRoute};

/// Owns everything that must persist across frames: the root component,
/// reconciler caches, focus state, drag/long-press state, and the persistent
/// render tree (D091). One [`FrameEngine`] per running app instance.
pub struct FrameEngine {
    root: Box<dyn Component>,
    font: tezzera_render::FontCache,

    // ── Reconciler state — persists across frames ──────────────────────
    prev_mounted: HashSet<u64>,
    element_cache: HashMap<u64, tezzera_core::Element>,
    render_tree: Rc<RefCell<tezzera_widgets::tree::RenderTree>>,

    // ── Focus + input state ─────────────────────────────────────────────
    focus_manager: tezzera_a11y::FocusManager,
    shift_held: bool,
    /// Active drag grab: a POSITIONAL hit (on_press_at) captured on
    /// MouseDown receives streamed MouseMove positions until MouseUp —
    /// slider thumbs, pickers. Plain hits never drag.
    active_drag: Option<Arc<dyn Fn(f32, f32) + Send + Sync>>,
    /// Set when a hover change (or other non-atom event) needs a repaint on
    /// the next frame; consumed by `needs_paint`.
    forced_repaint: bool,
    /// Long-press: cancel token for the in-flight press timer + press origin.
    lp_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    press_origin: Option<(f32, f32)>,
}

impl FrameEngine {
    /// Builds a fresh engine for `root`, marking every component dirty so
    /// the first `paint` call does a full build + repaint.
    pub fn new(root: Box<dyn Component>, font: tezzera_render::FontCache) -> Self {
        tezzera_state::reset_to_global_dirty();
        Self {
            root,
            font,
            prev_mounted: HashSet::new(),
            element_cache: HashMap::new(),
            render_tree: Rc::new(RefCell::new(tezzera_widgets::tree::RenderTree::new())),
            focus_manager: tezzera_a11y::FocusManager::new(),
            shift_held: false,
            active_drag: None,
            forced_repaint: false,
            lp_cancel: None,
            press_origin: None,
        }
    }

    /// The current semantic (accessibility/SEO) tree, derived from the
    /// render tree `paint` last built — call after at least one `paint()`
    /// (the render tree is empty before that). Used both by D099 assistive
    /// tech and, from D107/Phase 25, by build-time HTML/SEO export (see
    /// `tezzera-web-seo`'s `render_html`) — a headless caller can call
    /// `paint()` once into a throwaway `SkiaCanvas` purely to populate the
    /// render tree, then read this, with no real window/GPU needed
    /// (`SkiaCanvas` is a plain in-memory CPU pixmap).
    pub fn semantics(&self) -> tezzera_core::SemanticNode {
        self.render_tree.borrow().collect_semantics()
    }

    /// Runs one frame: build (if dirty), layout, paint into `canvas` and
    /// `overlay_canvas`, then dispatch `events`. Callers are responsible for
    /// presenting the canvases afterward (winit's `PlatformWindow` does this
    /// via `GpuPresenter`; an FFI host does the analogous thing).
    ///
    /// Returns whether any component's content may have changed this frame
    /// (`global_dirty || !dirty_ids.is_empty()` — deliberately excludes
    /// purely-visual causes like a resize or hover repaint, which affect
    /// pixels but never `Semantics`/text content). Used by the web target's
    /// D107/Phase 25 Step 4 shadow-DOM sync to decide whether it's worth
    /// re-deriving the semantic tree at all this frame — computed here
    /// rather than by the caller re-deriving it, since `dirty_ids` is
    /// drained by `take_dirty_components()` below and can only be read once
    /// per frame.
    pub fn paint(
        &mut self,
        canvas: &mut SkiaCanvas,
        overlay_canvas: &mut SkiaCanvas,
        events: &[tezzera_platform::InputEvent],
    ) -> bool {
        let root = &self.root;
        let font = &self.font;

        // ── Drain dirty-component set for this frame ───────────────────
        let global_dirty = tezzera_state::is_global_dirty();
        let dirty_ids = tezzera_state::take_dirty_components();
        let content_changed = global_dirty || !dirty_ids.is_empty();

        // ── Build root (only when dirty) ────────────────────────────────
        //
        // The root component (ComponentId(0)) owns all atoms created via
        // ctx.state(). When any of those atoms change, ComponentId(0) lands
        // in dirty_ids. We rebuild ONLY then; on clean frames the cached
        // element is reused, keeping `build()` side-effects out of the
        // render loop (e.g. an atom.set() inside build() would otherwise
        // cause an infinite loop).
        let root_component_id = tezzera_core::types::ComponentId(0);
        let root_is_dirty = global_dirty || dirty_ids.contains(&root_component_id);

        let element = if root_is_dirty || !self.element_cache.contains_key(&0) {
            let mut ctx = tezzera_core::Context::new(root_component_id);
            let el = root.build(&mut ctx);
            self.element_cache.insert(0, el.clone());
            el
        } else {
            self.element_cache.get(&0).unwrap().clone()
        };

        // ── Clear overlay registry from prior frame ─────────────────────
        clear_overlays();

        // ── Read active theme each frame so set_theme() takes effect ────
        // Widgets call set_theme() from button callbacks; the change is
        // picked up here on the very next frame.
        let current_theme = tezzera_theme::use_theme();

        // Layout in logical pixels so widget sizes and font sizes are
        // display-independent. play_picture scales to physical pixels.
        let win_w = canvas.logical_width() as f32;
        let win_h = canvas.logical_height() as f32;

        // ── Frame-skip (Phase 20 Step 5, first slice) ───────────────────
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
        let hover_frame = self.forced_repaint;
        self.forced_repaint = false;
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

        // ── Set up main display-list recording ──────────────────────────
        let mut recorder = tezzera_render::PictureRecorder::new();

        // Begin the persistent render tree frame (D091). Repainted
        // nodes re-declare their regions; skipped subtrees keep theirs.
        self.render_tree.borrow_mut().start_frame();
        let mut paint_ctx = tezzera_widgets::tree::PaintCtx {
            recorder: &mut recorder,
            rect: tezzera_core::types::Rect {
                origin: tezzera_core::types::Point { x: 0.0, y: 0.0 },
                size: tezzera_core::types::Size { width: win_w, height: win_h },
            },
            font,
            theme: current_theme.clone(),
            tree: Rc::clone(&self.render_tree),
            node: tezzera_widgets::tree::RenderTree::ROOT,
            owner: root_component_id,
            clip_rect: None,
        };

        let constraints = tezzera_layout::Constraints::tight(win_w, win_h);

        // ── Walk element tree — widgets record DrawCommands ─────────────
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
            &mut self.element_cache,
            &mut new_mounted,
        );
        self.render_tree.borrow_mut().finalize();

        // Self-animating widgets (spinner, shimmer) asked to keep going.
        if tezzera_widgets::tree::take_animation_request() {
            self.forced_repaint = true;
            tezzera_state::request_frame();
        }

        // ── Damage-scoped clear + replay (Phase 20 Step 5, slice 2) ─────
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
        canvas.play_picture(&picture, font);
        canvas.set_logical_clip(None);

        // The base canvas changed this frame — tell the platform to
        // re-upload its GPU texture (D089). Clean/hover frames skip
        // this block, leaving frame_dirty false so no upload happens.
        canvas.mark_frame_dirty();

        // ── Reconcile: fire lifecycle for mounted/unmounted components ──
        for &id in new_mounted.difference(&self.prev_mounted) {
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
        for &id in self.prev_mounted.difference(&new_mounted) {
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
        self.prev_mounted = new_mounted;
        } // needs_paint

        // ── Overlay pass — second recorder into overlay_canvas (D076) ───
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

            let tree_ref = self.render_tree.borrow();
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
                        loose_c, font, &current_theme,
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
                        font,
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
                overlay_canvas.play_picture(&ov_picture, font);
            }
        }

        // ── TransformLayer pass (D088/D090) ─────────────────────────────
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
            let tree_ref = self.render_tree.borrow();
            for (n, i) in tree_ref.transform_ids() {
                let entry = &tree_ref.node(n).transforms[i];
                let vp = entry.viewport_rect;

                // Content texture = child natural size at physical
                // resolution, capped. Pixmap starts transparent, so
                // areas the content does not cover reveal the base.
                let cw = (((entry.child_size.width  * scale).ceil() as u32)).clamp(1, MAX_TL_DIM);
                let ch = (((entry.child_size.height * scale).ceil() as u32)).clamp(1, MAX_TL_DIM);
                let mut content = tezzera_render::SkiaCanvas::new_hidpi(cw, ch, scale);
                content.play_picture(&entry.picture, font);

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


        // ── Sync focus manager from the render tree ─────────────────────
        // Collected from persistent nodes, so the Tab cycle survives
        // cache-hit frames where no widget repainted.
        self.focus_manager.sync_from_nodes(self.render_tree.borrow().collect_focus());

        // ── Route events — structural z-order (D092) ────────────────────
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
                        let hit = self.render_tree.borrow().hit_test(*x, *y);
                        if let Some((cb, positional)) = hit {
                            cb(*x, *y);
                            if positional {
                                self.active_drag = Some(cb);
                            }
                        }
                    }
                    // Arm a long-press timer if a region wants one.
                    self.press_origin = Some((*x, *y));
                    let lp = self.render_tree.borrow().long_press_test(*x, *y);
                    if let Some(cb) = lp {
                        use std::sync::atomic::{AtomicBool, Ordering};
                        let cancel = Arc::new(AtomicBool::new(false));
                        self.lp_cancel = Some(cancel.clone());
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
                    if let Some(cb) = &self.active_drag {
                        cb(*x, *y);
                    }
                    // Hover tracking — repaints only the changed nodes.
                    let target = self.render_tree.borrow().hover_test(*x, *y);
                    let changed = self.render_tree.borrow_mut().set_hover(target);
                    if changed {
                        self.forced_repaint = true;
                        tezzera_state::request_frame();
                    }
                    // Movement past the slop cancels a pending long-press.
                    if let Some((ox, oy)) = self.press_origin {
                        if (x - ox).abs() > 8.0 || (y - oy).abs() > 8.0 {
                            if let Some(c) = &self.lp_cancel { c.store(true, Ordering::Relaxed); }
                            self.lp_cancel = None;
                            self.press_origin = None;
                        }
                    }
                }
                tezzera_platform::InputEvent::MouseUp { .. } => {
                    use std::sync::atomic::Ordering;
                    self.active_drag = None;
                    if let Some(c) = &self.lp_cancel { c.store(true, Ordering::Relaxed); }
                    self.lp_cancel = None;
                    self.press_origin = None;
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
                        let cb = self.render_tree.borrow().scroll_test(*x, *y, *delta_x, *delta_y);
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
                    if self.shift_held {
                        self.focus_manager.focus_prev_node();
                    } else {
                        self.focus_manager.focus_next_node();
                    }
                }
                tezzera_platform::InputEvent::KeyDown {
                    key: tezzera_platform::Key::Shift
                } => { self.shift_held = true; }
                tezzera_platform::InputEvent::KeyUp {
                    key: tezzera_platform::Key::Shift
                } => { self.shift_held = false; }
                _ => {}
            }
        }

        content_changed
    }
}
