//! User-facing logging — the ergonomic front-end to the trace bus.
//!
//! `info!`/`warn!`/`error!`/`debug!`/`log!` emit a [`RosaceTrace::Log`] onto the
//! same [`TRACING_BUS`](crate::TRACING_BUS) the framework's own traces use, so
//! **one interceptor carries framework traces AND app logs** to every sink:
//! the colored console, the DevTools panel (flight recorder), a future
//! browser-tools socket, and any third-party subscriber. This is the whole
//! point of building logging INTO `rosace-trace` rather than pulling an
//! external logging crate — one bus, one interceptor model, zero extra deps.
//!
//! Unlike [`trace!`](crate::trace) (framework events, compiled out of release),
//! logs flow in **release too**, gated only by a runtime [max level](set_max_level).

use std::sync::atomic::{AtomicU8, Ordering};

use crate::event::{LogLevel, RosaceTrace};

/// The current max verbosity: records with `level as u8 <= MAX_LEVEL` are
/// emitted. Default = `Info` (2). Raise to `Debug`/`Trace` for more, lower to
/// `Warn`/`Error` for less. `255` here means "uninitialised → use the default".
static MAX_LEVEL: AtomicU8 = AtomicU8::new(u8::MAX);

const DEFAULT_MAX_LEVEL: u8 = LogLevel::Info as u8;

/// Set the global max log level. Records more verbose than this are dropped
/// before any allocation. (Also settable via `ROSACE_LOG=error|warn|info|debug|trace`
/// through [`init_from_env`].)
pub fn set_max_level(level: LogLevel) {
    MAX_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// The current max log level as a raw `u8` (fast path for the macros).
#[inline]
pub fn max_level() -> u8 {
    let v = MAX_LEVEL.load(Ordering::Relaxed);
    if v == u8::MAX {
        DEFAULT_MAX_LEVEL
    } else {
        v
    }
}

/// True if a record at `level` would be emitted — lets a macro skip formatting
/// entirely when the level is filtered out (zero cost for disabled logs).
#[inline]
pub fn enabled(level: LogLevel) -> bool {
    (level as u8) <= max_level()
}

/// Read `ROSACE_LOG` (`error`/`warn`/`info`/`debug`/`trace`, case-insensitive)
/// and set the max level from it. Called once at app launch; a no-op if unset
/// or unrecognized (keeps the default). Native-only (`std::env`).
#[cfg(not(target_arch = "wasm32"))]
pub fn init_from_env() {
    if let Ok(v) = std::env::var("ROSACE_LOG") {
        let level = match v.trim().to_ascii_lowercase().as_str() {
            "error" => Some(LogLevel::Error),
            "warn" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            "trace" => Some(LogLevel::Trace),
            _ => None,
        };
        if let Some(l) = level {
            set_max_level(l);
        }
    }
}

/// The macro back-end: format + emit a log record onto the bus. Called only
/// after the level check passes, so the `format!` allocation happens solely for
/// records that will actually be delivered.
#[doc(hidden)]
pub fn __emit(level: LogLevel, target: &'static str, args: std::fmt::Arguments) {
    crate::TRACING_BUS.emit(RosaceTrace::Log {
        level,
        target,
        message: std::fmt::format(args),
        timestamp: web_time::Instant::now(),
    });
}

/// Log at an explicit [`LogLevel`]. The level-specific macros
/// (`info!` etc.) forward here.
///
/// ```rust
/// use rosace_trace::{log, event::LogLevel};
/// log!(LogLevel::Info, "loaded {} items", 3);
/// ```
#[macro_export]
macro_rules! log {
    ($level:expr, $($arg:tt)+) => {{
        let __lvl = $level;
        if $crate::log::enabled(__lvl) {
            $crate::log::__emit(__lvl, ::core::module_path!(), ::core::format_args!($($arg)+));
        }
    }};
}

/// Log at `Error`.
#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => { $crate::log!($crate::event::LogLevel::Error, $($arg)+) };
}
/// Log at `Warn`.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => { $crate::log!($crate::event::LogLevel::Warn, $($arg)+) };
}
/// Log at `Info`.
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => { $crate::log!($crate::event::LogLevel::Info, $($arg)+) };
}
/// Log at `Debug`.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => { $crate::log!($crate::event::LogLevel::Debug, $($arg)+) };
}
/// Log at `Trace` (note: distinct from [`trace!`](crate::trace), which emits
/// structured framework events; this logs a message).
#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)+) => { $crate::log!($crate::event::LogLevel::Trace, $($arg)+) };
}
