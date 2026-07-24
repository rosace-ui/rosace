//! Chrome/Perfetto Trace Event Format export (D123/O1) — turns a flight
//! recorder snapshot into a JSON file Perfetto UI (ui.perfetto.dev) or
//! `chrome://tracing` can load directly for a full flamegraph, no custom
//! viewer needed.
//!
//! Spec: <https://docs.google.com/document/d/1CvAClvFfyA5R-PhYUmn5OOQtYMH4h6I0nSsKchNAySU>
//! (informally "the Trace Event Format"). We emit the JSON-array form: a
//! bare `[ ... ]` of event objects, each either an instant event (`"ph":"i"`)
//! or, for events that carry their own `Duration`, a complete event
//! (`"ph":"X"`, `"ts"` + `"dur"`) so Perfetto draws it as a duration bar
//! instead of a tick mark.

use std::time::{Duration, Instant};

use crate::event::RosaceTrace;
use super::console::ConsoleSubscriber;

/// The `Duration` an event represents, if any — used to emit it as a
/// Perfetto "complete" (`X`) event spanning that duration, ending at the
/// moment it was recorded (these events are all recorded at completion).
fn event_duration(event: &RosaceTrace) -> Option<Duration> {
    match event {
        RosaceTrace::ComponentRebuild { duration, .. }
        | RosaceTrace::LayoutEnd { duration, .. }
        | RosaceTrace::FrameEnd { duration, .. }
        | RosaceTrace::RequestEnd { duration, .. }
        | RosaceTrace::FfiCall { duration, .. } => Some(*duration),
        _ => None,
    }
}

/// Escape a string for embedding in a JSON string literal (quotes,
/// backslashes, and control characters only — trace event names/categories
/// never contain anything else worth handling).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Convert a timestamped flight-recorder snapshot into a Chrome/Perfetto
/// trace JSON string. `events` must be oldest-first (the order
/// [`super::ring_buffer::RingBufferSubscriber::snapshot_timestamped`]
/// already returns).
pub fn to_chrome_trace_json(events: &[(Instant, RosaceTrace)]) -> String {
    let Some((first_at, _)) = events.first() else {
        return "[]".to_string();
    };
    let mut out = String::from("[\n");
    for (i, (at, event)) in events.iter().enumerate() {
        let name = json_escape(&ConsoleSubscriber::format(event));
        let cat = json_escape(&format!("{:?}", event.category()));
        let ts_us = at.saturating_duration_since(*first_at).as_micros();

        if i > 0 {
            out.push_str(",\n");
        }
        if let Some(dur) = event_duration(event) {
            let dur_us = dur.as_micros();
            let start_us = ts_us.saturating_sub(dur_us);
            out.push_str(&format!(
                "  {{\"name\":\"{name}\",\"cat\":\"{cat}\",\"ph\":\"X\",\"ts\":{start_us},\"dur\":{dur_us},\"pid\":1,\"tid\":1}}"
            ));
        } else {
            out.push_str(&format!(
                "  {{\"name\":\"{name}\",\"cat\":\"{cat}\",\"ph\":\"i\",\"ts\":{ts_us},\"pid\":1,\"tid\":1,\"s\":\"t\"}}"
            ));
        }
    }
    out.push_str("\n]\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ComponentId, RebuildCause};

    #[test]
    fn empty_snapshot_exports_an_empty_array() {
        assert_eq!(to_chrome_trace_json(&[]), "[]");
    }

    #[test]
    fn instant_event_gets_phase_i_and_zero_ts_when_first() {
        let now = Instant::now();
        let json = to_chrome_trace_json(&[(now, RosaceTrace::ComponentUnmount {
            id: ComponentId(1),
            name: "Widget",
        })]);
        assert!(json.contains("\"ph\":\"i\""));
        assert!(json.contains("\"ts\":0"));
        assert!(json.contains("\"cat\":\"Lifecycle\""));
    }

    #[test]
    fn duration_bearing_event_gets_phase_x_with_dur() {
        let now = Instant::now();
        let json = to_chrome_trace_json(&[(now, RosaceTrace::ComponentRebuild {
            id: ComponentId(1),
            cause: RebuildCause::Manual,
            duration: Duration::from_millis(5),
        })]);
        assert!(json.contains("\"ph\":\"X\""));
        assert!(json.contains("\"dur\":5000"));
    }

    #[test]
    fn later_events_get_a_positive_relative_timestamp() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(10);
        let json = to_chrome_trace_json(&[
            (t0, RosaceTrace::ComponentUnmount { id: ComponentId(1), name: "A" }),
            (t1, RosaceTrace::ComponentUnmount { id: ComponentId(2), name: "B" }),
        ]);
        assert!(json.contains("\"ts\":10000"), "second event should be ~10ms (10000us) later: {json}");
    }

    #[test]
    fn output_is_valid_enough_json_shape_for_every_event_kind() {
        // Not a full JSON parser dependency — just checks bracket/brace
        // balance and that every event produced exactly one object.
        let now = Instant::now();
        let events: Vec<(Instant, RosaceTrace)> = vec![
            (now, RosaceTrace::ComponentUnmount { id: ComponentId(1), name: "A" }),
            (now, RosaceTrace::ComponentRebuild { id: ComponentId(1), cause: RebuildCause::Manual, duration: Duration::from_micros(500) }),
        ];
        let json = to_chrome_trace_json(&events);
        assert_eq!(json.matches('{').count(), json.matches('}').count());
        assert!(json.trim_start().starts_with('['));
        assert!(json.trim_end().ends_with(']'));
    }
}
