//! Platform Channel (D127) — a named, bidirectional method-call bridge to
//! native platform code, generalizing the request/result shape
//! `rosace-ffi::capability`'s camera/push proof already established (see
//! that module's doc: "a second capability would follow this exact same
//! shape, not a new architecture"). Lets app/native code exchange arbitrary
//! JSON-encoded data instead of the framework needing bespoke FFI functions
//! for every platform feature (contacts, a custom native SDK, sensors, …).
//!
//! Two independent call shapes, kept in separate files (single
//! responsibility — neither knows about the other):
//! - [`dispatch`] — **native calls Rust**, synchronously. One blocking call
//!   across the FFI boundary, answered inline. Use for anything fast.
//! - [`outgoing`] — **Rust calls native**, asynchronously. Native may take
//!   arbitrarily long (a system dialog, a slow SDK call) to answer, so the
//!   call is queued and the result delivered back later via a reactive
//!   `Atom`, the same wakeup mechanism every other piece of ROSACE state
//!   uses (not a new "async" concept — see `rosace-net::query`'s use of the
//!   identical background-thread-writes-an-atom pattern).

mod dispatch;
mod outgoing;

pub use dispatch::{clear_method_call_handler, dispatch as dispatch_call, set_method_call_handler, MethodHandler};
pub use outgoing::{invoke_method, report_call_error, report_call_result, take_outgoing_calls, ChannelCallState, OutgoingCall};

/// Intern a channel name as `&'static str` for tracing.
///
/// `RosaceTrace::FfiCall` takes `&'static str` (trace events outlive the
/// call that produced them), but a channel name arrives as a borrowed `&str`
/// across FFI. Channels are REGISTERED names — a small, finite set fixed at
/// startup — so interning them is bounded, unlike leaking per-call strings.
///
/// The cap is a backstop against a caller that generates channel names
/// dynamically: past it, tracing degrades to a constant rather than growing
/// the leak without limit.
pub(crate) fn leak_channel_name(name: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    /// Generous for real use (a handful of channels), low enough that a
    /// misuse cannot leak meaningfully.
    const MAX_INTERNED: usize = 256;

    static INTERNED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let set = INTERNED.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = set.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = set.get(name) {
        return found;
    }
    if set.len() >= MAX_INTERNED {
        return "<channel>";
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    set.insert(leaked);
    leaked
}

#[cfg(test)]
mod intern_tests {
    #[test]
    fn the_same_name_interns_to_the_same_pointer() {
        let a = super::leak_channel_name("dev.rosace.demo/math");
        let b = super::leak_channel_name("dev.rosace.demo/math");
        assert_eq!(a.as_ptr(), b.as_ptr(), "a repeat call must not leak again");
    }

    /// A caller generating channel names dynamically must not leak without
    /// bound — tracing degrades instead.
    #[test]
    fn interning_is_capped() {
        for i in 0..400 {
            super::leak_channel_name(&format!("ch.{i}"));
        }
        assert_eq!(super::leak_channel_name("ch.399"), "<channel>",
            "past the cap, names collapse to a constant");
    }
}
