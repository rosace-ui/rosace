pub mod console;
pub mod file;
pub mod log_console;
pub mod perfetto;
pub mod ring_buffer;

use std::sync::atomic::{AtomicBool, Ordering};

/// Install (once) the default colored console log sink onto `TRACING_BUS`, so
/// `info!`/`warn!`/… print to the terminal. Idempotent — safe to call from
/// every launch path (`App::launch`, the dev host, the FFI engine init).
pub fn install_log_console() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::TRACING_BUS.add_subscriber(std::sync::Arc::new(
        log_console::LogConsoleSubscriber::new(),
    ));
}

// ── Always-on flight recorder (D123/O1) ──────────────────────────────────

use std::sync::{Arc, OnceLock};
use crate::subscribers::ring_buffer::RingBufferSubscriber;

static FLIGHT_RECORDER: OnceLock<Arc<RingBufferSubscriber>> = OnceLock::new();

/// Install (once) the always-on flight recorder onto `TRACING_BUS` and
/// return its handle. Captures the last `capacity` MEANINGFUL events —
/// high-frequency ones (`AtomRead`/`FrameStart`/`PaintRegion`/…) are
/// excluded, so this is a cheap, bounded event history that is safe to
/// leave on in every debug build (D123/O1's governing rule). A DevTools
/// panel reads it via [`flight_recorder`]. Idempotent: later calls return
/// the same handle without adding a second subscriber.
pub fn install_flight_recorder(capacity: usize) -> Arc<RingBufferSubscriber> {
    FLIGHT_RECORDER
        .get_or_init(|| {
            let rec = Arc::new(RingBufferSubscriber::filtered(capacity, |e| {
                !e.is_high_frequency()
            }));
            crate::bus::TRACING_BUS.add_subscriber(rec.clone());
            rec
        })
        .clone()
}

/// The installed flight recorder, if any (DevTools reads its snapshot).
pub fn flight_recorder() -> Option<Arc<RingBufferSubscriber>> {
    FLIGHT_RECORDER.get().cloned()
}
