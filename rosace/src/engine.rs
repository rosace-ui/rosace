//! `FrameEngine` — the per-frame build/paint/input logic, extracted from
//! `App::launch`'s `run_layered` closure (Phase 24 Step 1, D106).
//!
//! This is a behavior-preserving extraction: the desktop/web path
//! (`App::launch` → `PlatformWindow::run_layered`) drives it exactly as
//! before. The point is to make the same logic drivable from a second place
//! — a native-host FFI boundary (`rosace-ffi`) that has no winit event loop
//! at all — without duplicating ~450 lines of reconciler/paint/input code.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use rosace_widgets::Component;
use rosace_core::types::Rect;
use rosace_render::SkiaCanvas;
use rosace_widgets::tree::{
    text_edit, FocusBehavior, InputBehavior,
    LayerPosition, Menu, NodeId, ScrimConfig,
};
use rosace_widgets::clipboard::ClipboardProvider as _;

use crate::{inflate_rect, paint_root, theme_color};

/// Translate a physical key + modifiers into a [`text_edit::Command`]
/// (D116 layer 4 — the abstract vocabulary a keymap produces). `word_mod`
/// is Alt (macOS's Option convention) OR Ctrl (Linux/Windows) — not
/// OS-branched, same spirit as the Cmd/Ctrl clipboard shortcuts. Lives
/// here rather than in `rosace-widgets::text_edit` because it needs
/// `rosace_platform::Key`, a lower layer that crate doesn't depend on;
/// the Command vocabulary itself stays platform-agnostic so a future
/// widget could construct/dispatch commands without touching `Key` at
/// all. Returns `None` for any key with no editing meaning.
/// Multi-click detection window (D116 Step 3) — same-node clicks closer
/// together than this in time and space count as a double/triple-click
/// rather than two independent single clicks.
const DOUBLE_CLICK_SECS: f32 = 0.4;
const DOUBLE_CLICK_SLOP: f32 = 5.0;

/// Touch-and-hold duration before a press on an editable selects the
/// word under it (D116 Step 7) — matches every mobile text-selection
/// convention and `LongPressable`'s own generic threshold, and is
/// indistinguishable from "click and hold with a mouse" in this event
/// model (touch already converts to plain MouseDown/Move/Up at the
/// platform boundary — see `rosace-platform`). Safe against headless
/// tests that sleep real wall-clock time mid-gesture: `cancel_pending_press`
/// is called on every subsequent keyboard event, not just the next
/// MouseDown/Up, closing the race a longer timeout would only widen.
const LONG_PRESS_SELECT_MS: u64 = 500;

use rosace_state::fire_after_ms;
/// A handle's hit radius (D116 Step 7) — how close a MouseDown must land
/// to a selection-handle anchor point to grab it instead of starting a
/// fresh click/drag.
const HANDLE_HIT_RADIUS: f32 = 12.0;

/// An action enqueued by the desktop right-click context menu's item
/// callbacks (D116 Step 7). `Menu::item` closures are `Arc<dyn Fn() +
/// Send + Sync>` with no captured engine access (the same `!Sync`/
/// `!Send` wall `EditController`'s op queue exists to cross) — each item
/// just pushes an action onto `FrameEngine::context_menu_actions`,
/// drained once per frame on the main thread, same timing as
/// `drain_controllers`.
#[derive(Clone, Copy, Debug)]
enum ContextMenuAction { Cut, Copy, Paste, SelectAll, Dismiss }

/// World-space anchor point for a selection handle at `char_idx` (D116
/// Step 7) — the bottom of the boundary's line, where every mobile text
/// editor's drag grip sits. `None` if `char_idx` isn't in any line (a
/// stale/empty snapshot).
fn handle_anchor(layout: &text_edit::TextLayoutSnapshot, char_idx: usize) -> Option<(f32, f32)> {
    let line = layout.lines.iter().find(|l| char_idx >= l.char_range.0 && char_idx <= l.char_range.1)?;
    Some((line.x_at(char_idx), line.y + line.height))
}

/// Draw the DevTools element-inspector chrome (D123/O2) into the overlay
/// recorder: a hover outline, a selected-node outline + tint, and a
/// bottom-left panel with the selected node's size/rect/constraints/
/// semantics readout. All geometry + text come from `rosace_devtools`
/// (unit-tested off a headless snapshot); this fn is only the drawing.
fn draw_dev_inspector(
    rec: &mut rosace_render::PictureRecorder,
    snapshot: &[rosace_widgets::tree::InspectNode],
    dev: &rosace_devtools::ElementInspector,
    cursor: (f32, f32),
    win_w: f32,
    win_h: f32,
    font: &rosace_render::FontCache,
) {
    use rosace_core::types::{Point, Rect, Size};
    use rosace_render::{Color, DrawCommand, FontWeight};

    let accent = Color::rgb(80, 200, 255);

    let outline = |rec: &mut rosace_render::PictureRecorder, r: (f32, f32, f32, f32), color: Color, w: f32| {
        rec.push(DrawCommand::StrokeRect {
            rect: Rect { origin: Point { x: r.0, y: r.1 }, size: Size { width: r.2, height: r.3 } },
            color,
            width: w,
        });
    };

    // Hover outline (thin) — only when it isn't the selected node.
    if let Some(h) = dev.hover {
        if dev.selected != Some(h) {
            if let Some(r) = rosace_devtools::node_rect(snapshot, h) {
                outline(rec, r, Color::rgba(80, 200, 255, 150), 1.0);
            }
        }
    }

    // Selected outline (thick) + faint fill.
    if let Some(s) = dev.selected {
        if let Some(r) = rosace_devtools::node_rect(snapshot, s) {
            rec.push(DrawCommand::FillRect {
                rect: Rect { origin: Point { x: r.0, y: r.1 }, size: Size { width: r.2, height: r.3 } },
                color: Color::rgba(80, 200, 255, 32),
            });
            outline(rec, r, accent, 2.0);
        }
    }

    // Panel — the selected node's readout, else a hint.
    let px = 12.0;
    let lh = font.line_height(px);
    let pad = 10.0;
    let lines: Vec<String> = match dev.selected.and_then(|s| rosace_devtools::panel_lines(snapshot, s)) {
        Some(l) => l,
        None => vec![
            "DevTools inspector".to_string(),
            "hover to highlight · click to select".to_string(),
            "Esc to deselect · F12 to close".to_string(),
        ],
    };
    let text_w = lines.iter()
        .map(|l| font.measure_text(l, px))
        .fold(0.0f32, f32::max);
    let panel_w = text_w + pad * 2.0;
    let panel_h = lines.len() as f32 * lh + pad * 2.0;
    // Bottom-left, clamped inside the window.
    let panel_x = 12.0f32.min((win_w - panel_w - 4.0).max(0.0));
    let panel_y = (win_h - panel_h - 12.0).max(0.0);
    let _ = cursor; // panel is corner-docked, not cursor-tracking (stable to read)

    rec.push(DrawCommand::FillRRect {
        rect: Rect { origin: Point { x: panel_x, y: panel_y }, size: Size { width: panel_w, height: panel_h } },
        radius: 8.0,
        color: Color::rgba(18, 22, 28, 240),
    });
    rec.push(DrawCommand::StrokeRRect {
        rect: Rect { origin: Point { x: panel_x, y: panel_y }, size: Size { width: panel_w, height: panel_h } },
        radius: 8.0,
        color: Color::rgba(80, 200, 255, 120),
        width: 1.0,
    });
    for (i, line) in lines.iter().enumerate() {
        // First line (the widget type) in the accent color + bold.
        let (color, weight) = if i == 0 {
            (accent, FontWeight::Bold)
        } else {
            (Color::rgb(210, 218, 226), FontWeight::Regular)
        };
        rec.push(DrawCommand::DrawText {
            text: line.clone(),
            origin: Point { x: panel_x + pad, y: panel_y + pad + i as f32 * lh },
            color,
            px,
            weight,
        });
    }
}

/// Whether the DevTools FAB is shown — dev builds only (never ships in
/// release). A tap on it opens the DevTools panel; no keyboard needed, so it
/// works on mobile too.
///
/// Excluded from `rosace`'s OWN unit tests (`cfg!(test)`, scoped to this
/// crate's test binary — a downstream app's debug build is unaffected, since
/// `cfg!(test)` there is a different compilation unit entirely). Without
/// this, every headless test engine got a real FAB overlay injected, and its
/// hit region — plus the process-global `DEVTOOLS_OPEN`/`DEVTOOLS_TAB` atoms
/// it reads/writes — leaked across tests sharing the same test-binary
/// process, making a cluster of typing/focus tests order- and state-
/// dependent (root-caused 2026-07-31; tracked since 2026-07-24 in
/// project_dev_release_state.md as "isolate global state per test").
fn devtools_fab_enabled() -> bool {
    cfg!(debug_assertions) && !cfg!(test)
}


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
    /// Last `build()` output. There is exactly one component, so this is one
    /// widget rather than a map: a clean frame reuses it instead of rebuilding.
    built: Option<rosace_widgets::tree::BoxedWidget>,
    /// Whether `on_mount` has fired for the root component.
    root_mounted: bool,
    /// This engine's root component id.
    ///
    /// Every engine used to hardcode `ComponentId(0)`, which is fine with one
    /// engine per process and wrong the moment there are two: the dirty set
    /// (`rosace_state::dirty_set`) is PROCESS-GLOBAL and keyed by
    /// `ComponentId`, so two engines both calling themselves 0 shared one
    /// dirty flag. Whichever drained `take_dirty_components()` first consumed
    /// the other's mark, and that engine silently skipped its rebuild —
    /// losing whatever the user had just typed.
    ///
    /// That is the root cause of the intermittent engine-test failures
    /// logged as WIDGET_FINDINGS L15: the suite runs tests in parallel, each
    /// with its own engine, all of them component 0.
    root_component_id: rosace_core::types::ComponentId,
    /// Whether the most recent `BackPressed` was consumed.
    ///
    /// The native host has to know: Android must finish the activity when
    /// the app declines, and must NOT when it popped a screen. Read straight
    /// after the paint that carried the event.
    last_back_handled: bool,
    /// Overlays emitted during BUILD (`Dialog::emit`/`Snackbar::emit`) —
    /// kept until the next rebuild so they survive cache-hit frames (see
    /// the clear-before-build comment in `paint`).
    render_tree: Rc<RefCell<rosace_widgets::tree::RenderTree>>,

    // ── Focus + input state ─────────────────────────────────────────────
    focus_manager: rosace_core::a11y::FocusManager,
    shift_held: bool,
    /// Held-modifier state for text-editing shortcuts (D112/Phase 28
    /// Step 1) — mirrors `shift_held`. Cmd/Ctrl+A/C/X/V trigger on
    /// EITHER being held (`ctrl_held || meta_held`), covering macOS's
    /// Cmd convention and Linux/Windows's Ctrl convention without
    /// branching on target OS.
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
    /// Mouse drag-to-select over an editable field (D116 Step 3): the
    /// node being dragged over and the anchor char index the drag
    /// started from. `MouseMove` extends `Selection::range(anchor,
    /// position_at(x, y))`; `MouseUp` clears it. Separate from
    /// `active_drag` (a captured closure) — editables use the same
    /// declare-then-query pattern as click-to-focus, since neither
    /// `FontCache` nor `Rc<RefCell<RenderTree>>` can cross into a
    /// `Send + Sync` closure.
    text_drag: Option<(NodeId, usize)>,
    /// Double/triple-click detection state (D116 Step 3) — a same-node
    /// click within `DOUBLE_CLICK_SECS` and `DOUBLE_CLICK_SLOP` px of
    /// the previous one increments `click_count` (capped at 3: single /
    /// word / line); anything else resets it to 1.
    last_click_at: f32,
    last_click_pos: (f32, f32),
    last_click_node: Option<NodeId>,
    click_count: u8,
    /// Set when a hover change (or other non-atom event) needs a repaint on
    /// the next frame; consumed by `needs_paint`.
    forced_repaint: bool,
    /// Long-press: cancel token for the in-flight press timer + press origin.
    lp_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    press_origin: Option<(f32, f32)>,
    /// A plain (non-positional) tap callback captured on MouseDown but not
    /// yet fired — clicks/taps must resolve on release, not on touch-down,
    /// so a scroll/drag that starts on top of a Button doesn't also fire
    /// its `on_press`. Cleared without firing if the pointer moves past
    /// `press_origin`'s slop (same cancellation `lp_cancel` already uses);
    /// invoked on `MouseUp` otherwise. Positional hits (sliders, on_press_at)
    /// are unaffected — those still fire immediately on down, see
    /// `active_drag`'s doc comment.
    /// Last pointer position seen, so hover can be re-resolved when the
    /// CONTENT moves under a stationary cursor.
    ///
    /// The composited scroll path used to make this unnecessary: it remapped
    /// the pointer through `child_coords` on every walk, so hover was
    /// recomputed from scratch each time. Painting scrolled content directly
    /// means the hovered node simply translates away with everything else,
    /// taking its highlight with it, while the row now under the cursor stays
    /// unlit.
    last_pointer: Option<(f32, f32)>,
    /// A scroll moved content this frame; hover must be re-resolved once the
    /// new rects exist.
    needs_rehover: bool,
    pending_press: Option<Arc<dyn Fn(f32, f32) + Send + Sync>>,
    /// Same deferral as `pending_press`, for overlay-hosted widgets
    /// (buttons inside a Dialog/Drawer/Dropdown/menu overlay).
    pending_overlay_press: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Nested-scroll chain captured on MouseDown (D-NESTED-SCROLL,
    /// 2026-08-02) — every enclosing `ScrollView` (or other nested-scroll
    /// participant) along the touched point, innermost first; see
    /// `rosace_widgets::tree::ScrollHandler`'s own doc for the full
    /// contract. Always captured (even when `pending_press`/`active_drag`
    /// also got set), but only WALKED on `MouseMove` once there's no
    /// competing `pending_press` still waiting to see if this is a tap —
    /// see the `MouseMove` handler. Empty when nothing scrollable is near
    /// the touch point.
    pending_scroll_chain: Vec<rosace_widgets::tree::ScrollHandler>,
    /// The point `pending_scroll_chain` was last walked from — each
    /// `MouseMove` feeds the chain `(x, y) - last_chain_point`, since
    /// `ScrollHandler` links take a DELTA, not an absolute position (see
    /// its own doc comment). Set to the touch-down point when a new chain
    /// is captured, then to the current point after every walk.
    last_chain_point: Option<(f32, f32)>,
    /// Desktop right-click context menu (D116 Step 7): the editable node
    /// it's open for and the click position it opened at (menu anchor).
    /// `None` when closed. Re-pushed as an overlay every frame while
    /// `Some`, mirroring how `Dropdown` re-pushes its own atom-backed
    /// overlay each paint — just engine-driven instead of atom-driven,
    /// since this menu has no backing widget in the tree.
    context_menu: Option<(NodeId, (f32, f32))>,
    /// See [`ContextMenuAction`]'s doc comment.
    context_menu_actions: Arc<std::sync::Mutex<Vec<ContextMenuAction>>>,
    /// A background long-press timer's pending "select the word at this
    /// char index" result (D116 Step 7) — set from a spawned thread
    /// (which cannot touch `RenderTree`/`FontCache`, same wall as every
    /// other editable mutation here), drained on the main thread each
    /// frame alongside `drain_controllers`.
    pending_long_press_select: Arc<std::sync::Mutex<Option<(NodeId, usize)>>>,
    /// Active selection-handle drag (D116 Step 7): the node, which
    /// endpoint is being dragged (`true` = the selection's end, `false` =
    /// its start), and the OTHER endpoint's char index captured at grab
    /// time (stays fixed for the whole drag, same shape as `text_drag`'s
    /// own `anchor`). `MouseMove` updates the dragged endpoint via
    /// `position_at`; `MouseUp` clears it.
    handle_drag: Option<(NodeId, bool, usize)>,
    /// Overlay dispatch routes RETAINED across engine-skipped frames
    /// (2026-07-19): with the GPU animation loop presenting every frame,
    /// a paint-time overlay (Dropdown menu) exists in the per-frame
    /// registry only on frames its OWNER repainted — clean frames must
    /// keep the previous routes (and the overlay canvas's previous
    /// pixels) or an open menu flickers out one present after it opens.
    /// DevTools element inspector (D123/O2). F12 toggles it; while on,
    /// hovering highlights the widget under the cursor and clicking selects
    /// it (both via `RenderTree::pick`), and a panel shows the selected
    /// node's size/rect/constraints/semantics. Debug-only chrome drawn on
    /// the overlay layer above everything; app input is intercepted while
    /// it's enabled.
    dev: rosace_devtools::ElementInspector,
    /// DevTools trace/network activity panel (D123/O5). Rendered inside the
    /// FAB-opened DevTools panel; read-only, never intercepts app input.
    /// Reads the flight recorder to format the DevTools panel's rows (the panel
    /// itself is a widget overlay; see `rosace_devtools::devtools_overlay`).
    trace_panel: rosace_devtools::TracePanel,
    /// Last cursor position (logical px) — the inspector reads it to place
    /// its panel and to re-`pick` on toggle without waiting for a move.
    cursor: (f32, f32),
}

impl FrameEngine {
    /// Builds a fresh engine for `root`, marking every component dirty so
    /// the first `paint` call does a full build + repaint.
    pub fn new(root: Box<dyn Component>, font: rosace_render::FontCache) -> Self {
        rosace_state::reset_to_global_dirty();
        // Process-unique, so concurrent engines never share a dirty flag.
        // See the `root_component_id` field doc.
        static NEXT_ROOT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root_component_id = rosace_core::types::ComponentId(
            NEXT_ROOT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        Self {
            root,
            font,
            built: None,
            root_mounted: false,
            root_component_id,
            last_back_handled: false,
            render_tree: Rc::new(RefCell::new(rosace_widgets::tree::RenderTree::new())),
            focus_manager: rosace_core::a11y::FocusManager::new(),
            shift_held: false,
            ctrl_held: false,
            meta_held: false,
            alt_held: false,
            active_drag: None,
            text_drag: None,
            last_click_at: -1000.0,
            last_click_pos: (0.0, 0.0),
            last_click_node: None,
            click_count: 0,
            forced_repaint: false,
            lp_cancel: None,
            press_origin: None,
            last_pointer: None,
            needs_rehover: false,
            pending_press: None,
            pending_overlay_press: None,
            pending_scroll_chain: Vec::new(),
            last_chain_point: None,
            context_menu: None,
            context_menu_actions: Arc::new(std::sync::Mutex::new(Vec::new())),
            pending_long_press_select: Arc::new(std::sync::Mutex::new(None)),
            handle_drag: None,
            // `ROSACE_DEVTOOLS=1` boots the element inspector already open —
            // handy when the thing you want to inspect is on the very first
            // frame, or when a window manager eats the F12 toggle.
            dev: {
                let mut d = rosace_devtools::ElementInspector::new();
                if std::env::var_os("ROSACE_DEVTOOLS").is_some() {
                    d.enabled = true;
                    // Boot the DevTools panel open too.
                    rosace_devtools::DEVTOOLS_OPEN.set(true);
                }
                d
            },
            trace_panel: rosace_devtools::TracePanel::new(),
            cursor: (0.0, 0.0),
        }
    }

    /// Swap the root component for a freshly-loaded one (Tier 2 dylib
    /// hot-reload). **Every object that may hold a handler from the OUTGOING
    /// module** — the element tree, the render tree (its hit/scroll/zoom/focus
    /// callbacks), captured drag closures, build/overlay routes, context-menu
    /// actions — is dropped HERE, while the old `Library` is still loaded, so
    /// their `Arc<dyn Fn>` drop-glue (whose code lives in that module) runs
    /// against valid memory. The dev host calls this AFTER loading the new
    /// module but BEFORE dropping the old `Library`; if any of these caches
    /// survived into the post-unload world, invoking or even *dropping* one
    /// would jump into freed code → a silent segfault (the exact crash Tier-2
    /// reload hit before this cleared them).
    ///
    /// After the wipe every component is marked dirty, so the next `paint`
    /// rebuilds from the NEW root with fresh closures. State atoms live in the
    /// shared runtime dylib keyed by `ComponentId`, so a same-shaped tree keeps
    /// its state across the swap.
    pub fn set_root(&mut self, root: Box<dyn Component>) {
        // The outgoing root leaves the tree, so its unmount hooks run here —
        // while its module is still loaded and its closures are still valid.
        if self.root_mounted {
            self.root.on_unmount();
            rosace_state::cleanup_store::fire_and_clear(self.root_component_id);
            rosace_state::clear_component(self.root_component_id);
        }

        // Replace the root first — the old `Box<dyn Component>`'s drop glue also
        // lives in the outgoing module, and the old lib is still loaded now.
        self.root = root;

        // Drop everything that can retain a module closure.
        self.built = None;
        self.root_mounted = false;
        *self.render_tree.borrow_mut() = rosace_widgets::tree::RenderTree::new();
        self.active_drag = None;
        self.text_drag = None;
        self.handle_drag = None;
        self.press_origin = None;
        self.lp_cancel = None;
        self.last_click_node = None;
        self.context_menu = None;
        if let Ok(mut actions) = self.context_menu_actions.lock() {
            actions.clear();
        }
        if let Ok(mut pend) = self.pending_long_press_select.lock() {
            *pend = None;
        }

        rosace_state::reset_to_global_dirty();
        rosace_state::request_frame();
    }

    /// The current semantic (accessibility/SEO) tree, derived from the
    /// render tree `paint` last built — call after at least one `paint()`
    /// (the render tree is empty before that). Used both by D099 assistive
    /// tech and, from D107/Phase 25, by build-time HTML/SEO export (see
    /// `rosace-web-seo`'s `render_html`) — a headless caller can call
    /// `paint()` once into a throwaway `SkiaCanvas` purely to populate the
    /// render tree, then read this, with no real window/GPU needed
    /// (`SkiaCanvas` is a plain in-memory CPU pixmap).
    /// Did the last [`rosace_platform::InputEvent::BackPressed`] get
    /// consumed? See [`Self::last_back_handled`]'s field doc.
    pub fn back_was_handled(&self) -> bool { self.last_back_handled }

    pub fn semantics(&self) -> rosace_core::SemanticNode {
        let mut root = self.render_tree.borrow().collect_semantics();

        // Pending announcements ride out as live-region children of the root.
        //
        // Appended here rather than declared by a widget because an
        // announcement belongs to no widget: it is spoken once and is gone,
        // and the thing that triggered it (a copy, a failed upload) often
        // draws nothing at all. Assistive tech speaks a live region when it
        // appears, so appearing in this tree IS the delivery.
        //
        // DRAINED, not read. Leaving them queued would re-speak every past
        // announcement on every subsequent publish.
        for (message, politeness) in rosace_core::a11y::announce::take() {
            root.children.push(
                // `Role::Text` rather than a dedicated status role: what
                // makes this an announcement is `live`, not the role, and
                // adding a variant would ripple through every exhaustive
                // match (HTML mapping, AccessKit mapping) for no behavioural
                // gain.
                rosace_core::SemanticNode::new()
                    .role(rosace_core::Role::Text)
                    .label(message)
                    .live(politeness),
            );
        }
        root
    }

    /// A read-only projection of the render tree — structure, geometry, type
    /// tags and interaction state.
    ///
    /// `RenderTree::inspect` has existed for the DevTools picker, but the
    /// tree itself is a private `Rc<RefCell<..>>` field, so nothing outside
    /// the engine could reach it. That made caching and identity behaviour
    /// testable only by instrumenting the walker by hand.
    pub fn inspect_tree(&self) -> Vec<rosace_widgets::tree::InspectNode> {
        self.render_tree.borrow().inspect()
    }

    /// How many nodes are in the Tab cycle right now.
    ///
    /// A trapping overlay narrows this to its own subtree, which is what
    /// makes a modal modal for keyboard users.
    pub fn focus_node_count(&self) -> usize {
        self.render_tree.borrow().collect_focus().len()
    }

    /// Scroll every scroll view above `node` so it comes into view.
    ///
    /// The engine owns the tree, so this is how an app reaches
    /// `RenderTree::reveal` from outside a widget's own paint — after a
    /// validation failure, say, or to follow a selection.
    pub fn reveal(&self, node: rosace_widgets::tree::NodeId, align: rosace_widgets::scroll::ScrollAlign) -> bool {
        self.render_tree.borrow().reveal(node, align)
    }

    /// The compositing layers derived from the current tree, in paint order.
    ///
    /// Each carries its enclosing layer, its screen placement and its
    /// inherited clip — the structure the flat `Vec<ScrollLayer>` handed to
    /// the platform cannot express.
    pub fn inspect_layers(&self) -> Vec<rosace_widgets::tree::Layer> {
        self.render_tree.borrow().layer_tree().layers
    }

    // ── Text editing dispatch (D112/Phase 28 Step 1) ────────────────────
    //
    // `TextInput::paint` can't mutate its own render-tree node (`paint`
    // takes `&self`) and a click/key callback can't capture the render
    // tree or `FontCache` (both fail `on_press_at`'s `Send + Sync` bound —
    // `Rc<RefCell<_>>` and `FontCache`'s own internal `RefCell` caches are
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
        // Input filters (D116 Step 8) — applied here, the ONE funnel
        // every edit source (typed chars, paste, IME commit, controller
        // ops) reaches, so a field declared with `.filters()` can't be
        // bypassed by any of them. Char-class filters strip disallowed
        // characters; `MaxLength` truncates. When filtering actually
        // changes the string, the selection is re-clamped to the
        // filtered length — a filtered-away char at the cursor must not
        // leave the caret pointing past the end of the value.
        let filters = {
            let tree = self.render_tree.borrow();
            tree.node(node_id).editable.as_ref().map(|e| e.filters.clone()).unwrap_or_default()
        };
        let (new_value, new_state) = if filters.is_empty() {
            (new_value, new_state)
        } else {
            let filtered = text_edit::apply_filters(&new_value, &filters);
            if filtered == new_value {
                (filtered, new_state)
            } else {
                let n = text_edit::char_count(&filtered);
                let head = new_state.selection.primary().head.min(n);
                let anchor = new_state.selection.primary().anchor.min(n);
                let clamped = new_state.with_selection(text_edit::Selection::range(anchor, head), new_state.last_edit_at);
                (filtered, clamped)
            }
        };

        let selection = new_state.selection.clone();
        let (on_change, controller) = {
            let mut tree = self.render_tree.borrow_mut();
            tree.node_mut(node_id).text_edit = new_state;
            // Also update the DECLARED value in place (Phase 32 bug fix,
            // found via the ScrollView typing repro): `focused_editable`
            // reads `EditableDecl.value`, which paint only refreshes on
            // the NEXT frame — so the second of two keystrokes processed
            // in one frame (fast typing, IME bursts, batched events) was
            // diffing against the stale value and DROPPING the first
            // character ("h","i" instead of "h","hi").
            if let Some(e) = tree.node_mut(node_id).editable.as_mut() {
                e.value = new_value.clone();
            }
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

    /// `(value, state)` for an explicit `node_id`, independent of focus —
    /// the context menu acts on whichever editable it was opened for, not
    /// necessarily whatever is currently focused (D116 Step 7).
    fn editable_at(&self, node_id: NodeId) -> Option<(String, text_edit::TextEditState)> {
        let tree = self.render_tree.borrow();
        let n = tree.node(node_id);
        let e = n.editable.as_ref()?;
        Some((e.value.clone(), n.text_edit.clone()))
    }

    /// Test-only: enqueue a context-menu action directly, bypassing
    /// `Menu`'s own pixel layout/hit-testing (already exercised by
    /// `Menu`'s own tests) — lets headless tests prove
    /// `drain_context_menu` reaches the real edit/clipboard, which is
    /// this step's actual point, without brittle "click at exactly this
    /// row's y" pixel math.
    /// Test-only: the target the engine would hand a context-menu item right
    /// now. Lets a test drive an app-supplied item with the REAL selection and
    /// controller instead of a hand-built stand-in, without the brittle "click
    /// at exactly this menu row's y" pixel math.
    #[cfg(test)]
    fn test_context_menu_target(&self) -> Option<text_edit::ContextMenuTarget> {
        let (node_id, _) = self.context_menu?;
        let (value, state) = self.editable_at(node_id)?;
        let controller = self.render_tree.borrow()
            .node(node_id).editable.as_ref().and_then(|e| e.controller.clone());
        Some(text_edit::ContextMenuTarget {
            selected_text: text_edit::selected_text(&value, &state),
            selection: state.selection_range(),
            value,
            controller,
        })
    }

    #[cfg(test)]
    fn test_enqueue_context_menu_action(&self, action: ContextMenuAction) {
        self.context_menu_actions.lock().unwrap().push(action);
    }

    /// Apply any context-menu actions enqueued since last frame (D116 Step
    /// 7) — the exact same `text_edit`/`rosace_widgets::clipboard` calls the
    /// Cmd/Ctrl+X/C/V/A keyboard shortcuts use, just triggered from a menu
    /// item instead of a `KeyDown` match arm. Closes the menu after ANY
    /// action, matching every desktop context menu's convention.
    fn drain_context_menu(&mut self) {
        let actions: Vec<ContextMenuAction> =
            std::mem::take(&mut *self.context_menu_actions.lock().unwrap());
        if actions.is_empty() {
            return;
        }
        let Some((node_id, _)) = self.context_menu else { return; };
        let now = rosace_widgets::tree::anim_clock();
        for action in actions {
            if matches!(action, ContextMenuAction::Dismiss) {
                self.context_menu = None;
                continue;
            }
            let Some((value, state)) = self.editable_at(node_id) else { continue; };
            match action {
                ContextMenuAction::SelectAll => {
                    if let Some((nv, ns)) = text_edit::apply_command(&value, &state, text_edit::Command::SelectAll, now) {
                        self.commit_text_edit(node_id, &value, nv, ns);
                    }
                }
                ContextMenuAction::Copy => {
                    if let Some(sel) = text_edit::selected_text(&value, &state) {
                        let _ = rosace_widgets::clipboard::SystemClipboard::new().write(&sel);
                    }
                }
                ContextMenuAction::Cut => {
                    if let Some(sel) = text_edit::selected_text(&value, &state) {
                        let _ = rosace_widgets::clipboard::SystemClipboard::new().write(&sel);
                        let (nv, ns) = text_edit::backspace(&value, &state, now);
                        self.commit_text_edit(node_id, &value, nv, ns);
                    }
                }
                ContextMenuAction::Paste => {
                    if let Some(text) = rosace_widgets::clipboard::SystemClipboard::new().read() {
                        if !text.is_empty() {
                            let (nv, ns) = text_edit::insert_str(&value, &state, &text, now);
                            self.commit_text_edit(node_id, &value, nv, ns);
                        }
                    }
                }
                ContextMenuAction::Dismiss => unreachable!(),
            }
            self.context_menu = None;
            self.forced_repaint = true;
            rosace_state::request_frame();
        }
    }

    /// Apply a background long-press timer's "select this word" result, if
    /// one landed since last frame (D116 Step 7) — see
    /// `pending_long_press_select`'s doc comment for why this can't
    /// happen directly on the spawned thread.
    fn drain_long_press_select(&mut self) {
        let pending = self.pending_long_press_select.lock().unwrap().take();
        let Some((node_id, pos)) = pending else { return; };
        let Some((value, mut state)) = self.editable_at(node_id) else { return; };
        let (s, e) = text_edit::word_range_at(&value, pos);
        state.selection = text_edit::Selection::range(s, e);
        let now = rosace_widgets::tree::anim_clock();
        state.last_edit_at = now;
        self.commit_text_edit(node_id, &value, value.clone(), state);
    }

    /// Cancel any in-flight long-press timer (D116 Step 7) — called
    /// whenever a real keyboard event arrives. A held-down press
    /// "surviving" through keystrokes has no real-world meaning (you
    /// can't type while still holding a touch/mouse press down in any
    /// scenario this engine can observe), and — found via a genuinely
    /// flaky headless test, not by inspection — relying on the NEXT
    /// MouseDown to cancel a stale timer is not enough: several existing
    /// tests type a full sentence (each character its own `engine.paint`
    /// call, each with real per-frame overhead) before their next click,
    /// and that typing overhead plus a deliberate debounce-window sleep
    /// can outrun even a generous long-press threshold. Cancelling
    /// eagerly on every keystroke closes the race outright rather than
    /// just widening it.
    fn cancel_pending_press(&mut self) {
        if let Some(c) = &self.lp_cancel {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
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
    /// pixels but never `SemanticsProps`/text content). Used by the web target's
    /// D107/Phase 25 Step 4 shadow-DOM sync to decide whether it's worth
    /// re-deriving the semantic tree at all this frame — computed here
    /// rather than by the caller re-deriving it, since `dirty_ids` is
    /// drained by `take_dirty_components()` below and can only be read once
    /// per frame.
    /// Drain queued accessibility actions into synthetic input events.
    ///
    /// A published semantic id packs the render-tree node and its
    /// semantics-entry index; `split_node_id` is the inverse of the packing
    /// in `collect_semantics`. A node with no painted rect (never laid out,
    /// or scrolled out of view) is skipped rather than dispatched at a
    /// nonsense coordinate.
    fn drain_a11y_actions(&self) -> Vec<rosace_platform::InputEvent> {
        use rosace_core::a11y::actions::{take, A11yAction};
        let requests = take();
        if requests.is_empty() {
            return Vec::new();
        }
        let tree = self.render_tree.borrow();
        let mut out = Vec::with_capacity(requests.len() * 2);
        for req in requests {
            let (node_id, _entry) = rosace_core::a11y::actions::split_node_id(req.node_id);
            let Some(rect) = tree.node_rect(node_id) else { continue };
            let (x, y) = (
                rect.origin.x + rect.size.width / 2.0,
                rect.origin.y + rect.size.height / 2.0,
            );
            match req.action {
                // Down AND up: a press that never releases leaves the control
                // stuck in its pressed visual, and any release-driven
                // behaviour (a Dismissible commit, a slider settle) never
                // runs.
                A11yAction::Activate => {
                    out.push(rosace_platform::InputEvent::MouseDown {
                        x, y, button: rosace_platform::MouseButton::Left,
                    });
                    out.push(rosace_platform::InputEvent::MouseUp {
                        x, y, button: rosace_platform::MouseButton::Left,
                    });
                }
                // Not routed yet: moving keyboard focus without activating
                // needs a focus lookup by node, which the FocusManager does
                // not expose. Dropped deliberately, so assistive tech can
                // fall back rather than believe it succeeded.
                A11yAction::Focus => {}
            }
        }
        out
    }

    pub fn paint(
        &mut self,
        canvas: &mut SkiaCanvas,
        overlay_canvas: &mut SkiaCanvas,
        events: &[rosace_platform::InputEvent],
    ) -> bool {
        let root = &self.root;
        let font = &self.font;

        // Publish the DPI/content scale so `ScrollView::should_auto_gpu` can
        // decide the GPU-vs-CPU path in PHYSICAL terms (its offscreen texture
        // is physical-px and capped) — one place, every platform.
        rosace_state::set_render_scale(canvas.scale());

        // ── Drain dirty-component set for this frame ───────────────────
        let global_dirty = rosace_state::is_global_dirty();
        let dirty_ids = rosace_state::take_dirty_components();
        let content_changed = global_dirty || !dirty_ids.is_empty();

        // ── Build root (only when dirty) ────────────────────────────────
        //
        // The root component owns all atoms created via
        // ctx.state(). When any of those atoms change, that id lands
        // in dirty_ids. We rebuild ONLY then; on clean frames the cached
        // element is reused, keeping `build()` side-effects out of the
        // render loop (e.g. an atom.set() inside build() would otherwise
        // cause an infinite loop).
        let root_component_id = self.root_component_id;
        let root_is_dirty = global_dirty || dirty_ids.contains(&root_component_id);

        // ── Clear overlay registry BEFORE build ─────────────────────────
        // Build-time emitters (`Dialog::emit`/`Snackbar::emit` called from
        // a component's `build()`) push into the registry during the build
        // below; clearing after it wiped them before they ever painted
        // (user-reported: the gallery's Modal/Non-modal/Snackbar buttons
        // did nothing). Their entries are then MOVED into
        // `self.build_overlays`, which persists across cache-hit frames —
        // build only runs when dirty, so per-frame draining alone would
        // make a dialog vanish on the first clean frame. Paint-time
        // pushers (Dropdown, Menu, Drawer) still drain per-frame below.

        let widget = if root_is_dirty || self.built.is_none() {
            // Attribute atom writes that happen DURING this build to this
            // component; writes from event handlers land outside it and
            // report UNKNOWN_COMPONENT rather than borrowing an id.
            let _building = rosace_state::current_component::enter(root_component_id);
            let mut ctx = rosace_core::Context::new(root_component_id);
            // Time the build and report WHY it ran. `ComponentRebuild` has
            // existed since the trace crate was written and was never
            // emitted, so the DevTools lifecycle timeline showed mounts and
            // unmounts with a hole where the actual work happens — the one
            // question a reactive framework exists to answer ("what made
            // this rebuild, and what did it cost?") had no data behind it.
            let build_start = std::time::Instant::now();
            let widget = root.build(&mut ctx);
            #[cfg(debug_assertions)]
            {
                use rosace_trace::{event::{RebuildCause, RosaceTrace}, trace};
                // `global_dirty` means something invalidated everything (first
                // frame, resize, a theme/media-query push); otherwise this id
                // was in the dirty set, which only an atom write puts it in.
                // Name the atom when we know it. `global_dirty` means
                // something invalidated everything (first frame, resize, an
                // OS theme/text-scale push) and no single atom is to blame.
                let cause = match rosace_state::dirty_set::last_cause(root_component_id) {
                    Some(atom) if !global_dirty => RebuildCause::AtomChanged(atom),
                    _ => RebuildCause::Manual,
                };
                trace!(RosaceTrace::ComponentRebuild {
                    id: root_component_id,
                    cause,
                    duration: build_start.elapsed(),
                });
            }
            self.built = Some(std::sync::Arc::clone(&widget));
            widget
        } else {
            std::sync::Arc::clone(self.built.as_ref().unwrap())
        };

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
        // A hover change repaints the widget subtree: the widgets must
        // actually re-run paint for the hovered one to change appearance.
        let hover_frame = self.forced_repaint;
        self.forced_repaint = false;
        // Widgets that changed their own state marked their nodes directly.
        // Applied here, before the walk, so the flags are in place when the
        // tree is visited. A node mark dirties no COMPONENT, so nothing
        // rebuilds and the frame below stays targeted — which is the whole
        // point: one widget updates instead of the screen.
        let marked = rosace_widgets::tree::take_dirty_nodes();
        let widget_state_frame = !marked.is_empty();
        if widget_state_frame {
            let mut tree = self.render_tree.borrow_mut();
            for id in marked {
                // Both flags on the node itself: the framework cannot know
                // whether the state change affected size — that depends on font
                // metrics, text scale and the widget's internals — so it
                // re-measures this ONE widget and compares. Ancestors get
                // needs_paint only, so they re-assemble without re-measuring.
                tree.mark_dirty_with_ancestors(id);
            }
        }

        let needs_paint = global_dirty
            || !dirty_ids.is_empty()
            || !canvas.has_drawn()   // fresh canvas after resize/scale change
            || window_resized
            || hover_frame
            || widget_state_frame;

        // Classify the frame for the widget layer's picture caches.
        //
        // STRUCTURAL means something rebuilt: every widget object is new, and
        // a node cannot tell a fresh-but-identical widget from a changed one
        // without drawing it. Caches are ignored, exactly as before this
        // existed.
        //
        // TARGETED means nothing rebuilt — the only changes are on nodes that
        // were explicitly marked, which today means hover and press. Those
        // re-record; their siblings replay.
        //
        // The unsafe direction is claiming targeted when something rebuilt,
        // which shows a stale picture. So this is a whitelist: targeted
        // requires that the ONLY reason we are painting is the forced-repaint
        // flag, with no dirty component, no global dirty, no resize and a
        // canvas that has already drawn something to keep.
        let structural = global_dirty
            || !dirty_ids.is_empty()
            || window_resized
            || !canvas.has_drawn();
        rosace_widgets::tree::set_structural_frame(structural);

        if needs_paint {
        // Reset the `WillPopScope` guards ONLY when the widget tree is about
        // to re-paint and repopulate them.
        //
        // Clearing unconditionally (where `clear_overlays` sits, above) looks
        // right and is wrong: on a cache-hit frame the engine replays cached
        // pictures and no widget `paint` runs, so the guards would be wiped
        // and never re-registered. A screen would then protect unsaved work
        // only on frames that happened to be dirty — which is to say, almost
        // never, and unpredictably. Same hazard the overlay registry hit
        // (see `build_overlays`' comment); found by a test that popped
        // successfully through a guard that should have blocked it.
        rosace_core::nav_back::clear_will_pop();

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
        rosace_widgets::tree::begin_frame();
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
            clip_rect: None,
        };

        // ── Engine chrome ───────────────────────────────────────────────
        // The framework's own UI: the text context menu, and the DevTools
        // FAB/panel below. Built here and promoted by `RootChrome` during
        // the paint walk, rather than injected into a parallel overlay pass
        // afterwards — a promoted node has to be declared DURING the walk.
        let mut chrome: Vec<crate::ChromeLayer> = Vec::new();

        // ── Context menu (D116 Step 7) — re-declared while open,
        // same "engine-driven instead of atom-driven" shape `Dropdown`
        // uses per-frame for its own overlay. Cut/Copy only appear when
        // there's an actual selection (real desktop convention — hidden,
        // not just grayed out, since `Menu` has no disabled-item concept).
        if let Some((node_id, (mx, my))) = self.context_menu {
            if let Some((value, state)) = self.editable_at(node_id) {
                let selected = text_edit::selected_text(&value, &state);
                let has_selection = selected.is_some();
                let actions = self.context_menu_actions.clone();

                // Built-ins first, expressed as ordinary items so the app's
                // transform hook can reorder or drop them — a read-only field
                // hiding Paste, say — exactly like its own additions.
                let builtin = |label: &str, needs_sel: bool, act: ContextMenuAction| {
                    let a = actions.clone();
                    text_edit::ContextMenuItem {
                        label: label.to_string(),
                        needs_selection: needs_sel,
                        action: Arc::new(move |_| a.lock().unwrap().push(act)),
                    }
                };
                let mut items = vec![
                    builtin("Cut", true, ContextMenuAction::Cut),
                    builtin("Copy", true, ContextMenuAction::Copy),
                    builtin("Paste", false, ContextMenuAction::Paste),
                    builtin("Select All", false, ContextMenuAction::SelectAll),
                ];

                // The field's own declaration carries its extra items and its
                // transform, so customisation is scoped to the widget rather
                // than living in a global registry.
                let (extra, transform, controller) = {
                    let tree = self.render_tree.borrow();
                    match tree.node(node_id).editable.as_ref() {
                        Some(e) => (e.menu_items.clone(), e.menu_transform.clone(), e.controller.clone()),
                        None => (Vec::new(), None, None),
                    }
                };
                items.extend(extra);
                if let Some(f) = &transform { f(&mut items); }
                items.retain(|i| !i.needs_selection || has_selection);

                let target = text_edit::ContextMenuTarget {
                    value: value.clone(),
                    selection: state.selection_range(),
                    selected_text: selected.clone(),
                    controller,
                };

                let mut menu = Menu::new();
                for item in items {
                    let target = target.clone();
                    let action = Arc::clone(&item.action);
                    let dismiss = actions.clone();
                    menu = menu.item(item.label.clone(), move || {
                        action(&target);
                        // Any item closes the menu, app-supplied or not.
                        dismiss.lock().unwrap().push(ContextMenuAction::Dismiss);
                    });
                }
                let dismiss_actions = actions.clone();
                chrome.push(crate::ChromeLayer {
                    kind: crate::ChromeKind::ContextMenu,
                    position: LayerPosition::Absolute(rosace_core::types::Point { x: mx, y: my }),
                    widget: std::sync::Arc::new(menu),
                    input: InputBehavior::Block,
                    focus: FocusBehavior::PassThrough,
                    scrim: Some(ScrimConfig {
                            color: rosace_render::Color::TRANSPARENT,
                            on_tap: Some(Arc::new(move || {
                                dismiss_actions.lock().unwrap().push(ContextMenuAction::Dismiss);
                            })),
                        exclude_rect: None,
                    }),
                });
            } else {
                self.context_menu = None;
            }
        }


        // DevTools (dev builds): a real widget tree (FAB + tabbed panel), so it
        // is laid out, painted, hit-tested and damage-tracked exactly like any
        // dialog — no hand-drawn chrome. Content comes from the always-on
        // flight recorder.
        if devtools_fab_enabled() {
            let events = rosace_trace::flight_recorder().map(|r| r.snapshot()).unwrap_or_default();
            let rows = self.trace_panel.rows_for(&events, rosace_devtools::DEVTOOLS_TAB.get(), 200);
            chrome.push(crate::ChromeLayer {
                kind:     crate::ChromeKind::DevTools,
                position: LayerPosition::Fill,
                widget:   rosace_devtools::devtools_overlay(rows),
                scrim:    None,
                // PassThrough: the panel and FAB take their own clicks, and
                // everything else must reach the app underneath.
                input:    InputBehavior::PassThrough,
                focus:    FocusBehavior::PassThrough,
            });
        }

        // Positioned promotions resolve against the window; publish it before
        // anything paints.
        rosace_widgets::tree::set_window_size(win_w, win_h);

        let constraints = rosace_layout::Constraints::tight(win_w, win_h);

        // ── Paint the root widget — widgets record DrawCommands ────────
        let mut damage: Option<Rect> = None;
        let root_widget: rosace_widgets::tree::BoxedWidget =
            std::sync::Arc::new(crate::RootChrome {
                app: std::sync::Arc::clone(&widget),
                chrome: std::mem::take(&mut chrome),
            });
        paint_root(
            &root_widget,
            constraints,
            &mut paint_ctx,
            &mut damage,
            root_is_dirty || hover_frame,  // subtree_dirty — repaint
            root_is_dirty,                 // subtree_relayout — only a rebuild resizes
        );
        self.render_tree.borrow_mut().finalize();

        // Content that moved under a stationary cursor changes what is
        // hovered, and only now are the new rects in place. Inlined rather
        // than a `&mut self` method: `root` is still borrowed here, and a
        // whole-self borrow would collide with it.
        if std::mem::take(&mut self.needs_rehover) {
            if let Some((hx, hy)) = self.last_pointer {
                let target = self.render_tree.borrow().hover_test(hx, hy);
                if self.render_tree.borrow_mut().set_hover(target) {
                    self.forced_repaint = true;
                    rosace_state::request_frame();
                }
            }
        }

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

        // ── Reconcile: the root component mounts once, on its first paint ──
        if !self.root_mounted {
            self.root_mounted = true;
            root.on_mount();
            #[cfg(debug_assertions)]
            {
                use rosace_trace::{event::RosaceTrace, location, trace};
                trace!(RosaceTrace::ComponentMount {
                    id: root_component_id,
                    name: root.type_name(),
                    location: location!(),
                });
            }
        }
        } // needs_paint

        // ── Composite pass — promoted layers into overlay_canvas ────────
        // Promoted content was painted during the main walk into its own
        // picture and deliberately NOT spliced into its parent's stream; it is
        // replayed here, above the base canvas, in declaration order.
        //
        // This used to be a second LAYOUT AND PAINT of a parallel overlay
        // stack, with a throwaway render tree per entry and a flattened route
        // list for input. All of that is gone: overlays are nodes now, so they
        // are laid out, painted, hit-tested, damage-tracked and read by
        // assistive tech through the one tree, and this is left doing nothing
        // but blitting pictures.
        let _ = rosace_widgets::tree::take_bottom_overlay_inset();
        let mut overlay_pass_ran = false;
        {
            let tree_ref = self.render_tree.borrow();
            let promoted = tree_ref.promoted_nodes();

            if !promoted.is_empty() || self.dev.enabled || self.trace_panel.enabled {
                overlay_pass_ran = true;
                // The engine owns the overlay clear (2026-07-19): fresh
                // entries repaint the canvas from scratch this frame.
                overlay_canvas.clear_transparent();
                // Tell the platform to refresh its retained GPU frame
                // items / re-upload the CPU texture (D089/D109 overlay-GPU
                // support, 2026-08-04) — without this, `frame_dirty` only
                // has its initial `true` value from canvas construction
                // and never flips again (nothing else calls
                // `mark_frame_dirty` on the OVERLAY canvas specifically,
                // unlike the base canvas at this function's own paint
                // site), so a GPU-shapes platform would show only the
                // very first overlay ever painted, frozen, forever after.
                // Caught by a unit test before this ever ran live.
                overlay_canvas.mark_frame_dirty();
                let mut ov_recorder = rosace_render::PictureRecorder::new();

                // ── Promoted layers (portals) ────────────────────────
                // Painted during the main walk into their own pictures and
                // deliberately NOT spliced into their parents' streams;
                // replayed here, above the base canvas, in declaration order.
                // Their commands are already in screen space — the promotion
                // boundary resolved that once, so nothing is translated here.
                for node in &promoted {
                    let p = tree_ref.node(*node).promoted.as_ref()
                        .expect("promoted_nodes only returns nodes that have one");
                    for cmd in &p.picture.commands {
                        ov_recorder.push(cmd.clone());
                    }
                }

                // ── DevTools inspector chrome (D123/O2) ──────────────────
                // Drawn LAST, above app overlays. Reads the same read-only
                // `inspect()` snapshot the picker uses, so what it outlines
                // is exactly what `pick` selected.
                if self.dev.enabled {
                    let snapshot = tree_ref.inspect();
                    draw_dev_inspector(
                        &mut ov_recorder, &snapshot, &self.dev, self.cursor,
                        win_w, win_h, font,
                    );
                }

                // Play overlay picture into the dedicated overlay canvas (D078).
                let ov_picture = ov_recorder.finish();
                overlay_canvas.play_picture(&ov_picture, font);
            }
        }
        // A frame that composited nothing must clear the overlay canvas, or
        // the last promoted layer stays on screen after it closes.
        //
        // This used to be an elaborate rule about when to keep or drop a
        // flattened route list, with a documented gap (an unrelated atom write
        // could clear an open menu). Promoted layers made it obsolete: they
        // live on their nodes, so they persist and disappear exactly when
        // their declaring widget does, with nothing to reconcile.
        if !overlay_pass_ran && content_changed && overlay_canvas.has_drawn() {
            overlay_canvas.clear_transparent();
        }

        // The TransformLayer pass lived here: every transform entry was
        // rendered into its own content-sized canvas and published as a
        // placed GPU layer the compositor sampled at a scroll offset. One
        // scroll path records into the ordinary paint stream now, so nothing
        // produces entries and there is nothing to publish.

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
        // ── Drain context-menu actions + long-press word-select (D116 Step 7)
        self.drain_context_menu();
        self.drain_long_press_select();

        // ── Route events — structural z-order (D092) ────────────────────
        // Overlay routes first (topmost entry first): the entry's own
        // regions win; its surface absorbs; outside taps fire the scrim
        // dismiss or are swallowed by Block; PassThrough falls through.
        // Anything unclaimed goes to the render-tree walk, where later
        // siblings (painted on top) win structurally.
        // ── Accessibility actions (D132 follow-up) ──────────────────────
        //
        // A screen-reader user selects a control and issues an *activate*
        // action rather than tapping it. Requests are queued by the platform
        // bridges (which sit below this crate and cannot call dispatch) and
        // drained here, on the UI thread, once per frame.
        //
        // They are turned into ORDINARY input events aimed at the node's
        // centre rather than invoking its callback directly. That way an
        // activation does exactly what a tap does — press state, focus
        // changes, whatever overlay it opens — instead of becoming a second
        // dispatch path that drifts from the real one.
        let a11y_events = self.drain_a11y_actions();
        for event in events.iter().chain(a11y_events.iter()) {
            // ── DevTools element inspector interception (D123/O2) ────────
            // F12 toggles it regardless of state; while enabled it OWNS the
            // pointer — hover highlights, click selects, Escape steps back
            // out — and the event never reaches the app. `pick` considers
            // EVERY painted node (not just interactive ones), so a plain
            // Container/Text is selectable.
            // (DevTools FAB/tab taps are handled by the widget overlay's own
            // hit regions now — no manual rect math here.)
            match event {
                rosace_platform::InputEvent::KeyDown { key: rosace_platform::Key::F12 } => {
                    self.dev.toggle();
                    self.forced_repaint = true;
                    rosace_state::request_frame();
                    continue;
                }
                // F11 toggles the DevTools trace/network activity panel (D123/O5).
                rosace_platform::InputEvent::KeyDown { key: rosace_platform::Key::F11 } => {
                    self.trace_panel.toggle();
                    self.forced_repaint = true;
                    rosace_state::request_frame();
                    continue;
                }
                _ if self.dev.enabled => {
                    match event {
                        rosace_platform::InputEvent::MouseMove { x, y } => {
                            self.cursor = (*x, *y);
                            let hit = self.render_tree.borrow().pick(*x, *y);
                            if self.dev.set_hover(hit) {
                                self.forced_repaint = true;
                                rosace_state::request_frame();
                            }
                            continue;
                        }
                        rosace_platform::InputEvent::MouseDown {
                            x, y, button: rosace_platform::MouseButton::Left
                        } => {
                            self.cursor = (*x, *y);
                            let hit = self.render_tree.borrow().pick(*x, *y);
                            self.dev.select(hit);
                            self.forced_repaint = true;
                            rosace_state::request_frame();
                            continue;
                        }
                        rosace_platform::InputEvent::KeyDown { key: rosace_platform::Key::Escape } => {
                            self.dev.on_escape();
                            self.forced_repaint = true;
                            rosace_state::request_frame();
                            continue;
                        }
                        // Swallow every other input while inspecting so the
                        // app underneath stays frozen for measurement.
                        rosace_platform::InputEvent::MouseUp { .. }
                        | rosace_platform::InputEvent::MouseDown { .. }
                        | rosace_platform::InputEvent::KeyDown { .. }
                        | rosace_platform::InputEvent::KeyUp { .. }
                        | rosace_platform::InputEvent::Scroll { .. } => continue,
                        _ => {}
                    }
                }
                _ => {}
            }
            match event {
                rosace_platform::InputEvent::MouseDown {
                    x, y, button: rosace_platform::MouseButton::Left
                } => {
                    rosace_widgets::tree::set_pointer(*x, *y);
                    // Cancel any still-in-flight long-press timer from a
                    // PREVIOUS press before considering a new one — a
                    // fresh MouseDown always supersedes an unreleased
                    // earlier one. Pre-existing gap (predates this step):
                    // overwriting `self.lp_cancel` with a new token
                    // (further below, when arming a new press) never
                    // actually cancelled the OLD spawned thread's own
                    // copy of the old token, so an old timer could still
                    // fire later against whatever is focused by then.
                    if let Some(c) = &self.lp_cancel {
                        c.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    // Overlays used to be dispatched here, from a flattened
                    // route list, ahead of the main tree. They are promoted
                    // nodes now and `hit_test` tries promoted layers first, so
                    // the ordinary path below already gives them priority —
                    // including press DEFERRAL, since `pending_press` defers
                    // every hit to MouseUp regardless of where it came from.
                    let mut handled = false;
                    // Selection-handle grab (D116 Step 7) takes priority
                    // over a normal click/drag — landing within
                    // `HANDLE_HIT_RADIUS` of either selection endpoint's
                    // on-screen anchor grabs that handle instead of
                    // repositioning the caret or starting a fresh
                    // drag-select.
                    if !handled {
                        if let Some((node_id, _, state, _)) = self.focused_editable() {
                            if let Some((s, e)) = state.selection_range() {
                                let tree = self.render_tree.borrow();
                                if let Some(editable) = tree.node(node_id).editable.as_ref() {
                                    // Glass selection (theme SelectionStyle,
                                    // single-line fields): the visible grips
                                    // hang at the magnified pill's edges, not
                                    // at the raw endpoints — grab THERE, via
                                    // the SAME `glass_lens` geometry the
                                    // widget paints with, so visuals and
                                    // anchors can never drift.
                                    let glass_style = (!editable.multiline)
                                        .then(|| rosace_theme::use_theme().ext::<rosace_widgets::SelectionStyle>().cloned())
                                        .flatten()
                                        .filter(|st| st.kind == rosace_widgets::SelectionKind::Glass);
                                    let glass = glass_style.and_then(|st| {
                                        let line = editable.layout.lines.first()?;
                                        let x0 = editable.layout.x_of(s)?;
                                        let x1 = editable.layout.x_of(e)?;
                                        Some(st.glass_lens(x0, x1, line.y, line.height))
                                    });
                                    let anchor = |idx: usize, is_head: bool| -> Option<(f32, f32)> {
                                        match &glass {
                                            Some(g) => Some((
                                                if is_head { g.bar_x.1 } else { g.bar_x.0 },
                                                g.grip_y,
                                            )),
                                            None => handle_anchor(&editable.layout, idx),
                                        }
                                    };
                                    let hit = [(s, false), (e, true)].into_iter().find(|&(idx, is_head)| {
                                        anchor(idx, is_head)
                                            .is_some_and(|(hx, hy)| {
                                                (hx - *x).powi(2) + (hy - *y).powi(2)
                                                    <= HANDLE_HIT_RADIUS.powi(2)
                                            })
                                    });
                                    if let Some((_, is_head)) = hit {
                                        drop(tree);
                                        let fixed = if is_head { s } else { e };
                                        self.handle_drag = Some((node_id, is_head, fixed));
                                        handled = true;
                                    }
                                }
                            }
                        }
                    }
                    // Captured below when an editable is hit, so the
                    // long-press-to-select-word timer (armed further down,
                    // alongside the generic `LongPressable` one) knows
                    // which node/position to select if the press holds.
                    let mut editable_press: Option<(NodeId, usize)> = None;
                    if !handled {
                        let (leaf, chain) = self.render_tree.borrow().hit_test(*x, *y);
                        // Always captured, regardless of what the leaf
                        // resolves to — see `pending_scroll_chain`'s own
                        // doc comment for why (blank scrollable space has
                        // no leaf at all but must still be draggable).
                        self.pending_scroll_chain = chain;
                        self.last_chain_point = Some((*x, *y));
                        // Report what the hit-test resolved to. The gesture
                        // panel has shipped empty since D123, and "the tap
                        // did nothing" is the single most common report on
                        // Android — a trace showing whether a handler was
                        // found AT ALL separates "no hit region there" from
                        // "found it and the callback did nothing", which are
                        // very different bugs and currently indistinguishable
                        // from the outside.
                        #[cfg(debug_assertions)]
                        {
                            use rosace_trace::{event::{GestureKind, RosaceTrace}, trace};
                            if let Some((_, positional)) = &leaf {
                                trace!(RosaceTrace::GestureReceived {
                                    kind: if *positional { GestureKind::Drag } else { GestureKind::Tap },
                                    handler: root_component_id,
                                });
                            }
                        }
                        if let Some((cb, positional)) = leaf {
                            if positional {
                                // Positional hits (slider thumbs, pickers)
                                // are drag gestures, not taps — they jump to
                                // the touch point immediately and stream
                                // MouseMove, same as before this fix.
                                cb(*x, *y);
                                self.active_drag = Some(cb);
                            } else {
                                // Plain click/tap (Button, ListTile, FAB, …):
                                // deferred to MouseUp, see `pending_press`.
                                self.pending_press = Some(cb);
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

                            // Double/triple-click detection (D116 Step 3)
                            // — same node, within the time+distance slop
                            // of the previous click, increments the run;
                            // anything else starts a fresh single click.
                            let (lx, ly) = self.last_click_pos;
                            let same_spot = (*x - lx).abs() <= DOUBLE_CLICK_SLOP
                                && (*y - ly).abs() <= DOUBLE_CLICK_SLOP;
                            if self.last_click_node == Some(node_id)
                                && now - self.last_click_at <= DOUBLE_CLICK_SECS
                                && same_spot
                            {
                                self.click_count = (self.click_count + 1).min(3);
                            } else {
                                self.click_count = 1;
                            }
                            self.last_click_at = now;
                            self.last_click_pos = (*x, *y);
                            self.last_click_node = Some(node_id);

                            let mut tree = self.render_tree.borrow_mut();
                            let node = tree.node_mut(node_id);
                            // Real click->glyph placement (D116 Step 3):
                            // the `TextLayoutSnapshot` built at paint time
                            // dissolves the `!Sync` FontCache wall Step 1
                            // worked around by always placing the caret at
                            // the end — dispatch queries plain geometry
                            // data here, no font access needed.
                            let mut drag_anchor = None;
                            if let Some(editable) = &node.editable {
                                let pos = editable.layout.position_at(*x, *y);
                                let selection = match self.click_count {
                                    1 => text_edit::Selection::single(pos),
                                    2 => {
                                        let (s, e) = text_edit::word_range_at(&editable.value, pos);
                                        text_edit::Selection::range(s, e)
                                    }
                                    _ => {
                                        let (s, e) = editable.layout.line_range_at(pos);
                                        text_edit::Selection::range(s, e)
                                    }
                                };
                                node.text_edit.selection = selection;
                                drag_anchor = Some(pos);
                            }
                            node.text_edit.last_edit_at = now;
                            drop(tree);
                            // Single clicks arm mouse drag-to-select;
                            // double/triple clicks stand on their own
                            // (dragging after a word/line select would
                            // fight the just-made selection).
                            if self.click_count == 1 {
                                if let Some(pos) = drag_anchor {
                                    self.text_drag = Some((node_id, pos));
                                    // Also a candidate for long-press-to-
                                    // select-word (D116 Step 7) — only for
                                    // a fresh single press, same reasoning
                                    // as the drag arm above.
                                    editable_press = Some((node_id, pos));
                                }
                            }
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
                            // Clear the stale IME anchor (D116 Step 6) — an
                            // unfocused editable must not leave the OS's
                            // CJK candidate window pinned to where the
                            // caret used to be.
                            rosace_core::set_ime_cursor_area(None);
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
                        fire_after_ms(500, move || {
                            if !cancel.load(Ordering::Relaxed) {
                                cb();
                                rosace_state::request_frame();
                            }
                        });
                    }
                    // Long-press-to-select-word on an editable (D116 Step
                    // 7) — the spawned thread can't touch `RenderTree`/
                    // `FontCache` directly (same wall as everything else
                    // editable-related here), so it just records the
                    // result for `drain_long_press_select` to apply on the
                    // main thread next frame.
                    if let Some((node_id, pos)) = editable_press {
                        use std::sync::atomic::{AtomicBool, Ordering};
                        let cancel = Arc::new(AtomicBool::new(false));
                        self.lp_cancel = Some(cancel.clone());
                        let pending = self.pending_long_press_select.clone();
                        fire_after_ms(LONG_PRESS_SELECT_MS, move || {
                            if !cancel.load(Ordering::Relaxed) {
                                *pending.lock().unwrap() = Some((node_id, pos));
                                rosace_state::request_frame();
                            }
                        });
                    }
                }
                // Desktop right-click context menu (D116 Step 7) — the
                // FFI/mobile-touch equivalent (long-press-outside-a-
                // selection could open the same menu) is a named follow-up,
                // not required by this step's exit bar (right-click has no
                // touch analogue on its own; mobile gets the menu via
                // Step 6's FFI work in a later real device session).
                rosace_platform::InputEvent::MouseDown {
                    x, y, button: rosace_platform::MouseButton::Right
                } => {
                    if let Some(node_id) = self.render_tree.borrow().editable_test(*x, *y) {
                        let focus_id = self.render_tree.borrow()
                            .node(node_id).focus_node.as_ref().map(|f| f.id());
                        if let Some(fid) = focus_id {
                            self.focus_manager.focus_specific(fid);
                        }
                        self.context_menu = Some((node_id, (*x, *y)));
                        self.forced_repaint = true;
                        rosace_state::request_frame();
                    }
                }
                rosace_platform::InputEvent::MouseMove { x, y } => {
                    use std::sync::atomic::Ordering;
                    rosace_widgets::tree::set_pointer(*x, *y);
                    if let Some(cb) = &self.active_drag {
                        cb(*x, *y);
                    }
                    // Mouse drag-to-select over an editable (D116 Step 3)
                    // — extend `Selection::range(anchor, head)` from the
                    // node's own `TextLayoutSnapshot`, re-queried every
                    // move since the widget doesn't change size mid-drag.
                    if let Some((node_id, anchor)) = self.text_drag {
                        let mut tree = self.render_tree.borrow_mut();
                        let node = tree.node_mut(node_id);
                        if let Some(editable) = &node.editable {
                            let head = editable.layout.position_at(*x, *y);
                            node.text_edit.selection = text_edit::Selection::range(anchor, head);
                        }
                        drop(tree);
                        self.forced_repaint = true;
                        rosace_state::request_frame();
                    }
                    // Selection-handle drag (D116 Step 7) — the dragged
                    // endpoint follows the pointer via `position_at`; the
                    // OTHER endpoint (captured at grab time) stays fixed.
                    if let Some((node_id, is_head, fixed)) = self.handle_drag {
                        let mut tree = self.render_tree.borrow_mut();
                        let node = tree.node_mut(node_id);
                        if let Some(editable) = &node.editable {
                            let moving = editable.layout.position_at(*x, *y);
                            node.text_edit.selection = if is_head {
                                text_edit::Selection::range(fixed, moving)
                            } else {
                                text_edit::Selection::range(moving, fixed)
                            };
                        }
                        drop(tree);
                        self.forced_repaint = true;
                        rosace_state::request_frame();
                    }
                    self.last_pointer = Some((*x, *y));
                    // Hover tracking — repaints only the changed nodes.
                    let target = self.render_tree.borrow().hover_test(*x, *y);
                    let changed = self.render_tree.borrow_mut().set_hover(target);
                    if changed {
                        self.forced_repaint = true;
                        rosace_state::request_frame();
                    }
                    // Movement past the slop cancels a pending long-press AND
                    // a pending plain click/tap — this is what lets a scroll
                    // or drag that starts on top of a Button/ListTile win
                    // over that widget's on_press instead of also firing it.
                    if let Some((ox, oy)) = self.press_origin {
                        if (x - ox).abs() > 8.0 || (y - oy).abs() > 8.0 {
                            if let Some(c) = &self.lp_cancel { c.store(true, Ordering::Relaxed); }
                            self.lp_cancel = None;
                            self.press_origin = None;
                            self.pending_press = None;
                            self.pending_overlay_press = None;
                        }
                    }
                    // Nested-scroll chain walk (D-NESTED-SCROLL, 2026-08-02):
                    // only once there's no competing tap still waiting to
                    // see if it survives (the block above just resolved
                    // that, same event) and no unrelated positional grab
                    // (slider) already owns this gesture. Tries the
                    // innermost scrollable ancestor first each move; a
                    // link only gets a turn once every link before it in
                    // the chain declined (already exhausted in this exact
                    // direction) — see `ScrollHandler`'s own doc for the
                    // full contract.
                    if self.pending_press.is_none() && self.active_drag.is_none() && !self.pending_scroll_chain.is_empty() {
                        let (lx, ly) = self.last_chain_point.unwrap_or((*x, *y));
                        let (dx, dy) = (x - lx, y - ly);
                        if dx != 0.0 || dy != 0.0 {
                            // TWO PASSES.
                            //
                            // First, innermost outward, everyone hard-clamped
                            // to their real bounds: a drag should scroll
                            // whatever can actually scroll.
                            //
                            // Only if nobody could, again allowing overscroll
                            // — and OUTERMOST FIRST this time. Overscroll is
                            // what happens when nothing else wants the
                            // movement, and the outer context should decide
                            // what it means: a `PullToRefresh` wrapping a list
                            // must get the pull rather than watch the list
                            // rubber-band. Restarting pass two from the
                            // innermost would hand it straight back to the
                            // Bounce view and pull-to-refresh could never
                            // fire, which is exactly what it did.
                            //
                            // A lone scrollable is unaffected: it declines
                            // pass one at its edge and takes pass two itself.
                            let consumed = self
                                .pending_scroll_chain
                                .iter()
                                .any(|h| h(dx, dy, false));
                            if !consumed {
                                for handler in self.pending_scroll_chain.iter().rev() {
                                    if handler(dx, dy, true) {
                                        break;
                                    }
                                }
                            }
                            self.forced_repaint = true;
                            rosace_state::request_frame();
                        }
                        self.last_chain_point = Some((*x, *y));
                    }
                }
                rosace_platform::InputEvent::MouseUp { x, y, .. } => {
                    use std::sync::atomic::Ordering;
                    self.active_drag = None;
                    self.text_drag = None;
                    self.handle_drag = None;
                    self.pending_scroll_chain.clear();
                    self.last_chain_point = None;
                    if let Some(c) = &self.lp_cancel { c.store(true, Ordering::Relaxed); }
                    self.lp_cancel = None;
                    self.press_origin = None;
                    if self.render_tree.borrow_mut().set_pressed(None) {
                        self.forced_repaint = true;
                        rosace_state::request_frame();
                    }
                    // Click/tap fires HERE, on release — not on MouseDown —
                    // so scrolling or dragging past a Button/ListTile never
                    // also triggers its on_press. Survives only if the
                    // pointer stayed within the slop the whole time (see
                    // the MouseMove cancellation above); otherwise this is
                    // already `None`.
                    if let Some(cb) = self.pending_press.take() {
                        cb(*x, *y);
                    }
                    if let Some(cb) = self.pending_overlay_press.take() {
                        cb();
                    }
                }
                rosace_platform::InputEvent::Scroll { x, y, delta_x, delta_y } => {
                    let handled = false;
                    if !handled {
                        let cb = self.render_tree.borrow().scroll_test(*x, *y, *delta_x, *delta_y);
                        if let Some(cb) = cb {
                            cb(*delta_x, *delta_y);
                        }
                    }
                    // Re-resolve hover AFTER this frame paints: the offset has
                    // changed but the rects have not moved yet, so testing now
                    // would just re-find the row the cursor is already on.
                    self.last_pointer = Some((*x, *y));
                    self.needs_rehover = true;
                }
                rosace_platform::InputEvent::Pinch { x, y, delta } => {
                    if let Some(cb) = self.render_tree.borrow().zoom_test(*x, *y) {
                        cb(*delta);
                    }
                }
                rosace_platform::InputEvent::BackPressed => {
                    // Resolution order (rosace_core::nav_back):
                    //   1. a dismissible overlay closes
                    //   2. else the registered navigator pops
                    //   3. else nothing — the platform does its default
                    //
                    // Overlays first is what stops the classic bug of a
                    // single back press closing a dialog AND the screen
                    // underneath it.
                    let dismissed = self.render_tree.borrow().topmost_dismisser()
                        .map(|on_tap| { on_tap(); true })
                        .unwrap_or(false);
                    let handled = dismissed || rosace_core::nav_back::dispatch_back();
                    self.last_back_handled = handled;
                }
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Escape
                } => {
                    // Dismiss the topmost overlay that has a scrim
                    // dismisser (dialog, sheet, dropdown).
                    if let Some(on_tap) = self.render_tree.borrow().topmost_dismisser() {
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
                    self.cancel_pending_press();
                    if !self.ctrl_held && !self.meta_held && !character.is_control() {
                        if let Some((node_id, value, state, _)) = self.focused_editable() {
                            let now = rosace_widgets::tree::anim_clock();
                            let (nv, ns) = text_edit::insert_char(&value, &state, *character, now);
                            self.commit_text_edit(node_id, &value, nv, ns);
                        }
                    }
                }
                // Real OS IME composition (D116 Step 6) — `rosace-platform`
                // translates winit's `WindowEvent::Ime` into
                // `rosace_platform::ime::ImeEvent` (the wire payload, reused as-is —
                // see `InputEvent::Ime`'s doc comment for why that crate is
                // safe to depend on from the platform layer). `Enabled` is
                // pure state (nothing to do — no field-scoped enable/disable
                // exists yet, see `app.rs`'s `set_ime_allowed` comment).
                rosace_platform::InputEvent::Ime(ime_event) => {
                    if let Some((node_id, value, state, _)) = self.focused_editable() {
                        let now = rosace_widgets::tree::anim_clock();
                        match ime_event {
                            rosace_platform::ime::ImeEvent::Preedit { text, cursor_range } => {
                                // winit's cursor_range is a BYTE range into
                                // `text` itself; the edit core is
                                // char-indexed (see text_edit.rs's module
                                // doc) — convert once here.
                                let cursor_in_text = cursor_range.map(|(b, _)| text[..b.min(text.len())].chars().count());
                                let (nv, ns) = text_edit::ime_set_preedit(&value, &state, text, cursor_in_text, now);
                                self.commit_text_edit(node_id, &value, nv, ns);
                            }
                            rosace_platform::ime::ImeEvent::Commit(text) => {
                                let (nv, ns) = text_edit::ime_commit(&value, &state, text, now);
                                self.commit_text_edit(node_id, &value, nv, ns);
                            }
                            rosace_platform::ime::ImeEvent::Enabled | rosace_platform::ime::ImeEvent::Disabled => {}
                        }
                    }
                }
                // App-lifecycle transition (D042/D110, Phase 29 Step 1) —
                // sent by a mobile native host over the FFI bridge
                // (`RSC_EVENT_LIFECYCLE_*`). One write to the global atom;
                // components subscribed via `use_app_lifecycle` are marked
                // dirty and re-render on this same frame's rebuild pass.
                rosace_platform::InputEvent::Lifecycle(state) => {
                    rosace_core::set_app_lifecycle(*state);
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
                                        let _ = rosace_widgets::clipboard::SystemClipboard::new().write(&sel);
                                    }
                                }
                                'x' => {
                                    if let Some(sel) = text_edit::selected_text(&value, &state) {
                                        let _ = rosace_widgets::clipboard::SystemClipboard::new().write(&sel);
                                        let (nv, ns) = text_edit::backspace(&value, &state, now);
                                        self.commit_text_edit(node_id, &value, nv, ns);
                                    }
                                }
                                'v' => {
                                    if let Some(text) = rosace_widgets::clipboard::SystemClipboard::new().read() {
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
                // Enter inserts a real newline — but ONLY for a multiline
                // field (`TextArea`, D116 Step 4); a single-line
                // `TextInput` has no editing meaning for Enter today (a
                // future submit-on-Enter affordance is a separate,
                // opt-in concern, not implied by this).
                rosace_platform::InputEvent::KeyDown {
                    key: rosace_platform::Key::Enter
                } => {
                    if let Some((node_id, value, state, multiline)) = self.focused_editable() {
                        if multiline {
                            let now = rosace_widgets::tree::anim_clock();
                            let (nv, ns) = text_edit::insert_char(&value, &state, '\n', now);
                            self.commit_text_edit(node_id, &value, nv, ns);
                        }
                    }
                }
                // Up/Down cross wrapped lines with goal-column memory
                // (D116 Step 4) — this needs real glyph geometry (which
                // line is "above", which boundary on it is nearest the
                // caret's x), so unlike every other movement command it
                // can't go through `apply_command` (pure string/index
                // math, no layout access); it queries the node's own
                // `TextLayoutSnapshot` directly, same wall-dissolving
                // pattern Step 3's click dispatch uses. Single-line
                // `TextInput` has only one line, so this intentionally
                // no-ops there (no "jump to Home/End" surprise).
                rosace_platform::InputEvent::KeyDown {
                    key: k @ (rosace_platform::Key::ArrowUp | rosace_platform::Key::ArrowDown)
                } => {
                    if let Some((node_id, _value, state, multiline)) = self.focused_editable() {
                        if multiline {
                            let now = rosace_widgets::tree::anim_clock();
                            let mut tree = self.render_tree.borrow_mut();
                            let node = tree.node_mut(node_id);
                            if let Some(editable) = &node.editable {
                                let lines = &editable.layout.lines;
                                if !lines.is_empty() {
                                    let cursor = state.cursor();
                                    let cur_line = lines.iter()
                                        .position(|l| cursor >= l.char_range.0 && cursor <= l.char_range.1)
                                        .unwrap_or(0);
                                    let goal_x = state.goal_x.unwrap_or_else(|| {
                                        editable.layout.x_of(cursor)
                                            .unwrap_or_else(|| lines[cur_line].boundary_x.first().copied().unwrap_or(0.0))
                                    });
                                    let going_up = matches!(k, rosace_platform::Key::ArrowUp);
                                    let target_line = if going_up {
                                        cur_line.checked_sub(1)
                                    } else if cur_line + 1 < lines.len() {
                                        Some(cur_line + 1)
                                    } else {
                                        None
                                    };
                                    let new_cursor = match target_line {
                                        Some(ti) => editable.layout.position_at(goal_x, lines[ti].y + 1.0),
                                        // No line above the first / below the
                                        // last — land at that line's own
                                        // start/end (real editors' convention)
                                        // rather than doing nothing.
                                        None if going_up => lines[cur_line].char_range.0,
                                        None => lines[cur_line].char_range.1,
                                    };
                                    let anchor = if self.shift_held { state.selection.primary().anchor } else { new_cursor };
                                    let mut new_state = state.with_selection(
                                        text_edit::Selection::range(anchor, new_cursor), now,
                                    );
                                    new_state.goal_x = Some(goal_x);
                                    node.text_edit = new_state;
                                    drop(tree);
                                    self.forced_repaint = true;
                                    rosace_state::request_frame();
                                }
                            }
                        }
                    }
                }
                // Movement/deletion — one generic arm through the
                // Key->Command keymap (`command_for_key`, D116 layer 4)
                // instead of one match arm per key. Escape/Tab/Shift/
                // Control/Meta/Alt/Char/Enter/Up/Down already claimed
                // their own events above, so `key` here is only ever
                // Backspace/Delete/Left/Right/Home/End/something unbound.
                rosace_platform::InputEvent::KeyDown { key } => {
                    self.cancel_pending_press();
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
    use rosace_core::Context;
    use rosace_widgets::tree::BoxedWidget;
    use rosace_widgets::IntoBoxedWidget as _;
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
        fn build(&self, _ctx: &mut Context) -> BoxedWidget {
            Button::new("Press me").variant(ButtonVariant::Primary).on_press(|| {}).boxed()
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
        fn build(&self, _ctx: &mut Context) -> BoxedWidget {
            // Content taller than `MAX_TL_DIM` (4096) keeps plain
            // `ScrollView::new` on the base (CPU) path automatically
            // (`should_auto_gpu` requires `extent <= MAX_TL_DIM`) — the
            // GPU-layer path is explicitly out of scope for Step 2's
            // drag/momentum (see `.steering/PHASE_26.md`), so this avoids
            // silently exercising the wrong path.
            rosace_widgets::tree::ScrollView::new(rosace_widgets::tree::Spacer::gap(200.0, 5000.0))
                .boxed()
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

    #[derive(Clone, Copy, PartialEq, Hash)]
    enum NavScreen { A, B }

    /// Root with a two-screen `ScreenNav`, matching the real `rsc new`
    /// codegen shape exactly (`ScreenTransitionView::new(body,
    /// nav.current_key(), outgoing, nav.previous_key(),
    /// nav.transition_handle(), nav.stack_keys())` in place of handing
    /// `body` straight to a container) — the real integration point for
    /// Step 3. Both screens are
    /// `Button`s (not bare `Text`) so both always declare real `SemanticsProps`
    /// regardless of `on_press`, giving the test a reliable signal for
    /// "is this screen's content actually painted this frame."
    struct NavRoot;
    impl Component for NavRoot {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let nav = rosace_nav::ScreenNav::new(ctx, NavScreen::A);
            let build_screen = {
                let nav = nav.clone();
                move |s: NavScreen| -> rosace_widgets::tree::BoxedWidget {
                    match s {
                        NavScreen::A => {
                            let nav = nav.clone();
                            Arc::new(Button::new("Screen A").on_press(move || { nav.push(NavScreen::B); }))
                        }
                        NavScreen::B => Arc::new(Button::new("Screen B")),
                    }
                }
            };
            let screen = nav.current().unwrap_or(NavScreen::A);
            let body = build_screen(screen);
            let outgoing = nav.previous().map(build_screen);
            rosace_widgets::tree::ScreenTransitionView::new(
                body, nav.current_key(), outgoing, nav.previous_key(),
                nav.transition_handle(), nav.stack_keys(),
            ).boxed()
        }
    }

    fn headless_nav_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(NavRoot), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200))
    }

    /// Regression coverage for the keyed-persistence fix (2026-08-01,
    /// real navigation + trackpad testing): Screen A has a real
    /// `ScrollView` (implicit per-node controller, exactly the path a real
    /// app uses — NOT an app-owned `ScrollController` threaded down, which
    /// would trivially survive regardless and not exercise the bug at all).
    /// Screen B is scrollable too, so both screens have a `scroll_ctrl` in
    /// the arena simultaneously — proof the fix doesn't just avoid a crash,
    /// it keeps each screen's position independently correct.
    #[derive(Clone, Copy, PartialEq, Hash)]
    enum ScrollNavScreen { A, B }

    struct ScrollNavRoot;
    impl Component for ScrollNavRoot {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let nav = rosace_nav::ScreenNav::new(ctx, ScrollNavScreen::A);
            // The nav button sits OUTSIDE the ScrollView (a fixed-position
            // sibling, like a real app's persistent AppBar/nav button) so
            // its on-screen position — and thus the click test coordinate
            // below — never shifts as the sibling ScrollView's content
            // scrolls. Content is a real 2000px-tall Container, so the
            // resulting offset is a genuine steady scroll position, not a
            // transient `Bounce` overscroll that would spring back to 0 on
            // its own before the click ever lands (this content is short
            // enough to overflow the 200px test viewport either way).
            let build_screen = {
                let nav = nav.clone();
                move |s: ScrollNavScreen| -> rosace_widgets::tree::BoxedWidget {
                    match s {
                        ScrollNavScreen::A => {
                            let nav = nav.clone();
                            Arc::new(Column::new()
                                .child(Button::new("Go to B").on_press(move || { nav.push(ScrollNavScreen::B); }))
                                .child(rosace_widgets::tree::Expanded::new(
                                    rosace_widgets::tree::ScrollView::new(Container::new().height(2000.0)),
                                )))
                        }
                        ScrollNavScreen::B => {
                            let nav = nav.clone();
                            Arc::new(Column::new()
                                .child(Button::new("Back to A").on_press(move || { nav.pop(); }))
                                .child(rosace_widgets::tree::Expanded::new(
                                    rosace_widgets::tree::ScrollView::new(Container::new().height(2000.0)),
                                )))
                        }
                    }
                }
            };
            let screen = nav.current().unwrap_or(ScrollNavScreen::A);
            let body = build_screen(screen);
            let outgoing = nav.previous().map(build_screen);
            rosace_widgets::tree::ScreenTransitionView::new(
                body, nav.current_key(), outgoing, nav.previous_key(),
                nav.transition_handle(), nav.stack_keys(),
            ).boxed()
        }
    }

    fn headless_scroll_nav_engine() -> (FrameEngine, SkiaCanvas, SkiaCanvas) {
        let engine = FrameEngine::new(Box::new(ScrollNavRoot), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200))
    }

    /// Unlike `scroll_offset` (which grabs the FIRST `scroll_ctrl` found by
    /// raw arena order — ambiguous once more than one screen has one, as in
    /// the keyed-persistence test below), this walks from the root through
    /// `children` only — the same traversal hit-testing/semantics/painting
    /// all use. Since `ScreenTransitionView` only places the CURRENTLY
    /// showing screen's key into its own positional child slot each frame
    /// (steady state), an inactive-but-still-cached screen's subtree is
    /// simply unreachable this way — so this always returns at most the
    /// active screen's own offset, never a stale/inactive one.
    fn reachable_scroll_offsets(engine: &FrameEngine) -> Vec<[f32; 2]> {
        fn walk(tree: &rosace_widgets::tree::RenderTree, id: rosace_widgets::tree::NodeId, out: &mut Vec<[f32; 2]>) {
            let n = tree.node(id);
            if let Some(c) = &n.scroll_ctrl { out.push(c.offset()); }
            for &child in &n.children { walk(tree, child, out); }
        }
        let tree = engine.render_tree.borrow();
        let mut out = Vec::new();
        walk(&tree, rosace_widgets::tree::RenderTree::ROOT, &mut out);
        out
    }

    #[test]
    fn scroll_position_survives_navigating_away_and_back() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_theme::provider::set_animations(false); // settle transitions instantly
        rosace_animate::set_frame_dt(1.0 / 60.0);
        let (mut engine, mut canvas, mut overlay) = headless_scroll_nav_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(reachable_scroll_offsets(&engine), vec![[0.0, 0.0]]);

        // Scroll Screen A's ScrollView down — target point is well inside
        // its viewport (below the fixed nav button at the top of the Column).
        for _ in 0..10 {
            let scroll = rosace_platform::InputEvent::Scroll {
                x: 100.0, y: 150.0, delta_x: 0.0, delta_y: -80.0,
            };
            engine.paint(&mut canvas, &mut overlay, &[scroll]);
        }
        let a_offset = reachable_scroll_offsets(&engine)[0];
        assert!(a_offset[1] > 0.0, "scrolling A must have moved its offset, got {a_offset:?}");

        // Navigate to B — click the fixed nav button at the TOP of the
        // Column, a position the sibling ScrollView's own scrolling never
        // shifts. Down then up, per this session's click-on-release fix. A
        // click's event dispatch only marks the nav route dirty; the actual
        // rebuild (swapping in Screen B) happens on the FOLLOWING paint —
        // same two-step pattern every other nav-transition test above uses.
        let click = || {
            [
                rosace_platform::InputEvent::MouseDown {
                    x: 50.0, y: 15.0, button: rosace_platform::MouseButton::Left,
                },
                rosace_platform::InputEvent::MouseUp {
                    x: 50.0, y: 15.0, button: rosace_platform::MouseButton::Left,
                },
            ]
        };
        engine.paint(&mut canvas, &mut overlay, &click());
        engine.paint(&mut canvas, &mut overlay, &[]);
        let on_b = semantic_labels(&engine);
        assert!(on_b.iter().any(|l| l == "Back to A"), "must have navigated to Screen B, got {on_b:?}");

        // Screen B starts unscrolled — its OWN scroll_ctrl, not A's leftover
        // offset aliased onto it (the actual bug this test guards against).
        // A's subtree is offstage right now (see `reachable_scroll_offsets`'
        // own doc comment), so this is unambiguously B's.
        assert_eq!(
            reachable_scroll_offsets(&engine), vec![[0.0, 0.0]],
            "Screen B must start fresh, not inherit A's offset"
        );

        // Navigate back to A.
        engine.paint(&mut canvas, &mut overlay, &click());
        engine.paint(&mut canvas, &mut overlay, &[]);
        let back_on_a = semantic_labels(&engine);
        assert!(back_on_a.iter().any(|l| l == "Go to B"), "must be back on Screen A, got {back_on_a:?}");

        assert_eq!(
            reachable_scroll_offsets(&engine), vec![a_offset],
            "Screen A's scroll position must survive the round trip, not reset to the top"
        );
    }

    /// D132 prerequisite: the semantic tree must carry per-node identity
    /// and geometry, not just label/role. Platform accessibility APIs are
    /// stateful — they hold node references across frames and must be able
    /// to answer "where is this on screen" — whereas the HTML/SEO consumer
    /// (D107) needed neither, which is why both were absent until now.
    /// Asserts against a REAL paint, not a hand-built tree: bounds have to
    /// come from the render tree's laid-out rect.
    #[test]
    fn semantic_nodes_carry_stable_ids_and_real_painted_bounds() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay) = headless_nav_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);

        fn find<'a>(n: &'a rosace_core::SemanticNode, label: &str) -> Option<&'a rosace_core::SemanticNode> {
            if n.label.as_deref() == Some(label) { return Some(n); }
            n.children.iter().find_map(|c| find(c, label))
        }

        let tree = engine.semantics();
        let btn = find(&tree, "Screen A").expect("the button must appear in the semantic tree");

        let id = btn.id.expect("a painted semantic node must carry a stable id");
        let bounds = btn.bounds.expect("a painted semantic node must carry its on-screen rect");

        // Geometry must be real, not a placeholder: the button fills the
        // 300x200 headless canvas under tight constraints, the same shape
        // every other engine test in this file relies on.
        assert!(
            bounds.size.width > 0.0 && bounds.size.height > 0.0,
            "bounds must come from the laid-out rect, got {bounds:?}"
        );

        // Identity must survive a repaint — otherwise a screen reader's
        // cursor and any "press the element named X" automation would be
        // invalidated on every frame.
        engine.paint(&mut canvas, &mut overlay, &[]);
        let again = engine.semantics();
        let btn2 = find(&again, "Screen A").expect("still present after a repaint");
        assert_eq!(btn2.id, Some(id), "the same widget must keep the same semantic id across repaints");
        assert_eq!(btn2.bounds, Some(bounds), "an unmoved widget must report the same bounds");
    }

    /// The `Semantics` widget (D132): apps must be able to annotate content
    /// the framework cannot understand — a hand-painted chart is just pixels
    /// — and to silence decoration that would otherwise be announced twice.
    #[test]
    fn semantics_widget_annotates_a_subtree_and_exclude_removes_it() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct Annotated;
        impl Component for Annotated {
            fn build(&self, _ctx: &mut Context) -> BoxedWidget {
                rosace_widgets::tree::Column::new()
                    // A chart the framework can't read, given meaning by the app.
                    .child(rosace_widgets::tree::Semantics::new(
                        rosace_widgets::tree::Text::new("::chart::"),
                    )
                    .role(rosace_core::Role::Image)
                    .label("Revenue, up 12%"))
                    // Decoration that must NOT be announced, even though the
                    // Text inside it declares semantics of its own.
                    .child(rosace_widgets::tree::Semantics::new(
                        rosace_widgets::tree::Text::new("decorative sparkle"),
                    )
                    .exclude())
                    .boxed()
            }
        }

        let engine = FrameEngine::new(Box::new(Annotated), rosace_render::FontCache::embedded());
        let (mut engine, mut canvas, mut overlay) =
            (engine, SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
        engine.paint(&mut canvas, &mut overlay, &[]);

        let labels = semantic_labels(&engine);
        assert!(
            labels.iter().any(|l| l == "Revenue, up 12%"),
            "the app-supplied label must reach the tree, got {labels:?}"
        );
        assert!(
            !labels.iter().any(|l| l == "decorative sparkle"),
            "an excluded subtree must be silent — its child's own semantics \
             must be pruned too, not just the wrapper: {labels:?}"
        );
    }

    /// Enforces `WIDGET_QUALITY_BAR.md` §5 ("declares a semantic role +
    /// label") as an actual gate rather than a prose checklist.
    ///
    /// This exists because §5 was written long before anything checked it,
    /// and widgets duly shipped without semantics — invisible to every screen
    /// reader and to the platform accessibility bridge (D132).
    ///
    /// It is deliberately **behavioural**: each widget is really painted and
    /// the resulting semantic tree inspected. A grep for `ctx.semantics(` gives
    /// false positives in both directions — `SearchBar` declares none of its
    /// own yet inherits `TextInput`'s by delegation, while a wrapper may hold
    /// the literal call and still contribute nothing.
    ///
    /// Widgets that are correctly SILENT belong in `TRANSPARENT` below, with a
    /// reason. Adding a widget to that list is a deliberate act; forgetting
    /// semantics entirely now fails the build instead of shipping quietly.
    #[test]
    fn widgets_meet_quality_bar_section_5_semantics() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        fn semantic_count(build: fn() -> w::BoxedWidget) -> usize {
            struct R(fn() -> w::BoxedWidget);
            impl Component for R {
                fn build(&self, _c: &mut Context) -> BoxedWidget {
                    (self.0)().boxed()
                }
            }
            let mut e = FrameEngine::new(Box::new(R(build)), rosace_render::FontCache::embedded());
            let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
            e.paint(&mut a, &mut b, &[]);
            fn n(x: &rosace_core::SemanticNode) -> usize {
                (if x.role != rosace_core::Role::Unknown || x.label.is_some() { 1 } else { 0 })
                    + x.children.iter().map(n).sum::<usize>()
            }
            n(&e.semantics())
        }

        /// A widget constructor under test.
        type Build = fn() -> w::BoxedWidget;
        /// `(name, builder)` — expected to announce something.
        type Speaks = (&'static str, Build);
        /// `(name, why it is silent, builder)`.
        type Silent = (&'static str, &'static str, Build);

        // Must announce something: a control, or content a user needs.
        let must_speak: Vec<Speaks> = vec![
            ("Button",           || Arc::new(w::Button::new("Save").on_press(|| {}))),
            ("Checkbox",         || Arc::new(w::Checkbox::new(true))),
            ("Switch",           || Arc::new(w::Switch::new(true))),
            ("Slider",           || Arc::new(w::Slider::new(0.5))),
            ("TextInput",        || Arc::new(w::TextInput::new().placeholder("Email"))),
            ("SearchBar",        || Arc::new(w::SearchBar::new())),
            ("Text",             || Arc::new(w::Text::new("hello"))),
            ("Pressable",        || Arc::new(w::Pressable::new(w::Text::new("tap"), || {}))),
            ("CircularProgress", || Arc::new(w::CircularProgress::new(0.5))),
            ("ProgressBar",      || Arc::new(w::ProgressBar::new(0.5))),
            ("Skeleton",         || Arc::new(w::Skeleton::new())),
            ("Tooltip",          || Arc::new(w::Tooltip::new("Delete", w::Text::new("x")))),
            ("Icon (labelled)",  || Arc::new(w::Icon::new(w::IconKind::Search).semantic_label("Search"))),
            // Added 2026-08-13 by the widget-audit sweep. None of these were
            // listed here at all, so they were not merely silent — they were
            // UNGUARDED, and nothing would have caught them going silent
            // again. That gap is the reason §5 could sit in the Quality Bar
            // for months while widgets shipped mute.
            //
            // NOTE the children: every wrapper here is given a SILENT child
            // (`Spacer`), never a `Text`. `semantic_count` sums the whole
            // subtree, so a wrapper whose child speaks passes this test even
            // when the wrapper itself declares nothing — which is exactly how
            // these went unnoticed. Verified by deleting each widget's
            // `ctx.semantics(..)` and watching this test fail.
            ("ListView",         || Arc::new(w::ListView::builder(3, 40.0, |_| Arc::new(w::Spacer::new(8.0))))),
            ("Menu",             || Arc::new(w::Menu::new().item("Copy", || {}))),
            ("Stepper",          || Arc::new(w::Stepper::new(2))),
            ("Dropdown",         || Arc::new(w::Dropdown::new(vec!["A", "B"], 0, false))),
            ("PullToRefresh",    || Arc::new(w::PullToRefresh::new(w::Spacer::new(8.0)))),
            ("Dismissible",      || Arc::new(w::Dismissible::new(w::Spacer::new(8.0)))),
            ("LongPressable",    || Arc::new(w::LongPressable::new(w::Spacer::new(8.0), || {}))),
        ];

        // Correctly silent, each for a stated reason.
        let transparent: Vec<Silent> = vec![
            ("Icon (bare)", "decorative by default — usually sits beside text that already says it",
             || Arc::new(w::Icon::new(w::IconKind::Search))),
            ("Card", "a styled box; its children carry the meaning",
             || Arc::new(w::Card::new(w::Spacer::new(8.0)))),
            ("Container", "pure layout/decoration",
             || Arc::new(w::Container::new())),
            ("Column", "pure layout", || Arc::new(w::Column::new())),
            ("Row", "pure layout", || Arc::new(w::Row::new())),
            ("Spacer", "empty space", || Arc::new(w::Spacer::new(8.0))),
            ("CustomPaint", "app-supplied pixels — annotate with the Semantics widget",
             || Arc::new(w::CustomPaint::new(|_, _| {}))),
        ];

        let mut failures = Vec::new();
        for (name, f) in must_speak {
            if semantic_count(f) == 0 {
                failures.push(format!(
                    "  {name} declares NO semantics — violates WIDGET_QUALITY_BAR §5"
                ));
            }
        }
        for (name, why, f) in transparent {
            if semantic_count(f) != 0 {
                failures.push(format!(
                    "  {name} is listed as intentionally transparent ({why}) but now declares \
                     semantics — either that is a bug, or move it to `must_speak`"
                ));
            }
        }
        assert!(failures.is_empty(), "WIDGET_QUALITY_BAR §5 violations:\n{}", failures.join("\n"));
    }

    /// User-reported (2026-08-09, showcase item #14): on macOS the tooltip
    /// never appears. Drives a real MouseMove onto the anchor and asserts the
    /// tip actually PAINTS — checked against the overlay canvas, not the
    /// semantic tree, since the tip label now legitimately sits in semantics
    /// even before hover (a screen-reader user can't hover at all).
    #[test]
    fn tooltip_paints_on_hover() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct Tip;
        impl Component for Tip {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                w::Tooltip::new("Delete this item", w::Text::new("target")).boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(Tip), rosace_render::FontCache::embedded());

        let mut canvas = SkiaCanvas::new(300, 200);
        let mut cold = SkiaCanvas::new(300, 200);
        e.paint(&mut canvas, &mut cold, &[]);
        assert!(!cold.has_drawn(), "no tooltip before the pointer arrives");

        // Pointer moves onto the anchor (which fills the canvas).
        let mv = rosace_platform::InputEvent::MouseMove { x: 150.0, y: 100.0 };
        e.paint(&mut canvas, &mut SkiaCanvas::new(300, 200), &[mv]);
        // Hover flips a `forced_repaint` flag, so the tip lands on the FOLLOWING
        // frame — exactly what a real event loop delivers.
        let mut hot = SkiaCanvas::new(300, 200);
        e.paint(&mut canvas, &mut hot, &[]);
        assert!(
            hot.has_drawn(),
            "the tooltip must paint into the overlay layer while its anchor is hovered"
        );
    }

    /// The real-world shape: a tooltip wrapping an INTERACTIVE child.
    /// `Tooltip::new("...", Button::new("Hover me"))` is what the showcase —
    /// and any real app — actually writes, and it is the case the user
    /// reported broken on macOS.
    #[test]
    fn tooltip_paints_when_wrapping_an_interactive_child() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct Tip;
        impl Component for Tip {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                w::Tooltip::new("A helpful tip", w::Button::new("Hover me").on_press(|| {}))
                    .boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(Tip), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(300, 200);
        e.paint(&mut canvas, &mut SkiaCanvas::new(300, 200), &[]);

        let mv = rosace_platform::InputEvent::MouseMove { x: 150.0, y: 100.0 };
        e.paint(&mut canvas, &mut SkiaCanvas::new(300, 200), &[mv]);
        let mut hot = SkiaCanvas::new(300, 200);
        e.paint(&mut canvas, &mut hot, &[]);
        assert!(
            hot.has_drawn(),
            "a tooltip wrapping a Button must still appear: the button registers its own \
             hover region and wins the topmost hover test, so the tooltip's own node never \
             sees hovered() unless hover propagates to ancestors"
        );
    }

    /// User-reported on iOS at ~150% Dynamic Type (2026-08-09): text grew but
    /// the boxes did not, so rows overflowed into their dividers and the FAB
    /// was clipped.
    ///
    /// Cause: `measure_text` applied `text_scale` but `line_height` did not,
    /// so widths tracked the OS setting while heights stayed at 100%. Layout
    /// and paint must agree on one size — this asserts BOTH axes respond.
    #[test]
    fn os_text_scale_grows_line_height_not_just_width() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let font = rosace_render::FontCache::embedded();

        rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
            text_scale: 1.0, ..Default::default()
        });
        let w1 = font.measure_text("Widgets", 16.0);
        let h1 = font.line_height(16.0);

        rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
            text_scale: 2.0, ..Default::default()
        });
        let w2 = font.measure_text("Widgets", 16.0);
        let h2 = font.line_height(16.0);

        // Restore before asserting so a failure can't leak scale into other tests.
        rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
            text_scale: 1.0, ..Default::default()
        });

        assert!(w2 > w1 * 1.8, "width must track text_scale: {w1} -> {w2}");
        assert!(
            h2 > h1 * 1.8,
            "line height must track text_scale too, or rows keep their 100% \
             height while the glyphs inside them grow: {h1} -> {h2}"
        );
    }

    /// Companion to `os_text_scale_grows_line_height_not_just_width`: it is
    /// not enough for the FONT to scale, the CONTROL must grow with it.
    /// `Button::layout` estimated width as `len * size * 0.6` and used a
    /// fixed height, so neither saw `text_scale` and the label spilled out
    /// of the pill at 150% Dynamic Type (reported live on iOS, 2026-08-09).
    #[test]
    fn controls_grow_with_os_text_scale() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        fn button_size(scale: f32) -> rosace_core::types::Size {
            rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
                text_scale: scale, ..Default::default()
            });
            let font = rosace_render::FontCache::embedded();
            let theme = rosace_theme::built_in::dark_theme();
            let ctx = w::LayoutCtx::new(
                rosace_layout::Constraints::loose(1000.0, 1000.0),
                &font,
                &theme,
            );
            w::Button::new("Get Started").on_press(|| {}).layout(&ctx)
        }

        let small = button_size(1.0);
        let large = button_size(2.0);
        rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
            text_scale: 1.0, ..Default::default()
        });

        assert!(
            large.width > small.width * 1.4,
            "the button must widen so the label still fits: {} -> {}",
            small.width, large.width
        );
        assert!(
            large.height > small.height,
            "and grow taller, or a scaled label is clipped vertically: {} -> {}",
            small.height, large.height
        );
    }

    /// A tappable ListTile must be ACTIONABLE, not merely announced.
    /// Platform a11y layers decide "can I activate this?" from the role, so
    /// a row declaring `ListItem` was read out and then offered no action
    /// (reported live on iOS, 2026-08-09).
    #[test]
    fn a_tappable_list_row_is_a_control_an_inert_one_is_not() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        fn role_of(build: fn() -> w::BoxedWidget) -> rosace_core::Role {
            struct R(fn() -> w::BoxedWidget);
            impl Component for R {
                fn build(&self, _c: &mut Context) -> BoxedWidget { (self.0)().boxed() }
            }
            let mut e = FrameEngine::new(Box::new(R(build)), rosace_render::FontCache::embedded());
            e.paint(&mut SkiaCanvas::new(300, 120), &mut SkiaCanvas::new(300, 120), &[]);
            fn find(n: &rosace_core::SemanticNode) -> Option<rosace_core::Role> {
                if n.label.as_deref().map(|l| l.starts_with("Widgets")).unwrap_or(false) {
                    return Some(n.role.clone());
                }
                n.children.iter().find_map(find)
            }
            find(&e.semantics()).expect("the row must be in the tree")
        }

        assert_eq!(
            role_of(|| Arc::new(
                w::ListTile::new("Widgets").subtitle("one page each").on_press(|| {})
            )),
            rosace_core::Role::Button,
            "a row with a tap handler must be exposed as an activatable control"
        );
        assert_eq!(
            role_of(|| Arc::new(w::ListTile::new("Widgets").subtitle("one page each"))),
            rosace_core::Role::ListItem,
            "a row with no handler stays structural — claiming it is a button would be a lie"
        );
    }

    /// Text was clipped at large scale (reported live on iOS, 2026-08-09:
    /// the welcome subtitle lost its last word).
    ///
    /// `Text::layout` wrapped against the width it ASKED for
    /// (`constraints.max_width`) but returned the longest LINE as its width.
    /// `Text::paint` then wrapped against the rect it was GIVEN — the
    /// narrower value — producing more lines than layout had allotted, which
    /// `lines.truncate(fit)` silently dropped.
    ///
    /// Laying out twice makes it observable with no private access: measuring
    /// at the width layout just reported must not need more height.
    #[test]
    fn wrapped_text_height_is_stable_at_the_width_it_reports() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();

        // The reported failure was at raised Dynamic Type, so exercise that.
        rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
            text_scale: 2.0, ..Default::default()
        });

        let text = w::Text::new(
            "A tour of what you can build — widgets, platform channels, and more.",
        );

        let first = text.layout(&w::LayoutCtx::new(
            rosace_layout::Constraints::loose(300.0, 1000.0), &font, &theme,
        ));
        // Re-measure at exactly the width it just claimed to need.
        let second = text.layout(&w::LayoutCtx::new(
            rosace_layout::Constraints::loose(first.width, 1000.0), &font, &theme,
        ));

        rosace_core::media_query::set_media_query(rosace_core::MediaQuery {
            text_scale: 1.0, ..Default::default()
        });

        assert_eq!(
            second.height, first.height,
            "re-wrapping at the reported width must not need more lines, or paint \
             (which uses that width) draws more than layout reserved and the tail \
             gets truncated: {first:?} -> {second:?}"
        );
    }

    /// Widgets must take their type size from the theme, not hardcode one.
    ///
    /// The typography scale was already right (body_large 17, matching
    /// Material 3 and iOS), but widgets ignored it: `ListTile` drew its title
    /// at 11 px and `AppBar` its title at 13 px — roughly two thirds of the
    /// platform body size. Side by side with native UI the whole app read as
    /// shrunken (reported repeatedly, screenshot comparison 2026-08-09).
    ///
    /// Guards the resolution, not exact numbers, so retuning the theme scale
    /// stays a one-place change.
    #[test]
    fn widget_type_sizes_come_from_the_theme_scale() {
        use rosace_widgets::tree as w;
        let theme = rosace_theme::built_in::dark_theme();

        assert_eq!(
            w::ListTile::new("Widgets").resolved_title_size(&theme),
            theme.typography.body_large.size,
            "a list row's title is body text, not fine print"
        );
        assert_eq!(
            w::ListTile::new("Widgets").subtitle("x").resolved_subtitle_size(&theme),
            theme.typography.body_medium.size,
        );
        assert_eq!(
            w::AppBar::new("showcase").resolved_title_size(&theme),
            theme.typography.title_large.size,
            "a top app bar's title is a title, not a caption"
        );

        // An explicit size still wins — theme-defaulted must not mean
        // theme-forced (the widget standard is "max-customizable").
        assert_eq!(w::ListTile::new("x").title_size(9.0).resolved_title_size(&theme), 9.0);
        assert_eq!(w::AppBar::new("x").title_size(9.0).resolved_title_size(&theme), 9.0);
    }

    /// User-reported (2026-08-12): tapping INSIDE a sheet dismissed it.
    /// A sheet is a modal surface — it owns the clicks that land on it, and
    /// only a scrim tap OUTSIDE should dismiss. `Dialog` declares
    /// `InputBehavior::Block` and gets this right; `Sheet` declared
    /// `PassThrough`, so an inside tap skipped the engine's absorb step and
    /// fell through to the scrim's on_tap.
    ///
    /// Asserts on the open ATOM, not the overlay canvas: a clean frame never
    /// touches that canvas, so `has_drawn` reports false for "nothing
    /// repainted" as well as for "dismissed" — two different things.
    #[test]
    fn tapping_inside_a_sheet_does_not_dismiss_it() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let open = rosace_state::Atom::new(rosace_state::next_atom_id(), true);
        struct S(rosace_state::Atom<bool>);
        impl Component for S {
            fn build(&self, _ctx: &mut Context) -> BoxedWidget {
                use rosace_widgets::tree::OverlayApi;
                let o = self.0.clone();
                w::Text::new("host")
                    .sheet(o.get(), || Arc::new(w::Text::new("sheet body")))
                    .on_open_change({ let o = o.clone(); move |v| o.set(v) })
                    .boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(S(open.clone())), rosace_render::FontCache::embedded());
        let (mut c, mut o) = (SkiaCanvas::new(400, 600), SkiaCanvas::new(400, 600));
        e.paint(&mut c, &mut o, &[]);
        assert!(open.get(), "sheet must start open");

        // Bottom-anchored and CONTENT-sized (not full-width), so the inside
        // point must be within the panel's actual rect — near the left edge,
        // low on the screen.
        e.paint(&mut c, &mut o, &[
            rosace_platform::InputEvent::MouseDown { x: 40.0, y: 580.0, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x: 40.0, y: 580.0, button: rosace_platform::MouseButton::Left },
        ]);
        assert!(open.get(), "a tap INSIDE the sheet must not dismiss it");

        // And the scrim must still work: well above the panel is outside it.
        e.paint(&mut c, &mut o, &[
            rosace_platform::InputEvent::MouseDown { x: 40.0, y: 40.0, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x: 40.0, y: 40.0, button: rosace_platform::MouseButton::Left },
        ]);
        assert!(!open.get(), "a tap OUTSIDE, on the scrim, must still dismiss");
    }

    /// Companion to the sheet case: a Drawer is also a modal surface, and
    /// was reported dismissing on inside taps too. It already declared
    /// `InputBehavior::Block`, so this pins that it genuinely works rather
    /// than assuming it from the flag.
    #[test]
    fn tapping_inside_a_drawer_does_not_dismiss_it() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let open = rosace_state::Atom::new(rosace_state::next_atom_id(), true);
        // `Drawer` is not a Widget — a host widget calls `emit()` from its
        // own paint (the Scaffold does this in a real app).
        struct Host(rosace_state::Atom<bool>);
        impl w::Widget for Host {
            fn paint(&self, ctx: &mut w::PaintCtx) {
                let open = self.0.clone();
                w::Drawer::new(open.get(), || Arc::new(w::Text::new("drawer body")))
                    .on_open_change(move |v| open.set(v))
                    .emit(ctx);
                let _ = ctx;
            }
        }
        struct D(rosace_state::Atom<bool>);
        impl Component for D {
            fn build(&self, _ctx: &mut Context) -> BoxedWidget {
                Host(self.0.clone()).boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(D(open.clone())), rosace_render::FontCache::embedded());
        let (mut c, mut o) = (SkiaCanvas::new(400, 600), SkiaCanvas::new(400, 600));
        e.paint(&mut c, &mut o, &[]);

        // Left-anchored, full height — a point near the left edge is inside.
        e.paint(&mut c, &mut o, &[
            rosace_platform::InputEvent::MouseDown { x: 40.0, y: 300.0, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x: 40.0, y: 300.0, button: rosace_platform::MouseButton::Left },
        ]);
        assert!(open.get(), "a tap INSIDE the drawer panel must not dismiss it");

        // Far right is past the panel — that is the scrim.
        e.paint(&mut c, &mut o, &[
            rosace_platform::InputEvent::MouseDown { x: 380.0, y: 300.0, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x: 380.0, y: 300.0, button: rosace_platform::MouseButton::Left },
        ]);
        assert!(!open.get(), "a tap OUTSIDE, on the scrim, must still dismiss");
    }

    /// `non_dismissible()` — dismissal was unconditional before, so a
    /// "you must choose" surface (unsaved changes, a sheet mid-upload) had no
    /// way to refuse a scrim tap.
    #[test]
    fn a_non_dismissible_overlay_ignores_the_scrim_but_still_closes_by_state() {
        use rosace_widgets::tree as w;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let open = rosace_state::Atom::new(rosace_state::next_atom_id(), true);
        struct S(rosace_state::Atom<bool>);
        impl Component for S {
            fn build(&self, _ctx: &mut Context) -> BoxedWidget {
                use rosace_widgets::tree::OverlayApi;
                w::Text::new("host")
                    .dialog(self.0.get(), || Arc::new(w::Text::new("must choose")))
                    .non_dismissible()
                    .boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(S(open.clone())), rosace_render::FontCache::embedded());
        let (mut c, mut o) = (SkiaCanvas::new(400, 600), SkiaCanvas::new(400, 600));
        e.paint(&mut c, &mut o, &[]);

        // Corner tap — squarely on the scrim, which would normally dismiss.
        e.paint(&mut c, &mut o, &[
            rosace_platform::InputEvent::MouseDown { x: 10.0, y: 10.0, button: rosace_platform::MouseButton::Left },
            rosace_platform::InputEvent::MouseUp   { x: 10.0, y: 10.0, button: rosace_platform::MouseButton::Left },
        ]);
        assert!(open.get(), "a non-dismissible overlay must survive a scrim tap");

        // The owner can still close it — refusing the scrim must not trap it.
        open.set(false);
        e.paint(&mut c, &mut o, &[]);
        assert!(!open.get(), "setting the atom must still close it");
    }

    /// Retained overlay trees (the prerequisite for animated/draggable
    /// sheets). Overlays were painted into a tree rebuilt every frame, so
    /// `animate_to`'s eased value in `node.anim` was destroyed each frame: it
    /// read `None`, snapped to target, and never animated — while also never
    /// settling, so anything driving it would request frames forever.
    ///
    /// Asserts the observable consequence: a widget inside an overlay must
    /// see its animation ADVANCE over frames rather than snap.
    #[test]
    fn animation_state_survives_across_frames_inside_an_overlay() {
        use rosace_widgets::tree as w;
        use std::sync::{Arc, Mutex};
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_animate::set_frame_dt(1.0 / 60.0);

        // Records the eased value the overlay's widget observes each frame.
        let seen: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

        struct Probe(Arc<Mutex<Vec<f32>>>);
        impl w::Widget for Probe {
            fn layout(&self, ctx: &w::LayoutCtx) -> rosace_core::types::Size {
                ctx.constraints.constrain(rosace_core::types::Size { width: 100.0, height: 100.0 })
            }
            fn paint(&self, ctx: &mut w::PaintCtx) {
                // Seed once, then ease toward 1.0 on every paint. `animate_to`
                // is UNSETTLED while easing, so it requests the next frame
                // itself — which is what keeps an overlay animating now that
                // overlays are painted only when something changed, rather
                // than unconditionally every frame.
                //
                // With a RETAINED node the eased value climbs across frames.
                // With one wiped each frame, `seed_anim_if_unset` re-seeds
                // 0.0 every time and every paint produces the SAME first
                // step — no progression, however many frames run.
                ctx.seed_anim_if_unset(0.0);
                let v = ctx.animate_to(1.0, 300.0);
                self.0.lock().unwrap().push(v);
            }
        }

        let open = rosace_state::Atom::new(rosace_state::next_atom_id(), true);
        struct S(rosace_state::Atom<bool>, Arc<Mutex<Vec<f32>>>);
        impl Component for S {
            fn build(&self, _ctx: &mut Context) -> BoxedWidget {
                use rosace_widgets::tree::OverlayApi;
                let seen = Arc::clone(&self.1);
                w::Text::new("host")
                    .sheet(self.0.get(), move || Arc::new(Probe(Arc::clone(&seen))))
                    .boxed()
            }
        }

        let mut e = FrameEngine::new(
            Box::new(S(open.clone(), Arc::clone(&seen))),
            rosace_render::FontCache::embedded(),
        );
        let (mut c, mut o) = (SkiaCanvas::new(400, 600), SkiaCanvas::new(400, 600));
        for _ in 0..5 {
            e.paint(&mut c, &mut o, &[]);
        }

        let vals = seen.lock().unwrap().clone();
        assert!(vals.len() >= 3, "the overlay must have painted several frames, got {vals:?}");
        assert!(
            vals.iter().all(|v| *v > 1e-6 && *v < 1.0 - 1e-6),
            "the eased value should still be in flight across these frames: {vals:?}"
        );
        // The discriminator, and it is strictly stronger than "some value lies
        // between 0 and 1": the value must PROGRESS. A node wiped between
        // frames re-seeds and produces the same first step forever, which
        // would pass an any() check and fail this one.
        assert!(
            vals.windows(2).all(|w| w[1] > w[0]),
            "the eased value did not advance across frames — per-node animation \
             state is not surviving inside the overlay: {vals:?}"
        );
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
        fn build(&self, _ctx: &mut Context) -> BoxedWidget {
            rosace_widgets::tree::Image::bytes(TINY_PNG.to_vec())
                .width(50.0)
                .height(50.0)
                .boxed()
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

    #[derive(Clone, Copy, PartialEq, Hash)]
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
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let nav = rosace_nav::ScreenNav::new(ctx, HeroScreen::List);
            let build_screen = {
                let nav = nav.clone();
                move |s: HeroScreen| -> rosace_widgets::tree::BoxedWidget {
                    match s {
                        HeroScreen::List => {
                            let nav = nav.clone();
                            Arc::new(Column::new().child(
                                Container::new()
                                    .width(20.0)
                                    .height(20.0)
                                    .background(Color::rgb(0, 0, 255))
                                    .on_press(move || { nav.push(HeroScreen::Detail); })
                                    .hero_tag("cover"),
                            ))
                        }
                        HeroScreen::Detail => Arc::new(Column::new().child(
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
            rosace_widgets::tree::ScreenTransitionView::new(
                body, nav.current_key(), outgoing, nav.previous_key(),
                nav.transition_handle(), nav.stack_keys(),
            ).boxed()
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

    /// Blue across BOTH canvases.
    ///
    /// A hero in flight is a PROMOTED layer, and promoted layers composite
    /// into the overlay canvas — that is what "promoted to the root layer"
    /// means. It used to be replayed straight into the main picture, so
    /// counting only `canvas` was enough. Counting one canvas now reports
    /// zero mid-flight and reads as "the hero never morphed".
    fn blue_pixel_count_both(canvas: &SkiaCanvas, overlay: &SkiaCanvas) -> usize {
        blue_pixel_count(canvas) + blue_pixel_count(overlay)
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
            let area = blue_pixel_count_both(&canvas, &overlay);
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

    use rosace_widgets::tree::{TextArea, TextInput};
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
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            TextInput::new()
                .value(name.get())
                .on_change({
                    let name = name.clone();
                    move |v| name.set(v)
                })
                .boxed()
        }
    }

    /// Phase 32 user-reported repro: the widget_gallery's TextInput sits
    /// inside a ScrollView and typed text never appeared.
    struct ScrolledTextInput {
        captured: Arc<OnceLock<rosace_state::Atom<String>>>,
    }
    impl Component for ScrolledTextInput {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            let input = TextInput::new()
                .value(name.get())
                .width(170.0)
                .on_change({
                    let name = name.clone();
                    move |v| name.set(v)
                });
            rosace_widgets::tree::ScrollView::new(
                rosace_widgets::tree::Column::new()
                    .child(input)
                    .child(rosace_widgets::tree::Spacer::gap(170.0, 800.0)),
            )
            .boxed()
        }
    }

    #[test]
    fn typing_into_a_text_input_inside_a_scroll_view_updates_its_value() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(OnceLock::new());
        let mut engine = FrameEngine::new(
            Box::new(ScrolledTextInput { captured: captured.clone() }),
            rosace_render::FontCache::embedded(),
        );
        let mut canvas = SkiaCanvas::new(300, 400);
        let mut overlay = SkiaCanvas::new(300, 400);
        engine.paint(&mut canvas, &mut overlay, &[]);

        engine.paint(&mut canvas, &mut overlay, &[click(50.0, 18.0)]);
        // Both chars in ONE frame — the batched-keystroke case that used
        // to drop the first character (see commit_text_edit's fix note).
        engine.paint(&mut canvas, &mut overlay, &[text('h'), text('i')]);
        engine.paint(&mut canvas, &mut overlay, &[]);
        let value = captured.get().expect("atom captured").get();
        assert_eq!(value, "hi", "typed text must reach the app atom through a ScrollView");
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
        // Typing always leaves the caret at the end regardless of where
        // the initial click landed — Home first so Delete has something
        // after the cursor to remove.
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

    // ── TextLayoutSnapshot: click-to-glyph, drag, multi-click (D116 Step 3) ──
    //
    // `TextInput`'s own default styling puts text at `rect.origin.x + 10.0`
    // measured with the embedded font at its default 11.0px size — the
    // exact geometry `TextLayoutSnapshot::position_at` is built from at
    // paint time. These tests measure that same geometry independently
    // (via a fresh `FontCache::embedded()`) to compute an exact expected
    // click x for a known char index, then assert dispatch lands on that
    // exact index — not an approximate/eyeballed position.

    /// `TextInput`/`TextArea`'s font size when neither widget nor test
    /// overrides it (D127 typography routing, 2026-08-03): resolved from
    /// the default light theme's `typography.body_medium`, NOT a hardcoded
    /// literal — these tests must track whatever the real widget actually
    /// measures/paints at, or a future typography-scale tweak silently
    /// desyncs them from the code they're supposed to be verifying.
    fn default_text_input_px() -> f32 {
        rosace_theme::built_in::light_theme().typography.body_medium.size
    }

    fn embedded_x_for(prefix: &str) -> f32 {
        10.0 + rosace_render::FontCache::embedded().measure_text(prefix, default_text_input_px())
    }

    fn mouse_move(x: f32, y: f32) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::MouseMove { x, y }
    }
    fn mouse_up(x: f32, y: f32) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::MouseUp { x, y, button: rosace_platform::MouseButton::Left }
    }

    // Every test below sleeps a real (sub-second) amount of wall-clock
    // time to exercise the double-click debounce window against
    // `anim_clock()`'s real `Instant`, so — like the animation tests
    // above — they take `ANIMATION_GLOBAL_TEST_LOCK` to avoid a
    // concurrently-running test's own frame/dirty churn landing inside
    // this engine's `needs_paint` window and staling its `TextLayoutSnapshot`
    // mid-sequence (found empirically: these tests were flaky under
    // `cargo test`'s default parallelism without the lock, reliable with it).

    #[test]
    fn click_mid_string_places_the_caret_at_the_exact_clicked_index() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        // Settle past the double-click debounce window so the next click
        // below is unambiguously a fresh single click, regardless of how
        // close its x lands to the initial focusing click's x=20.
        std::thread::sleep(std::time::Duration::from_millis(450));

        // Click exactly at the boundary after "hel" (index 3) — must place
        // the caret there, not at the end (the old Step 1 simplification).
        let x = embedded_x_for("hel");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "helXlo", "click must place the caret at the exact clicked glyph boundary");
    }

    #[test]
    fn click_at_the_very_start_places_the_caret_before_the_first_character() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        std::thread::sleep(std::time::Duration::from_millis(450));

        let x = embedded_x_for("");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "Xhello");
    }

    #[test]
    fn mouse_drag_produces_the_exact_expected_selection() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");
        std::thread::sleep(std::time::Duration::from_millis(450));

        // Drag from the boundary after "hello" (index 5) to after "hello "
        // (index 6) — selects exactly the space character.
        let x0 = embedded_x_for("hello");
        let x1 = embedded_x_for("hello ");
        engine.paint(&mut canvas, &mut overlay, &[click(x0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_move(x1, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_up(x1, 18.0)]);
        // Typing now must replace exactly the dragged-over selection.
        type_str(&mut engine, &mut canvas, &mut overlay, "_");
        assert_eq!(atom.get().unwrap().get(), "hello_world", "drag selection must span exactly the dragged range");
    }

    #[test]
    fn mouse_drag_backwards_still_produces_the_correct_selection() {
        // Anchor after the drag's start x, head before it — the selection
        // must still normalize to the same range regardless of drag
        // direction (matches every real text editor).
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");
        std::thread::sleep(std::time::Duration::from_millis(450));

        let x0 = embedded_x_for("hello ");
        let x1 = embedded_x_for("hello");
        engine.paint(&mut canvas, &mut overlay, &[click(x0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_move(x1, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_up(x1, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "_");
        assert_eq!(atom.get().unwrap().get(), "hello_world");
    }

    #[test]
    fn double_click_selects_the_word_under_the_cursor() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");
        std::thread::sleep(std::time::Duration::from_millis(450));

        // Both clicks land inside "world" (after "hello " = index 6).
        let x = embedded_x_for("hello wo");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "hello X", "double-click must select the whole word, not just the clicked char");
    }

    #[test]
    fn triple_click_selects_the_whole_line() {
        // Single-line TextInput: triple-click selects everything, same as
        // Cmd/Ctrl+A — `TextArea` (Step 4) gets real per-line selection
        // for free from the same `line_range_at` primitive.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");
        std::thread::sleep(std::time::Duration::from_millis(450));

        let x = embedded_x_for("hello wo");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "X");
    }

    #[test]
    fn a_slow_second_click_does_not_count_as_a_double_click() {
        // Real double-click detection, not "any two clicks on the same
        // spot" — a click outside the debounce window must reset the
        // count and behave as an ordinary single click (plain caret
        // placement, no word selected).
        // This test drives a REAL wall-clock sleep past the double-click
        // debounce window, which (like the animation tests above) touches
        // process-global frame/dirty state (`rosace_state`, `anim_clock`)
        // shared by the whole test binary — take the same lock those use
        // to avoid cross-test interleaving corrupting this engine's
        // needs_paint/dirty bookkeeping mid-sleep.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");

        let x = embedded_x_for("hello wo");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        // `anim_clock()` is real wall-clock time (not the animation
        // system's simulated `frame_dt`) — sleep past the debounce
        // window for a real second click.
        std::thread::sleep(std::time::Duration::from_millis(450));
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "hello woXrld", "a slow second click must place the caret, not select the word");
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
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
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
                .boxed()
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
        use rosace_widgets::clipboard::ClipboardProvider;
        let cb = rosace_widgets::clipboard::SystemClipboard::new();
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
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
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
                .boxed()
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
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            TextInput::new()
                .value(name.get())
                .controller(self.controller.clone())
                .on_change({ let name = name.clone(); move |v| name.set(v) })
                .boxed()
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

    // ── TextArea: multiline, wrap, Enter, goal-column Up/Down (D116 Step 4) ──

    struct OneTextArea {
        captured: Arc<OnceLock<rosace_state::Atom<String>>>,
        height: f32,
    }
    impl Component for OneTextArea {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            TextArea::new()
                .value(name.get())
                .width(400.0)
                .height(self.height)
                .on_change({ let name = name.clone(); move |v| name.set(v) })
                .boxed()
        }
    }

    fn headless_text_area_engine(height: f32) -> (FrameEngine, SkiaCanvas, SkiaCanvas, Arc<OnceLock<rosace_state::Atom<String>>>) {
        let captured = Arc::new(OnceLock::new());
        let engine = FrameEngine::new(Box::new(OneTextArea { captured: captured.clone(), height }), rosace_render::FontCache::embedded());
        (engine, SkiaCanvas::new(400, 300), SkiaCanvas::new(400, 300), captured)
    }

    // TextArea's `paint` calls `request_animation()` every focused frame
    // (caret blink) and reads `anim_clock()`, the same process-global
    // state the animation tests above guard with `ANIMATION_GLOBAL_TEST_LOCK`
    // — these tests do enough frames (many keystrokes each) that they were
    // observed to occasionally destabilize an UNRELATED, otherwise-stable
    // pre-existing test when run concurrently under `cargo test`'s default
    // parallelism. Taking the same lock here fixed it.

    #[test]
    fn enter_inserts_a_real_newline_and_typing_continues_on_the_next_line() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_area_engine(100.0);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "ab");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Enter)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "cd");
        assert_eq!(atom.get().unwrap().get(), "ab\ncd", "Enter must insert a real newline in a multiline field");
    }

    #[test]
    fn enter_does_nothing_on_a_single_line_text_input() {
        // The multiline gate on Enter (`focused_editable().3`) must
        // actually gate — a single-line TextInput ignores Enter entirely,
        // same as before this feature existed.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "ab");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Enter)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "cd");
        assert_eq!(atom.get().unwrap().get(), "abcd", "Enter must be a no-op on a single-line TextInput");
    }

    #[test]
    fn arrow_down_twice_returns_to_the_original_goal_column_after_a_shorter_line() {
        // Three explicit lines — "xxxxxxxxxx" (10), "xxx" (3),
        // "xxxxxxxxxx" (10) again, all the SAME repeated character so
        // relative on-screen widths are monotonic in char count
        // regardless of the real (proportional) font's exact metrics.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_area_engine(200.0);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "xxxxxxxxxx");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Enter)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "xxx");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Enter)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "xxxxxxxxxx");
        assert_eq!(atom.get().unwrap().get(), "xxxxxxxxxx\nxxx\nxxxxxxxxxx");

        // Cursor is at the document end (index 25) — walk it back to
        // index 7 (column 7 of the first line) with real ArrowLeft
        // dispatch.
        for _ in 0..18 {
            engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowLeft)]);
        }

        // Down into "xxx" (only 3 chars wide) — must clamp to its end,
        // not panic or land past it.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowDown)]);
        // Down again into the second "xxxxxxxxxx" — goal-column memory
        // must restore column 7 (NOT stay clamped at column 3), proving
        // the goal x survived the intermediate short line untouched.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowDown)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "|");

        assert_eq!(
            atom.get().unwrap().get(),
            "xxxxxxxxxx\nxxx\nxxxxxxx|xxx",
            "goal-column memory must restore the original column after passing through a shorter line"
        );
    }

    #[test]
    fn arrow_up_at_the_first_line_moves_to_that_lines_start() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_area_engine(200.0);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        // No line above the first — ArrowUp lands at that line's own
        // start rather than doing nothing.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::ArrowUp)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "Xhello");
    }

    #[test]
    fn wheel_scroll_changes_which_line_a_click_lands_on() {
        // A tiny viewport over a many-line document — real proof the
        // scroll offset participates in click->glyph placement, not just
        // in what's painted. Each line is `line_i` so the resulting atom
        // content reveals exactly which line the click landed on.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_area_engine(40.0);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 5.0)]);
        for i in 0..20 {
            if i > 0 {
                engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Enter)]);
            }
            type_str(&mut engine, &mut canvas, &mut overlay, &format!("line_{i}"));
        }

        // Scroll down several lines' worth, then click near the TOP of
        // the (now scrolled) viewport.
        // Negative delta_y scrolls content down/offset up — same
        // convention proven by `wheel_scroll_still_springs_back_...`
        // above for `ScrollView`; `TextArea` wires wheel input through
        // the identical `ScrollController::scroll_by(0, -dy)` `ListView`
        // uses.
        let scroll = rosace_platform::InputEvent::Scroll { x: 20.0, y: 5.0, delta_x: 0.0, delta_y: -80.0 };
        engine.paint(&mut canvas, &mut overlay, &[scroll]);
        engine.paint(&mut canvas, &mut overlay, &[click(0.0, 5.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "|");

        let value = atom.get().unwrap().get();
        assert!(!value.starts_with("|line_0"), "after scrolling down, a click near the top must NOT still land on the very first line: {value:?}");
    }

    #[test]
    fn scrolled_to_the_bottom_the_last_line_is_fully_inside_the_viewport() {
        // Regression (found live 2026-07-12): `max_scroll` was computed
        // from bare `content_h`, ignoring the PAD*2 the text is drawn
        // inside — so at max scroll the last line's bottom sat exactly
        // PAD px past the clip, permanently half-cut. The scrollable
        // extent must be `content_h + PAD*2`.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let height = 100.0_f32;
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_area_engine(height);
        engine.paint(&mut canvas, &mut overlay, &[]);
        let text = (0..20).map(|i| format!("line_{i}")).collect::<Vec<_>>().join("\n");
        atom.get().unwrap().set(text);
        engine.paint(&mut canvas, &mut overlay, &[]);

        // Wheel far past the end — the clamp on the next paint must land
        // on the true max, not the old PAD-short value.
        let scroll = rosace_platform::InputEvent::Scroll { x: 20.0, y: 5.0, delta_x: 0.0, delta_y: -100_000.0 };
        engine.paint(&mut canvas, &mut overlay, &[scroll]);
        engine.paint(&mut canvas, &mut overlay, &[]);

        const PAD: f32 = 10.0; // TextArea's internal text padding
        let line_h = rosace_render::FontCache::embedded().line_height(default_text_input_px());
        let n_lines = 20.0_f32;
        let expected_max = n_lines * line_h + PAD * 2.0 - height;
        let offset = scroll_offset(&engine).expect("TextArea registers a scroll controller");
        assert!(
            (offset[1] - expected_max).abs() < 0.5,
            "max scroll must include the text padding: got {}, expected {expected_max}",
            offset[1],
        );
        // The geometric truth the user actually sees: the last line's
        // bottom edge (PAD + n*line_h - scroll) sits INSIDE the viewport.
        let last_line_bottom = PAD + n_lines * line_h - offset[1];
        assert!(
            last_line_bottom <= height + 0.01,
            "last line must not be clipped at max scroll: bottom at {last_line_bottom}, viewport height {height}",
        );
    }

    #[test]
    fn wheel_scrolling_away_from_the_caret_is_not_snapped_back_by_scroll_into_view() {
        // Regression (found live 2026-07-12): caret scroll-into-view ran
        // on EVERY focused paint — and a focused TextArea repaints every
        // frame for the caret blink — so with the caret on a bottom line,
        // every wheel-scroll-up was reverted within a frame ("no
        // scrolling when the cursor is at the bottom"), and a mid-document
        // caret clamped scrolling to a viewport-sized window around
        // itself. The chase must fire only when the caret MOVES.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let height = 100.0_f32;
        let (mut engine, mut canvas, mut overlay, _atom) = headless_text_area_engine(height);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        for i in 0..20 {
            if i > 0 {
                engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Enter)]);
            }
            type_str(&mut engine, &mut canvas, &mut overlay, &format!("line_{i}"));
        }
        // Typing chased the caret to the bottom.
        let at_bottom = scroll_offset(&engine).unwrap()[1];
        assert!(at_bottom > 0.0, "typing 20 lines into a 100px field must have scrolled down, got {at_bottom}");

        // Wheel all the way back up (positive delta_y decreases the
        // offset — the inverse of the convention proven in
        // `wheel_scroll_changes_which_line_a_click_lands_on`), then paint
        // an EMPTY frame — the caret-blink frame that used to snap back.
        let scroll = rosace_platform::InputEvent::Scroll { x: 20.0, y: 5.0, delta_x: 0.0, delta_y: 100_000.0 };
        engine.paint(&mut canvas, &mut overlay, &[scroll]);
        engine.paint(&mut canvas, &mut overlay, &[]);
        let after_wheel = scroll_offset(&engine).unwrap()[1];
        assert!(
            after_wheel < 1.0,
            "wheel-scrolling to the top with the caret at the bottom must STICK (caret did not move): got {after_wheel}",
        );

        // A real caret move (typing) must chase again — back to the
        // bottom. One settling paint: the edit lands on the engine's
        // next frame (same convention the EditController test documents).
        type_str(&mut engine, &mut canvas, &mut overlay, "x");
        engine.paint(&mut canvas, &mut overlay, &[]);
        let after_type = scroll_offset(&engine).unwrap()[1];
        assert!(
            (after_type - at_bottom).abs() < 1.0,
            "typing must scroll the caret back into view: got {after_type}, expected ~{at_bottom}",
        );
    }

    // ── Tooltip position inside a scroll layer (Phase 32 bug fix) ────────

    // ── Build-time overlay emission (Phase 32 bug fix) ───────────────────

    /// The gallery pattern, in its supported form: the overlay is DECLARED in
    /// `build()` and promoted during paint.
    ///
    /// It used to be emitted from `build()` directly, through a thread-local
    /// registry. Promotion needs a `PaintCtx`, which `build()` has not got, so
    /// that entry point is gone — but the invariant it existed for is the one
    /// this test is really about, and it now holds for free: a promoted node
    /// survives a cache-hit frame because a replaying parent never resets it.
    struct BuildEmitsSnackbar {
        captured: Arc<OnceLock<rosace_state::Atom<bool>>>,
    }
    impl Component for BuildEmitsSnackbar {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            use rosace_widgets::tree::OverlayApi;
            let open = ctx.state(false);
            let _ = self.captured.set(open.clone());
            rosace_widgets::tree::Text::new("body")
                .toast(open.get(), || {
                    Arc::new(rosace_widgets::tree::Snackbar::new("saved"))
                })
                .boxed()
        }
    }

    #[test]
    fn a_declared_snackbar_paints_and_survives_cache_hit_frames() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(OnceLock::new());
        let mut engine = FrameEngine::new(
            Box::new(BuildEmitsSnackbar { captured: captured.clone() }),
            rosace_render::FontCache::embedded(),
        );
        let mut canvas = SkiaCanvas::new(300, 200);
        let mut overlay = SkiaCanvas::new(300, 200);

        engine.paint(&mut canvas, &mut overlay, &[]);
        assert!(!overlay.has_drawn(), "closed: no overlay content");

        captured.get().unwrap().set(true);
        overlay.clear_transparent();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert!(overlay.has_drawn(), "open (dirty frame): the declared snackbar must paint");

        // A cache-hit frame (nothing dirty) must KEEP showing it: build does
        // not rerun, and the promoted node persists because its replaying
        // parent never resets it.
        overlay.clear_transparent();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert!(overlay.has_drawn(), "open (cache-hit frame): the snackbar must persist");

        captured.get().unwrap().set(false);
        overlay.clear_transparent();
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert!(!overlay.has_drawn(), "closed again: the snackbar must disappear");
    }

    // ── App lifecycle (D042/D110, Phase 29 Step 1) ────────────────────────

    /// Records the lifecycle state seen by each `build()` call, in order —
    /// so the test can tell a real subscription-driven rebuild apart from
    /// a rebuild-every-frame false positive.
    struct LifecycleReader {
        log: Arc<std::sync::Mutex<Vec<rosace_core::LifecycleState>>>,
    }
    impl Component for LifecycleReader {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let state = rosace_core::use_app_lifecycle(ctx);
            self.log.lock().unwrap().push(state);
            rosace_widgets::tree::Container::new().boxed()
        }
    }

    #[test]
    fn a_lifecycle_event_re_renders_a_subscribed_component_with_the_new_state() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        rosace_core::set_app_lifecycle(rosace_core::LifecycleState::Active);

        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let root = LifecycleReader { log: log.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 100);
        let mut overlay = SkiaCanvas::new(200, 100);

        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(
            *log.lock().unwrap(),
            vec![rosace_core::LifecycleState::Active],
            "first build must see the Active default"
        );

        // An idle frame must NOT rebuild — otherwise the assertions below
        // would pass even with the subscription broken.
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(log.lock().unwrap().len(), 1, "idle frame must reuse the cached element");

        // The event is dispatched AFTER this frame's build, marking the
        // subscribed root dirty; the NEXT frame rebuilds with the new state.
        engine.paint(&mut canvas, &mut overlay, &[
            rosace_platform::InputEvent::Lifecycle(rosace_core::LifecycleState::Background),
        ]);
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(
            log.lock().unwrap().last().copied(),
            Some(rosace_core::LifecycleState::Background),
            "the subscribed component must re-render with the reported state"
        );

        rosace_core::set_app_lifecycle(rosace_core::LifecycleState::Active); // reset
    }

    // ── SpanSource + CursorStyle (D116 Step 5) ────────────────────────────

    /// Every `changed_range` the spans hook was called with, in order.
    type ChangedRangeLog = Arc<std::sync::Mutex<Vec<Option<(usize, usize)>>>>;

    struct OneSpannedTextInput {
        captured: Arc<OnceLock<rosace_state::Atom<String>>>,
        log: ChangedRangeLog,
    }
    impl Component for OneSpannedTextInput {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            let log = self.log.clone();
            TextInput::new()
                .value(name.get())
                .width(400.0)
                .on_change({ let name = name.clone(); move |v| name.set(v) })
                .spans(move |_s, changed_range| {
                    log.lock().unwrap().push(changed_range);
                    Vec::new()
                })
                .boxed()
        }
    }

    #[test]
    fn spans_hook_is_called_with_the_small_edits_changed_range_not_the_whole_document() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(OnceLock::new());
        let log: ChangedRangeLog = Arc::new(std::sync::Mutex::new(Vec::new()));
        let root = OneSpannedTextInput { captured: captured.clone(), log: log.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(400, 60);
        let mut overlay = SkiaCanvas::new(400, 60);

        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world, this is a long base sentence");
        log.lock().unwrap().clear(); // ignore the setup typing's own calls

        // One small edit: append a single '!'. `paint()` processes the
        // Text event AFTER this frame's own build/paint walk (so THIS
        // frame's `spans_fn` call still reflects the state from before
        // the '!'); one more empty-event frame lets the widget's own
        // paint see the now-committed edit and call the hook again.
        type_str(&mut engine, &mut canvas, &mut overlay, "!");
        engine.paint(&mut canvas, &mut overlay, &[]);

        let entries = log.lock().unwrap().clone();
        assert!(!entries.is_empty(), "the spans hook must be called at least once after the edit");
        let n = captured.get().unwrap().get().chars().count();
        assert_eq!(
            *entries.last().unwrap(),
            Some((n - 1, n)),
            "SpanSource must receive only the small edit's changed range, not the whole document"
        );
    }

    #[test]
    fn spans_hook_paints_a_span_in_its_own_color_distinct_from_the_default_text_color() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Real proof the widget actually USES the returned spans to paint
        // (not just calls the hook and discards the result) — a span
        // covering the whole value in a distinctive color must produce
        // pixels of exactly that color.
        let captured: Arc<OnceLock<rosace_state::Atom<String>>> = Arc::new(OnceLock::new());
        struct BoldRedSpanInput { captured: Arc<OnceLock<rosace_state::Atom<String>>> }
        impl Component for BoldRedSpanInput {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let name: rosace_state::Atom<String> = ctx.state(String::from("hi"));
                let _ = self.captured.set(name.clone());
                TextInput::new()
                    .value(name.get())
                    .width(200.0)
                    .focused()
                    .spans(|s, _changed| {
                        vec![text_edit::Span::new((0, s.chars().count())).color(rosace_render::Color::rgb(255, 0, 0))]
                    })
                    .boxed()
            }
        }
        let root = BoldRedSpanInput { captured: captured.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 60);
        let mut overlay = SkiaCanvas::new(200, 60);
        engine.paint(&mut canvas, &mut overlay, &[]);

        // Tolerant match, not exact (255,0,0,255) — glyph anti-aliasing
        // means even a fully-covered stroke pixel may blend slightly.
        let red_pixels = canvas.pixels().chunks_exact(4)
            .filter(|p| p[0] > 180 && p[1] < 60 && p[2] < 60 && p[3] > 180)
            .count();
        assert!(red_pixels > 0, "a span covering the whole value in red must produce real reddish pixels, got none");
    }

    #[test]
    fn cursor_style_color_override_paints_the_caret_in_that_color() {
        struct ColoredCursorInput;
        impl Component for ColoredCursorInput {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let name: rosace_state::Atom<String> = ctx.state(String::from("hi"));
                TextInput::new()
                    .value(name.get())
                    .width(200.0)
                    .focused()
                    .cursor_style(text_edit::CursorStyle {
                        color: rosace_render::Color::rgb(0, 255, 0),
                        ..Default::default()
                    })
                    .boxed()
            }
        }
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut engine = FrameEngine::new(Box::new(ColoredCursorInput), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 60);
        let mut overlay = SkiaCanvas::new(200, 60);
        engine.paint(&mut canvas, &mut overlay, &[]);
        // The caret blinks against REAL wall-clock time
        // (`last_edit_at`/`anim_clock()`) — a click refreshes
        // `last_edit_at` to "now", and the blink is solid-on for the
        // first 0.5s after that, so the NEXT paint is guaranteed to
        // render it regardless of the test binary's own uptime at the
        // moment this test happens to run.
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[]);

        let green_pixels = canvas.pixels().chunks_exact(4)
            .filter(|p| p[0] < 60 && p[1] > 180 && p[2] < 60 && p[3] > 180)
            .count();
        assert!(green_pixels > 0, "a green CursorStyle override must paint real greenish pixels for the caret, got none");
    }

    // ── Real OS IME (D116 Step 6) ──────────────────────────────────────────

    fn ime_preedit(text: &str, cursor_range: Option<(usize, usize)>) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::Ime(rosace_platform::ime::ImeEvent::Preedit { text: text.to_string(), cursor_range })
    }
    fn ime_commit(text: &str) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::Ime(rosace_platform::ime::ImeEvent::Commit(text.to_string()))
    }

    #[test]
    fn ime_preedit_shows_provisional_text_then_commit_finalizes_it() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi ");

        // Composing "にほ" — each keystroke sends a fresh Preedit that
        // REPLACES the provisional buffer (real IME behavior), not one
        // that appends to it.
        engine.paint(&mut canvas, &mut overlay, &[ime_preedit("に", None)]);
        assert_eq!(atom.get().unwrap().get(), "hi に", "preedit text must show up in the live value, provisionally");
        engine.paint(&mut canvas, &mut overlay, &[ime_preedit("にほ", None)]);
        assert_eq!(atom.get().unwrap().get(), "hi にほ", "a later preedit update must REPLACE the earlier provisional text");

        // Commit finalizes it as real text.
        engine.paint(&mut canvas, &mut overlay, &[ime_commit("日本")]);
        assert_eq!(atom.get().unwrap().get(), "hi 日本", "commit must replace the provisional text with the final candidate");

        // Typing after commit continues normally, proving the cursor
        // landed right after the committed text, not somewhere stale.
        type_str(&mut engine, &mut canvas, &mut overlay, "!");
        assert_eq!(atom.get().unwrap().get(), "hi 日本!");
    }

    #[test]
    fn ime_commit_undoes_the_whole_composition_in_one_step() {
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi ");
        engine.paint(&mut canvas, &mut overlay, &[ime_preedit("に", None)]);
        engine.paint(&mut canvas, &mut overlay, &[ime_preedit("にほ", None)]);
        engine.paint(&mut canvas, &mut overlay, &[ime_commit("日本")]);
        assert_eq!(atom.get().unwrap().get(), "hi 日本");

        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('z'))]);
        assert_eq!(
            atom.get().unwrap().get(), "hi ",
            "one Cmd+Z must remove the WHOLE committed word (back to before composition started), \
             not just the last intermediate preedit snapshot"
        );
    }

    #[test]
    fn ime_commit_with_no_preceding_preedit_just_inserts_at_the_cursor() {
        // Some IMEs commit directly for a single-candidate confirmation,
        // with no Preedit event first.
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi ");
        engine.paint(&mut canvas, &mut overlay, &[ime_commit("日本")]);
        assert_eq!(atom.get().unwrap().get(), "hi 日本");
    }

    #[test]
    fn ime_preedit_paints_an_underline_decoration_under_the_provisional_text() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        struct OneUnderlineInput;
        impl Component for OneUnderlineInput {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let name: rosace_state::Atom<String> = ctx.state(String::new());
                TextInput::new()
                    .value(name.get())
                    .width(200.0)
                    .on_change(move |v| name.set(v))
                    .boxed()
            }
        }
        let mut engine = FrameEngine::new(Box::new(OneUnderlineInput), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 60);
        let mut overlay = SkiaCanvas::new(200, 60);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);

        let before = canvas.pixels().to_vec();
        engine.paint(&mut canvas, &mut overlay, &[ime_preedit("に", None)]);
        let after = canvas.pixels().to_vec();
        assert_ne!(before, after, "an active IME composition must change what's painted (text + underline), not render identically to before");
    }

    // ── Context menu + touch selection handles (D116 Step 7) ─────────────

    fn right_down(x: f32, y: f32) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::MouseDown { x, y, button: rosace_platform::MouseButton::Right }
    }
    fn right_up(x: f32, y: f32) -> rosace_platform::InputEvent {
        rosace_platform::InputEvent::MouseUp { x, y, button: rosace_platform::MouseButton::Right }
    }

    #[test]
    fn right_click_selects_all_via_the_context_menu() {
        // Real proof the menu item's callback reaches all the way back
        // into a real edit — not just that a menu renders. Select All is
        // the one action that needs no PRE-existing selection to exercise.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");

        engine.paint(&mut canvas, &mut overlay, &[right_down(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[right_up(20.0, 18.0)]);
        // The menu's "Select All" item — find it via the overlay route's
        // hit callback the same way a real click would, by simulating a
        // Left click at the item's on-screen position. Since the exact
        // pixel layout of `Menu` isn't this test's concern, drive it
        // through the SAME `ContextMenuAction` queue a real click would
        // enqueue onto, proving `drain_context_menu` applies it correctly
        // — the menu's own rendering/hit-testing is `Menu`'s existing,
        // already-tested responsibility, not re-tested here.
        engine.test_enqueue_context_menu_action(ContextMenuAction::SelectAll);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        assert_eq!(atom.get().unwrap().get(), "", "Select All via the context menu must select the whole field, so Backspace clears it entirely");
    }

    /// The framework supplies the selection and a way to write back; the APP
    /// supplies what an item MEANS. Both halves are exercised here: a
    /// read-only item that just observes the selection (the "Search the web"
    /// shape) and a mutating one that edits through the controller (the
    /// "Bold" shape).
    #[test]
    fn an_app_can_add_context_menu_items_and_reshape_the_built_ins() {
        use rosace_widgets::tree::text_edit::{ContextMenuItem, EditController};
        use std::sync::Mutex;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // What the read-only item observed when it ran.
        let observed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        // The items the menu actually offered, in order — captured through
        // the transform hook, which the engine hands the finished list.
        let offered: Arc<Mutex<Vec<ContextMenuItem>>> = Arc::new(Mutex::new(Vec::new()));

        struct App {
            captured: Arc<OnceLock<rosace_state::Atom<String>>>,
            controller: EditController,
            observed: Arc<Mutex<Option<String>>>,
            offered: Arc<Mutex<Vec<ContextMenuItem>>>,
        }
        impl Component for App {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let name: rosace_state::Atom<String> = ctx.state(String::new());
                let _ = self.captured.set(name.clone());
                let observed = Arc::clone(&self.observed);
                let offered = Arc::clone(&self.offered);
                let ctrl = self.controller.clone();
                TextInput::new()
                    .value(name.get())
                    .controller(self.controller.clone())
                    .on_change({ let name = name.clone(); move |v| name.set(v) })
                    // Read-only: looks at the selection, writes nothing.
                    .context_menu_item(
                        ContextMenuItem::new("Search the web", move |t| {
                            *observed.lock().unwrap() = t.selected_text.clone();
                        })
                        .needs_selection(),
                    )
                    // Mutating: edits through the controller, the same path a
                    // toolbar button uses.
                    .context_menu_item(
                        ContextMenuItem::new("Bold", move |t| {
                            if let (Some((s, e)), Some(sel)) = (t.selection, t.selected_text.clone()) {
                                ctrl.replace_range(s, e, format!("**{sel}**"));
                            }
                        })
                        .needs_selection(),
                    )
                    // A read-only field would drop Paste this way.
                    .context_menu(move |items| {
                        items.retain(|i| i.label != "Paste");
                        *offered.lock().unwrap() = items.clone();
                    })
                    .boxed()
            }
        }

        let captured = Arc::new(OnceLock::new());
        let controller = EditController::new();
        let mut engine = FrameEngine::new(
            Box::new(App {
                captured: captured.clone(),
                controller: controller.clone(),
                observed: Arc::clone(&observed),
                offered: Arc::clone(&offered),
            }),
            rosace_render::FontCache::embedded(),
        );
        let (mut canvas, mut overlay) = (SkiaCanvas::new(200, 60), SkiaCanvas::new(200, 60));

        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hi");

        // Select the text THROUGH the menu — `drain_context_menu` ignores
        // actions while no menu is open, so it has to be opened first. This
        // also closes it again.
        engine.paint(&mut canvas, &mut overlay, &[right_down(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[right_up(20.0, 18.0)]);
        engine.test_enqueue_context_menu_action(ContextMenuAction::SelectAll);
        engine.paint(&mut canvas, &mut overlay, &[]);

        // Reopen it, now with a live selection, so selection-gated items show.
        engine.paint(&mut canvas, &mut overlay, &[right_down(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[right_up(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[]);

        let items = offered.lock().unwrap().clone();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        assert!(
            labels.iter().any(|l| l == "Search the web") && labels.iter().any(|l| l == "Bold"),
            "the app's own items are missing from the menu: {labels:?}"
        );
        assert!(
            !labels.iter().any(|l| l == "Paste"),
            "the transform hook removed Paste but it is still offered: {labels:?}"
        );
        assert!(
            labels.iter().any(|l| l == "Cut"),
            "built-ins the app did not remove must survive: {labels:?}"
        );

        // Run the app's items against the target the ENGINE built — the real
        // selection and the real controller, not a stand-in.
        let target = engine.test_context_menu_target().expect("a target while the menu is open");
        (items.iter().find(|i| i.label == "Search the web").expect("present").action)(&target);
        assert_eq!(
            observed.lock().unwrap().as_deref(),
            Some("hi"),
            "a read-only item must see the live selection"
        );

        (items.iter().find(|i| i.label == "Bold").expect("present").action)(&target);
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(
            captured.get().unwrap().get(),
            "**hi**",
            "a mutating item must reach the real field through the controller"
        );
    }

    #[test]
    fn right_click_copy_and_paste_round_trip_through_the_real_clipboard() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let cb = rosace_widgets::clipboard::SystemClipboard::new();
        let original = cb.read();

        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        // Select "hello" (Cmd+A) so Copy has something real to grab.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('a'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);

        engine.paint(&mut canvas, &mut overlay, &[right_down(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[right_up(20.0, 18.0)]);
        engine.test_enqueue_context_menu_action(ContextMenuAction::Copy);
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(cb.read().as_deref(), Some("hello"), "Copy via the context menu must write the real selection to the real system clipboard");

        // Clear the field, then Paste back via the menu.
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Char('a'))]);
        engine.paint(&mut canvas, &mut overlay, &[key_up(rosace_platform::Key::Meta)]);
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        assert_eq!(atom.get().unwrap().get(), "");

        engine.paint(&mut canvas, &mut overlay, &[right_down(20.0, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[right_up(20.0, 18.0)]);
        engine.test_enqueue_context_menu_action(ContextMenuAction::Paste);
        engine.paint(&mut canvas, &mut overlay, &[]);
        assert_eq!(atom.get().unwrap().get(), "hello", "Paste via the context menu must insert the real clipboard content");

        match original {
            Some(text) => { let _ = cb.write(&text); }
            None => cb.clear(),
        }
    }

    #[test]
    fn right_click_opens_the_menu_over_the_field_that_was_clicked() {
        // A right-click must focus/target the RIGHT-CLICKED field, not
        // whatever happened to be focused before — same invariant
        // `edit_controller_targets_the_correct_field_among_several`
        // already proves for controllers.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let first = Arc::new(OnceLock::new());
        let second = Arc::new(OnceLock::new());
        let root = TwoTextInputs { first: first.clone(), second: second.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(200, 200);
        let mut overlay = SkiaCanvas::new(200, 200);

        engine.paint(&mut canvas, &mut overlay, &[]);
        // Focus + populate the FIRST field via a normal click.
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "one");
        // Right-click the SECOND field (below the first, per `TwoTextInputs`'s
        // 30px-tall rows — y=45 lands mid-row, well clear of the boundary)
        // without ever left-clicking it first.
        engine.paint(&mut canvas, &mut overlay, &[right_down(20.0, 45.0)]);
        engine.paint(&mut canvas, &mut overlay, &[right_up(20.0, 45.0)]);
        engine.test_enqueue_context_menu_action(ContextMenuAction::SelectAll);
        engine.paint(&mut canvas, &mut overlay, &[]);
        type_str(&mut engine, &mut canvas, &mut overlay, "two");
        assert_eq!(first.get().unwrap().get(), "one", "the first field must be untouched by the second field's right-click");
        assert_eq!(second.get().unwrap().get(), "two", "typing after the second field's context menu must land in the SECOND field, not the first");
    }

    #[test]
    fn long_press_on_an_editable_selects_the_word_under_it() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");

        let x = embedded_x_for("hello wo"); // lands inside "world"
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        // Hold — no MouseMove/MouseUp — past the long-press threshold.
        // `LONG_PRESS_SELECT_MS` is 500; sleep comfortably past it.
        std::thread::sleep(std::time::Duration::from_millis(650));
        engine.paint(&mut canvas, &mut overlay, &[]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "hello X", "a long press must select the whole word under it, same as a double-click");
    }

    #[test]
    fn a_quick_press_and_release_does_not_trigger_long_press_select() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");

        let x = embedded_x_for("hello wo");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_up(x, 18.0)]);
        std::thread::sleep(std::time::Duration::from_millis(650));
        engine.paint(&mut canvas, &mut overlay, &[]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "hello woXrld", "releasing promptly must cancel the long-press timer, leaving a plain caret insert");
    }

    #[test]
    fn dragging_a_selection_handle_extends_the_selection() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (mut engine, mut canvas, mut overlay, atom) = headless_text_input_engine();
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello world");

        // Double-click selects "world" (6..11) — creates the handles this
        // test then drags.
        let x = embedded_x_for("hello wo");
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[click(x, 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[]); // repaint so the handle anchors reflect the new selection

        // Grab the START handle (at "hello " boundary, index 6) and drag
        // it back to the very start of the field — extends the selection
        // to cover "hello world" entirely.
        let handle_x = embedded_x_for("hello ");
        let px = default_text_input_px();
        let line_h = rosace_render::FontCache::embedded().line_height(px);
        let handle_y = 18.0 - (px / 2.0) + line_h; // matches TextInput's own ty + line_h
        engine.paint(&mut canvas, &mut overlay, &[
            rosace_platform::InputEvent::MouseDown { x: handle_x, y: handle_y, button: rosace_platform::MouseButton::Left },
        ]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_move(embedded_x_for(""), 18.0)]);
        engine.paint(&mut canvas, &mut overlay, &[mouse_up(embedded_x_for(""), 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "X");
        assert_eq!(atom.get().unwrap().get(), "X", "dragging the start handle to the field's start must extend the selection to cover the whole value");
    }

    // ── rosace-forms wiring + input filters (D116 Step 8) ─────────────────

    struct OneFilteredTextInput {
        captured: Arc<OnceLock<rosace_state::Atom<String>>>,
        filters: Vec<text_edit::InputFilter>,
    }
    impl Component for OneFilteredTextInput {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let name: rosace_state::Atom<String> = ctx.state(String::new());
            let _ = self.captured.set(name.clone());
            TextInput::new()
                .value(name.get())
                .width(300.0)
                .filters(self.filters.clone())
                .on_change({ let name = name.clone(); move |v| name.set(v) })
                .boxed()
        }
    }

    #[test]
    fn digits_filter_strips_non_digit_characters_typed_through_real_dispatch() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(OnceLock::new());
        let root = OneFilteredTextInput { captured: captured.clone(), filters: vec![text_edit::InputFilter::digits()] };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(300, 60);
        let mut overlay = SkiaCanvas::new(300, 60);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "a1b2c3");
        assert_eq!(captured.get().unwrap().get(), "123", "a digits-only filter must strip letters as they're typed, not just on submit");
    }

    #[test]
    fn max_length_filter_truncates_typing_through_real_dispatch() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(OnceLock::new());
        let root = OneFilteredTextInput { captured: captured.clone(), filters: vec![text_edit::InputFilter::max_length(3)] };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(300, 60);
        let mut overlay = SkiaCanvas::new(300, 60);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        assert_eq!(captured.get().unwrap().get(), "hel", "typing past MaxLength must truncate, every keystroke, not just at the end");
    }

    #[test]
    fn max_length_filter_still_lets_backspace_shrink_the_value() {
        // A real correctness risk of clamping the selection on every
        // filtered commit: backspace itself produces a SHORTER value
        // than before filtering even runs, so `filtered == new_value`
        // there — must not somehow re-lengthen or get stuck.
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(OnceLock::new());
        let root = OneFilteredTextInput { captured: captured.clone(), filters: vec![text_edit::InputFilter::max_length(3)] };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(300, 60);
        let mut overlay = SkiaCanvas::new(300, 60);
        engine.paint(&mut canvas, &mut overlay, &[]);
        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "hello");
        engine.paint(&mut canvas, &mut overlay, &[key(rosace_platform::Key::Backspace)]);
        assert_eq!(captured.get().unwrap().get(), "he");
    }

    struct OneFormTextInput {
        captured_field: Arc<OnceLock<rosace_widgets::forms::FormField>>,
        submitted: Arc<std::sync::atomic::AtomicBool>,
    }
    impl Component for OneFormTextInput {
        fn build(&self, ctx: &mut Context) -> BoxedWidget {
            let field = ctx.state(rosace_widgets::forms::FormField::new("name")).get()
                .rule(rosace_widgets::forms::Required);
            let _ = self.captured_field.set(field.clone());

            // The submit button below gates on `form.is_valid()`, which lives
            // in a DIFFERENT widget — marking the field's own node repaints
            // the input but cannot repaint the button. So the app owns that
            // dependency: the field reports a change, and this bumps state the
            // component reads, which rebuilds.
            let revision = ctx.state(0u32);
            field.on_change({
                let r = revision.clone();
                move || r.set(r.get().wrapping_add(1))
            });
            let _ = revision.get();
            let form = rosace_widgets::forms::Form::new().field(field.clone());
            let submitted = self.submitted.clone();
            Column::new()
                // `.field()` is the WHOLE binding — deliberately no
                // separate `.on_change()` call after it (that would
                // override the binding, per `.field()`'s own doc
                // comment; `field.get()` IS the value to read back).
                .child(TextInput::new().width(300.0).field(field.clone()))
                .child(Button::new("Submit").disabled_if(!form.is_valid()).on_press(move || {
                    let submitted = submitted.clone();
                    form.submit(move || { submitted.store(true, std::sync::atomic::Ordering::Relaxed); });
                }))
                .boxed()
        }
    }

    #[test]
    fn typing_in_a_bound_field_updates_the_forms_live_validity() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured_field = Arc::new(OnceLock::new());
        let submitted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let root = OneFormTextInput { captured_field: captured_field.clone(), submitted: submitted.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(300, 120);
        let mut overlay = SkiaCanvas::new(300, 120);
        engine.paint(&mut canvas, &mut overlay, &[]);

        let field = captured_field.get().unwrap();
        assert!(!field.is_valid(), "an empty Required field must be invalid from the very first paint, before any typing");
        assert!(!field.is_touched(), "but not yet flagged touched — no error caption until the user reaches it");

        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "alice");
        let field = captured_field.get().unwrap();
        assert!(field.is_touched());
        assert!(field.is_valid(), "a Required field with real text must become valid live, through real keyboard dispatch");
        assert_eq!(field.get(), "alice", "the field's own shared value must reflect real keyboard dispatch");
    }

    #[test]
    fn submit_button_gates_on_form_validity_and_calls_the_real_submit_callback() {
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let captured_field = Arc::new(OnceLock::new());
        let submitted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let root = OneFormTextInput { captured_field: captured_field.clone(), submitted: submitted.clone() };
        let mut engine = FrameEngine::new(Box::new(root), rosace_render::FontCache::embedded());
        let mut canvas = SkiaCanvas::new(300, 120);
        let mut overlay = SkiaCanvas::new(300, 120);
        engine.paint(&mut canvas, &mut overlay, &[]);

        // The submit button sits below the 36px-tall TextInput. A real
        // click resolves on release (see `pending_press`), so tests must
        // send MouseUp too, not just MouseDown.
        let submit_y = 50.0;
        engine.paint(&mut canvas, &mut overlay, &[click(60.0, submit_y), mouse_up(60.0, submit_y)]);
        assert!(!submitted.load(std::sync::atomic::Ordering::Relaxed), "a disabled button (empty Required field) must not register the click at all");

        engine.paint(&mut canvas, &mut overlay, &[click(20.0, 18.0), mouse_up(20.0, 18.0)]);
        type_str(&mut engine, &mut canvas, &mut overlay, "alice");
        engine.paint(&mut canvas, &mut overlay, &[click(60.0, submit_y), mouse_up(60.0, submit_y)]);
        assert!(submitted.load(std::sync::atomic::Ordering::Relaxed), "a real click on a now-enabled submit button must run Form::submit's callback");
    }

    /// A screen-reader activation must press the real control.
    ///
    /// Roles and labels shipped in D132 while activation did not: assistive
    /// tech could read the UI and then not use it. This drives the whole
    /// path — publish the tree, find a button by its LABEL the way a screen
    /// reader would, queue an Activate against the id the tree published,
    /// and assert the button's own `on_press` ran.
    #[test]
    fn an_accessibility_activation_presses_the_control() {
        use rosace_core::a11y::actions::{request, A11yAction};
        use std::sync::atomic::{AtomicBool, Ordering};
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let pressed = Arc::new(AtomicBool::new(false));
        struct OneButton(Arc<AtomicBool>);
        impl Component for OneButton {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                let hit = self.0.clone();
                rosace_widgets::tree::Button::new("Save")
                    .on_press(move || hit.store(true, Ordering::SeqCst))
                    .boxed()
            }
        }

        let mut e = FrameEngine::new(Box::new(OneButton(pressed.clone())),
                                     rosace_render::FontCache::embedded());
        let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
        e.paint(&mut a, &mut b, &[]);

        // Locate the button the way assistive tech does: by its announced
        // label, not by a coordinate the test computed.
        fn find(n: &rosace_core::SemanticNode, label: &str) -> Option<u64> {
            if n.label.as_deref() == Some(label) { return n.id; }
            n.children.iter().find_map(|c| find(c, label))
        }
        let id = find(&e.semantics(), "Save").expect("the button must be announced");

        assert!(!pressed.load(Ordering::SeqCst), "not pressed before the action");
        request(id, A11yAction::Activate);
        e.paint(&mut a, &mut b, &[]);
        assert!(pressed.load(Ordering::SeqCst),
            "an Activate action did not reach the button's on_press");
    }

    /// A stale id must not panic. Assistive tech acts on the tree it was
    /// last given, which may name a node that no longer exists — a row in a
    /// list that shortened between the announcement and the tap.
    #[test]
    fn an_activation_for_a_vanished_node_is_ignored() {
        use rosace_core::a11y::actions::{request, A11yAction};
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct Empty;
        impl Component for Empty {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                rosace_widgets::tree::Spacer::new(8.0).boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(Empty), rosace_render::FontCache::embedded());
        let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
        e.paint(&mut a, &mut b, &[]);

        request(u64::MAX, A11yAction::Activate);
        e.paint(&mut a, &mut b, &[]); // must not panic
    }


    /// The system back intent, resolved in the documented order.
    ///
    /// Android's back button and iOS's edge swipe were not wired at all:
    /// pressing back on a screen three deep EXITED the app instead of going
    /// back. This drives the real path — a nav stack, a real overlay, and
    /// `InputEvent::BackPressed` — rather than calling `dispatch_back`
    /// directly, because the ordering between overlays and the navigator is
    /// the part that is easy to get wrong.
    #[test]
    fn back_closes_an_overlay_first_then_pops_then_declines() {
        use rosace_nav::ScreenNav;
        use rosace_widgets::OverlayApi;
        use std::sync::Mutex;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        #[derive(Clone, PartialEq, Debug)]
        enum R { Home, Detail }

        struct App {
            depth: Arc<Mutex<usize>>,
            dialog: Arc<Mutex<bool>>,
            // Captured so the test can open the dialog directly instead of
            // guessing a coordinate that hits the button.
            open_atom: Arc<OnceLock<rosace_state::Atom<bool>>>,
        }
        impl Component for App {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let nav = ScreenNav::new(ctx, R::Home);
                // Push a second screen ONCE, so there is somewhere to pop
                // back to. Guarded by its own state: an unguarded
                // `if depth == 1 { push }` re-pushes on the rebuild that
                // the pop itself triggers, so back would appear to do
                // nothing.
                let seeded = ctx.state(false);
                if !seeded.get() {
                    seeded.set(true);
                    nav.push(R::Detail);
                }
                *self.depth.lock().unwrap() = nav.depth();

                let open = ctx.state(false);
                let _ = self.open_atom.set(open.clone());
                *self.dialog.lock().unwrap() = open.get();
                let o = open.clone();
                rosace_widgets::tree::Button::new("Open")
                    .on_press(move || o.set(true))
                    .dialog(open.get(), || Arc::new(
                        rosace_widgets::tree::Dialog::new("Hi").message("there"),
                    ))
                    .on_open_change({ let open = open.clone(); move |v| open.set(v) })
                    .boxed()
            }
        }

        let depth = Arc::new(Mutex::new(0));
        let dialog = Arc::new(Mutex::new(false));
        let open_atom = Arc::new(OnceLock::new());
        let mut e = FrameEngine::new(
            Box::new(App {
                depth: depth.clone(),
                dialog: dialog.clone(),
                open_atom: open_atom.clone(),
            }),
            rosace_render::FontCache::embedded(),
        );
        let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));

        let back = || rosace_platform::InputEvent::BackPressed;

        e.paint(&mut a, &mut b, &[]);
        assert_eq!(*depth.lock().unwrap(), 2, "pushed a second screen");

        // Open the dialog, then back must close IT and leave the stack alone.
        open_atom.get().unwrap().set(true);
        e.paint(&mut a, &mut b, &[]);
        assert!(*dialog.lock().unwrap(), "the dialog should be open");

        e.paint(&mut a, &mut b, &[back()]);
        e.paint(&mut a, &mut b, &[]);
        assert!(e.back_was_handled(), "closing an overlay consumes the intent");
        assert!(!*dialog.lock().unwrap(), "back must close the dialog");
        assert_eq!(*depth.lock().unwrap(), 2,
            "back must NOT also pop the screen under the dialog");

        // Now back pops the screen.
        e.paint(&mut a, &mut b, &[back()]);
        e.paint(&mut a, &mut b, &[]);
        assert!(e.back_was_handled(), "popping consumes the intent");
        assert_eq!(*depth.lock().unwrap(), 1, "back must pop to the root");
    }

    /// At the root the app must DECLINE, so Android leaves the app rather
    /// than trapping the user in it.
    #[test]
    fn back_at_the_root_is_declined_so_the_platform_can_act() {
        use rosace_nav::ScreenNav;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct RootOnly;
        impl Component for RootOnly {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let _nav = ScreenNav::new(ctx, 0u8);
                rosace_widgets::tree::Text::new("root").boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(RootOnly), rosace_render::FontCache::embedded());
        let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
        e.paint(&mut a, &mut b, &[]);
        e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::BackPressed]);
        assert!(!e.back_was_handled(),
            "a root navigator must not swallow back — the user could not leave the app");
    }


    /// `WillPopScope` must gate EVERY way out of a screen.
    ///
    /// The gate lives inside `ScreenNav::pop`, not in the back-intent
    /// handler, precisely so the AppBar's own back button cannot walk past
    /// it. A screen that protects unsaved work from the system gesture but
    /// loses it to the toolbar button would be worse than no guard at all,
    /// so both routes are driven here.
    #[test]
    fn will_pop_scope_blocks_the_system_back_and_the_app_bar_button_alike() {
        use rosace_nav::ScreenNav;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Mutex;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct App {
            depth: Arc<Mutex<usize>>,
            asked: Arc<AtomicUsize>,
            dirty: Arc<AtomicBool>,
            nav_out: Arc<OnceLock<ScreenNav<u8>>>,
        }
        impl Component for App {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let nav = ScreenNav::new(ctx, 0u8);
                let seeded = ctx.state(false);
                if !seeded.get() { seeded.set(true); nav.push(1u8); }
                *self.depth.lock().unwrap() = nav.depth();
                let _ = self.nav_out.set(nav.clone());

                let (asked, dirty) = (self.asked.clone(), self.dirty.clone());
                rosace_widgets::tree::WillPopScope::new(
                    rosace_widgets::tree::Text::new("editor"),
                )
                .on_will_pop(move || {
                    asked.fetch_add(1, Ordering::SeqCst);
                    // Real shape: allow only once the work is saved.
                    !dirty.load(Ordering::SeqCst)
                })
                .boxed()
            }
        }

        let depth = Arc::new(Mutex::new(0));
        let asked = Arc::new(AtomicUsize::new(0));
        let dirty = Arc::new(AtomicBool::new(true));
        let nav_out = Arc::new(OnceLock::new());
        let mut e = FrameEngine::new(
            Box::new(App {
                depth: depth.clone(), asked: asked.clone(),
                dirty: dirty.clone(), nav_out: nav_out.clone(),
            }),
            rosace_render::FontCache::embedded(),
        );
        let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
        e.paint(&mut a, &mut b, &[]);
        assert_eq!(*depth.lock().unwrap(), 2);

        // 1. SYSTEM back — blocked, and still CONSUMED so Android does not
        //    finish the activity out from under the question the guard asked.
        e.paint(&mut a, &mut b, &[rosace_platform::InputEvent::BackPressed]);
        e.paint(&mut a, &mut b, &[]);
        assert_eq!(asked.load(Ordering::SeqCst), 1, "the guard must be consulted");
        assert_eq!(*depth.lock().unwrap(), 2, "blocked: still on the same screen");
        assert!(e.back_was_handled(),
            "a blocked pop still consumes the intent — otherwise Android exits");

        // 2. The APP BAR button calls nav.pop() directly. Same guard.
        let nav = nav_out.get().unwrap().clone();
        assert!(!nav.pop(), "pop must report it was blocked");
        e.paint(&mut a, &mut b, &[]);
        assert_eq!(asked.load(Ordering::SeqCst), 2, "consulted again");
        assert_eq!(*depth.lock().unwrap(), 2, "the toolbar button cannot bypass the guard");

        // 3. Work saved -> the same guard now allows, via BOTH routes.
        dirty.store(false, Ordering::SeqCst);
        assert!(nav.pop(), "an allowing guard lets the pop through");
        e.paint(&mut a, &mut b, &[]);
        assert_eq!(*depth.lock().unwrap(), 1, "back at the root");
    }


    /// A rebuild must be traceable back to the atom that caused it.
    ///
    /// `ComponentRebuild` existed since the trace crate was written and was
    /// never emitted, so the DevTools lifecycle timeline showed mounts and
    /// unmounts with a hole exactly where the work happens. Worse, the cause
    /// was unknowable: `mark_dirty` threw the atom id away at the one point
    /// that had it, so even once emitted the answer to "why did this
    /// rebuild?" would have been a shrug.
    #[test]
    fn a_rebuild_is_traced_with_the_atom_that_caused_it() {
        use rosace_trace::event::{RebuildCause, RosaceTrace};
        use std::sync::Mutex;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct Sink(Arc<Mutex<Vec<RebuildCause>>>);
        impl rosace_trace::TraceSubscriber for Sink {
            fn on_trace(&self, e: &RosaceTrace) {
                if let RosaceTrace::ComponentRebuild { cause, .. } = e {
                    self.0.lock().unwrap().push(cause.clone());
                }
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        rosace_trace::TRACING_BUS.clear_subscribers();
        rosace_trace::TRACING_BUS.add_subscriber(Arc::new(Sink(seen.clone())));

        struct Counter(Arc<OnceLock<rosace_state::Atom<i32>>>);
        impl Component for Counter {
            fn build(&self, ctx: &mut Context) -> BoxedWidget {
                let n = ctx.state(0i32);
                let _ = self.0.set(n.clone());
                rosace_widgets::tree::Text::new(n.get().to_string()).boxed()
            }
        }

        let atom_out = Arc::new(OnceLock::new());
        let mut e = FrameEngine::new(Box::new(Counter(atom_out.clone())),
                                     rosace_render::FontCache::embedded());
        let (mut a, mut b) = (SkiaCanvas::new(200, 100), SkiaCanvas::new(200, 100));
        e.paint(&mut a, &mut b, &[]); // first frame: globally dirty
        seen.lock().unwrap().clear();

        // A real atom write is the only thing that dirties this component.
        let atom = atom_out.get().unwrap().clone();
        atom.set(7);
        e.paint(&mut a, &mut b, &[]);

        let events = seen.lock().unwrap().clone();
        rosace_trace::TRACING_BUS.clear_subscribers();
        assert!(!events.is_empty(), "the rebuild emitted no ComponentRebuild at all");
        match &events[0] {
            RebuildCause::AtomChanged(id) => assert_eq!(*id, atom.id(),
                "traced the wrong atom as the cause"),
            other => panic!("expected AtomChanged, got {other:?} — the cause is not being carried"),
        }
    }


    /// A tap must be traceable. "The tap did nothing" is the most common
    /// bug report on Android, and the gesture panel shipped empty — so
    /// there was no way to tell "no hit region there" from "a handler ran
    /// and did nothing", which are different bugs with different fixes.
    #[test]
    fn a_tap_emits_a_gesture_trace_when_it_finds_a_handler() {
        use rosace_trace::event::{GestureKind, RosaceTrace};
        use std::sync::Mutex;
        let _guard = ANIMATION_GLOBAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        struct Sink(Arc<Mutex<Vec<GestureKind>>>);
        impl rosace_trace::TraceSubscriber for Sink {
            fn on_trace(&self, e: &RosaceTrace) {
                if let RosaceTrace::GestureReceived { kind, .. } = e {
                    self.0.lock().unwrap().push(kind.clone());
                }
            }
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        rosace_trace::TRACING_BUS.clear_subscribers();
        rosace_trace::TRACING_BUS.add_subscriber(Arc::new(Sink(seen.clone())));

        struct OneButton;
        impl Component for OneButton {
            fn build(&self, _c: &mut Context) -> BoxedWidget {
                // Inside a Column so the button does NOT fill the canvas —
                // otherwise every coordinate is a hit and the miss case
                // cannot be expressed.
                rosace_widgets::tree::Column::new()
                    .child(rosace_widgets::tree::Button::new("Tap me").on_press(|| {}))
                    .boxed()
            }
        }
        let mut e = FrameEngine::new(Box::new(OneButton), rosace_render::FontCache::embedded());
        let (mut a, mut b) = (SkiaCanvas::new(300, 200), SkiaCanvas::new(300, 200));
        e.paint(&mut a, &mut b, &[]);

        // Miss first: empty space must NOT report a gesture, or the trace
        // would say a handler was found when none was.
        e.paint(&mut a, &mut b, &[click(290.0, 190.0)]);
        assert!(seen.lock().unwrap().is_empty(), "a miss must not report a handler");

        // Then a real hit on the button.
        e.paint(&mut a, &mut b, &[click(20.0, 18.0)]);
        let got = seen.lock().unwrap().clone();
        rosace_trace::TRACING_BUS.clear_subscribers();
        assert!(matches!(got.first(), Some(GestureKind::Tap)),
            "a tap on a button must trace as Tap, got {got:?}");
    }

}
