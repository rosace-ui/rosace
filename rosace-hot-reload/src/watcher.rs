use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};
use std::fs;
use crate::event::ChangeEvent;
use crate::debounce::Debouncer;

/// Polling interval for the file watcher background thread.
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Debounce window — changes within this window are collapsed.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(100);

/// A lightweight polling file watcher.
///
/// Watches registered directories recursively for `.rs` file changes.
/// Runs a background thread; results are delivered via a channel.
pub struct FileWatcher {
    #[allow(dead_code)]
    sender: Sender<ChangeEvent>,
    watched: Arc<Mutex<Vec<PathBuf>>>,
    stop: Arc<Mutex<bool>>,
}

impl FileWatcher {
    /// Create a new watcher. Returns the watcher handle and a receiver for events.
    pub fn new() -> (Self, Receiver<ChangeEvent>) {
        let (tx, rx) = mpsc::channel();
        let watched: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(Mutex::new(false));

        let watcher = Self {
            sender: tx.clone(),
            watched: Arc::clone(&watched),
            stop: Arc::clone(&stop),
        };

        let watched_clone = Arc::clone(&watched);
        let stop_clone = Arc::clone(&stop);

        thread::spawn(move || {
            let mut known: HashMap<PathBuf, SystemTime> = HashMap::new();
            let mut debouncer = Debouncer::new(DEBOUNCE_WINDOW);

            loop {
                // Check stop signal
                if *stop_clone.lock().unwrap() { break; }

                // Collect all watched paths
                let paths = watched_clone.lock().unwrap().clone();

                // Scan all .rs files under each watched path
                for root in &paths {
                    scan_dir(root, &mut known, &tx, &mut debouncer);
                }

                thread::sleep(POLL_INTERVAL);
            }
        });

        (watcher, rx)
    }

    /// Add a directory to watch recursively.
    pub fn watch(&self, path: impl Into<PathBuf>) {
        self.watched.lock().unwrap().push(path.into());
    }

    /// Stop the background thread.
    pub fn stop(&self) {
        *self.stop.lock().unwrap() = true;
    }

    /// How many directories are being watched.
    pub fn watch_count(&self) -> usize {
        self.watched.lock().unwrap().len()
    }
}

fn scan_dir(
    dir: &Path,
    known: &mut HashMap<PathBuf, SystemTime>,
    tx: &Sender<ChangeEvent>,
    debouncer: &mut Debouncer,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip target/ and hidden dirs
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name == "target" { continue; }
            scan_dir(&path, known, tx, debouncer);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            if let Ok(meta) = fs::metadata(&path) {
                if let Ok(modified) = meta.modified() {
                    let prev = known.get(&path).copied();
                    known.insert(path.clone(), modified);
                    if let Some(p) = prev {
                        if modified != p && debouncer.should_emit() {
                            let _ = tx.send(ChangeEvent { path, at: modified });
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::time::Duration;
    use std::thread;

    #[test]
    fn watcher_new_returns_receiver() {
        let (_watcher, rx) = FileWatcher::new();
        // receiver should exist and have no events yet
        assert!(rx.try_recv().is_err(), "no events on fresh watcher");
    }

    #[test]
    fn watcher_watch_increments_count() {
        let (watcher, _rx) = FileWatcher::new();
        assert_eq!(watcher.watch_count(), 0);
        watcher.watch("/tmp");
        assert_eq!(watcher.watch_count(), 1);
    }

    #[test]
    fn watcher_watch_multiple_paths() {
        let (watcher, _rx) = FileWatcher::new();
        watcher.watch("/tmp");
        watcher.watch("/var");
        watcher.watch("/usr");
        assert_eq!(watcher.watch_count(), 3);
    }

    #[test]
    fn watcher_stop_does_not_panic() {
        let (watcher, _rx) = FileWatcher::new();
        watcher.stop();
        // calling stop twice should also not panic
        watcher.stop();
    }

    #[test]
    fn watcher_detects_file_change() {
        // Create temp dir with a .rs file
        let tmp = std::env::temp_dir().join(format!("rosace_hot_reload_test_{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()));
        fs::create_dir_all(&tmp).unwrap();

        let rs_file = tmp.join("test_widget.rs");
        {
            let mut f = fs::File::create(&rs_file).unwrap();
            writeln!(f, "// initial").unwrap();
        }

        let (watcher, rx) = FileWatcher::new();
        watcher.watch(tmp.clone());

        // Let the background thread do an initial scan to establish baselines
        thread::sleep(Duration::from_millis(350));

        // Modify the file — use set_modified via a new write to bump mtime
        // Sleep 10ms first to ensure mtime differs from initial
        thread::sleep(Duration::from_millis(10));
        {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&rs_file)
                .unwrap();
            writeln!(f, "// modified").unwrap();
        }

        // Wait for the watcher to pick up the change
        thread::sleep(Duration::from_millis(350));

        watcher.stop();

        // Drain the receiver
        let events: Vec<ChangeEvent> = {
            let mut v = Vec::new();
            while let Ok(e) = rx.try_recv() {
                v.push(e);
            }
            v
        };

        // Clean up
        let _ = fs::remove_dir_all(&tmp);

        assert!(!events.is_empty(), "expected at least one ChangeEvent for modified .rs file");
        assert!(events.iter().any(|e| e.path == rs_file));
    }
}
