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
