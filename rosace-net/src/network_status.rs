//! `use_network_status` (D012's decided shape, built by D113/Phase 30
//! Step 4): app-wide connectivity as a `GlobalAtom`, with a lazy
//! attempt-based desktop prober.
//!
//! Desktop/native: the first `use_network_status` call starts ONE probe
//! thread for the process — a TCP connect attempt (no payload) to
//! well-known anycast endpoints every few seconds; reachability of any
//! means Online. Attempt-based rather than OS-API-based on desktop per
//! `PHASE_30.md` Step 4 (macOS `SCNetworkReachability`/`NWPathMonitor`
//! equivalents per-OS are a later refinement; the probe is truthful —
//! it measures what apps actually care about, "can I reach the
//! internet", not just link state).
//!
//! Mobile: the same atom is the seam for the D106 native-host capability
//! (`NWPathMonitor` on iOS / `ConnectivityManager` on Android reporting
//! through the bridge, camera/lifecycle-shaped) — the host would call
//! [`set_network_status`], and [`use_network_status`]'s prober stays
//! dormant if a host reported first. The native-host halves are a named
//! deferral to a device session (the iOS Simulator shares the Mac's
//! network, so the desktop prober already behaves correctly there).
//!
//! wasm32: no probe thread (threads panic on wasm) — status stays
//! `Unknown`, the documented named-gap (`navigator.onLine` is the future
//! web backend, tracked in `PHASE_30.md`).

use std::sync::atomic::{AtomicBool, Ordering};

use rosace_core::Context;
use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

/// Connectivity as observed by the prober (or reported by a mobile host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetworkStatus {
    /// No probe has completed yet (startup) — treat as "don't warn yet".
    #[default]
    Unknown,
    Online,
    Offline,
}

/// Reserved atom ID — continues the ladder (`0xFFF6`..`0xFFFF` are taken;
/// see `rosace-ffi/src/capability.rs`'s doc for the list).
const NETWORK_STATUS_ATOM_ID: AtomId = AtomId(0xFFF5);

static NETWORK_STATUS: GlobalAtom<NetworkStatus> =
    GlobalAtom::new(NETWORK_STATUS_ATOM_ID, || NetworkStatus::Unknown);

/// True once anything (prober or a native host) has taken ownership of
/// writing the atom — a host report suppresses the desktop prober.
static SOURCE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Report connectivity — the mobile native-host entry point (D106
/// capability shape), also what the prober uses internally. Only writes
/// (and re-renders subscribers) on a real change.
pub fn set_network_status(status: NetworkStatus) {
    SOURCE_ACTIVE.store(true, Ordering::SeqCst);
    if NETWORK_STATUS.get() != status {
        NETWORK_STATUS.set(status);
    }
}

/// Non-subscribing read for engine/worker code.
pub fn network_status() -> NetworkStatus {
    NETWORK_STATUS.get()
}

/// Probe targets: TCP connect only, no payload. Cloudflare + Google DNS
/// anycast on 443/53 — two independent providers so one outage isn't
/// read as "offline".
#[cfg(not(target_arch = "wasm32"))]
const PROBE_ADDRS: [&str; 2] = ["1.1.1.1:443", "8.8.8.8:53"];

#[cfg(not(target_arch = "wasm32"))]
const PROBE_TIMEOUT_SECS: u64 = 3;
#[cfg(not(target_arch = "wasm32"))]
const PROBE_INTERVAL_SECS: u64 = 4;

#[cfg(not(target_arch = "wasm32"))]
fn probe_once() -> NetworkStatus {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    for addr in PROBE_ADDRS {
        let parsed: SocketAddr = match addr.parse() {
            Ok(a) => a,
            Err(_) => continue,
        };
        if TcpStream::connect_timeout(&parsed, Duration::from_secs(PROBE_TIMEOUT_SECS)).is_ok() {
            return NetworkStatus::Online;
        }
    }
    NetworkStatus::Offline
}

static PROBER_STARTED: AtomicBool = AtomicBool::new(false);

/// Read connectivity from a component's `build()`, subscribing it for
/// re-render on changes. Starts the process-wide prober on first use
/// (native only; never if a host already reported).
pub fn use_network_status(ctx: &Context) -> NetworkStatus {
    NETWORK_STATUS.get_or_init().subscribe(ctx.component_id());

    #[cfg(not(target_arch = "wasm32"))]
    if !PROBER_STARTED.swap(true, Ordering::SeqCst) && !SOURCE_ACTIVE.load(Ordering::SeqCst) {
        std::thread::spawn(|| loop {
            let observed = probe_once();
            if NETWORK_STATUS.get() != observed {
                NETWORK_STATUS.set(observed);
            }
            std::thread::sleep(std::time::Duration::from_secs(PROBE_INTERVAL_SECS));
        });
    }

    NETWORK_STATUS.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Process-global atom — serialize, restore Unknown before release
    // (the capability.rs / app_lifecycle.rs test convention).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn host_report_updates_and_deduplicates() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_network_status(NetworkStatus::Online);
        assert_eq!(network_status(), NetworkStatus::Online);
        set_network_status(NetworkStatus::Offline);
        assert_eq!(network_status(), NetworkStatus::Offline);
        NETWORK_STATUS.set(NetworkStatus::Unknown); // reset
    }

    #[test]
    fn use_network_status_subscribes_for_re_render() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let component = rosace_core::ComponentId(9201);
        let ctx = Context::new(component);
        let _ = use_network_status(&ctx);

        let _ = rosace_state::dirty_set::take_dirty_components();
        // A change must dirty the subscriber (route through the atom
        // directly — set_network_status dedupes against current value).
        NETWORK_STATUS.set(NetworkStatus::Offline);
        assert!(
            rosace_state::dirty_set::take_dirty_components().contains(&component),
            "status change must mark the subscribed component dirty"
        );
        NETWORK_STATUS.get_or_init().unsubscribe(component);
        NETWORK_STATUS.set(NetworkStatus::Unknown); // reset
    }
}
