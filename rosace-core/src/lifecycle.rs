use crate::context::Context;

/// Runs `f` exactly once when the component first mounts, and registers the
/// returned cleanup function to run exactly once when the component unmounts.
///
/// This is idempotent: if `build()` is called multiple times (once per frame),
/// the setup runs only on the first call thanks to persistent hook state.
pub fn on_mount<F, Cleanup>(ctx: &mut Context, f: F)
where
    F: FnOnce() -> Cleanup + Send + 'static,
    Cleanup: FnOnce() + Send + 'static,
{
    let already_mounted = ctx.state(false);
    if !already_mounted.get() {
        already_mounted.set(true);
        let cleanup = f();
        ctx.on_cleanup(cleanup);
    }
}

/// Registers `f` to run exactly once when the component unmounts.
///
/// Idempotent: repeated calls from the same hook slot (across frames) register
/// the cleanup only on first call.
pub fn on_unmount(ctx: &mut Context, f: impl FnOnce() + Send + 'static) {
    let already_registered = ctx.state(false);
    if !already_registered.get() {
        already_registered.set(true);
        ctx.on_cleanup(f);
    }
}
