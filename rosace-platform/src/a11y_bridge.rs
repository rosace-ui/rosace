//! Platform accessibility bridge (D132) — pushes ROSACE's semantic tree to
//! the OS assistive-technology APIs via AccessKit.
//!
//! ROSACE draws every pixel itself into a GPU/CPU surface, so the OS has no
//! idea what is inside the window: to VoiceOver, an un-bridged ROSACE window
//! is one opaque rectangle. This module is the missing translation layer. It
//! takes the same [`SemanticNode`] tree that D107 already renders to HTML for
//! web SEO and republishes it through AccessKit, which speaks NSAccessibility
//! on macOS, UI Automation on Windows, and AT-SPI on Linux.
//!
//! **Structure mirrors `web_seo_sync`** (the web half of the same "one
//! semantic tree, many consumers" idea): platform owns a thread-local sink,
//! and the umbrella crate — which is the layer that actually owns the engine —
//! calls [`sync`] with `engine.semantics()`. `rosace-platform` sits *below*
//! the engine and cannot reach up to pull the tree itself.
//!
//! Two AccessKit constraints shape this file:
//!
//! 1. **The adapter must be created before the window is first shown**, or it
//!    panics. `resumed()` therefore builds the window invisible, calls
//!    [`init`], then shows it.
//! 2. **A `TreeUpdate` must be fully self-contained**: every node in it must
//!    be the root or a child of another node in the same update, and updating
//!    a node requires re-sending its unchanged fields too. So [`sync`] emits
//!    the whole tree each time rather than a diff. That is affordable because
//!    the caller already gates on "this frame may have changed something",
//!    exactly as the web shadow-DOM sync does.

use std::cell::{Cell, RefCell};
use std::sync::Mutex;

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId, Rect,
    Tree, TreeId, TreeUpdate,
};
use accesskit_winit::Adapter;
use rosace_core::{Role, SemanticNode};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// AccessKit requires a single root node that owns everything else. Our own
/// ids come from render-tree `NodeId`s shifted left by 8 (see
/// `SemanticNode::id`), so 0 is never produced by a real widget and is safe
/// to reserve.
const ROOT_ID: NodeId = NodeId(0);

thread_local! {
    /// The live adapter. Thread-local rather than passed around because the
    /// umbrella's render closure has no handle to `AppState` — the same
    /// reason `web_seo_sync` keeps its shadow root this way.
    static ADAPTER: RefCell<Option<Adapter>> = const { RefCell::new(None) };

    /// The window's current scale factor.
    ///
    /// AccessKit's `bounds` contract is explicit that coordinates must be
    /// **physical** pixels relative to the window origin, whereas our
    /// `cached_rect` is in logical pixels. On a 2x Retina display, feeding
    /// logical values straight through puts every element at half its true
    /// position and size — verified live: the OS reported the "Get Started"
    /// button as 65x17 at 483,308 when it is really about twice that, and a
    /// click at the reported point missed the button entirely.
    static SCALE: Cell<f64> = const { Cell::new(1.0) };
}

/// The most recent tree, shared with the activation handler.
///
/// `ActivationHandler` is called by the platform on *its* schedule (and, on
/// some platforms, its own thread) the moment assistive tech attaches — which
/// may be long before or after any frame we paint. It must be able to answer
/// with a full tree immediately, so the latest one is parked here rather than
/// regenerated on demand. `Arc<Mutex<_>>` because the handler is required to
/// be `Send + 'static`; the thread-local adapter above cannot be.
static LATEST: Mutex<Option<TreeUpdate>> = Mutex::new(None);

fn latest() -> &'static Mutex<Option<TreeUpdate>> {
    &LATEST
}

/// Hands the platform whatever tree we last published.
struct Activation;

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        latest().lock().ok().and_then(|t| t.clone())
    }
}

/// Assistive tech can request actions (click, focus, scroll-into-view).
///
/// Deliberately a no-op for now: honouring them means routing back into the
/// engine's hit-test/dispatch path, which lives above this crate. Announcing
/// and navigating the tree — the part that makes the app *readable* to a
/// screen reader, and queryable by automation — works without it. Wiring
/// actions is the natural follow-up, and is called out as such in D132 rather
/// than silently pretended.
struct Actions;

impl ActionHandler for Actions {
    fn do_action(&mut self, _request: ActionRequest) {}
}

struct Deactivation;

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        if let Ok(mut t) = latest().lock() {
            *t = None;
        }
    }
}

/// Creates the adapter for `window`. Must be called while the window is still
/// invisible (AccessKit panics otherwise), and before any [`sync`] call.
pub fn init(event_loop: &ActiveEventLoop, window: &Window) {
    let adapter = Adapter::with_direct_handlers(
        event_loop,
        window,
        Activation,
        Actions,
        Deactivation,
    );
    SCALE.with(|s| s.set(window.scale_factor()));
    ADAPTER.with(|a| *a.borrow_mut() = Some(adapter));
}

/// Forwards a winit event to the adapter so it can track focus and window
/// geometry. Cheap no-op when accessibility was never activated.
pub fn process_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::ScaleFactorChanged { scale_factor, .. } = event {
        SCALE.with(|s| s.set(*scale_factor));
    }
    ADAPTER.with(|a| {
        if let Some(adapter) = a.borrow_mut().as_mut() {
            adapter.process_event(window, event);
        }
    });
}

/// Publishes `tree` to the OS accessibility layer.
///
/// The tree is built on every call, not only while assistive tech is
/// attached. That is deliberate and was found the hard way: building it
/// *only* inside `update_if_active` deadlocks activation. The platform
/// activates by calling [`ActivationHandler::request_initial_tree`], which
/// must answer synchronously from [`LATEST`] — it is `Send + 'static` and has
/// no route back to the engine to build one itself. If `LATEST` is only
/// filled from inside `update_if_active`, and `update_if_active` only runs
/// once active, neither ever happens: verified live, the window exposed an
/// empty `AXGroup` with zero children.
///
/// So the cost is one tree walk per *content-changed* frame (the caller
/// already gates on that), and `update_if_active` still skips the far more
/// expensive platform round-trip when nothing is listening.
pub fn sync(tree: &SemanticNode) {
    ADAPTER.with(|a| {
        let mut slot = a.borrow_mut();
        let Some(adapter) = slot.as_mut() else { return };
        let update = build_update(tree);
        if let Ok(mut latest) = latest().lock() {
            *latest = Some(update.clone());
        }
        adapter.update_if_active(move || update);
    });
}

/// Flattens our nested [`SemanticNode`] tree into AccessKit's flat
/// `(NodeId, Node)` list.
fn build_update(tree: &SemanticNode) -> TreeUpdate {
    let mut nodes = Vec::new();
    let mut root = Node::new(accesskit::Role::Window);
    let mut root_children = Vec::new();

    for child in &tree.children {
        push_node(child, &mut nodes, &mut root_children);
    }
    root.set_children(root_children);
    nodes.push((ROOT_ID, root));

    TreeUpdate {
        nodes,
        tree: Some(Tree {
            root: ROOT_ID,
            toolkit_name: Some("ROSACE".into()),
            toolkit_version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
        tree_id: TreeId::ROOT,
        // Focus tracking is engine-owned and not yet plumbed through the
        // semantic tree, so the root holds focus. Named as a gap in D132
        // rather than faked by guessing at the first focusable node.
        focus: ROOT_ID,
    }
}

/// Appends `node` and its descendants to `out`, recording its id in `siblings`.
///
/// Nodes without an id are ones we never painted (hand-built trees); they are
/// skipped rather than given a synthetic id, which would collide across frames
/// and break the identity guarantee the whole bridge rests on.
fn push_node(node: &SemanticNode, out: &mut Vec<(NodeId, Node)>, siblings: &mut Vec<NodeId>) {
    let Some(raw) = node.id else {
        // No identity of its own — splice its children into the parent so the
        // subtree is still reachable rather than dropped.
        for child in &node.children {
            push_node(child, out, siblings);
        }
        return;
    };
    let id = NodeId(raw);
    let ak_role = map_role(&node.role);
    let mut ak = Node::new(ak_role);

    // AccessKit takes a static-text node's content from `value`, not `label`
    // — see `accesskit_consumer`'s `label_comes_from_value()`, which is true
    // exactly for `Role::Label`. Setting only `label` there yields a node the
    // OS reports with no name AND no value (verified live on macOS: two
    // `AXStaticText` children with `name=missing value`). Our `Role::Text`
    // maps to `Role::Label`, so its text has to be routed accordingly.
    let text_lives_in_value = ak_role == accesskit::Role::Label;

    if let Some(label) = &node.label {
        if text_lives_in_value {
            ak.set_value(label.clone());
        } else {
            ak.set_label(label.clone());
        }
    }
    // An explicit value always wins — for a TextInput the label is the field's
    // NAME and the value is its typed content, and they are not interchangeable.
    if let Some(value) = &node.value {
        ak.set_value(value.clone());
    }
    if let Some(b) = node.bounds {
        // Window-relative, top-down, and PHYSICAL pixels — AccessKit's
        // documented contract. Our rect is logical, so it scales here.
        let k = SCALE.with(|s| s.get());
        ak.set_bounds(Rect::new(
            b.origin.x as f64 * k,
            b.origin.y as f64 * k,
            (b.origin.x + b.size.width) as f64 * k,
            (b.origin.y + b.size.height) as f64 * k,
        ));
    }

    let mut children = Vec::new();
    for child in &node.children {
        push_node(child, out, &mut children);
    }
    ak.set_children(children);

    out.push((id, ak));
    siblings.push(id);
}

/// Maps ROSACE roles onto AccessKit's.
///
/// `Role::Unknown` becomes `GenericContainer` rather than being dropped: a
/// node with a label but no specific role is still worth announcing, and
/// dropping it would silently orphan its children.
fn map_role(role: &Role) -> accesskit::Role {
    match role {
        Role::Button => accesskit::Role::Button,
        Role::Checkbox => accesskit::Role::CheckBox,
        Role::Radio => accesskit::Role::RadioButton,
        Role::Link => accesskit::Role::Link,
        Role::Heading => accesskit::Role::Heading,
        Role::Text => accesskit::Role::Label,
        Role::TextInput => accesskit::Role::TextInput,
        Role::Image => accesskit::Role::Image,
        Role::List => accesskit::Role::List,
        Role::ListItem => accesskit::Role::ListItem,
        Role::Slider => accesskit::Role::Slider,
        Role::ProgressBar => accesskit::Role::ProgressIndicator,
        Role::Switch => accesskit::Role::Switch,
        Role::Dialog => accesskit::Role::Dialog,
        Role::Alert => accesskit::Role::Alert,
        Role::MenuItem => accesskit::Role::MenuItem,
        Role::Tab => accesskit::Role::Tab,
        Role::TabPanel => accesskit::Role::TabPanel,
        Role::SpinButton => accesskit::Role::SpinButton,
        Role::Menu => accesskit::Role::Menu,
        Role::Unknown => accesskit::Role::GenericContainer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_core::types::{Point, Rect as CoreRect, Size};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> CoreRect {
        CoreRect { origin: Point { x, y }, size: Size { width: w, height: h } }
    }

    #[test]
    fn flattens_a_nested_tree_and_preserves_parent_child_links() {
        let tree = SemanticNode::new().child(
            SemanticNode::new()
                .id(1 << 8)
                .role(Role::Button)
                .label("Save")
                .bounds(rect(10.0, 20.0, 100.0, 40.0))
                .child(SemanticNode::new().id(2 << 8).role(Role::Text).label("Save")),
        );

        let update = build_update(&tree);
        let ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
        assert!(ids.contains(&(1 << 8)), "button must be present, got {ids:?}");
        assert!(ids.contains(&(2 << 8)), "nested text must be present, got {ids:?}");
        assert!(ids.contains(&0), "the synthetic root must be present, got {ids:?}");

        let (_, button) = update.nodes.iter().find(|(id, _)| id.0 == (1 << 8)).unwrap();
        assert_eq!(button.label(), Some("Save"));
        assert_eq!(
            button.children(),
            &[NodeId(2 << 8)],
            "the button must own its nested text, not orphan it"
        );
    }

    #[test]
    fn static_text_content_goes_to_value_because_accesskit_reads_it_there() {
        // Regression for a live macOS finding: `Role::Text` maps to
        // AccessKit's `Role::Label`, whose content AccessKit reads from
        // `value` (see `label_comes_from_value`). Putting it only in `label`
        // produced AXStaticText nodes with no name and no value at all.
        let tree = SemanticNode::new()
            .child(SemanticNode::new().id(1 << 8).role(Role::Text).label("Welcome to ROSACE"));
        let update = build_update(&tree);
        let (_, n) = update.nodes.iter().find(|(id, _)| id.0 == (1 << 8)).unwrap();
        assert_eq!(
            n.value(), Some("Welcome to ROSACE"),
            "static text must expose its content via value, or the OS reads nothing"
        );
    }

    #[test]
    fn a_text_input_keeps_its_name_and_its_typed_content_separate() {
        // The inverse case: a labelled control must NOT have its name
        // overwritten by the value routing above.
        let tree = SemanticNode::new().child(
            SemanticNode::new().id(2 << 8).role(Role::TextInput).label("Email").value("ada@example.com"),
        );
        let update = build_update(&tree);
        let (_, n) = update.nodes.iter().find(|(id, _)| id.0 == (2 << 8)).unwrap();
        assert_eq!(n.label(), Some("Email"), "the field's accessible name");
        assert_eq!(n.value(), Some("ada@example.com"), "the field's typed content");
    }

    #[test]
    fn bounds_convert_from_origin_size_to_corner_coordinates() {
        let tree = SemanticNode::new().child(
            SemanticNode::new().id(1 << 8).role(Role::Button).bounds(rect(10.0, 20.0, 100.0, 40.0)),
        );
        let update = build_update(&tree);
        let (_, n) = update.nodes.iter().find(|(id, _)| id.0 == (1 << 8)).unwrap();
        let b = n.bounds().expect("bounds must survive the mapping");
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (10.0, 20.0, 110.0, 60.0));
    }

    #[test]
    fn an_id_less_node_splices_its_children_up_rather_than_dropping_them() {
        // A hand-built (never-painted) wrapper must not swallow real nodes.
        let tree = SemanticNode::new().child(
            SemanticNode::new()
                .role(Role::Unknown)
                .child(SemanticNode::new().id(7 << 8).role(Role::Button).label("Deep")),
        );
        let update = build_update(&tree);
        let ids: Vec<u64> = update.nodes.iter().map(|(id, _)| id.0).collect();
        assert!(ids.contains(&(7 << 8)), "the id-less wrapper must not drop its child: {ids:?}");
        let (_, root) = update.nodes.iter().find(|(id, _)| id.0 == 0).unwrap();
        assert_eq!(root.children(), &[NodeId(7 << 8)], "child must reparent onto the root");
    }
}
