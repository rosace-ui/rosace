//! `FrameEngine` — the per-frame build/paint/input logic, extracted from
//! `App::launch`'s `run_layered` closure (Phase 24 Step 1, D106).
//!
//! This is a behavior-preserving extraction: the desktop/web path
//! (`App::launch` → `PlatformWindow::run_layered`) drives it exactly as
//! before. The point is to make the same logic drivable from a second place
//! — a native-host FFI boundary (`rosace-ffi`) that has no winit event loop
//! at all — without duplicating ~450 lines of reconciler/paint/input code.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use rosace_core::Component;
use rosace_core::types::Rect;
use rosace_render::SkiaCanvas;
use rosace_widgets::tree::{clear_overlays, drain_overlays, text_edit, NodeId};
use rosace_clipboard::ClipboardProvider as _;

use crate::{inflate_rect, rect_contains, theme_color, walk_element, OverlayRoute};

/// Translate a physical key + modifiers into a [`text_edit::Command`]
/// (D116 layer 4 — the abstract vocabulary a keymap produces). `word_mod`
/// is Alt (macOS's Option convention) OR Ctrl (Linux/Windows) — not
/// OS-branched, same spirit as the Cmd/Ctrl clipboard shortcuts. Lives
/// here rather than in `rosace-widgets::text_edit` because it needs
/// `rosace_platform::Key`, a lower layer that crate doesn't depend on;
/// the Command vocabulary itself stays platform-agnostic so a future
/// widget could construct/dispatch commands without touching `Key` at
/// all. Returns `None` for any key with no editing meaning.
fn command_for_key(key: rosace_platform::Key, shift: bool, word_mod: bool) -> Option<text_edit::Command> {
    use rosace_platform::Key;
    use text_edit::Command::*;
    Some(match key {
        Key::ArrowLeft if word_mod => if shift { ExtendWordLeft } else { MoveWordLeft },
        Key::ArrowLeft => if shift { ExtendLeft } else { MoveLeft },
        Key::ArrowRight if word_mod => if shift { ExtendWordRight } else { MoveWordRight },
        Key::ArrowRight => if shift { ExtendRight } else { MoveRight },
        Key::Home => if shift { ExtendHome } else { MoveHome },
        Key::End => if shift { ExtendEnd } else { MoveEnd },
        Key::Backspace if word_mod => DeleteWordBack,
        Key::Backspace => Backspace,
        Key::Delete if word_mod => DeleteWordForward,
        Key::Delete => DeleteForward,
        _ => return None,
    })
}

/// Owns everything that must persist across frames: the root component,
/// reconciler caches, focus state, drag/long-press state, and the persistent
/// render tree (D091). One [`FrameEngine`] per running app instance.
pub struct FrameEngine {
    root: Box<dyn Component>,
    font: rosace_render::FontCache,

    // ── Reconciler state — persists across frames ──────────────────────
    prev_mounted: HashSet<u64>,
    element_cache: HashMap<u64, rosace_core::Element>,
    render_tree: Rc<RefCell<rosace_widgets::tree::RenderTree>>,

    // ── Focus + input state ─────────────────────────────────────────────
    focus_manager: rosace_a11y::FocusManager,
    shift_held: bool,
    /// Held-modifier state for text-editing shortcuts (D112/Phase 28 Step
    /// 1) — mirrors `shift_held`. Cmd/Ctrl+A/C/X/V trigger on EITHER
    /// being held (`ctrl_held || meta_held`), covering macOS's Cmd
    /// convention and Linux/Windows's Ctrl convention without branching
    /// on target OS.
    ctrl_held: bool,
    meta_held: bool,
    /// Word-navigation modifier (Alt/Ctrl+Arrow, D116 Step 2) — mirrors
    /// `shift_held`. `ctrl_held` alone already triggers word movement too
    /// (see `command_for_key`'s `word_mod`); this exists so macOS's
    /// Option-key convention works without requiring Ctrl.
    alt_held: bool,
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
    pub fn new(root: Box<dyn Component>, font: rosace_render::FontCache) -> Self {
        rosace_state::reset_to_global_dirty();
        Self {
            root,
            font,
            prev_mounted: HashSet::new(),
            element_cache: HashMap::new(),
            render_tree: Rc::new(RefCell::new(rosace_widgets::tree::RenderTree::new())),
            focus_manager: rosace_a11y::FocusManager::new(),
            shift_held: false,
            ctrl_held: false,
            meta_held: false,
            alt_held: false,
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
    /// `rosace-web-seo`'s `render_html`) — a headless caller can call
    /// `paint()` once into a throwaway `SkiaCanvas` purely to populate the
    /// render tree, then read this, with no real window/GPU needed
    /// (`SkiaCanvas` is a plain in-memory CPU pixmap).
    pub fn semantics(&self) -> rosace_core::SemanticNode {
        self.render_tree.borrow().collect_semantics()
    }

    // ── Text editing dispatch (D112/Phase 28 Step 1) ────────────────────
    //
    // `TextInput::paint` can't mutate its own render-tree node (`paint`
    // takes `&self`) and a click/key callback can't capture the render
    // tree or `FontCache` (both fail `on_press_at`'s `Send + Sync` bound —
    // `Rc<RefCell<_>>` and fontdue's internal `RefCell` caches are
    // neither). So, like `pressed`/`hovered` before it, real text editing
    // is DISPATCHER-owned: the engine looks up the focused editable node
    // directly and mutates its persistent `text_edit` state here.

    /// The render-tree node behind the currently focused widget, if it
    /// declared itself editable this paint. `None` when nothing is
    /// focused, the focused thing isn't editable (a focused `Button`,
    /// say), or its `FocusNode` is stale (shouldn't happen post-sync, but
    /// cheap to guard).
    fn focused_editable(&self) -> Option<(NodeId, String, text_edit::TextEditState, bool)> {
        let focused_id = self.focus_manager.focused?;
        let tree = self.render_tree.borrow();
        let node_id = tree.focus_owner(focused_id)?;
        let n = tree.node(node_id);
        let e = n.editable.as_ref()?;
        Some((node_id, e.value.clone(), n.text_edit.clone(), e.multiline))
    }

    /// Write a computed `(new_value, new_state)` back: persists the
    /// caret/selection to the render tree, reports the value upward via
    /// `on_change` ONLY when it actually changed (a pure cursor move must
    /// not fire `on_change` — it would spuriously re-notify the app with
    /// an unchanged string every arrow-key press), publishes the node's
    /// `EditController` snapshot if it has one (D116 — so a toolbar
    /// reading `controller.value()`/`.selection()` sees the LATEST real
    /// state regardless of whether the edit came from the keyboard or a
    /// prior controller call; this is the ONLY path every edit source
    /// funnels through, so it's the one place that can guarantee that),
    /// and forces a repaint — required even for a value-less move, to
    /// show the caret land.
    fn commit_text_edit(
        &mut self, node_id: NodeId, old_value: &str,
        new_value: String, new_state: text_edit::TextEditState,
    ) {
        let selection = new_state.selection.clone();
        let (on_change, controller) = {
            let mut tree = self.render_tree.borrow_mut();
            tree.node_mut(node_id).text_edit = new_state;
            let editable = tree.node(node_id).editable.as_ref();
            let on_change = if new_value != old_value {
                editable.map(|e| e.on_change.clone())
            } else {
                None
            };
            let controller = editable.and_then(|e| e.controller.clone());
            (on_change, controller)
        };
        if let Some(c) = &controller {
            c.update_snapshot(new_value.clone(), selection);
        }
        if let Some(cb) = on_change {
            cb(new_value);
        }
        self.forced_repaint = true;
        rosace_state::request_frame();
    }

    /// Drain every editable node's [`text_edit::EditController`] pending
    /// ops (D116) and apply them — independent of `focus_manager`, since a
    /// controller is reachable from OUTSIDE the widget tree entirely (a
    /// toolbar button has no render-tree node of its own to route
    /// through). Collects `(NodeId, controller, ops)` in one immutable
    /// pass first — can't mutate the tree while iterating it.
    fn drain_controllers(&mut self) {
        let pending: Vec<(NodeId, Vec<text_edit::ControllerOp>)> = {
            let tree = self.render_tree.borrow();
            tree.nodes_indexed()
                .filter_map(|(id, n)| {
                    let c = n.editable.as_ref()?.controller.as_ref()?;
                    let ops = c.take_ops();
                    if ops.is_empty() { None } else { Some((id, ops)) }
                })
                .collect()
        };
        for (node_id, ops) in pending {
            for op in ops {
                self.apply_controller_op(node_id, op);
            }
        }
    }

    /// Apply one [`text_edit::ControllerOp`] to `node_id` via the exact
    /// same commit path keyboard dispatch uses, which also publishes the
    /// node's controller snapshot (so `.value()`/`.selection()` read
    /// back correctly on the app's very next call, not one frame late).
    fn apply_controller_op(&mut self, node_id: NodeId, op: text_edit::ControllerOp) {
        let (value, state) = {
            let tree = self.render_tree.borrow();
            let n = tree.node(node_id);
            let Some(e) = &n.editable else { return; };
            (e.value.clone(), n.text_edit.clone())
        };
        let now = rosace_widgets::tree::anim_clock();
        let result = match op {
            text_edit::ControllerOp::ReplaceRange(s, e, text) =>
                Some(text_edit::replace_range(&value, &state, s, e, &text, now)),
            text_edit::ControllerOp::InsertAtCursor(text) =>
                Some(text_edit::insert_str(&value, &state, &text, now)),
            text_edit::ControllerOp::SetSelection(sel) =>
                Some((value.clone(), state.with_selection(sel, now))),
            text_edit::ControllerOp::SelectAll =>
                Some((value.clone(), text_edit::select_all(&value, &state, now))),
            text_edit::ControllerOp::Undo => text_edit::undo(&value, &state, now),
            text_edit::ControllerOp::Redo => text_edit::redo(&value, &state, now),
        };
        // `commit_text_edit` already publishes the node's controller
        // snapshot (looked up from `editable.controller` itself) — no
        // separate update needed here.
        if let Some((new_value, new_state)) = result {
            self.commit_text_edit(node_id, &value, new_value, new_state);
        }
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
        events: &[rosace_platform::InputEvent],
    ) -> bool {
        let root = &self.root;
        let font = &self.font;

        // ── Drain dirty-component set for this frame ───────────────────
        let global_dirty = rosace_state::is_global_dirty();
        let dirty_ids = rosace_state::take_dirty_components();
        let content_changed = global_dirty || !dirty_ids.is_empty();

        // ── Build root (only when dirty) ────────────────────────────────
        //
        // The root component (ComponentId(0)) owns all atoms created via
        // ctx.state(). When any of those atoms change, ComponentId(0) lands
        // in dirty_ids. We rebuild ONLY then; on clean frames the cached
        // element is reused, keeping `build()` side-effects out of the
        // render loop (e.g. an atom.set() inside build() would otherwise
        // cause an infinite loop).
        let root_component_id = rosace_core::types::ComponentId(0);
        let root_is_dirty = global_dirty || dirty_ids.contains(&root_component_id);

        let element = if root_is_dirty || !self.element_cache.contains_key(&0) {
            let mut ctx = rosace_core::Context::new(root_component_id);
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
        let current_theme = rosace_theme::use_theme();

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
            e, rosace_platform::InputEvent::WindowResized { .. }
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
        // GPU-shapes mode (D109/Phase 27) is ALWAYS a full repaint: the
        // frame is re-expressed as ordered items (quads + segments) from
        // the full picture each paint — damage-scoped pixel clearing is a
        // CPU-buffer economy that doesn't apply (frame-skip still does,
        // via `needs_paint` above).
        let full_repaint = global_dirty || window_resized || !canvas.has_drawn()
            || canvas.gpu_shapes();
        let bg = theme_color(&current_theme.colors.background);

        // ── Set up main display-list recording ──────────────────────────
        let mut recorder = rosace_render::PictureRecorder::new();

        // Begin the persistent render tree frame (D091). Repainted
        // nodes re-declare their regions; skipped subtrees keep theirs.
        self.render_tree.borrow_mut().start_frame();
        let mut paint_ctx = rosace_widgets::tree::PaintCtx {
            recorder: &mut recorder,
            rect: rosace_core::types::Rect {
                origin: rosace_core::types::Point { x: 0.0, y: 0.0 },
                size: rosace_core::types::Size { width: win_w, height: win_h },
            },
            font,
            theme: current_theme.clone(),
            tree: Rc::clone(&self.render_tree),
            node: rosace_widgets::tree::RenderTree::ROOT,
            owner: root_component_id,
            clip_rect: None,
        };

        let constraints = rosace_layout::Constraints::tight(win_w, win_h);

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
        if rosace_widgets::tree::take_animation_request() {
            self.forced_repaint = true;
            rosace_state::request_frame();
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
            let cid = rosace_core::types::ComponentId(id);
            root.on_mount();
            #[cfg(debug_assertions)]
            {
                use rosace_trace::{event::RosaceTrace, location, trace};
                trace!(RosaceTrace::ComponentMount {
                    id: cid,
                    name: root.type_name(),
                    location: location!(),
                });
            }
            let _ = cid;
        }
        for &id in self.prev_mounted.difference(&new_mounted) {
            let cid = rosace_core::types::ComponentId(id);
            rosace_state::cleanup_store::fire_and_clear(cid);
            rosace_state::clear_component(cid);
            root.on_unmount();
            #[cfg(debug_assertions)]
            {
                use rosace_trace::{event::RosaceTrace, trace};
                trace!(RosaceTrace::ComponentUnmount {
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
            use rosace_core::types::{Point, Rect, Size};
            use rosace_widgets::tree::LayerPosition;

            let tree_ref = self.render_tree.borrow();
            let overlay_ids = tree_ref.overlay_ids();

            if !overlay_ids.is_empty() || !legacy_overlays.is_empty() {
                let mut ov_recorder = rosace_render::PictureRecorder::new();

                let entries = overlay_ids.iter()
                    .map(|&(n, i)| &tree_ref.node(n).overlays[i])
                    .chain(legacy_overlays.iter());

                for entry in entries {
                    if let Some(scrim) = &entry.scrim {
                        let scrim_rect = Rect {
                            origin: Point { x: 0.0, y: 0.0 },
                            size: Size { width: win_w, height: win_h },
                        };
                        ov_recorder.push(rosace_render::DrawCommand::FillRect {
                            rect: scrim_rect,
                            color: scrim.color,
                        });
                    }

                    let loose_c = rosace_layout::Constraints::loose(win_w, win_h);
                    let lctx = rosace_widgets::tree::LayoutCtx::new(
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
                        rosace_widgets::tree::RenderTree::new(),
                    ));
                    let mut ov_ctx = rosace_widgets::tree::PaintCtx::root(
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
            let mut scroll_layers: Vec<rosace_platform::ScrollLayer> = Vec::new();
            let tree_ref = self.render_tree.borrow();
            for (n, i) in tree_ref.transform_ids() {
                let entry = &tree_ref.node(n).transforms[i];
                let vp = entry.viewport_rect;

                // Content texture = child natural size at physical
                // resolution, capped. Pixmap starts transparent, so
                // areas the content does not cover reveal the base.
                let cw = (((entry.child_size.width  * scale).ceil() as u32)).clamp(1, MAX_TL_DIM);
                let ch = (((entry.child_size.height * scale).ceil() as u32)).clamp(1, MAX_TL_DIM);
                let mut content = rosace_render::SkiaCanvas::new_hidpi(cw, ch, scale);
                // GPU-shapes mode propagates to scroll content (D109 C2):
                // shapes become quads, text becomes segments, and the
                // compositor renders them into the offscreen scroll
                // texture — no full content-buffer CPU raster or copy.
                content.set_gpu_shapes(canvas.gpu_shapes());
                content.play_picture(&entry.picture, font);

                let (pixels, items) = if canvas.gpu_shapes() {
                    (Vec::new(), content.take_frame_items())
                } else {
                    (content.pixels().to_vec(), Vec::new())
                };
                scroll_layers.push(rosace_platform::ScrollLayer {
                    id: n as u64,
                    pixels,
                    width:  cw,
                    height: ch,
                    dest: (
                        vp.origin.x * scale, vp.origin.y * scale,
                        vp.size.width * scale, vp.size.height * scale,
                    ),
                    items,
                });
            }
            drop(tree_ref);
            rosace_platform::publish_scroll_layers(scroll_layers);
        }


        // ── Sync focus manager from the render tree ─────────────────────
        // Collected from persistent nodes, so the Tab cycle survives
        // cache-hit frames where no widget repainted.
        self.focus_manager.sync_from_nodes(self.render_tree.borrow().collect_focus());

        // ── Drain EditController ops (D116) ──────────────────────────────
        // Runs every frame, independent of focus/events — a toolbar
        // button's `on_press` enqueues onto the controller from OUTSIDE
        // the widget tree entirely (see `EditController`'s doc comment),
        // so this is the only place those ops actually apply.
        self.drain_controllers();

        // ── Route events — structural z-order (D092) ────────────────────
        // Overlay routes first (topmost entry first): the entry's own
        // regions win; its surface absorbs; outside taps fire the scrim
        // dismiss or are swallowed by Block; PassThrough falls through.
        // Anything unclaimed goes to the render-tree walk, where later
        // siblings (painted on top) win structurally.
        for event in events {
            match event {
                rosace_platform::InputEvent::MouseDown {
                    x, y, button: rosace_platform::MouseButton::Left
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
                        if route.input == rosace_widgets::tree::InputBehavior::Block {
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
                        // Click-to-focus for editable widgets (D112/Phase
                        // 28 Step 1) — independent of the hit_test above:
                        // an editable widget doesn't register a plain hit
                        // callback (mutating the caret needs the render
                        // tree directly, unreachable from a captured
                        // Send+Sync closure — see EditableDecl's doc
                        // comment), it's found via its own declared rect.
                        // Scoped to the same `!handled` fallback as
                        // hit_test above; an editable inside a modal
                        // dialog's own overlay route is a follow-up once
                        // forms/dialogs are exercised together (Step 4).
                        let editable_hit = self.render_tree.borrow().editable_test(*x, *y);
                        if let Some(node_id) = editable_hit {
                            // Route through FocusManager (`focus_specific`),
                            // not a raw `FocusNode::request()` — the manager
                            // owns the "exactly one focused at a time"
                            // invariant AND is the source `focused_editable`
                            // reads from; calling `.request()` directly sets
                            // only that node's own reactive flag, leaving
                            // `FocusManager.focused` (and thus every later
                            // keystroke's target lookup) unset. Found the
                            // hard way — this exact gap is why the first
                            // pass at this dispatch never actually typed
                            // anything, caught by the headless integration
                            // tests below, not by eyeballing a screenshot.
                            let focus_id = self.render_tree.borrow()
                                .node(node_id).focus_node.as_ref().map(|f| f.id());
                            if let Some(fid) = focus_id {
                                self.focus_manager.focus_specific(fid);
                            }
                            let now = rosace_widgets::tree::anim_clock();
                            let mut tree = self.render_tree.borrow_mut();
                            let node = tree.node_mut(node_id);
                            // Step 1 scoping (still true post-D116; Step 3
                            // is the named follow-up): place the caret at
                            // the end on click, not at the clicked glyph
                            // — precise click->position needs font
                            // metrics, and `FontCache` can't cross into
                            // this dispatch path (its internal `RefCell`
                            // caches are `!Sync`).
                            if let Some(editable) = &node.editable {
                                let end = text_edit::char_count(&editable.value);
                                node.text_edit.selection = text_edit::Selection::single(end);
                            }
                            node.text_edit.last_edit_at = now;
                            drop(tree);
                            self.forced_repaint = true;
                            rosace_state::request_frame();
                        } else if self.focus_manager.focused.is_some() {
                            // Clicking truly blank space blurs whatever was
                            // focused — standard desktop convention, and the
                            // only way a caret ever stops blinking in a
                            // field the user clicked away from (Tab-cycling
                            // already unfocuses cleanly via FocusManager;
                            // this covers the mouse path).
                            self.focus_manager.blur();
                        }
                    }
                    // Press/tap feedback (D108/Phase 26 Step 1): mirror hover
                    // resolution at the moment of MouseDown, held until
                    // MouseUp regardless of small cursor drift meanwhile.
                    let press_target = self.render_tree.borrow().hover_test(*x, *y);
                    if self.render_tree.borrow_mut().set_pressed(press_target) {
                        self.forced_repaint = true;
                        rosace_state::request_frame();
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
                                rosace_state::request_frame();
                            }
                        });
                    }
                }
                rosace_platform::InputEvent::MouseMove { x, y } => {
                    use std::sync::atomic::Ordering;
                    if let Some(cb) = &self.active_drag {
                        cb(*x, *y);
                    }
                    // Hover tracking — repaints only the changed nodes.
                    let target = self.render_tree.borrow().hover_test(*x, *y);
                    let changed = self.render_tree.borrow_mut().set_hover(target);
                    if changed {
                        self.forced_repaint = true;
                        rosace_state::request_frame();
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
                rosace_platform::InputEvent::MouseUp { .. } => {
                    use std::sync::atomic::Ordering;
                    self.active_drag = None;
                    if let Some(c) = &self.lp_cancel { c.store(true, Ordering::Relaxed); }
                    self.lp_cancel = None;
                    self.press_origin = None;
                    if self.render_tree.borrow_mut().set_pressed(None) {
                        self.forced_repaint = true;
                        rosace_state::request_frame();
                    }
                }
                rosace_platform::InputEvent::Scroll { x, y, delta_x, delta_y } => {
                    let mut handled = false;
                    for route in overlay_routes.iter().rev() {
                        let candidates: Vec<_> = route.scrolls.iter().rev()
                            .filter(|(r, _, _)| rect_contains(r, *x, *y))
                            .map(|(_, a, cb)| (*a, cb.clone()))
                            .collect();
                        if let Some(cb) = rosace_widgets::tree::render_tree::select_scroll_handler(
                            &candidates, *delta_x, *delta_y,
                        ) {
                            cb(*delta_x, *delta_y);
                            handled = true;
                            break;
                        }
                        if rect_contains(&route.rect, *x, *y)
                            && route.input == rosace_widgets::tree::InputBehavior::Block
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
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Escape
                } => {
                    // Dismiss the topmost overlay that has a scrim
                    // dismisser (dialog, sheet, dropdown).
                    if let Some(on_tap) = overlay_routes.iter().rev()
                        .find_map(|r| r.on_tap.clone())
                    {
                        on_tap();
                    }
                }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Tab
                } => {
                    if self.shift_held {
                        self.focus_manager.focus_prev_node();
                    } else {
                        self.focus_manager.focus_next_node();
                    }
                }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Shift
                } => { self.shift_held = true; }
                rosace_platform::InputEvent::KeyUp {
                    key: rosace_platform::Key::Shift
                } => { self.shift_held = false; }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Control
                } => { self.ctrl_held = true; }
                rosace_platform::InputEvent::KeyUp {
                    key: rosace_platform::Key::Control
                } => { self.ctrl_held = false; }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Meta
                } => { self.meta_held = true; }
                rosace_platform::InputEvent::KeyUp {
                    key: rosace_platform::Key::Meta
                } => { self.meta_held = false; }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Alt
                } => { self.alt_held = true; }
                rosace_platform::InputEvent::KeyUp {
                    key: rosace_platform::Key::Alt
                } => { self.alt_held = false; }

                // ── Text editing (D112/Phase 28, Command layer D116) ────
                // Literal character insertion goes through `Text`, NOT
                // `KeyDown{Char}` — `Text` is winit's already-composed,
                // layout/shift-aware source (a `KeyDown{Char('a')}` fires
                // ALONGSIDE `Text{'a'}` for every plain letter today, so
                // handling both would double-insert). Gated off entirely
                // while Ctrl/Meta is held, in case a platform still
                // populates `event.text` for a modified key — belt and
                // braces against accidentally typing a shortcut's letter.
                rosace_platform::InputEvent::Text { character } => {
                    if !self.ctrl_held && !self.meta_held && !character.is_control() {
                        if let Some((node_id, value, state, _)) = self.focused_editable() {
                            let now = rosace_widgets::tree::anim_clock();
                            let (nv, ns) = text_edit::insert_char(&value, &state, *character, now);
                            self.commit_text_edit(node_id, &value, nv, ns);
                        }
                    }
                }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Char(c)
                } => {
                    // Shortcut letters ONLY — plain typing is Text's job
                    // (see the comment above). Cmd (macOS) or Ctrl
                    // (Linux/Windows) triggers either way, deliberately
                    // not OS-branched. Must be matched BEFORE the generic
                    // `KeyDown { key }` arm below (Rust picks the first
                    // matching arm; that one is unconstrained and would
                    // otherwise swallow every `Char` too).
                    if self.ctrl_held || self.meta_held {
                        if let Some((node_id, value, state, multiline)) = self.focused_editable() {
                            let now = rosace_widgets::tree::anim_clock();
                            match c.to_ascii_lowercase() {
                                'a' => {
                                    if let Some((nv, ns)) = text_edit::apply_command(
                                        &value, &state, text_edit::Command::SelectAll, now,
                                    ) {
                                        self.commit_text_edit(node_id, &value, nv, ns);
                                    }
                                }
                                'c' => {
                                    if let Some(sel) = text_edit::selected_text(&value, &state) {
                                        let _ = rosace_clipboard::SystemClipboard::new().write(&sel);
                                    }
                                }
                                'x' => {
                                    if let Some(sel) = text_edit::selected_text(&value, &state) {
                                        let _ = rosace_clipboard::SystemClipboard::new().write(&sel);
                                        let (nv, ns) = text_edit::backspace(&value, &state, now);
                                        self.commit_text_edit(node_id, &value, nv, ns);
                                    }
                                }
                                'v' => {
                                    if let Some(text) = rosace_clipboard::SystemClipboard::new().read() {
                                        let clean: String = if multiline {
                                            text.chars().filter(|c| !c.is_control() || *c == '\n').collect()
                                        } else {
                                            text.chars().filter(|c| !c.is_control()).collect()
                                        };
                                        if !clean.is_empty() {
                                            let (nv, ns) = text_edit::insert_str(&value, &state, &clean, now);
                                            self.commit_text_edit(node_id, &value, nv, ns);
                                        }
                                    }
                                }
                                // Undo/Redo (D116 Step 2): Cmd/Ctrl+Z undoes;
                                // Shift+Cmd/Ctrl+Z OR Cmd/Ctrl+Y redoes —
                                // covering both common conventions rather
                                // than picking one, same "not OS-branched"
                                // spirit as the rest of this arm.
                                'z' if self.shift_held => {
                                    if let Some((nv, ns)) = text_edit::redo(&value, &state, now) {
                                        self.commit_text_edit(node_id, &value, nv, ns);
                                    }
                                }
                                'z' => {
                                    if let Some((nv, ns)) = text_edit::undo(&value, &state, now) {
                                        self.commit_text_edit(node_id, &value, nv, ns);
                                    }
                                }
                                'y' => {
                                    if let Some((nv, ns)) = text_edit::redo(&value, &state, now) {
                                        self.commit_text_edit(node_id, &value, nv, ns);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // Movement/deletion — one generic arm through the
                // Key->Command keymap (`command_for_key`, D116 layer 4)
                // instead of one match arm per key. Escape/Tab/Shift/
                // Control/Meta/Alt/Char already claimed their own events
                // above, so `key` here is only ever Backspace/Delete/an
                // arrow/Home/End/something unbound.
                rosace_platform::InputEvent::KeyDown { key } => {
                    let word_mod = self.alt_held || self.ctrl_held;
                    if let Some(cmd) = command_for_key(*key, self.shift_held, word_mod) {
                        if let Some((node_id, value, state, _)) = self.focused_editable() {
                            let now = rosace_widgets::tree::anim_clock();
                            if let Some((nv, ns)) = text_edit::apply_command(&value, &state, cmd, now) {
                                self.commit_text_edit(node_id, &value, nv, ns);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        content_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::{Component, Context, Element};
    use rosace_render::Color;
    use rosace_widgets::tree::{Button, ButtonVariant, Column, Container, HeroApi, PressApi, Widget};

    /// `rosace_theme::provider`'s theme is a process-wide `GlobalAtom` —
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
        let engine = FrameEngine::new(Box::new(OneButton), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(200, 60), SkiaCanvas::new(200, 60))
    }

    #[test]
    fn press_then_release_sets_and_clears_render_tree_pressed_state() {
        let (mut engine, mut canvas, mut overlay) = headless_engine();
        // First frame: build + layout, no events — populates hit regions.
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = rosace_platform::InputEvent::MouseDown {
            x: 100.0, y: 30.0, button: rosace_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);
        assert!(
            engine.render_tree.borrow().nodes_iter().any(|n| n.pressed),
            "MouseDown over the button must mark some node pressed"
        );

        let up = rosace_platform::InputEvent::MouseUp {
            x: 100.0, y: 30.0, button: rosace_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[up]);
        assert!(
            engine.render_tree.borrow().nodes_iter().all(|n| !n.pressed),
            "MouseUp must clear pressed state"
        );
    }

    #[test]
    fn press_eases_the_button_toward_full_emphasis_over_several_frames() {
        // `frame_dt` is ALSO process-global (`rosace_animate::set_frame_dt`)
        // — same lock as the animation-enabled tests, for the same reason:
        // another test setting a different frame_dt mid-run would corrupt
        // this one's convergence math. Found for real: adding the wheel
        // momentum test (which also sets frame_dt) made this test flaky
        // under `cargo test`'s parallel execution.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A deterministic synthetic frame_dt, not real wall-clock time
        // between fast test calls — otherwise convergence speed (and thus
        // this test's pass/fail) would depend on machine speed.
        rosace_animate::set_frame_dt(0.05);

        let (mut engine, mut canvas, mut overlay) = headless_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = rosace_platform::InputEvent::MouseDown {
            x: 100.0, y: 30.0, button: rosace_platform::MouseButton::Left,
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
            rosace_widgets::tree::ScrollView::new(rosace_widgets::tree::Spacer::gap(200.0, 5000.0))
                .into_element()
        }
    }

    fn headless_scroll_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(TallScroll), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(200, 400), SkiaCanvas::new(200, 400))
    }

    fn scroll_offset(engine: &FrameEngine) -> Option<[f32; 2]> {
        engine.render_tree.borrow().nodes_iter().find_map(|n| n.scroll_ctrl.as_ref().map(|c| c.offset()))
    }

    #[test]
    fn drag_pans_content_and_momentum_coasts_after_release() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_animate::set_frame_dt(0.05);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(scroll_offset(&engine), Some([0.0, 0.0]));

        let down = rosace_platform::InputEvent::MouseDown {
            x: 100.0, y: 300.0, button: rosace_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);

        // Drag upward (finger/cursor moves to a smaller y) — content should
        // follow, increasing the scroll offset, same as a real touch/mouse
        // drag on any platform.
        let move1 = rosace_platform::InputEvent::MouseMove { x: 100.0, y: 260.0 };
        engine.paint(&mut canvas, &mut overlay, &[move1]);
        let after_first_move = scroll_offset(&engine).unwrap();
        assert!(after_first_move[1] > 0.0, "dragging up must increase the scroll offset, got {after_first_move:?}");

        let move2 = rosace_platform::InputEvent::MouseMove { x: 100.0, y: 220.0 };
        engine.paint(&mut canvas, &mut overlay, &[move2]);
        let after_second_move = scroll_offset(&engine).unwrap();
        assert!(
            after_second_move[1] > after_first_move[1],
            "continued drag must keep increasing offset: {after_first_move:?} -> {after_second_move:?}"
        );

        let up = rosace_platform::InputEvent::MouseUp {
            x: 100.0, y: 220.0, button: rosace_platform::MouseButton::Left,
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
        // event alone. An earlier version had ROSACE inject its OWN
        // momentum on top of wheel input too, which fought the OS's tail:
        // confirmed via a real screen recording, frame-by-frame — settled
        // at the bottom, then overscrolled again on its own a second later,
        // then re-settled — a genuine oscillation, not a one-off glitch.
        // This test proves the fix: once wheel events stop, the offset
        // does NOT keep moving on its own (in-bounds, no coast source left
        // to conflict with the OS's real momentum-phase stream).
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dt = 1.0 / 60.0;
        rosace_animate::set_frame_dt(dt);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        // A burst of wheel events, one per frame, simulating an active
        // trackpad scroll gesture in progress. Small deltas, well within
        // bounds (content is 5000px tall, viewport 400px) — no overscroll.
        for _ in 0..15 {
            let scroll = rosace_platform::InputEvent::Scroll {
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
        rosace_animate::set_frame_dt(dt);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        // Scroll up past the top edge (negative delta direction pushes
        // toward 0 then past it) — many small events so resistance still
        // lets it go negative.
        for _ in 0..30 {
            let scroll = rosace_platform::InputEvent::Scroll {
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
        rosace_animate::set_frame_dt(0.05);
        rosace_theme::provider::set_animations(false);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = rosace_platform::InputEvent::MouseDown {
            x: 100.0, y: 300.0, button: rosace_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down]);
        let move1 = rosace_platform::InputEvent::MouseMove { x: 100.0, y: 220.0 };
        engine.paint(&mut canvas, &mut overlay, &[move1]);
        let up = rosace_platform::InputEvent::MouseUp {
            x: 100.0, y: 220.0, button: rosace_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[up]);
        let at_release = scroll_offset(&engine).unwrap();

        for _ in 0..10 {
            engine.paint(&mut canvas, &mut overlay, &[]);
        }
        let after = scroll_offset(&engine).unwrap();
        assert_eq!(after, at_release, "no coast at all once animations are disabled");

        rosace_theme::provider::set_animations(true); // don't leak into other tests
    }

    // ── D108/Phase 26 Step 3: nav transitions ──────────────────────────────

    #[derive(Clone, Copy, PartialEq)]
    enum NavScreen { A, B }

    /// Root with a two-screen `ScreenNav`, matching the real `rsc new`
    /// codegen shape exactly (`ScreenTransitionView::new(body, outgoing,
    /// nav.transition_handle())` in place of handing `body` straight to a
    /// container) — the real integration point for Step 3. Both screens are
    /// `Button`s (not bare `Text`) so both always declare real `Semantics`
    /// regardless of `on_press`, giving the test a reliable signal for
    /// "is this screen's content actually painted this frame."
    struct NavRoot;
    impl Component for NavRoot {
        fn build(&self, ctx: &mut Context) -> Element {
            let nav = rosace_nav::ScreenNav::new(ctx, NavScreen::A);
            let build_screen = {
                let nav = nav.clone();
                move |s: NavScreen| -> rosace_widgets::tree::BoxedWidget {
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
            rosace_widgets::tree::ScreenTransitionView::new(body, outgoing, nav.transition_handle())
                .into_element()
        }
    }

    fn headless_nav_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(NavRoot), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200))
    }

    fn semantic_labels(engine: &FrameEngine) -> Vec<String> {
        fn walk(node: &rosace_core::SemanticNode, out: &mut Vec<String>) {
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
        rosace_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_nav_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        let initial = semantic_labels(&engine);
        assert!(initial.iter().any(|l| l == "Screen A"), "must start on Screen A, got {initial:?}");
        assert!(!initial.iter().any(|l| l == "Screen B"), "Screen B must not exist yet, got {initial:?}");

        // Click "Screen A" — its rect is the whole 300x200 canvas (root
        // fills it under tight constraints, same pattern every other
        // engine test in this file uses).
        let down = rosace_platform::InputEvent::MouseDown {
            x: 150.0, y: 100.0, button: rosace_platform::MouseButton::Left,
        };
        let up = rosace_platform::InputEvent::MouseUp {
            x: 150.0, y: 100.0, button: rosace_platform::MouseButton::Left,
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
        rosace_theme::provider::set_animations(false);
        rosace_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_nav_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        let down = rosace_platform::InputEvent::MouseDown {
            x: 150.0, y: 100.0, button: rosace_platform::MouseButton::Left,
        };
        let up = rosace_platform::InputEvent::MouseUp {
            x: 150.0, y: 100.0, button: rosace_platform::MouseButton::Left,
        };
        engine.paint(&mut canvas, &mut overlay, &[down, up]);
        engine.paint(&mut canvas, &mut overlay, &[]);

        let labels = semantic_labels(&engine);
        assert!(labels.iter().any(|l| l == "Screen B"), "must show Screen B immediately, got {labels:?}");
        assert!(!labels.iter().any(|l| l == "Screen A"), "must NOT still paint Screen A when animations are disabled, got {labels:?}");

        rosace_theme::provider::set_animations(true); // don't leak into other tests
    }

    // ── D108/Phase 26 Step 4: image load-in fade ────────────────────────────

    /// A real, valid 1x1 PNG (red pixel) — same bytes already proven to
    /// decode correctly by `rosace-render`'s own `image_handle_from_valid_png`.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
        0x00, 0x90, 0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08,
        0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59,
        0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    struct OneImage;
    impl Component for OneImage {
        fn build(&self, _ctx: &mut Context) -> Element {
            rosace_widgets::tree::Image::bytes(TINY_PNG.to_vec())
                .width(50.0)
                .height(50.0)
                .into_element()
        }
    }

    fn headless_image_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(OneImage), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(100, 100), SkiaCanvas::new(100, 100))
    }

    // D111 corrects D108/Phase 26 Step 4's default image load-in fade: an
    // `animate_to`-driven per-node fade was bound to a `ListView` row's
    // positional slot, not the image's own identity (slots are reassigned
    // to different data as the visible window scrolls — see D111), so a
    // scrolled list showed the wrong image mid-fade or no fade at all.
    // `Image` now always renders at full opacity immediately; these tests
    // confirm that's true both with and without the global animation
    // toggle, i.e. this widget has no animation-dependent behavior at all.
    #[test]
    fn real_decoded_image_always_renders_at_full_opacity_immediately() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay) = headless_image_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert!(
            engine.render_tree.borrow().nodes_iter().all(|n| n.anim.is_none()),
            "Image must not drive any per-node animated scalar — no default fade"
        );
    }

    #[test]
    fn real_decoded_image_full_opacity_is_unaffected_by_the_animation_toggle() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_theme::provider::set_animations(false);
        let (mut engine, mut canvas, mut overlay) = headless_image_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert!(engine.render_tree.borrow().nodes_iter().all(|n| n.anim.is_none()));
        rosace_theme::provider::set_animations(true); // don't leak into other tests
    }

    // ── D108/Phase 26 Step 5: Hero/shared-element transitions ──────────────

    #[derive(Clone, Copy, PartialEq)]
    enum HeroScreen { List, Detail }

    /// A blue square hero-tagged "cover" — small (20x20) on `List`, large
    /// (80x80) on `Detail`, same tag on both, same shape `NavRoot` above
    /// uses (`ScreenTransitionView` fed `body`/`outgoing` built from
    /// `ScreenNav`). Wrapped in a `Column` so its `paint()` actually
    /// measures+positions it at its own declared size — `ScreenTransitionView`
    /// paints its children at the FULL viewport rect regardless of their own
    /// `layout()` (confirmed by reading `screen_transition_view.rs`), so an
    /// un-wrapped `Container` as root would just fill the whole canvas and
    /// give no size-morph signal to observe.
    struct HeroRoot;
    impl Component for HeroRoot {
        fn build(&self, ctx: &mut Context) -> Element {
            let nav = rosace_nav::ScreenNav::new(ctx, HeroScreen::List);
            let build_screen = {
                let nav = nav.clone();
                move |s: HeroScreen| -> rosace_widgets::tree::BoxedWidget {
                    match s {
                        HeroScreen::List => {
                            let nav = nav.clone();
                            Box::new(Column::new().child(
                                Container::new()
                                    .width(20.0)
                                    .height(20.0)
                                    .background(Color::rgb(0, 0, 255))
                                    .on_press(move || { nav.push(HeroScreen::Detail); })
                                    .hero_tag("cover"),
                            ))
                        }
                        HeroScreen::Detail => Box::new(Column::new().child(
                            Container::new()
                                .width(80.0)
                                .height(80.0)
                                .background(Color::rgb(0, 0, 255))
                                .hero_tag("cover"),
                        )),
                    }
                }
            };
            let screen = nav.current().unwrap_or(HeroScreen::List);
            let body = build_screen(screen);
            let outgoing = nav.previous().map(build_screen);
            rosace_widgets::tree::ScreenTransitionView::new(body, outgoing, nav.transition_handle())
                .into_element()
        }
    }

    fn headless_hero_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(HeroRoot), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200))
    }

    /// Count of pixels an exact, fully-opaque match for pure blue — a rough
    /// but real area measurement read straight off the actual rendered
    /// canvas (same rigor as `rosace-render`'s own `blit_rgba` pixel
    /// tests), not a render-tree-level assertion.
    fn blue_pixel_count(canvas: &SkiaCanvas) -> usize {
        canvas.pixels().chunks_exact(4)
            .filter(|p| p[0] == 0 && p[1] == 0 && p[2] == 255 && p[3] == 255)
            .count()
    }

    #[test]
    fn hero_tagged_widget_morphs_position_and_size_across_a_push_transition() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_hero_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        let list_area = blue_pixel_count(&canvas);
        assert!(list_area > 300 && list_area < 500, "List screen's 20x20 hero must render at its natural size outside any transition, got {list_area} px");

        // Click the hero-tagged Container itself (it carries `on_press`) —
        // Column top-aligns its single child at the Column's own origin
        // (0,0), so (10, 10) lands inside its 20x20 rect.
        let down = rosace_platform::InputEvent::MouseDown { x: 10.0, y: 10.0, button: rosace_platform::MouseButton::Left };
        let up = rosace_platform::InputEvent::MouseUp { x: 10.0, y: 10.0, button: rosace_platform::MouseButton::Left };
        engine.paint(&mut canvas, &mut overlay, &[down, up]);

        // Scan every frame of the flight for real evidence of interpolation:
        // some frame's blue area must land strictly between the two
        // screens' natural sizes (400 vs 6400 px), not jump straight from
        // one to the other in a single frame.
        let mut saw_intermediate = false;
        for _ in 0..90 {
            engine.paint(&mut canvas, &mut overlay, &[]);
            let area = blue_pixel_count(&canvas);
            if area > 600 && area < 6000 {
                saw_intermediate = true;
            }
        }
        assert!(saw_intermediate, "expected at least one frame with the hero mid-morph (blue area strictly between the 20x20 source and 80x80 destination), never saw one");

        // Settled: only the Detail screen's natural 80x80 size remains —
        // the floating morphed copy is gone, the real (no-longer-suppressed)
        // Detail-screen Container renders normally in its place.
        let detail_area = blue_pixel_count(&canvas);
        assert!(detail_area > 6000 && detail_area < 6800, "Detail screen's 80x80 hero must render at its natural size once settled, got {detail_area} px");
    }

    #[test]
    fn hero_tag_is_a_pass_through_with_no_active_transition() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_hero_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        // Steady state, never touched by a transition: renders at its own
        // declared 20x20 size, same as a plain (untagged) Container would.
        let area = blue_pixel_count(&canvas);
        assert!(area > 300 && area < 500, "a Hero-tagged widget outside any transition must render exactly like its untagged inner widget, got {area} px");
    }

    // ── Text editing (D112/Phase 28 Step 1) ──────────────────────────────
    //
    // Driven through `engine.paint(canvas, overlay, events)` with real
    // synthetic `InputEvent`s — the same production dispatch code a real
    // OS keystroke reaches — and asserted against the REAL app-owned atom
    // an `on_change` closure writes to, not just the render tree's
    // ephemeral `text_edit` state. This is the substitute for on-device
    // OS-level input verification: synthetic `CGEvent` mouse/keyboard
    // injection into another process requires Accessibility permission
    // this sandbox doesn't have (confirmed empirically — a real click
    // landed on the field's declared rect, real window frontmost, and
    // produced no observable effect; the same gap was hit and documented
    // earlier in this project). A headless `FrameEngine` integration test
    // is not a weaker substitute: it exercises the exact same
    // `rosace/src/engine.rs` dispatch code real input reaches, and — unlike
    // eyeballing a screenshot — asserts an exact resulting value.

    use rosace_widgets::tree::TextInput;
    use std::sync::OnceLock;

    /// Root with a single real, atom-bound `TextInput` — `on_change`
    /// writes into the SAME atom `build()` reads `.value()` from, the
    /// exact controlled wiring a real app uses. `captured` lets the test
    /// read that atom's live value after painting; `Component` requires
    /// `Send + Sync` so an `Rc<RefCell<_>>` field (used for this same
    /// purpose in web/FFI code elsewhere) isn't an option here — a
    /// `OnceLock` is, and the atom's identity is stable across rebuilds
    /// (D091 position-based persistence) so capturing it once is enough.
    struct OneTextInput {
        captured: Arc<OnceLock<rosace_state::Atom<String>>>,
    }
    impl Component for OneTextInput {
        fn build(&self, ctx: &mut Context) -> Element {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            TextInput::new()
                .value(name.get())
                .on_change({
                    let name = name.clone();
                    move |v| name.set(v)
                })
                .into_element()
        }
    }

    fn headless_text_input_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas, Arc<OnceLock<rosace_state::Atom<String>>>) {
        let captured = Arc::new(OnceLock::new());
        let engine = FrameEngine::new(Box::new(OneTextInput { captured: captured.clone() }), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(200, 60), SkiaCanvas::new(200, 60), captured)
    }

    fn click(x: f32, y: f32) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Left }
    }
    fn text(c: char) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::Text { character: c }
    }
    fn key(k: rosace_platform::Key) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::KeyDown { key: k }
    }
    fn key_up(k: rosace_platform::Key) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::KeyUp { key: k }
    }
    fn type_str(engine: &mut FrameEngine, canvas: &mut SkiaCanvas, overlay: &mut SkiaCanvas, s: &str) {
        for c in s.chars() {
            engine.paint(canvas, overlay, &[text(c)]);
        }
    }

    #[test]
    fn click_focuses_the_input_and_typed_text_reaches_the_bound_atom() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]); // populate editable/focus regions
        assert_eq!(atom.get().unwrap().get(), "", "starts empty");

        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");

        assert_eq!(atom.get().unwrap().get(), "hi", "typed text must reach the app-owned atom via on_change");
    }

    #[test]
    fn typing_before_any_click_does_nothing_nothing_is_focused_yet() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");
        assert_eq!(atom.get().unwrap().get(), "", "no widget is focused, so Text events must be dropped");
    }

    #[test]
    fn backspace_removes_the_last_character() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        assert_eq!(atom.get().unwrap().get(), "h");
    }

    #[test]
    fn delete_forward_removes_the_char_after_the_cursor() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");
        // Click places the caret at the end (Step 1 scoping) — Home first
        // so Delete has something after the cursor to remove.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Home)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Delete)]);
        assert_eq!(atom.get().unwrap().get(), "i");
    }

    #[test]
    fn arrow_left_then_insert_lands_in_the_middle_not_appended_at_the_end() {
        // Real proof the caret tracks a POSITION, not just "always append":
        // type "ac", move left once, type "b" -> must read "abc".
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "ac");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowLeft)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "b");
        assert_eq!(atom.get().unwrap().get(), "abc");
    }

    #[test]
    fn shift_arrow_selects_then_typing_replaces_the_selection() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Home)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Shift)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Shift)]);
        // "he" now selected; typing must replace it, not insert alongside.
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "Xllo");
    }

    #[test]
    fn cmd_a_selects_all_then_backspace_clears_everything() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('a'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        assert_eq!(atom.get().unwrap().get(), "");
    }

    #[test]
    fn ctrl_a_also_triggers_select_all_not_only_meta() {
        // Linux/Windows convention — deliberately not OS-branched, see
        // the dispatch comment in `paint`.
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Control)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('a'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Control)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        assert_eq!(atom.get().unwrap().get(), "");
    }

    #[test]
    fn clicking_blank_space_blurs_the_focused_input() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");
        assert_eq!(atom.get().unwrap().get(), "hi");

        // Click well outside the input's rect (200x60 canvas) — blank space.
        engine.paint(&mut canvas, &mut overlay, &[click(199.0, 59.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "MORE");
        assert_eq!(atom.get().unwrap().get(), "hi", "typing after a blank-space blur must not reach the now-unfocused input");
    }

    /// Two real, independently atom-bound `TextInput`s stacked in a
    /// Column — Tab must move focus from the first to the second, and
    /// typed text after Tab must land in the SECOND field's atom, not the
    /// first's (proves `focus_owner` resolves the CORRECT node, not just
    /// "whichever was focused first").
    struct TwoTextInputs {
        first: Arc<OnceLock<rosace_state::Atom<String>>>,
        second: Arc<OnceLock<rosace_state::Atom<String>>>,
    }
    impl Component for TwoTextInputs {
        fn build(&self, ctx: &mut Context) -> Element {
            let a: rosace_state::Atom<String> = ctx.state(String::new());
            let b: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.first.set(a.clone());
            let _ = self.second.set(b.clone());
            Column::new()
                .child(TextInput::new().height(30.0).value(a.get()).on_change({
                    let a = a.clone(); move |v| a.set(v)
                }))
                .child(TextInput::new().height(30.0).value(b.get()).on_change({
                    let b = b.clone(); move |v| b.set(v)
                }))
                .into_element()
        }
    }

    #[test]
    fn tab_moves_focus_from_the_first_input_to_the_second() {
        let first = Arc::new(OnceLock::new());
        let second = Arc::new(OnceLock::new());
        let engine_root = TwoTextInputs { first: first.clone(), second: second.clone() };
        let mut engine = FrameEngine::new(Box::new(engine_root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 100);
        let mut overlay = SkiaCanvas::new(200, 100);

        engine.paint(&mut canvas, &mut overlay, &[]);
        // Click into the FIRST field (near the top of the column).
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 12.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "one");
        assert_eq!(first.get().unwrap().get(), "one");
        assert_eq!(second.get().unwrap().get(), "");

        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Tab)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "two");
        assert_eq!(first.get().unwrap().get(), "one", "the first field must be unchanged");
        assert_eq!(second.get().unwrap().get(), "two", "typed text after Tab must land in the SECOND field");
    }

    #[test]
    fn cut_then_paste_round_trips_through_the_real_system_clipboard() {
        // Touches the REAL OS clipboard (rosace-clipboard's own test
        // suite only exercises NoopClipboard) — save and restore whatever
        // was there so this test leaves no lasting side effect on the
        // developer's actual clipboard.
        use rosace_clipboard::ClipboardProvider;
        let cb = rosace_clipboard::SystemClipboard::new();
        let original = cb.read();

        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");

        // Select "llo" (chars 2..5): Home, then Shift+Right x3.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Home)]);
        for _ in 0..2 {
            engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        }
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Shift)]);
        for _ in 0..3 {
            engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        }
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Shift)]);

        // Cmd+X: cuts "llo" to the real clipboard, leaves "he".
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('x'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);
        assert_eq!(atom.get().unwrap().get(), "he", "cut must remove the selection from the field");
        assert_eq!(cb.read().as_deref(), Some("llo"), "cut must write the selection to the real system clipboard");

        // Cmd+V at the end: pastes "llo" back -> "hello" again.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('v'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);
        assert_eq!(atom.get().unwrap().get(), "hello", "paste must insert the real clipboard's content at the caret");

        match original {
            Some(text) => { let _ = cb.write(&text); }
            None => cb.clear(),
        }
    }

    // ── D116 Step 2: undo/redo, word ops, EditController ─────────────────

    #[test]
    fn cmd_z_undoes_typing_through_real_dispatch() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");
        assert_eq!(atom.get().unwrap().get(), "hi");

        // Typed within the coalesce window (real, but fast, wall-clock
        // gap between these calls) — one Cmd+Z removes the whole group.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('z'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);
        assert_eq!(atom.get().unwrap().get(), "", "Cmd+Z must undo the coalesced typing group");
    }

    #[test]
    fn shift_cmd_z_and_cmd_y_both_redo() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");

        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('z'))]);
        assert_eq!(atom.get().unwrap().get(), "");

        // Shift+Cmd+Z redoes.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Shift)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('z'))]);
        assert_eq!(atom.get().unwrap().get(), "hi", "Shift+Cmd+Z must redo");
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Shift)]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);

        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('z'))]);
        assert_eq!(atom.get().unwrap().get(), "", "undo again");

        // Cmd+Y also redoes (the Windows-convention alternative).
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('y'))]);
        assert_eq!(atom.get().unwrap().get(), "hi", "Cmd+Y must redo too");
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);
    }

    #[test]
    fn ctrl_backspace_deletes_the_preceding_word_through_real_dispatch() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");

        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Control)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Control)]);
        assert_eq!(atom.get().unwrap().get(), "hello ", "Ctrl+Backspace must delete the whole preceding word");
    }

    #[test]
    fn alt_arrow_moves_by_word_then_insert_lands_at_the_word_boundary() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");

        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Home)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Alt)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Alt)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "!");
        assert_eq!(atom.get().unwrap().get(), "hello! world", "Alt+Right must land right after \"hello\"");
    }

    /// Two real, independently atom-bound fields wired to their own
    /// `EditController`s — proves `drain_controllers` finds the RIGHT
    /// node when more than one exists, mirroring `tab_moves_focus_from_
    /// the_first_input_to_the_second`'s precedent for keyboard dispatch.
    struct TwoControlledTextInputs {
        first: Arc<OnceLock<rosace_state::Atom<String>>>,
        second: Arc<OnceLock<rosace_state::Atom<String>>>,
        first_ctrl: text_edit::EditController,
        second_ctrl: text_edit::EditController,
    }
    impl Component for TwoControlledTextInputs {
        fn build(&self, ctx: &mut Context) -> Element {
            let a: rosace_state::Atom<String> = ctx.state(String::new());
            let b: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.first.set(a.clone());
            let _ = self.second.set(b.clone());
            Column::new()
                .child(TextInput::new().height(30.0).value(a.get()).controller(self.first_ctrl.clone()).on_change({
                    let a = a.clone(); move |v| a.set(v)
                }))
                .child(TextInput::new().height(30.0).value(b.get()).controller(self.second_ctrl.clone()).on_change({
                    let b = b.clone(); move |v| b.set(v)
                }))
                .into_element()
        }
    }

    #[test]
    fn edit_controller_targets_the_correct_field_among_several() {
        let first = Arc::new(OnceLock::new());
        let second = Arc::new(OnceLock::new());
        let first_ctrl = text_edit::EditController::new();
        let second_ctrl = text_edit::EditController::new();
        let root = TwoControlledTextInputs {
            first: first.clone(), second: second.clone(),
            first_ctrl: first_ctrl.clone(), second_ctrl: second_ctrl.clone(),
        };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 100);
        let mut overlay = SkiaCanvas::new(200, 100);
        engine.paint(&mut canvas, &mut overlay, &[]);

        // No focus/click at all — purely programmatic, proving the
        // controller path is independent of FocusManager entirely.
        second_ctrl.insert_at_cursor("only the second field");
        engine.paint(&mut canvas, &mut overlay, &[]);

        assert_eq!(first.get().unwrap().get(), "", "the FIRST field must be untouched");
        assert_eq!(second.get().unwrap().get(), "only the second field");
        assert_eq!(second_ctrl.value(), "only the second field");
    }

    /// The exact scenario D116/PHASE_28.md's Step 2 exit bar names: a
    /// markdown toolbar's Bold button reads the field's live selection
    /// through its `EditController` and wraps it — entirely through real
    /// keyboard-driven selection (Shift+arrows) THEN a controller call
    /// simulating a button's `on_press`, with no direct render-tree
    /// access at any point (a real toolbar button couldn't have any).
    struct OneControlledTextInput {
        captured: Arc<OnceLock<rosace_state::Atom<String>>>,
        controller: text_edit::EditController,
    }
    impl Component for OneControlledTextInput {
        fn build(&self, ctx: &mut Context) -> Element {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            TextInput::new()
                .value(name.get())
                .controller(self.controller.clone())
                .on_change({ let name = name.clone(); move |v| name.set(v) })
                .into_element()
        }
    }

    #[test]
    fn edit_controller_wraps_a_live_keyboard_selection_like_a_real_toolbar_bold_button() {
        let captured = Arc::new(OnceLock::new());
        let controller = text_edit::EditController::new();
        let root = OneControlledTextInput { captured: captured.clone(), controller: controller.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 60);
        let mut overlay = SkiaCanvas::new(200, 60);

        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");
        assert_eq!(captured.get().unwrap().get(), "hello world");
        // The controller must already reflect keyboard-driven typing —
        // not just controller-originated edits (the bug this test guards
        // against: a stale snapshot would read "" here).
        assert_eq!(controller.value(), "hello world");

        // Select "world" (chars 6..11) via Shift+Right, same as a real
        // user dragging or double-clicking would leave behind.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Home)]);
        for _ in 0..6 {
            engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        }
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Shift)]);
        for _ in 0..5 {
            engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowRight)]);
        }
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Shift)]);

        // The toolbar Bold button's `on_press`: reads the CONTROLLER's
        // live value/selection (all it has access to) and wraps it.
        let value = controller.value();
        let (start, end) = controller.selection().primary_range();
        assert_eq!((start, end), (6, 11), "controller.selection() must reflect the real keyboard selection");
        let word = &value[start..end];
        controller.replace_range(start, end, format!("**{word}**"));

        // Ops apply on the engine's next frame (documented on
        // EditController::value()) — matches how a real app's next paint
        // picks up a controller call made from a button callback.
        engine.paint(&mut canvas, &mut overlay, &[]);

        assert_eq!(captured.get().unwrap().get(), "hello **world**", "the wrap must reach the real app atom via on_change");
        assert_eq!(controller.value(), "hello **world**");
    }
}
