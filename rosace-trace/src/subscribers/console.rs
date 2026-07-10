use std::sync::atomic::{AtomicUsize, Ordering};

use crate::bus::TraceSubscriber;
use crate::event::{Method, RosaceTrace};

/// Controls which event categories the `ConsoleSubscriber` prints.
///
/// Mirrors the `--trace=<category>` CLI flags from `rsc dev`.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleFilter {
    /// Print all events.
    All,
    /// Print only atom read/write events.
    State,
    /// Print only network request events.
    Network,
    /// Print only frame and layout timing events.
    Performance,
    /// Print only events for a specific component name.
    Component(String),
}

/// Writes formatted trace events to stderr.
///
/// Output format mirrors the Phase 1 dev-tools terminal layout:
/// ```text
/// [MOUNT]   HomeScreen        src/screens/home.rs:12
/// [ATOM]    FEED.set()        64 items
/// [REBUILD] FeedList          cause: FEED  0.8ms
/// [FRAME]   #847              2.1ms ✓
/// [REQUEST] GET /api/feed     200  145ms  cache:miss
/// ```
pub struct ConsoleSubscriber {
    filter: ConsoleFilter,
    event_count: AtomicUsize,
}

impl ConsoleSubscriber {
    /// Creates a new console subscriber printing all events.
    pub fn new() -> Self {
        Self {
            filter: ConsoleFilter::All,
            event_count: AtomicUsize::new(0),
        }
    }

    /// Creates a new console subscriber with the given filter.
    pub fn with_filter(filter: ConsoleFilter) -> Self {
        Self {
            filter,
            event_count: AtomicUsize::new(0),
        }
    }

    /// Returns the total number of events received (regardless of filter).
    pub fn event_count(&self) -> usize {
        self.event_count.load(Ordering::Relaxed)
    }

    fn should_print(&self, event: &RosaceTrace) -> bool {
        match &self.filter {
            ConsoleFilter::All => true,
            ConsoleFilter::State => {
                matches!(event, RosaceTrace::AtomRead { .. } | RosaceTrace::AtomWrite { .. })
            }
            ConsoleFilter::Network => matches!(
                event,
                RosaceTrace::RequestStart { .. } | RosaceTrace::RequestEnd { .. }
            ),
            ConsoleFilter::Performance => matches!(
                event,
                RosaceTrace::FrameStart { .. }
                    | RosaceTrace::FrameEnd { .. }
                    | RosaceTrace::LayoutStart { .. }
                    | RosaceTrace::LayoutEnd { .. }
            ),
            ConsoleFilter::Component(name) => match event {
                RosaceTrace::ComponentMount { name: n, .. } => *n == name.as_str(),
                RosaceTrace::ComponentUnmount { name: n, .. } => *n == name.as_str(),
                // Rebuilds carry ComponentId, not name — include all for now.
                RosaceTrace::ComponentRebuild { .. } => true,
                _ => false,
            },
        }
    }

    pub fn format(event: &RosaceTrace) -> String {
        match event {
            RosaceTrace::ComponentMount { name, location, .. } => {
                format!(
                    "[MOUNT]   {:<20} {}:{}",
                    name, location.file, location.line
                )
            }
            RosaceTrace::ComponentUnmount { name, .. } => {
                format!("[UNMOUNT] {}", name)
            }
            RosaceTrace::ComponentRebuild { cause, duration, .. } => {
                format!(
                    "[REBUILD] cause: {:?}  {:.1}ms",
                    cause,
                    duration.as_secs_f64() * 1000.0
                )
            }
            RosaceTrace::AtomRead { atom, component } => {
                format!("[ATOM]    read  atom:{} by component:{}", atom.0, component.0)
            }
            RosaceTrace::AtomWrite { atom, location, .. } => {
                format!(
                    "[ATOM]    write atom:{}  {}:{}",
                    atom.0, location.file, location.line
                )
            }
            RosaceTrace::LayoutStart { component, constraints } => {
                format!(
                    "[LAYOUT]  start component:{}  w:[{:.0}..{:?}] h:[{:.0}..{:?}]",
                    component.0,
                    constraints.min_width,
                    constraints.max_width,
                    constraints.min_height,
                    constraints.max_height
                )
            }
            RosaceTrace::LayoutEnd { component, size, duration } => {
                format!(
                    "[LAYOUT]  end   component:{}  {:.0}x{:.0}  {:.2}ms",
                    component.0,
                    size.width,
                    size.height,
                    duration.as_secs_f64() * 1000.0
                )
            }
            RosaceTrace::FrameStart { frame, .. } => {
                format!("[FRAME]   #{:<6} start", frame)
            }
            RosaceTrace::FrameEnd { frame, duration, dropped } => {
                let budget = if *dropped { "✗ DROPPED" } else { "✓" };
                format!(
                    "[FRAME]   #{:<6} {:.2}ms {}",
                    frame,
                    duration.as_secs_f64() * 1000.0,
                    budget
                )
            }
            RosaceTrace::PaintRegion { rect } => {
                format!(
                    "[PAINT]   ({:.0},{:.0}) {:.0}x{:.0}",
                    rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
                )
            }
            RosaceTrace::RouteChange { from, to, transition } => {
                let from_str = from.as_ref().map(|r| r.0.as_str()).unwrap_or("(none)");
                format!(
                    "[ROUTE]   {} → {}  {}",
                    from_str, to.0, transition.0
                )
            }
            RosaceTrace::RequestStart { url, method, .. } => {
                let m = match method {
                    Method::Get => "GET",
                    Method::Post => "POST",
                    Method::Put => "PUT",
                    Method::Delete => "DELETE",
                    Method::Patch => "PATCH",
                    Method::Other(s) => s.as_str(),
                };
                format!("[REQUEST] {} {}", m, url)
            }
            RosaceTrace::RequestEnd { id, status, duration, cached, size } => {
                let cache = if *cached { "cache:hit" } else { "cache:miss" };
                format!(
                    "[REQUEST] id:{}  {}  {:.0}ms  {}  {}b",
                    id.0,
                    status,
                    duration.as_secs_f64() * 1000.0,
                    cache,
                    size
                )
            }
            RosaceTrace::FfiCall { fn_name, duration } => {
                format!(
                    "[FFI]     {}  {:.2}ms",
                    fn_name,
                    duration.as_secs_f64() * 1000.0
                )
            }
            RosaceTrace::FfiError { fn_name, error } => {
                format!("[FFI]     {} ERROR: {}", fn_name, error)
            }
            RosaceTrace::GestureReceived { kind, handler } => {
                format!("[GESTURE] {:?}  handler:{}", kind, handler.0)
            }
            RosaceTrace::ShaderRegister { pipeline, wgsl_len } => {
                format!("[SHADER]  register pipeline:{}  wgsl:{}b", pipeline, wgsl_len)
            }
        }
    }
}

impl Default for ConsoleSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceSubscriber for ConsoleSubscriber {
    fn on_trace(&self, event: &RosaceTrace) {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        if self.should_print(event) {
            eprintln!("{}", Self::format(event));
        }
    }
}
