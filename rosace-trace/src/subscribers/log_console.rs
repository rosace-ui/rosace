//! Colored terminal sink for log records — streams `info!`/`warn!`/… to stdout
//! with a level-colored, timestamped line. This is the default visible logging
//! output for `rsc dev`/`rsc run`; other sinks (DevTools panel, a browser-tools
//! socket) subscribe to the same `Log` events off the bus.

use std::io::{IsTerminal, Write};

use crate::bus::TraceSubscriber;
use crate::event::RosaceTrace;

/// Prints `RosaceTrace::Log` events to stdout, one colored line each. Ignores
/// every other (structured) event — those have their own sinks. Colors are
/// auto-disabled when stdout isn't a TTY (piped/redirected), so log files stay
/// clean.
pub struct LogConsoleSubscriber {
    color: bool,
    start: web_time::Instant,
}

impl LogConsoleSubscriber {
    /// Auto-detects color (on only when stdout is a terminal).
    pub fn new() -> Self {
        Self {
            color: std::io::stdout().is_terminal(),
            start: web_time::Instant::now(),
        }
    }

    /// Force color on/off (e.g. off for a log file, on for a color-capable pipe).
    pub fn with_color(mut self, on: bool) -> Self {
        self.color = on;
        self
    }
}

impl Default for LogConsoleSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl TraceSubscriber for LogConsoleSubscriber {
    fn on_trace(&self, event: &RosaceTrace) {
        let RosaceTrace::Log { level, target, message, timestamp } = event else {
            return; // not a log record — a structured trace; other sinks handle it
        };
        // Seconds since the sink started — a cheap, monotonic, timezone-free
        // stamp that reads well in a dev terminal.
        let secs = timestamp.saturating_duration_since(self.start).as_secs_f64();
        let line = if self.color {
            format!(
                "\x1b[2m{secs:8.3}\x1b[0m {}{}\x1b[0m \x1b[2m{target}\x1b[0m {message}\n",
                level.ansi(),
                level.label(),
            )
        } else {
            format!("{secs:8.3} {} {target} {message}\n", level.label())
        };
        // One locked write so lines from multiple threads don't interleave.
        let out = std::io::stdout();
        let _ = out.lock().write_all(line.as_bytes());
    }
}
