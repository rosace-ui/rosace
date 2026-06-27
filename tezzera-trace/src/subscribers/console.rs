use std::sync::atomic::{AtomicUsize, Ordering};

use crate::bus::TraceSubscriber;
use crate::event::{Method, TezzeraTrace};

/// Controls which event categories the `ConsoleSubscriber` prints.
///
/// Mirrors the `--trace=<category>` CLI flags from `tzr dev`.
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

    fn should_print(&self, event: &TezzeraTrace) -> bool {
        match &self.filter {
            ConsoleFilter::All => true,
            ConsoleFilter::State => {
                matches!(event, TezzeraTrace::AtomRead { .. } | TezzeraTrace::AtomWrite { .. })
            }
            ConsoleFilter::Network => matches!(
                event,
                TezzeraTrace::RequestStart { .. } | TezzeraTrace::RequestEnd { .. }
            ),
            ConsoleFilter::Performance => matches!(
                event,
                TezzeraTrace::FrameStart { .. }
                    | TezzeraTrace::FrameEnd { .. }
                    | TezzeraTrace::LayoutStart { .. }
                    | TezzeraTrace::LayoutEnd { .. }
            ),
            ConsoleFilter::Component(name) => match event {
                TezzeraTrace::ComponentMount { name: n, .. } => *n == name.as_str(),
                TezzeraTrace::ComponentUnmount { name: n, .. } => *n == name.as_str(),
                // Rebuilds carry ComponentId, not name — include all for now.
                TezzeraTrace::ComponentRebuild { .. } => true,
                _ => false,
            },
        }
    }

    pub fn format(event: &TezzeraTrace) -> String {
        match event {
            TezzeraTrace::ComponentMount { name, location, .. } => {
                format!(
                    "[MOUNT]   {:<20} {}:{}",
                    name, location.file, location.line
                )
            }
            TezzeraTrace::ComponentUnmount { name, .. } => {
                format!("[UNMOUNT] {}", name)
            }
            TezzeraTrace::ComponentRebuild { cause, duration, .. } => {
                format!(
                    "[REBUILD] cause: {:?}  {:.1}ms",
                    cause,
                    duration.as_secs_f64() * 1000.0
                )
            }
            TezzeraTrace::AtomRead { atom, component } => {
                format!("[ATOM]    read  atom:{} by component:{}", atom.0, component.0)
            }
            TezzeraTrace::AtomWrite { atom, location, .. } => {
                format!(
                    "[ATOM]    write atom:{}  {}:{}",
                    atom.0, location.file, location.line
                )
            }
            TezzeraTrace::LayoutStart { component, constraints } => {
                format!(
                    "[LAYOUT]  start component:{}  w:[{:.0}..{:?}] h:[{:.0}..{:?}]",
                    component.0,
                    constraints.min_width,
                    constraints.max_width,
                    constraints.min_height,
                    constraints.max_height
                )
            }
            TezzeraTrace::LayoutEnd { component, size, duration } => {
                format!(
                    "[LAYOUT]  end   component:{}  {:.0}x{:.0}  {:.2}ms",
                    component.0,
                    size.width,
                    size.height,
                    duration.as_secs_f64() * 1000.0
                )
            }
            TezzeraTrace::FrameStart { frame, .. } => {
                format!("[FRAME]   #{:<6} start", frame)
            }
            TezzeraTrace::FrameEnd { frame, duration, dropped } => {
                let budget = if *dropped { "✗ DROPPED" } else { "✓" };
                format!(
                    "[FRAME]   #{:<6} {:.2}ms {}",
                    frame,
                    duration.as_secs_f64() * 1000.0,
                    budget
                )
            }
            TezzeraTrace::PaintRegion { rect } => {
                format!(
                    "[PAINT]   ({:.0},{:.0}) {:.0}x{:.0}",
                    rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
                )
            }
            TezzeraTrace::RouteChange { from, to, transition } => {
                let from_str = from.as_ref().map(|r| r.0.as_str()).unwrap_or("(none)");
                format!(
                    "[ROUTE]   {} → {}  {}",
                    from_str, to.0, transition.0
                )
            }
            TezzeraTrace::RequestStart { url, method, .. } => {
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
            TezzeraTrace::RequestEnd { id, status, duration, cached, size } => {
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
            TezzeraTrace::FfiCall { fn_name, duration } => {
                format!(
                    "[FFI]     {}  {:.2}ms",
                    fn_name,
                    duration.as_secs_f64() * 1000.0
                )
            }
            TezzeraTrace::FfiError { fn_name, error } => {
                format!("[FFI]     {} ERROR: {}", fn_name, error)
            }
            TezzeraTrace::GestureReceived { kind, handler } => {
                format!("[GESTURE] {:?}  handler:{}", kind, handler.0)
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
    fn on_trace(&self, event: &TezzeraTrace) {
        self.event_count.fetch_add(1, Ordering::Relaxed);
        if self.should_print(event) {
            eprintln!("{}", Self::format(event));
        }
    }
}
