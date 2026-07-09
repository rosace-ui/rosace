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
                    // Press/tap feedback (D108/Phase 26 Step 1): mirror hover
                    // resolution at the moment of MouseDown, held until
                    // MouseUp regardless of small cursor drift meanwhile.
                    let press_target = self.render_tree.borrow().hover_test(*x, *y);
                    if self.render_tree.borrow_mut().set_pressed(press_target) {
                        self.forced_repaint = true;
                        tezzera_state::request_frame();
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
                    if self.render_tree.borrow_mut().set_pressed(None) {
                        self.forced_repaint = true;
                        tezzera_state::request_frame();
                    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_core::{Component, Context, Element};
    use tezzera_widgets::tree::{Button, ButtonVariant, Widget};

    /// `tezzera_theme::provider`'s theme is a process-wide `GlobalAtom` —
    /// `cargo test` runs test functions on parallel threads within the same
    /// process by default, so any test that mutates
    /// `ThemeData.animation.enabled` (as
    /// `disabling_animations_stops_coasting_immediately_on_release` does)
    /// would otherwise race with any other test whose behavior depends on
    /// that flag being `true` (the animate/coast tests). Discovered for
    /// real — this test was flaky when run alongside the others until this
    /// lock was added, not a hypothetical.
    static ANIMATION_GLOBAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Root that fills the whole canvas with a single pressable Button
    /// (tight root constraints, D108/Phase 26 Step 1's real integration
    /// point: `engine.rs`'s MouseDown/MouseUp -> `RenderTree::set_pressed`
    /// -> `PaintCtx::pressed()` -> `Button::paint`'s `animate_to`).
    struct OneButton;
    impl Component for OneButton {
        fn build(&self, _ctx: &mut Context) -> Element {
            Button::new("Press me").variant(ButtonVariant::Primary).on_press(|| {}).into_element()
        }
    }

    fn headless_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(OneButton), tezzera_render::FontCache::embedded());
        (engine, SkiaCanvas::new(200, 60), SkiaCanvas::new(200, 60))
    }

    #[test]
    fn press_then_release_sets_and_clears_render_tree_pressed_state() {
        let (mut engine, mut canvas, mut overlay) = headless_engine();
        // First frame: build + layout, no events — populates hit regions.
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = tezzera_platform::InputEvent::MouseDown {
            x: 100.0, y: 30.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);
        assert!(
            engine.render_tree.borrow().nodes_iter().any(|n| n.pressed),
            "MouseDown over the button must mark some node pressed"
        );

        let up = tezzera_platform::InputEvent::MouseUp {
            x: 100.0, y: 30.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[up]);
        assert!(
            engine.render_tree.borrow().nodes_iter().all(|n| !n.pressed),
            "MouseUp must clear pressed state"
        );
    }

    #[test]
    fn press_eases_the_button_toward_full_emphasis_over_several_frames() {
        // `frame_dt` is ALSO process-global (`tezzera_animate::set_frame_dt`)
        // — same lock as the animation-enabled tests, for the same reason:
        // another test setting a different frame_dt mid-run would corrupt
        // this one's convergence math. Found for real: adding the wheel
        // momentum test (which also sets frame_dt) made this test flaky
        // under `cargo test`'s parallel execution.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A deterministic synthetic frame_dt, not real wall-clock time
        // between fast test calls — otherwise convergence speed (and thus
        // this test's pass/fail) would depend on machine speed.
        tezzera_animate::set_frame_dt(0.05);

        let (mut engine, mut canvas, mut overlay) = headless_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = tezzera_platform::InputEvent::MouseDown {
            x: 100.0, y: 30.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);
        let first = engine.render_tree.borrow().nodes_iter().find_map(|n| n.anim);

        // Re-paint several more frames with no new events — animate_to keeps
        // easing toward the 1.0 press target via its own frame-request loop.
        for _ in 0..30 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let settled = engine.render_tree.borrow().nodes_iter().find_map(|n| n.anim);

        assert!(first.is_some(), "first pressed frame must observe an eased value");
        assert!(
            settled.unwrap() > first.unwrap(),
            "emphasis must have eased further toward the press target over subsequent frames: {:?} -> {:?}",
            first, settled
        );
        assert!(
            (settled.unwrap() - 1.0).abs() < 0.01,
            "emphasis must settle at the full press target (1.0), got {:?}",
            settled
        );
    }

    /// Root with a `ScrollView` over content taller than the viewport — the
    /// real integration point for D108/Phase 26 Step 2 (`ctx.on_press_at`
    /// drag-pan -> `ScrollController::apply_momentum`, `ctx.pressed()` ->
    /// `ScrollController::coast`), driven through the actual `engine.rs`
    /// MouseDown/MouseMove/MouseUp dispatch, not a controller-level unit test.
    struct TallScroll;
    impl Component for TallScroll {
        fn build(&self, _ctx: &mut Context) -> Element {
            // Content taller than `MAX_TL_DIM` (4096) keeps plain
            // `ScrollView::new` on the base (CPU) path automatically
            // (`should_auto_gpu` requires `extent <= MAX_TL_DIM`) — the
            // GPU-layer path is explicitly out of scope for Step 2's
            // drag/momentum (see `.steering/PHASE_26.md`), so this avoids
            // silently exercising the wrong path.
            tezzera_widgets::tree::ScrollView::new(tezzera_widgets::tree::Spacer::gap(200.0, 5000.0))
                .into_element()
        }
    }

    fn headless_scroll_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(TallScroll), tezzera_render::FontCache::embedded());
        (engine, SkiaCanvas::new(200, 400), SkiaCanvas::new(200, 400))
    }

    fn scroll_offset(engine: &FrameEngine) -> Option<[f32; 2]> {
        engine.render_tree.borrow().nodes_iter().find_map(|n| n.scroll_ctrl.as_ref().map(|c| c.offset()))
    }

    #[test]
    fn drag_pans_content_and_momentum_coasts_after_release() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        tezzera_animate::set_frame_dt(0.05);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(scroll_offset(&engine), Some([0.0, 0.0]));

        let down = tezzera_platform::InputEvent::MouseDown {
            x: 100.0, y: 300.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);

        // Drag upward (finger/cursor moves to a smaller y) — content should
        // follow, increasing the scroll offset, same as a real touch/mouse
        // drag on any platform.
        let move1 = tezzera_platform::InputEvent::MouseMove { x: 100.0, y: 260.0 };
        engine.paint(&mut canvas, &mut overlay, &[move1]);
        let after_first_move = scroll_offset(&engine).unwrap();
        assert!(after_first_move[1] > 0.0, "dragging up must increase the scroll offset, got {after_first_move:?}");

        let move2 = tezzera_platform::InputEvent::MouseMove { x: 100.0, y: 220.0 };
        engine.paint(&mut canvas, &mut overlay, &[move2]);
        let after_second_move = scroll_offset(&engine).unwrap();
        assert!(
            after_second_move[1] > after_first_move[1],
            "continued drag must keep increasing offset: {after_first_move:?} -> {after_second_move:?}"
        );

        let up = tezzera_platform::InputEvent::MouseUp {
            x: 100.0, y: 220.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[up]);
        let at_release = scroll_offset(&engine).unwrap();

        // Coast for several more frames with no new input — real momentum,
        // tracked from the actual drag speed, must carry it further, not
        // stop dead at release.
        for _ in 0..10 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let after_coast = scroll_offset(&engine).unwrap();
        assert!(
            after_coast[1] > at_release[1],
            "momentum must carry the offset further after release: {at_release:?} -> {after_coast:?}"
        );
    }

    #[test]
    fn wheel_scroll_does_not_coast_on_its_own_once_events_stop() {
        // D108/Phase 26 Step 2, revised after real trackpad testing: wheel
        // input applies its delta directly and does NOT inject a synthetic
        // velocity for `coast` to keep decaying. Confirmed via winit's own
        // macOS backend source: a trackpad's coast feel is largely the OS's
        // OWN native momentum-phase event stream (`NSEvent.momentumPhase`),
        // which winit collapses into the same `TouchPhase::Moved` as real
        // finger movement — no reliable way to tell them apart from the
        // event alone. An earlier version had TEZZERA inject its OWN
        // momentum on top of wheel input too, which fought the OS's tail:
        // confirmed via a real screen recording, frame-by-frame — settled
        // at the bottom, then overscrolled again on its own a second later,
        // then re-settled — a genuine oscillation, not a one-off glitch.
        // This test proves the fix: once wheel events stop, the offset
        // does NOT keep moving on its own (in-bounds, no coast source left
        // to conflict with the OS's real momentum-phase stream).
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dt = 1.0 / 60.0;
        tezzera_animate::set_frame_dt(dt);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        // A burst of wheel events, one per frame, simulating an active
        // trackpad scroll gesture in progress. Small deltas, well within
        // bounds (content is 5000px tall, viewport 400px) — no overscroll.
        for _ in 0..15 {
            let scroll = tezzera_platform::InputEvent::Scroll {
                x: 100.0, y: 200.0, delta_x: 0.0, delta_y: -8.0,
            };
            engine.paint(&mut canvas, &mut overlay, &[scroll]);
        }
        let at_burst_end = scroll_offset(&engine).unwrap();
        assert!(at_burst_end[1] > 0.0, "the burst itself must have moved the offset, got {at_burst_end:?}");

        // Fingers lift — no more Scroll events. Wait past the wheel-idle
        // grace period. In-bounds, so there's nothing to coast or spring
        // back from — the offset must stay exactly where the wheel deltas
        // left it, not keep drifting under its own synthetic momentum.
        for _ in 0..20 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let after_idle = scroll_offset(&engine).unwrap();
        assert_eq!(
            after_idle, at_burst_end,
            "in-bounds offset must not keep moving once wheel events stop: {at_burst_end:?} -> {after_idle:?}"
        );
    }

    #[test]
    fn wheel_scroll_still_springs_back_from_overscroll_once_idle_with_no_injected_velocity() {
        // Companion to the test above: removing wheel's synthetic velocity
        // must not also remove overscroll recovery. `coast`'s
        // already-overscrolled check runs independent of velocity, so a
        // wheel-driven overscroll (via `apply_momentum`'s own resistance)
        // still springs back once the gesture goes idle, even though no
        // velocity was ever tracked for it.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dt = 1.0 / 60.0;
        tezzera_animate::set_frame_dt(dt);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        // Scroll up past the top edge (negative delta direction pushes
        // toward 0 then past it) — many small events so resistance still
        // lets it go negative.
        for _ in 0..30 {
            let scroll = tezzera_platform::InputEvent::Scroll {
                x: 100.0, y: 200.0, delta_x: 0.0, delta_y: 8.0,
            };
            engine.paint(&mut canvas, &mut overlay, &[scroll]);
        }
        let at_burst_end = scroll_offset(&engine).unwrap();
        assert!(at_burst_end[1] < 0.0, "must be overscrolled above the top, got {at_burst_end:?}");

        // Wait past the wheel-idle grace period — spring-back should kick
        // in even with zero tracked velocity.
        for _ in 0..30 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let after_idle = scroll_offset(&engine).unwrap();
        assert!(
            after_idle[1] > at_burst_end[1] && after_idle[1] <= 0.0,
            "must have eased back toward the top bound (0), not stayed frozen at the overscroll: {at_burst_end:?} -> {after_idle:?}"
        );
    }

    #[test]
    fn disabling_animations_stops_coasting_immediately_on_release() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        tezzera_animate::set_frame_dt(0.05);
        tezzera_theme::provider::set_animations(false);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = tezzera_platform::InputEvent::MouseDown {
            x: 100.0, y: 300.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);
        let move1 = tezzera_platform::InputEvent::MouseMove { x: 100.0, y: 220.0 };
        engine.paint(&mut canvas, &mut overlay, &[move1]);
        let up = tezzera_platform::InputEvent::MouseUp {
            x: 100.0, y: 220.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[up]);
        let at_release = scroll_offset(&engine).unwrap();

        for _ in 0..10 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let after = scroll_offset(&engine).unwrap();
        assert_eq!(after, at_release, "no coast at all once animations are disabled");

        tezzera_theme::provider::set_animations(true); // don't leak into other tests
    }

    // ── D108/Phase 26 Step 3: nav transitions ──────────────────────────────

    #[derive(Clone, Copy, PartialEq)]
    enum NavScreen { A, B }

    /// Root with a two-screen `ScreenNav`, matching the real `tzr new`
    /// codegen shape exactly (`ScreenTransitionView::new(body, outgoing,
    /// nav.transition_handle())` in place of handing `body` straight to a
    /// container) — the real integration point for Step 3. Both screens are
    /// `Button`s (not bare `Text`) so both always declare real `Semantics`
    /// regardless of `on_press`, giving the test a reliable signal for
    /// "is this screen's content actually painted this frame."
    struct NavRoot;
    impl Component for NavRoot {
        fn build(&self, ctx: &mut Context) -> Element {
            let nav = tezzera_nav::ScreenNav::new(ctx, NavScreen::A);
            let build_screen = {
                let nav = nav.clone();
                move |s: NavScreen| -> tezzera_widgets::tree::BoxedWidget {
                    match s {
                        NavScreen::A => {
                            let nav = nav.clone();
                            Box::new(Button::new("Screen A").on_press(move || { nav.push(NavScreen::B); }))
                        }
                        NavScreen::B => Box::new(Button::new("Screen B")),
                    }
                }
            };
            let screen = nav.current().unwrap_or(NavScreen::A);
            let body = build_screen(screen);
            let outgoing = nav.previous().map(build_screen);
            tezzera_widgets::tree::ScreenTransitionView::new(body, outgoing, nav.transition_handle())
                .into_element()
        }
    }

    fn headless_nav_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(NavRoot), tezzera_render::FontCache::embedded());
        (engine, SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200))
    }

    fn semantic_labels(engine: &FrameEngine) -> Vec<String> {
        fn walk(node: &tezzera_core::SemanticNode, out: &mut Vec<String>) {
            if let Some(l) = &node.label { out.push(l.clone()); }
            for c in &node.children { walk(c, out); }
        }
        let mut out = Vec::new();
        walk(&engine.semantics(), &mut out);
        out
    }

    #[test]
    fn push_paints_both_screens_mid_transition_then_settles_to_only_the_incoming_one() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        tezzera_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_nav_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        let initial = semantic_labels(&engine);
        assert!(initial.iter().any(|l| l == "Screen A"), "must start on Screen A, got {initial:?}");
        assert!(!initial.iter().any(|l| l == "Screen B"), "Screen B must not exist yet, got {initial:?}");

        // Click "Screen A" — its rect is the whole 300x200 canvas (root
        // fills it under tight constraints, same pattern every other
        // engine test in this file uses).
        let down = tezzera_platform::InputEvent::MouseDown {
            x: 150.0, y: 100.0, button: tezzera_platform::MouseButton::Left,
        };
        let up = tezzera_platform::InputEvent::MouseUp {
            x: 150.0, y: 100.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down, up]);

        // Next frame: the transition is active, ScreenTransitionView paints
        // BOTH the outgoing (Screen A) and incoming (Screen B) widgets —
        // real proof `nav.push` -> `ScreenTransitionView` actually wired up,
        // not just that the stack changed.
        engine.paint(&mut canvas, &mut overlay, &[]);
        let mid = semantic_labels(&engine);
        assert!(mid.iter().any(|l| l == "Screen A"), "outgoing Screen A must still be painted mid-transition, got {mid:?}");
        assert!(mid.iter().any(|l| l == "Screen B"), "incoming Screen B must be painted mid-transition, got {mid:?}");

        // Let the spring settle — many frames, matching the pattern used to
        // settle `ScreenTransition` in its own unit tests.
        for _ in 0..120 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let settled = semantic_labels(&engine);
        assert!(settled.iter().any(|l| l == "Screen B"), "must have settled showing Screen B, got {settled:?}");
        assert!(!settled.iter().any(|l| l == "Screen A"), "outgoing Screen A must be gone once settled, got {settled:?}");
    }

    #[test]
    fn push_is_instant_with_no_double_paint_when_animations_are_disabled() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        tezzera_theme::provider::set_animations(false);
        tezzera_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_nav_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = tezzera_platform::InputEvent::MouseDown {
            x: 150.0, y: 100.0, button: tezzera_platform::MouseButton::Left,
        };
        let up = tezzera_platform::InputEvent::MouseUp {
            x: 150.0, y: 100.0, button: tezzera_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down, up]);
        engine.paint(&mut canvas, &mut overlay, &[]);

        let labels = semantic_labels(&engine);
        assert!(labels.iter().any(|l| l == "Screen B"), "must show Screen B immediately, got {labels:?}");
        assert!(!labels.iter().any(|l| l == "Screen A"), "must NOT still paint Screen A when animations are disabled, got {labels:?}");

        tezzera_theme::provider::set_animations(true); // don't leak into other tests
    }
}
