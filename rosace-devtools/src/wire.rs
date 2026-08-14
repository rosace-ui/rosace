//! The wire format an external DevTools client reads.
//!
//! # ROSACE emits; it does not store
//!
//! The framework's job here ends at "something happened, here it is". It
//! keeps no history, no database and no session — whoever connects decides
//! what is worth keeping and for how long. A web client that wants an hour
//! of timeline keeps an hour; one that only wants the last event keeps one.
//!
//! That is not merely a preference. Storage means a retention policy, a
//! memory budget and an eviction rule, and every app would pay for those
//! whether or not anyone is ever looking. The in-process flight recorder
//! (`rosace_trace::install_flight_recorder`) is the one exception and is
//! deliberately bounded and debug-only — a crash breadcrumb, not a store.
//!
//! # Two shapes, because they answer different questions
//!
//! * **Events** are a stream: "what just happened". Serialised as they are
//!   emitted and forwarded to whoever is listening.
//! * **Snapshots** are a pull: "what does the world look like right now".
//!   A client that connects mid-session is otherwise blind to everything
//!   that happened before it arrived — the tree exists, but no event
//!   describes it.
//!
//! Trying to serve the second from the first is the mistake to avoid: you
//! cannot reconstruct a tree by replaying events unless you were listening
//! from process start, which no external client ever is.

use serde::Serialize;

use rosace_trace::event::RosaceTrace;
use rosace_widgets::tree::render_tree::InspectNode;

/// One trace event, flattened for the wire.
///
/// A mirror of [`RosaceTrace`] rather than a `Serialize` derive on it: that
/// would put serde into `rosace-trace`, which everything depends on and
/// which currently carries one dependency. It also decouples the wire
/// format from the internal enum, so adding a variant does not silently
/// change what clients receive.
#[derive(Debug, Clone, Serialize)]
pub struct WireEvent {
    /// Stable machine name — `"component_rebuild"`, `"request_end"`, …
    pub kind: &'static str,
    /// Coarse grouping, matching the internal `TraceCategory`.
    pub category: &'static str,
    /// Milliseconds since the client is irrelevant — this is the duration
    /// the event itself carries, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    /// Human-readable summary. Deliberately the same text the console
    /// subscriber prints, so a client shows what a developer already
    /// recognises from the terminal.
    pub detail: String,
}

impl WireEvent {
    pub fn from_trace(e: &RosaceTrace) -> Self {
        use RosaceTrace as T;
        let (kind, detail): (&'static str, String) = match e {
            T::ComponentMount { id, name, .. } => ("component_mount", format!("{name} #{}", id.0)),
            T::ComponentUnmount { id, name } => ("component_unmount", format!("{name} #{}", id.0)),
            T::ComponentRebuild { id, cause, .. } =>
                ("component_rebuild", format!("#{} ({cause:?})", id.0)),
            T::AtomRead { atom, component } =>
                ("atom_read", format!("atom {} -> #{}", atom.0, component.0)),
            T::AtomWrite { atom, old, new, by, .. } =>
                ("atom_write", format!("atom {} {old:?} -> {new:?} by #{}", atom.0, by.0)),
            T::LayoutStart { .. } => ("layout_start", String::new()),
            T::LayoutEnd { .. } => ("layout_end", String::new()),
            T::FrameStart { frame, .. } => ("frame_start", format!("frame {frame}")),
            T::FrameEnd { frame, .. } => ("frame_end", format!("frame {frame}")),
            T::PaintRegion { rect } =>
                ("paint_region", format!("{:.0}x{:.0}", rect.size.width, rect.size.height)),
            T::RouteChange { from, to, .. } => ("route_change", format!("{from:?} -> {to:?}")),
            T::RequestStart { id, url, method, .. } =>
                ("request_start", format!("#{} {method:?} {url}", id.0)),
            T::RequestEnd { id, status, cached, size, .. } =>
                ("request_end", format!("#{} {status} {size}B{}", id.0, if *cached { " (cached)" } else { "" })),
            T::FfiCall { fn_name, .. } => ("ffi_call", (*fn_name).to_string()),
            T::FfiError { fn_name, error } => ("ffi_error", format!("{fn_name}: {error}")),
            T::GestureReceived { kind, handler } =>
                ("gesture", format!("{kind:?} -> #{}", handler.0)),
            T::ShaderRegister { pipeline, wgsl_len } =>
                ("shader_register", format!("pipeline {pipeline} ({wgsl_len} bytes)")),
            T::Log { level, message, .. } => ("log", format!("[{level:?}] {message}")),
        };
        Self {
            kind,
            category: category_name(e),
            duration_ms: duration_of(e).map(|d| d.as_secs_f64() * 1000.0),
            detail,
        }
    }
}

fn category_name(e: &RosaceTrace) -> &'static str {
    use rosace_trace::event::TraceCategory as C;
    match e.category() {
        C::State => "state",
        C::Lifecycle => "lifecycle",
        C::Layout => "layout",
        C::Frame => "frame",
        C::Render => "render",
        C::Route => "route",
        C::Network => "network",
        C::Ffi => "ffi",
        C::Gesture => "gesture",
        C::Shader => "shader",
        C::Log => "log",
    }
}

fn duration_of(e: &RosaceTrace) -> Option<std::time::Duration> {
    use RosaceTrace as T;
    match e {
        T::ComponentRebuild { duration, .. }
        | T::LayoutEnd { duration, .. }
        | T::FrameEnd { duration, .. }
        | T::RequestEnd { duration, .. }
        | T::FfiCall { duration, .. } => Some(*duration),
        _ => None,
    }
}

/// One render-tree node, flattened for the wire.
///
/// A projection of [`InspectNode`], not a copy: handlers, pictures and
/// `Arc`s inside the real node are neither serialisable nor useful to a
/// client. What survives is what answers "what is on screen and where".
#[derive(Debug, Clone, Serialize)]
pub struct WireNode {
    pub id: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Widget type name, from `std::any::type_name`.
    pub tag: &'static str,
    /// `[x, y, w, h]` in logical pixels, absent if never painted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rect: Option<[f32; 4]>,
    /// Declared accessibility entries as `(role, label)`.
    pub semantics: Vec<(String, Option<String>)>,
    pub hit_count: usize,
    pub scroll_count: usize,
    pub overlay_count: usize,
    pub has_editable: bool,
    pub hovered: bool,
    pub pressed: bool,
}

impl From<&InspectNode> for WireNode {
    fn from(n: &InspectNode) -> Self {
        Self {
            id: n.id,
            parent: n.parent,
            children: n.children.clone(),
            tag: n.tag,
            rect: n.rect.map(|r| [r.origin.x, r.origin.y, r.size.width, r.size.height]),
            semantics: n.semantics.iter()
                .map(|(role, label)| (format!("{role:?}"), label.clone()))
                .collect(),
            hit_count: n.hit_count,
            scroll_count: n.scroll_count,
            overlay_count: n.overlay_count,
            has_editable: n.has_editable,
            hovered: n.hovered,
            pressed: n.pressed,
        }
    }
}

/// A whole-tree snapshot: the answer to "what is on screen right now".
#[derive(Debug, Clone, Serialize)]
pub struct TreeSnapshot {
    pub nodes: Vec<WireNode>,
}

impl TreeSnapshot {
    pub fn from_inspect(nodes: &[InspectNode]) -> Self {
        Self { nodes: nodes.iter().map(WireNode::from).collect() }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"nodes\":[]}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_trace::event::{ComponentId, RebuildCause, RosaceTrace};

    #[test]
    fn an_event_carries_a_stable_kind_a_category_and_a_duration() {
        let e = RosaceTrace::ComponentRebuild {
            id: ComponentId(3),
            cause: RebuildCause::Manual,
            duration: std::time::Duration::from_millis(7),
        };
        let w = WireEvent::from_trace(&e);
        assert_eq!(w.kind, "component_rebuild");
        assert_eq!(w.category, "lifecycle");
        assert_eq!(w.duration_ms, Some(7.0));
        assert!(w.detail.contains("#3"));
    }

    /// The `kind` strings are a CONTRACT with an out-of-process client that
    /// ships separately and may be older or newer than the app. Renaming one
    /// silently breaks a filter in a client nobody can recompile from here.
    #[test]
    fn event_kinds_are_stable_names_not_debug_output() {
        let cases = [
            (RosaceTrace::FfiError { fn_name: "ch", error: "boom".into() }, "ffi_error", "ffi"),
            (RosaceTrace::GestureReceived {
                kind: rosace_trace::event::GestureKind::Tap,
                handler: ComponentId(1),
            }, "gesture", "gesture"),
        ];
        for (e, kind, cat) in cases {
            let w = WireEvent::from_trace(&e);
            assert_eq!(w.kind, kind);
            assert_eq!(w.category, cat);
        }
    }

    /// An event with no duration must OMIT the field rather than send zero —
    /// a client cannot tell "took 0ms" from "has no duration" otherwise, and
    /// would draw a 0ms bar for a gesture.
    #[test]
    fn an_event_without_a_duration_omits_the_field() {
        let e = RosaceTrace::GestureReceived {
            kind: rosace_trace::event::GestureKind::Tap,
            handler: ComponentId(1),
        };
        let json = serde_json::to_string(&WireEvent::from_trace(&e)).unwrap();
        assert!(!json.contains("duration_ms"), "absent, not zero: {json}");
    }

    #[test]
    fn a_tree_snapshot_serialises_structure_and_geometry() {
        use rosace_widgets::tree::RenderTree;
        let mut t = RenderTree::new();
        t.start_frame();
        let child = t.slot(RenderTree::ROOT, true);
        t.node_mut(child).cached_rect = Some(rosace_core::types::Rect {
            origin: rosace_core::types::Point { x: 1.0, y: 2.0 },
            size: rosace_core::types::Size { width: 3.0, height: 4.0 },
        });
        t.finalize();

        let snap = TreeSnapshot::from_inspect(&t.inspect());
        assert!(snap.nodes.len() >= 2, "root plus the child");
        let json = snap.to_json();
        assert!(json.contains("\"rect\":[1.0,2.0,3.0,4.0]"), "geometry must survive: {json}");
        assert!(json.contains("\"children\""), "structure must survive");
    }
}
