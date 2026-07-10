use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

use crate::bus::TraceSubscriber;
use crate::event::RosaceTrace;
use crate::subscribers::console::ConsoleSubscriber;

/// Writes formatted trace events to a file, one event per line.
///
/// Intended for crash dumps and post-mortem analysis. Uses a buffered writer
/// for efficiency; the buffer is flushed after each event to ensure events are
/// not lost if the process crashes.
pub struct FileSubscriber {
    writer: Mutex<BufWriter<File>>,
}

impl FileSubscriber {
    /// Opens (or creates) the file at `path` and writes events to it.
    ///
    /// Appends to existing content so that multiple runs accumulate in one file.
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
        })
    }
}

impl TraceSubscriber for FileSubscriber {
    fn on_trace(&self, event: &RosaceTrace) {
        let line = ConsoleSubscriber::format(event);
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(w, "{}", line);
            let _ = w.flush();
        }
    }
}
