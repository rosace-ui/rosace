//! Shared test-only utilities.
#![cfg(test)]

use std::sync::Mutex;

/// `std::env::set_current_dir` is process-wide, not per-thread — tests that
/// need to chdir (to exercise cwd-relative file reads like `rsc.toml`/
/// `Cargo.toml`) must serialize against each other or they can race under
/// `cargo test`'s default parallel execution, since two tests could both
/// have chdir'd into different temp dirs at once. Acquire this lock for the
/// duration of any such test.
pub static CWD_LOCK: Mutex<()> = Mutex::new(());
