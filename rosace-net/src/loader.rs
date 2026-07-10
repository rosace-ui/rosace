use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use crate::load_state::LoadState;
use crate::http::http_get;

/// Result of an image load operation.
pub type ImageBytes = Vec<u8>;

/// Message from the loader thread.
#[derive(Debug)]
pub enum LoaderMessage {
    Loaded { url: String, bytes: ImageBytes },
    Failed { url: String, error: String },
}

/// Async image loader backed by a thread pool (single background thread per load).
pub struct ImageLoader {
    tx: Sender<LoaderMessage>,
    rx: Receiver<LoaderMessage>,
    state: std::collections::HashMap<String, LoadState<ImageBytes>>,
}

impl ImageLoader {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, state: std::collections::HashMap::new() }
    }

    /// Begin loading a URL (no-op if already loading or loaded).
    pub fn load(&mut self, url: impl Into<String>) {
        let url = url.into();
        match self.state.get(&url) {
            Some(LoadState::Idle) | None => {}
            _ => return, // already loading or loaded
        }
        self.state.insert(url.clone(), LoadState::Loading);
        let tx = self.tx.clone();
        let url_clone = url.clone();
        thread::spawn(move || {
            match http_get(&url_clone) {
                Ok(bytes) => { let _ = tx.send(LoaderMessage::Loaded { url: url_clone, bytes }); }
                Err(e)    => { let _ = tx.send(LoaderMessage::Failed { url: url_clone, error: e }); }
            }
        });
    }

    /// Poll for completed loads. Call each frame.
    pub fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                LoaderMessage::Loaded { url, bytes } => {
                    self.state.insert(url, LoadState::Loaded(bytes));
                }
                LoaderMessage::Failed { url, error } => {
                    self.state.insert(url, LoadState::Failed(error));
                }
            }
        }
    }

    /// Get the current load state for a URL.
    pub fn state(&self, url: &str) -> &LoadState<ImageBytes> {
        self.state.get(url).unwrap_or(&LoadState::Idle)
    }

    /// Number of URLs tracked.
    pub fn tracked(&self) -> usize { self.state.len() }

    /// Number of successfully loaded URLs.
    pub fn loaded_count(&self) -> usize {
        self.state.values().filter(|s| s.is_loaded()).count()
    }

    /// Inject a load result directly (used in tests to avoid network calls).
    #[cfg(test)]
    pub(crate) fn inject_loaded(&mut self, url: impl Into<String>, bytes: ImageBytes) {
        self.state.insert(url.into(), LoadState::Loaded(bytes));
    }

    /// Inject a failed result directly (used in tests).
    #[cfg(test)]
    pub(crate) fn inject_failed(&mut self, url: impl Into<String>, error: impl Into<String>) {
        self.state.insert(url.into(), LoadState::Failed(error.into()));
    }
}

impl Default for ImageLoader { fn default() -> Self { Self::new() } }

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_new_empty() {
        let loader = ImageLoader::new();
        assert_eq!(loader.tracked(), 0);
        assert_eq!(loader.loaded_count(), 0);
    }

    #[test]
    fn loader_load_sets_loading_state() {
        let mut loader = ImageLoader::new();
        // We call load but the thread will fail (no real server) —
        // immediately after calling load(), state should be Loading.
        loader.load("http://localhost:19999/nonexistent.png");
        assert!(loader.state("http://localhost:19999/nonexistent.png").is_loading());
    }

    #[test]
    fn loader_load_idempotent_when_loading() {
        let mut loader = ImageLoader::new();
        loader.load("http://localhost:19999/img.png");
        // Call load again — should remain Loading, not reset.
        loader.load("http://localhost:19999/img.png");
        assert!(loader.state("http://localhost:19999/img.png").is_loading());
        assert_eq!(loader.tracked(), 1);
    }

    #[test]
    fn loader_load_idempotent_when_loaded() {
        let mut loader = ImageLoader::new();
        loader.inject_loaded("http://example.com/img.png", vec![1, 2, 3]);
        // load() should be a no-op since it's already Loaded.
        loader.load("http://example.com/img.png");
        // Still loaded, still 1 entry.
        assert!(loader.state("http://example.com/img.png").is_loaded());
        assert_eq!(loader.tracked(), 1);
    }

    #[test]
    fn loader_tracked_count() {
        let mut loader = ImageLoader::new();
        loader.load("http://localhost:19999/a.png");
        loader.load("http://localhost:19999/b.png");
        assert_eq!(loader.tracked(), 2);
    }

    #[test]
    fn loader_poll_no_panic_when_empty() {
        let mut loader = ImageLoader::new();
        // Should not panic even with nothing in the channel.
        loader.poll();
    }

    #[test]
    fn loader_loaded_count_zero_initially() {
        let loader = ImageLoader::new();
        assert_eq!(loader.loaded_count(), 0);
    }
}
