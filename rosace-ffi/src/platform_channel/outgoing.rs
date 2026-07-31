//! Async outgoing calls — Rust asks native to do something that might take a
//! while (a system dialog, a slow native SDK call), so native can't just
//! block and answer inline the way `dispatch.rs` does. One job: queue the
//! call for native's per-frame poll to pick up, and correlate whatever
//! result eventually comes back to the right caller.
//!
//! Mirrors `rosace-ffi::capability`'s existing camera/push shape (request
//! queue + result atom + host-side native call) exactly — that module's own
//! doc comment already called this out as the pattern a second capability
//! would follow. The difference here is `call_id` correlation instead of one
//! fixed slot per capability, since Platform Channel calls are arbitrary and
//! concurrent rather than one arbitrary global boolean each.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde_json::Value;

use rosace_state::Atom;

/// The result of an `invoke_method` call, read the same way any other
/// reactive state is — `.get()` inside `build()` auto-subscribes the
/// component, so a widget reading this re-renders the instant the real
/// result (or error) lands, same as `CAMERA_PERMISSION.get()`.
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelCallState {
    /// Queued, not yet delivered to or answered by native.
    Pending,
    /// Native answered successfully.
    Resolved(Value),
    /// Native reported a failure (or no handler exists for the call).
    Failed(String),
}

/// One call queued for native to pick up on its next frame-tick poll.
pub struct OutgoingCall {
    pub call_id: u64,
    pub channel: String,
    pub method: String,
    pub args_json: String,
}

static OUTGOING: Mutex<Vec<OutgoingCall>> = Mutex::new(Vec::new());
static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1);

/// Calls awaiting a result, keyed by `call_id`. One-shot: a `call_id` is
/// removed the moment its result or error arrives, so a duplicate/stale
/// report from a misbehaving host is simply a no-op rather than corrupting
/// a later call that happens to reuse... (it never does — `call_id` only
/// ever increases and is never reused).
static WAITERS: Mutex<Option<HashMap<u64, Atom<ChannelCallState>>>> = Mutex::new(None);

fn waiters() -> std::sync::MutexGuard<'static, Option<HashMap<u64, Atom<ChannelCallState>>>> {
    let mut guard = WAITERS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

/// Ask native to run `method` on `channel` with `args`. Returns immediately
/// with an `Atom` the caller can `.get()` — reactively `Pending` until
/// native reports back (see [`report_call_result`]/[`report_call_error`]).
/// App code typically stores this in its own `ctx.state(...)` so it
/// survives across re-renders until it resolves.
pub fn invoke_method(channel: impl Into<String>, method: impl Into<String>, args: Value) -> Atom<ChannelCallState> {
    let call_id = NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed);
    let atom = Atom::new(rosace_state::next_atom_id(), ChannelCallState::Pending);

    waiters().as_mut().unwrap().insert(call_id, atom.clone());
    OUTGOING.lock().unwrap_or_else(|e| e.into_inner()).push(OutgoingCall {
        call_id,
        channel: channel.into(),
        method: method.into(),
        args_json: args.to_string(),
    });

    atom
}

/// Polled by the native host once per frame tick (alongside
/// `rsc_engine_frame`, same shape `take_camera_request` established) —
/// drains and returns every call queued since the last poll.
pub fn take_outgoing_calls() -> Vec<OutgoingCall> {
    std::mem::take(&mut *OUTGOING.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Called by the native host once `call_id`'s native-side work finishes
/// successfully. Parses `result_json` and resolves the call's `Atom`, which
/// notifies subscribers same as any other state change.
pub fn report_call_result(call_id: u64, result_json: &str) {
    let value: Value = serde_json::from_str(result_json).unwrap_or(Value::Null);
    if let Some(atom) = waiters().as_mut().unwrap().remove(&call_id) {
        atom.set(ChannelCallState::Resolved(value));
    }
}

/// Called by the native host when `call_id`'s native-side work fails (or
/// there was no handler registered on the native side for that channel).
pub fn report_call_error(call_id: u64, message: impl Into<String>) {
    if let Some(atom) = waiters().as_mut().unwrap().remove(&call_id) {
        atom.set(ChannelCallState::Failed(message.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_then_take_delivers_exactly_the_queued_call() {
        let atom = invoke_method("test.chan", "doThing", Value::from(42));
        assert_eq!(atom.get(), ChannelCallState::Pending);

        let calls = take_outgoing_calls();
        let call = calls.iter().find(|c| c.channel == "test.chan" && c.method == "doThing")
            .expect("the call must be queued");
        assert_eq!(call.args_json, "42");

        report_call_result(call.call_id, "\"done\"");
        assert_eq!(atom.get(), ChannelCallState::Resolved(Value::from("done")));
    }

    #[test]
    fn take_outgoing_calls_drains_so_a_second_poll_sees_nothing_new() {
        invoke_method("test.chan2", "m", Value::Null);
        let first = take_outgoing_calls();
        assert!(!first.is_empty());
        let second = take_outgoing_calls();
        assert!(second.iter().all(|c| c.channel != "test.chan2"));
    }

    #[test]
    fn report_call_error_resolves_the_atom_as_failed() {
        let atom = invoke_method("test.chan3", "m", Value::Null);
        let call = take_outgoing_calls().into_iter().find(|c| c.channel == "test.chan3").unwrap();
        report_call_error(call.call_id, "native refused");
        assert_eq!(atom.get(), ChannelCallState::Failed("native refused".to_string()));
    }

    #[test]
    fn a_stale_or_duplicate_report_for_an_unknown_call_id_is_a_harmless_no_op() {
        // Must not panic — a misbehaving/late-reporting host is a fact of
        // life at this boundary, not a caller bug we can prevent Rust-side.
        report_call_result(999_999_999, "1");
        report_call_error(999_999_998, "whatever");
    }
}
