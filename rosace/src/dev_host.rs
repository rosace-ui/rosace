//! Tier-2 dylib hot-reload dev host (D102). Compiled ONLY under `rsc-hot` on
//! native desktop — the stable "host" side of the host/module split.
//!
//! It owns the window + [`FrameEngine`] and loads the app's UI as a reloadable
//! `dylib`. `rsc dev` rebuilds that dylib on each edit; the host notices the
//! file changed, `dlopen`s the new one, and swaps the root Component via
//! [`FrameEngine::set_root`] — so `build()` logic, handlers, structure, any code
//! reloads live. App state lives in the shared `rosace` dylib (keyed by
//! `ComponentId`), so it survives the swap.
//!
//! The reload is ordered (load-new → swap → drop-old) so a closure from the
//! outgoing module is never invoked after its library unloads.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rosace_core::Component;
use rosace_platform::PlatformWindow;

use crate::FrameEngine;

/// The C-ABI symbol every reloadable app module exports (the scaffold / a future
/// `#[rsc::app]` macro emits it): returns a fresh boxed root Component.
const ROOT_SYMBOL: &[u8] = b"__rsc_dev_root";

/// Signature of [`ROOT_SYMBOL`].
type RootFn = extern "C" fn() -> Box<dyn Component>;

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Copy the module dylib to a unique temp path and `dlopen` THAT. `dlopen`
/// caches by path, so re-opening the same path returns the *old* code; a fresh
/// filename each generation guarantees the newly-built code is what loads.
/// Returns the owning `Library` (kept alive) plus the fresh root Component.
unsafe fn load_module(src: &Path, generation: u64) -> Result<(libloading::Library, Box<dyn Component>), String> {
    let tmp = std::env::temp_dir()
        .join(format!("rsc_hot_{}_{}.dylib", std::process::id(), generation));
    std::fs::copy(src, &tmp).map_err(|e| format!("copy module → temp: {e}"))?;
    let lib = libloading::Library::new(&tmp).map_err(|e| format!("dlopen: {e}"))?;
    let root_fn: libloading::Symbol<RootFn> = lib
        .get(ROOT_SYMBOL)
        .map_err(|e| format!("module is missing `{}` (add it via the scaffold's dev entry): {e}",
                             String::from_utf8_lossy(ROOT_SYMBOL)))?;
    let root = root_fn();
    Ok((lib, root))
}

/// Run an app from a reloadable module dylib, hot-swapping whenever `module`
/// changes on disk. Blocks on the window event loop (returns when it closes).
pub fn run(title: &str, width: u32, height: u32, module: PathBuf) {
    #[cfg(debug_assertions)]
    rosace_trace::install_flight_recorder(2000);

    // Colored console logging (info!/warn!/…) + ROSACE_LOG level.
    rosace_trace::install_log_console();
    rosace_trace::init_from_env();

    // Turn crashes into readable reports instead of a silent process death.
    install_crash_handlers();

    let font = rosace_render::FontCache::bundled();
    // A sane default theme; the app's own root sets its real theme in `build()`.
    rosace_theme::set_theme(crate::dark_theme());

    let mut generation = 0u64;
    // Every module generation ever loaded is KEPT LOADED for the whole dev
    // session — we never `dlclose`. Unloading is unsafe: even after the engine
    // drops its element/render/handler caches, the post-swap rebuild still runs
    // component lifecycle (unmount) and in-flight drops that touch the outgoing
    // module's code; freeing that code out from under a live reference is a
    // silent segfault. Leaking a few MB per reload across a dev session is a
    // trivial cost for a crash-proof swap (the OS reclaims it on exit). This is
    // the standard hot-reload trade-off (hot-lib-reloader / Bevy do the same).
    let mut libs: Vec<libloading::Library> = Vec::new();
    let root = match unsafe { load_module(&module, generation) } {
        Ok((lib, root)) => {
            libs.push(lib);
            root
        }
        Err(e) => {
            eprintln!("  [hot-reload] initial module load failed: {e}");
            return;
        }
    };
    let mut engine = FrameEngine::new(root, font);
    let mut last = mtime(&module);
    println!("  [hot-reload] Tier 2 dylib host live — edit a screen and save; watching {}",
             module.display());

    // The window loop is reactive — it only paints on events/dirty — so an idle
    // window would never notice a rebuilt dylib. This poller watches the
    // module's mtime and pokes the frame scheduler awake when it changes; the
    // actual reload happens in the paint closure below (on the loop thread,
    // which owns the engine). Runs for the process's life.
    {
        let module = module.clone();
        let mut seen = mtime(&module);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let now = mtime(&module);
            if now.is_some() && now != seen {
                seen = now;
                rosace_state::request_frame(); // wake the loop → closure reloads
            }
        });
    }

    PlatformWindow::new()
        .title(title.to_string())
        .size(width, height)
        .run_layered(move |canvas, overlay_canvas, events| {
            // Rebuilt on disk? (mtime changed since we last loaded it.)
            let now = mtime(&module);
            if now.is_some() && now != last {
                last = now;
                generation += 1;
                match unsafe { load_module(&module, generation) } {
                    Ok((new_lib, new_root)) => {
                        // Retain the new lib for the session (never unloaded);
                        // the old ones stay loaded too, so no old code is freed.
                        libs.push(new_lib);
                        engine.set_root(new_root);
                        println!("  [hot-reload] ⚡ swapped module (gen {generation})");
                    }
                    // Keep the last-good module loaded — a broken edit doesn't kill the session.
                    Err(e) => eprintln!("  [hot-reload] reload failed, kept previous version: {e}"),
                }
            }
            // Paint under catch_unwind: a panic in the app's `build()`/paint
            // (a bad edit, an out-of-bounds, etc.) is CAUGHT and reported, and
            // the dev session survives to the next frame instead of the whole
            // window vanishing. (Native crashes — segfaults — can't be caught
            // here; the signal handler + supervisor report those.)
            let painted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                engine.paint(canvas, overlay_canvas, events)
            }));
            if painted.is_err() {
                eprintln!(
                    "  [hot-reload] ⚠️  a frame panicked (see the report above) — \
                     kept the app running; fix the code and save to recover"
                );
            }
        });
}

/// Install dev crash handlers: a panic hook that prints a clear, backtrace'd
/// report, and a native-signal handler (SIGSEGV/SIGABRT/SIGBUS) that turns an
/// otherwise-silent crash into a labeled message before the process dies. Both
/// write to stderr, which `rsc dev` streams to your terminal.
fn install_crash_handlers() {
    // Rust panics → a banner + message + backtrace (works even without
    // RUST_BACKTRACE, which developers rarely remember to set).
    std::panic::set_hook(Box::new(|info| {
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".into());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".into());
        let bt = std::backtrace::Backtrace::force_capture();
        eprintln!(
            "\n╔══ ROSACE panic ═══════════════════════════════════════════\n\
             ║ {msg}\n║ at {loc}\n╚═══════════════════════════════════════════════════════════\n{bt}"
        );
    }));

    // Native signals (segfault etc.) → a one-line labeled report. This runs in
    // a signal context, so it does only async-signal-safe work (a raw write),
    // then restores the default handler and re-raises so the OS still produces
    // its crash report / correct exit signal for the supervisor to read.
    #[cfg(all(feature = "rsc-hot", unix))]
    unsafe {
        for sig in [libc::SIGSEGV, libc::SIGABRT, libc::SIGBUS, libc::SIGILL] {
            libc::signal(sig, crash_signal_handler as libc::sighandler_t);
        }
    }
}

#[cfg(all(feature = "rsc-hot", unix))]
extern "C" fn crash_signal_handler(sig: libc::c_int) {
    // Async-signal-safe: a single `write(2)` of a fixed message, no allocation.
    let name: &[u8] = match sig {
        libc::SIGSEGV => b"\n\xF0\x9F\x92\xA5 ROSACE crash: SIGSEGV (segfault) - invalid memory access.\n",
        libc::SIGABRT => b"\n\xF0\x9F\x92\xA5 ROSACE crash: SIGABRT (abort).\n",
        libc::SIGBUS  => b"\n\xF0\x9F\x92\xA5 ROSACE crash: SIGBUS (bad memory alignment/access).\n",
        libc::SIGILL  => b"\n\xF0\x9F\x92\xA5 ROSACE crash: SIGILL (illegal instruction).\n",
        _ => b"\n\xF0\x9F\x92\xA5 ROSACE crash: fatal signal.\n",
    };
    unsafe {
        libc::write(2, name.as_ptr() as *const libc::c_void, name.len());
        // Restore default handling and re-raise so the OS finishes the job and
        // the parent (`rsc dev`) sees the real terminating signal.
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}
