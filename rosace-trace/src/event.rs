use std::time::Duration;
use web_time::Instant;

/// Unique identifier for a component instance in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentId(pub u64);

/// Unique identifier for an atom instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AtomId(pub u64);

/// Unique identifier for a network request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);

/// Source location captured at a trace emit site.
#[derive(Debug, Clone)]
pub struct Location {
    pub file: &'static str,
    pub line: u32,
}

/// 2D size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// 2D point in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

/// Simplified layout constraints carried in trace events.
///
/// `None` on a max field means the axis is unbounded (scroll axis or top-level).
#[derive(Debug, Clone)]
pub struct TraceConstraints {
    pub min_width: f32,
    pub max_width: Option<f32>,
    pub min_height: f32,
    pub max_height: Option<f32>,
}

/// A snapshot of an atom's value for trace events.
#[derive(Debug, Clone)]
pub enum TraceValue {
    /// Value formatted via its `Debug` impl.
    Debug(String),
    /// Value type does not implement `Debug`.
    Opaque,
}

/// Why a component was scheduled for rebuild by the refresh engine.
#[derive(Debug, Clone)]
pub enum RebuildCause {
    /// A subscribed atom changed.
    AtomChanged(AtomId),
    /// Parent component was rebuilt, child must follow.
    ParentRebuilt,
    /// Component's own props changed.
    PropsChanged,
    /// Manually triggered rebuild.
    Manual,
}

/// A navigation route (opaque string for tracing; typed routes live in rosace-nav).
#[derive(Debug, Clone)]
pub struct Route(pub String);

/// A navigation transition name.
#[derive(Debug, Clone)]
pub struct Transition(pub String);

/// HTTP method for request tracing.
#[derive(Debug, Clone)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Other(String),
}

/// Input gesture kind.
#[derive(Debug, Clone)]
pub enum GestureKind {
    Tap,
    LongPress,
    Drag,
    Swipe,
    Pinch,
    Scroll,
}

/// Unified event type emitted by all ROSACE systems.
///
/// All emit sites are gated behind `#[cfg(debug_assertions)]` via the `trace!()`
/// macro — zero cost in production builds.
#[derive(Debug, Clone)]
pub enum RosaceTrace {
    /// A component was added to the tree.
    ComponentMount {
        id: ComponentId,
        name: &'static str,
        location: Location,
    },
    /// A component was removed from the tree.
    ComponentUnmount {
        id: ComponentId,
        name: &'static str,
    },
    /// A component was rebuilt by the refresh engine.
    ComponentRebuild {
        id: ComponentId,
        cause: RebuildCause,
        duration: Duration,
    },
    /// An atom value was read; the reading component auto-subscribed.
    AtomRead {
        atom: AtomId,
        component: ComponentId,
    },
    /// An atom value was written.
    AtomWrite {
        atom: AtomId,
        old: TraceValue,
        new: TraceValue,
        by: ComponentId,
        location: Location,
    },
    /// Layout measurement pass started for a component.
    LayoutStart {
        component: ComponentId,
        constraints: TraceConstraints,
    },
    /// Layout measurement pass completed for a component.
    LayoutEnd {
        component: ComponentId,
        size: Size,
        duration: Duration,
    },
    /// A new frame render began.
    FrameStart {
        frame: u64,
        timestamp: Instant,
    },
    /// A frame render completed.
    FrameEnd {
        frame: u64,
        duration: Duration,
        /// True if this frame exceeded the 16.67ms (60fps) or 8.33ms (120fps) budget.
        dropped: bool,
    },
    /// A dirty screen region was repainted.
    PaintRegion {
        rect: Rect,
    },
    /// The active route changed.
    RouteChange {
        from: Option<Route>,
        to: Route,
        transition: Transition,
    },
    /// A network request was initiated.
    RequestStart {
        id: RequestId,
        url: String,
        method: Method,
        component: ComponentId,
    },
    /// A network request completed.
    RequestEnd {
        id: RequestId,
        status: u16,
        duration: Duration,
        cached: bool,
        size: usize,
    },
    /// An FFI call returned successfully.
    FfiCall {
        fn_name: &'static str,
        duration: Duration,
    },
    /// An FFI call returned an error.
    FfiError {
        fn_name: &'static str,
        error: String,
    },
    /// A gesture was received and dispatched to a handler.
    GestureReceived {
        kind: GestureKind,
        handler: ComponentId,
    },
    /// A shader pipeline was registered (D109). Emitted at `register_shader`
    /// time — before compilation, which happens when the platform drains the
    /// queue into the compositor (eager, never lazy-on-first-paint).
    ShaderRegister {
        pipeline: u64,
        wgsl_len: usize,
    },
}
