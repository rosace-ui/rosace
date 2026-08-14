//! Sync dispatch — native calls a Rust-registered handler and gets an
//! immediate answer (D127). One job: look up the handler registered for a
//! channel and invoke it. No queueing, no threads, no atoms — this is a
//! plain blocking function call across the FFI boundary (the same shape
//! `rsc_engine_init` already uses, just walked in the opposite direction),
//! so it must only be used for work that finishes fast. Anything that needs
//! to wait on the native side (a system dialog, a slow native SDK call)
//! belongs in `outgoing.rs` instead.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use serde_json::Value;

/// A synchronous method-call handler an app registers to receive calls FROM
/// native. Must return quickly — it runs inline on whatever thread native
/// calls `dispatch` from, blocking that thread until it returns.
pub type MethodHandler = Box<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

type Registry = HashMap<String, MethodHandler>;

fn registry() -> &'static RwLock<Registry> {
    static HANDLERS: OnceLock<RwLock<Registry>> = OnceLock::new();
    HANDLERS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register `handler` for `channel`, replacing any previous handler for the
/// same name. Call once, e.g. at app startup — mirrors Flutter's
/// `MethodChannel(name).setMethodCallHandler(...)`.
pub fn set_method_call_handler(channel: impl Into<String>, handler: MethodHandler) {
    registry()
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(channel.into(), handler);
}

/// Remove `channel`'s handler, if any (e.g. on a screen that owned it being
/// torn down). Calls to a channel with no handler resolve as an error, not
/// a panic — see `dispatch`.
pub fn clear_method_call_handler(channel: &str) {
    registry().write().unwrap_or_else(|e| e.into_inner()).remove(channel);
}

/// Look up `channel`'s handler and invoke it with `method` and the decoded
/// `args_json`. Returns the JSON-encoded result on success, or a JSON object
/// `{"error": "..."}` on failure (unknown channel, malformed args, or the
/// handler itself returning `Err`) — the caller (the generated
/// `rsc_platform_channel_dispatch` FFI function) always gets *some* valid
/// JSON back, never has to special-case "no result."
pub fn dispatch(channel: &str, method: &str, args_json: &str) -> String {
    use rosace_trace::{event::RosaceTrace, trace};

    let args: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let started = std::time::Instant::now();
    let result = {
        let reg = registry().read().unwrap_or_else(|e| e.into_inner());
        match reg.get(channel) {
            Some(handler) => handler(method, args),
            None => Err(format!("no handler registered for channel '{channel}'")),
        }
    };

    // The FFI boundary is where mobile bugs live and it was completely
    // dark: `FfiCall`/`FfiError` were defined and rendered by DevTools but
    // never emitted, so a channel call that silently failed on device
    // looked identical to one that was never made.
    //
    // `fn_name` is `&'static str`, so it names the CHANNEL (a registered,
    // long-lived name) rather than the per-call method, which is borrowed.
    // The channel is the useful grouping anyway — "which bridge is slow" is
    // the question you actually ask.
    let name: &'static str = crate::platform_channel::leak_channel_name(channel);
    match &result {
        Ok(_) => {
            trace!(RosaceTrace::FfiCall { fn_name: name, duration: started.elapsed() });
        }
        Err(e) => {
            trace!(RosaceTrace::FfiError { fn_name: name, error: e.clone() });
        }
    }

    match result {
        Ok(v) => v.to_string(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Channel handlers are process-global — serialize tests that touch the
    // shared registry, same reasoning as capability.rs's TEST_LOCK.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_registered_handler_answers_a_call() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_method_call_handler("test.math", Box::new(|method, args| {
            if method == "add" {
                let nums: Vec<i64> = serde_json::from_value(args).unwrap();
                Ok(Value::from(nums.iter().sum::<i64>()))
            } else {
                Err("unknown method".into())
            }
        }));

        assert_eq!(dispatch("test.math", "add", "[2,3]"), "5");

        clear_method_call_handler("test.math");
    }

    #[test]
    fn calling_an_unregistered_channel_returns_an_error_object_not_a_panic() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_method_call_handler("test.nonexistent");
        let result = dispatch("test.nonexistent", "anything", "null");
        assert!(result.contains("\"error\""));
    }

    #[test]
    fn a_handler_returning_err_becomes_an_error_object() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_method_call_handler("test.fails", Box::new(|_, _| Err("boom".to_string())));
        let result = dispatch("test.fails", "anything", "null");
        assert!(result.contains("boom"));
        clear_method_call_handler("test.fails");
    }

    #[test]
    fn malformed_args_json_decodes_as_null_rather_than_panicking() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_method_call_handler("test.echo", Box::new(|_, args| Ok(args)));
        assert_eq!(dispatch("test.echo", "echo", "{not valid json"), "null");
        clear_method_call_handler("test.echo");
    }
}
