//! In-app DevTools trace/network panel (D123/O5 — starting cut).
//!
//! A corner overlay that streams recent **activity** off the flight recorder:
//! network requests, logs, navigation, FFI, and lifecycle — the same
//! `RosaceTrace` events every other sink sees, just rendered on-screen. Toggled
//! with **F11** (F12 is the element inspector). This is the read-only "what is
//! my app doing right now" view; deeper per-category drilldowns build on it.

use rosace_trace::event::{RosaceTrace, TraceCategory};

/// The DevTools panel's tabs. Each filters the flight recorder to one lens.
pub const DEVTOOLS_TABS: [&str; 3] = ["All", "Logs", "Network"];

/// Whether an event belongs under the given tab index (see [`DEVTOOLS_TABS`]).
fn in_tab(cat: TraceCategory, tab: usize) -> bool {
    match tab {
        1 => matches!(cat, TraceCategory::Log),     // Logs
        2 => matches!(cat, TraceCategory::Network), // Network
        _ => is_activity(cat),                      // All
    }
}

/// Which categories the "All" tab shows — the "meaningful activity" stream.
/// High-frequency events (atom reads, per-frame layout/paint) are already
/// excluded by the flight recorder, so this is a readable feed, not a firehose.
fn is_activity(cat: TraceCategory) -> bool {
    matches!(
        cat,
        TraceCategory::Network
            | TraceCategory::Log
            | TraceCategory::Route
            | TraceCategory::Ffi
            | TraceCategory::Lifecycle
    )
}

/// State for the trace/network panel: just whether it's open. The event data
/// lives in the flight recorder; the panel is a stateless view over it.
#[derive(Default)]
pub struct TracePanel {
    pub enabled: bool,
}

impl TracePanel {
    pub fn new() -> Self {
        Self { enabled: false }
    }

    /// Flip open/closed (F11). Returns the new state.
    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }

    /// The lines to render: a header plus the most recent `max` activity events
    /// (newest last), each formatted for one row. `snapshot` is the flight
    /// recorder's buffer (oldest-first).
    pub fn lines(&self, snapshot: &[RosaceTrace], max: usize) -> Vec<String> {
        let mut rows: Vec<String> = snapshot
            .iter()
            .filter(|ev| is_activity(ev.category()))
            .map(row)
            .collect();

        let total = rows.len();
        if rows.len() > max {
            rows = rows.split_off(rows.len() - max);
        }

        let mut out = Vec::with_capacity(rows.len() + 1);
        out.push(format!("DevTools · activity ({total})   F11 to close"));
        if rows.is_empty() {
            out.push("(no network / logs / navigation yet)".to_string());
        } else {
            out.extend(rows);
        }
        out
    }

    /// The most recent `max` rows for one DevTools tab (see [`DEVTOOLS_TABS`]),
    /// newest last. No header — the panel draws the tab bar itself. An empty
    /// result yields a single placeholder row.
    pub fn rows_for(&self, snapshot: &[RosaceTrace], tab: usize, max: usize) -> Vec<String> {
        let mut rows: Vec<String> = snapshot
            .iter()
            .filter(|ev| in_tab(ev.category(), tab))
            .map(row)
            .collect();
        if rows.len() > max {
            rows = rows.split_off(rows.len() - max);
        }
        if rows.is_empty() {
            rows.push(match tab {
                1 => "(no logs yet — try info!(\"…\") in your app)".to_string(),
                2 => "(no network requests yet)".to_string(),
                _ => "(no activity yet)".to_string(),
            });
        }
        rows
    }
}

/// One compact row for the panel — a tagged, human-readable line per event.
fn row(ev: &RosaceTrace) -> String {
    match ev {
        RosaceTrace::RequestStart { method, url, .. } => {
            format!("NET  → {:?} {}", method, truncate(url, 48))
        }
        RosaceTrace::RequestEnd { status, duration, cached, size, .. } => {
            let c = if *cached { " (cached)" } else { "" };
            format!(
                "NET  ← {} {:.0}ms {}b{c}",
                status,
                duration.as_secs_f64() * 1000.0,
                size
            )
        }
        RosaceTrace::Log { level, target, message, .. } => {
            format!("LOG  {} {} {}", level.label().trim(), short_target(target), truncate(message, 52))
        }
        RosaceTrace::RouteChange { to, .. } => format!("NAV  → route {}", to.0),
        RosaceTrace::FfiCall { fn_name, duration } => {
            format!("FFI  {} {:.1}ms", fn_name, duration.as_secs_f64() * 1000.0)
        }
        RosaceTrace::FfiError { fn_name, error } => format!("FFI  ✗ {} {}", fn_name, truncate(error, 40)),
        RosaceTrace::ComponentMount { name, .. } => format!("UI   + {}", name),
        RosaceTrace::ComponentUnmount { name, .. } => format!("UI   - {}", name),
        other => format!("{:?}", other.category()),
    }
}

/// Keep only the last path segment of a module path so rows stay short
/// (`my_app::screens::home` → `home`).
fn short_target(target: &str) -> &str {
    target.rsplit("::").next().unwrap_or(target)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_trace::event::{LogLevel, Method, RequestId, ComponentId};
    use web_time::Instant;

    #[test]
    fn shows_network_and_logs_but_not_noise() {
        let panel = TracePanel::new();
        let snap = vec![
            RosaceTrace::RequestStart {
                id: RequestId(1),
                url: "https://api.example.com/users".into(),
                method: Method::Get,
                component: ComponentId(0),
            },
            RosaceTrace::Log {
                level: LogLevel::Info,
                target: "app::screens::home",
                message: "loaded".into(),
                timestamp: Instant::now(),
            },
        ];
        let lines = panel.lines(&snap, 10);
        assert!(lines[0].contains("activity (2)"));
        assert!(lines.iter().any(|l| l.starts_with("NET  →")));
        assert!(lines.iter().any(|l| l.contains("LOG  INFO home loaded")));
    }

    #[test]
    fn empty_shows_placeholder() {
        let panel = TracePanel::new();
        let lines = panel.lines(&[], 10);
        assert!(lines.iter().any(|l| l.contains("no network")));
    }

    #[test]
    fn caps_at_max_keeping_newest() {
        let panel = TracePanel::new();
        let snap: Vec<RosaceTrace> = (0..20)
            .map(|i| RosaceTrace::Log {
                level: LogLevel::Info,
                target: "t",
                message: format!("m{i}"),
                timestamp: Instant::now(),
            })
            .collect();
        let lines = panel.lines(&snap, 5);
        // header + 5 rows, newest (m19) present, oldest (m0) dropped.
        assert_eq!(lines.len(), 6);
        assert!(lines.iter().any(|l| l.contains("m19")));
        assert!(!lines.iter().any(|l| l.contains("m0 ")));
    }
}
