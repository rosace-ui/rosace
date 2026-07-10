use std::sync::mpsc::Receiver;

use rosace_trace::event::{RosaceTrace, TraceValue};

/// Formats RosaceTrace events as a human-readable ASCII log.
pub struct TraceViewer {
    /// Maximum number of events to keep in the rolling log buffer.
    pub max_events: usize,
    events: Vec<String>,
}

impl TraceViewer {
    pub fn new() -> Self {
        Self { max_events: 200, events: Vec::new() }
    }

    pub fn max_events(mut self, n: usize) -> Self {
        self.max_events = n;
        self
    }

    /// Process all available events from the trace bus (non-blocking drain).
    pub fn drain(&mut self, rx: &Receiver<RosaceTrace>) {
        while let Ok(ev) = rx.try_recv() {
            let line = format_event(&ev);
            if self.events.len() >= self.max_events {
                self.events.remove(0);
            }
            self.events.push(line);
        }
    }

    /// Render the event log as an ASCII panel.
    pub fn render(&self) -> String {
        if self.events.is_empty() {
            return "[TraceViewer] No events yet.\n".to_string();
        }
        let mut out = String::from("┌─ ROSACE TRACE ─────────────────────────────────\n");
        for (i, line) in self.events.iter().rev().take(20).enumerate() {
            out.push_str(&format!("│ {:>3}  {}\n", self.events.len() - i, line));
        }
        out.push_str("└─────────────────────────────────────────────────\n");
        out
    }

    /// Total events recorded since last clear.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Clear the event buffer.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Last N events as strings.
    pub fn last(&self, n: usize) -> Vec<&str> {
        self.events.iter().rev().take(n).map(|s| s.as_str()).collect()
    }
}

impl Default for TraceViewer {
    fn default() -> Self {
        Self::new()
    }
}

fn format_trace_value(v: &TraceValue) -> String {
    match v {
        TraceValue::Debug(s) => s.clone(),
        TraceValue::Opaque => "<opaque>".to_string(),
    }
}

fn format_event(ev: &RosaceTrace) -> String {
    match ev {
        RosaceTrace::ComponentMount { id, name, location } => {
            format!("ComponentMount  id={} name={} @ {}:{}", id.0, name, location.file, location.line)
        }
        RosaceTrace::ComponentUnmount { id, name } => {
            format!("ComponentUnmount  id={} name={}", id.0, name)
        }
        RosaceTrace::ComponentRebuild { id, cause, duration } => {
            format!("ComponentRebuild  id={} cause={:?} dur={:.2}ms", id.0, cause, duration.as_secs_f32() * 1000.0)
        }
        RosaceTrace::AtomRead { atom, component } => {
            format!("AtomRead  atom_id={} by_component={}", atom.0, component.0)
        }
        RosaceTrace::AtomWrite { atom, old, new, by, location } => {
            format!(
                "AtomWrite  atom_id={} old={} new={} by={} @ {}:{}",
                atom.0,
                format_trace_value(old),
                format_trace_value(new),
                by.0,
                location.file,
                location.line
            )
        }
        RosaceTrace::LayoutStart { component, constraints } => {
            format!(
                "LayoutStart  component={} min=({},{}) max=({},{})",
                component.0,
                constraints.min_width,
                constraints.min_height,
                constraints.max_width.map_or("∞".to_string(), |v| format!("{}", v)),
                constraints.max_height.map_or("∞".to_string(), |v| format!("{}", v)),
            )
        }
        RosaceTrace::LayoutEnd { component, size, duration } => {
            format!(
                "LayoutEnd  component={} size={}x{} dur={:.2}ms",
                component.0, size.width, size.height, duration.as_secs_f32() * 1000.0
            )
        }
        RosaceTrace::FrameStart { frame, .. } => {
            format!("FrameStart  frame={}", frame)
        }
        RosaceTrace::FrameEnd { frame, duration, dropped } => {
            format!(
                "FrameEnd  frame={} dur={:.2}ms{}",
                frame,
                duration.as_secs_f32() * 1000.0,
                if *dropped { " [DROPPED]" } else { "" }
            )
        }
        RosaceTrace::PaintRegion { rect } => {
            format!(
                "PaintRegion  x={} y={} w={} h={}",
                rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
            )
        }
        RosaceTrace::RouteChange { from, to, transition } => {
            format!(
                "RouteChange  {} -> {} ({})",
                from.as_ref().map_or("<none>", |r| &r.0),
                to.0,
                transition.0
            )
        }
        RosaceTrace::RequestStart { id, url, method, component } => {
            format!("RequestStart  id={} {:?} {} component={}", id.0, method, url, component.0)
        }
        RosaceTrace::RequestEnd { id, status, duration, cached, size } => {
            format!(
                "RequestEnd  id={} status={} dur={:.2}ms cached={} size={}B",
                id.0, status, duration.as_secs_f32() * 1000.0, cached, size
            )
        }
        RosaceTrace::FfiCall { fn_name, duration } => {
            format!("FfiCall  fn={} dur={:.2}ms", fn_name, duration.as_secs_f32() * 1000.0)
        }
        RosaceTrace::FfiError { fn_name, error } => {
            format!("FfiError  fn={} err={}", fn_name, error)
        }
        RosaceTrace::GestureReceived { kind, handler } => {
            format!("GestureReceived  kind={:?} handler={}", kind, handler.0)
        }
        RosaceTrace::ShaderRegister { pipeline, wgsl_len } => {
            format!("ShaderRegister  pipeline={} wgsl_len={}", pipeline, wgsl_len)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use rosace_trace::event::{ComponentId, Location, RosaceTrace};

    use super::*;

    #[test]
    fn trace_viewer_new_empty() {
        let viewer = TraceViewer::new();
        assert_eq!(viewer.event_count(), 0);
        assert_eq!(viewer.max_events, 200);
    }

    #[test]
    fn trace_viewer_drain_empty_channel() {
        let mut viewer = TraceViewer::new();
        let (_tx, rx) = mpsc::channel::<RosaceTrace>();
        viewer.drain(&rx);
        assert_eq!(viewer.event_count(), 0);
    }

    #[test]
    fn trace_viewer_render_no_events() {
        let viewer = TraceViewer::new();
        let output = viewer.render();
        assert!(output.contains("No events yet"));
    }

    #[test]
    fn trace_viewer_last_returns_recent() {
        let mut viewer = TraceViewer::new();
        let (tx, rx) = mpsc::channel::<RosaceTrace>();
        tx.send(RosaceTrace::ComponentMount {
            id: ComponentId(1),
            name: "Alpha",
            location: Location { file: "a.rs", line: 1 },
        })
        .unwrap();
        tx.send(RosaceTrace::ComponentUnmount {
            id: ComponentId(2),
            name: "Beta",
        })
        .unwrap();
        drop(tx);
        viewer.drain(&rx);
        assert_eq!(viewer.event_count(), 2);
        let last = viewer.last(1);
        assert_eq!(last.len(), 1);
        // last() returns in rev order — most recent first
        assert!(last[0].contains("Beta"));
    }

    #[test]
    fn trace_viewer_respects_max_events() {
        let mut viewer = TraceViewer::new().max_events(3);
        let (tx, rx) = mpsc::channel::<RosaceTrace>();
        for i in 0..5u64 {
            tx.send(RosaceTrace::ComponentUnmount {
                id: rosace_trace::event::ComponentId(i),
                name: "X",
            })
            .unwrap();
        }
        drop(tx);
        viewer.drain(&rx);
        assert_eq!(viewer.event_count(), 3);
    }
}
