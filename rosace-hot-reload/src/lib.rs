//! Hot-reload support for ROSACE.
//!
//! Watches source directories and triggers rebuilds on `.rs` file changes.
//!
//! # Example
//! ```rust,no_run
//! use rosace_hot_reload::{FileWatcher, RebuildRunner, RebuildTarget};
//!
//! let (watcher, rx) = FileWatcher::new();
//! watcher.watch("src");
//!
//! let runner = RebuildRunner::new().target(RebuildTarget::Desktop);
//! runner.run_loop(rx); // blocks
//! ```

pub mod debounce;
pub mod event;
pub mod rebuild;
pub mod watcher;

pub use debounce::Debouncer;
pub use event::ChangeEvent;
pub use rebuild::{RebuildRunner, RebuildTarget};
pub use watcher::FileWatcher;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_watcher_and_runner_types_are_reexported() {
        // Verify that all public types are accessible from the crate root.
        let (watcher, _rx) = FileWatcher::new();
        let _ = watcher.watch_count();

        let _runner = RebuildRunner::new()
            .target(RebuildTarget::Desktop)
            .package("rosace-core");

        let _event = ChangeEvent {
            path: std::path::PathBuf::from("src/lib.rs"),
            at: std::time::SystemTime::now(),
        };

        let mut _debouncer = Debouncer::new(std::time::Duration::from_millis(100));
        assert!(_debouncer.should_emit());
    }
}
