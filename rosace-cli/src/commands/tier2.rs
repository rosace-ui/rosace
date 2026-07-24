//! Tier-2 dylib hot-reload for `rsc dev` (desktop) — D102.
//!
//! The proven recipe (see `.steering/HOT_RELOAD.md` + the memory note), turned
//! into a real dev loop:
//!   1. Collect the app's native link directives (sqlite etc.) from a
//!      metadata build — a Rust `dylib` must re-link native libs that a
//!      dependency's inline code pulls in.
//!   2. Build the app's lib as a reloadable `dylib` (`-C prefer-dynamic` +
//!      `rosace/rsc-hot` + those link args).
//!   3. Generate + build a tiny **host** binary that links the SAME shared
//!      `rosace` dylib and calls `rosace::dev_host::run(module)`.
//!   4. Launch the host (with the dyld search path set) — it owns the window,
//!      loads the module, and swaps it whenever the dylib's mtime changes.
//!   5. Watch `src/`; on edit, rebuild ONLY the module dylib. The host notices
//!      and hot-swaps — so `build()`, handlers, structure, ANY code reloads
//!      live, no restart, state preserved.
//!
//! Everything lives under `target/rsc-hot/` so it shares one `librosace.dylib`
//! instance (the single state-store singleton that makes state survive a swap).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The ONE constant `RUSTFLAGS` used for every build in the shared target dir.
/// Must be identical across the module + host builds or cargo re-fingerprints
/// and rebuilds the world (the source of the earlier thrash). `prefer-dynamic`
/// → host + module share one `librosace.dylib` (single state-store singleton);
/// `link-dead-code` → the shared dylib keeps + exports statics the module needs
/// but rosace itself doesn't reference (e.g. winit's event-loop guard).
const HOT_RUSTFLAGS: &str = "-C prefer-dynamic -C link-dead-code";

/// Apply the constant hot-reload build env to a cargo command: the fixed
/// RUSTFLAGS, the shared target dir, and `RUSTC_BOOTSTRAP=1` so the stable
/// toolchain accepts the `-Z share-generics=off` the module build needs.
fn hot_env(cmd: &mut Command, target: &Path) {
    cmd.env("RUSTFLAGS", HOT_RUSTFLAGS)
        .env("CARGO_TARGET_DIR", target)
        .env("RUSTC_BOOTSTRAP", "1");
}

/// Entry point from `rsc dev` (desktop, no `--watch`). Returns `Err` with a
/// message starting `NOT_TIER2_READY:` when the app can't do Tier 2 (missing
/// `__rsc_dev_root`), so the caller can fall back to the Tier-1 `cargo run`.
pub fn run(debounce_ms: u64) -> Result<(), String> {
    let app_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let lib_name = lib_crate_name(&app_dir)
        .ok_or("could not determine the library crate name from Cargo.toml")?;

    if !exports_dev_root(&app_dir) {
        return Err(format!(
            "NOT_TIER2_READY: {lib_name} has no `__rsc_dev_root` export \
             (Tier-2 hot-reload entry). Add it to src/lib.rs, or scaffold a new app."
        ));
    }

    let target = app_dir.join("target").join("rsc-hot");
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    println!("rsc dev — Tier 2 dylib hot-reload (desktop)");
    println!("  Collecting native link flags…");
    let link_flags = collect_native_link_flags(&app_dir, &target)?;

    println!("  Building the app module (dylib)…");
    build_module(&app_dir, &target, &link_flags)?;
    let module = target.join("debug").join(format!("lib{lib_name}.dylib"));
    if !module.exists() {
        return Err(format!("module dylib not produced at {}", module.display()));
    }

    println!("  Building the dev host…");
    let host_bin = build_host(&app_dir, &target)?;

    // dyld needs the shared dylib dir + the rust std dir (cargo run injects
    // this automatically; a directly-spawned binary does not).
    let dyld = dyld_search_path(&target)?;

    println!("  Launching…\n");
    let mut child = Command::new(&host_bin)
        .arg(&module)
        .env("DYLD_FALLBACK_LIBRARY_PATH", &dyld)
        .spawn()
        .map_err(|e| format!("failed to launch dev host: {e}"))?;

    // Watch src/ and rebuild the module dylib on edit; the host hot-swaps it.
    watch_and_rebuild(&app_dir, &target, &link_flags, &mut child, std::time::Duration::from_millis(debounce_ms));
    Ok(())
}

/// The library crate name — `[lib] name` if set, else `[package] name` (Cargo's
/// default), with `-` normalized to `_` as the dylib file uses.
fn lib_crate_name(app_dir: &Path) -> Option<String> {
    let toml = std::fs::read_to_string(app_dir.join("Cargo.toml")).ok()?;
    // Prefer an explicit [lib] name.
    let mut in_lib = false;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_lib = t == "[lib]";
            continue;
        }
        if in_lib && t.starts_with("name") {
            if let Some(v) = toml_str_value(t) {
                return Some(v.replace('-', "_"));
            }
        }
    }
    // Fall back to [package] name.
    let mut in_pkg = false;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[package]";
            continue;
        }
        if in_pkg && t.starts_with("name") {
            if let Some(v) = toml_str_value(t) {
                return Some(v.replace('-', "_"));
            }
        }
    }
    None
}

/// `key = "value"` → `value`.
fn toml_str_value(line: &str) -> Option<String> {
    let after = line.split_once('=')?.1.trim();
    Some(after.trim_matches('"').to_string())
}

/// True when the app exports the Tier-2 entrypoint. A cheap source scan (avoids
/// a build just to answer "is this app Tier-2-ready?").
fn exports_dev_root(app_dir: &Path) -> bool {
    std::fs::read_to_string(app_dir.join("src").join("lib.rs"))
        .map(|s| s.contains("__rsc_dev_root"))
        .unwrap_or(false)
}

/// Reuse the app's own `rosace = …` dependency spec so the host links the SAME
/// rosace source (path or published version) the app does — just with
/// `rsc-hot` on. Returns the full dependency line for the host's Cargo.toml.
fn rosace_dep_for_host(app_dir: &Path) -> Result<String, String> {
    let toml = std::fs::read_to_string(app_dir.join("Cargo.toml"))
        .map_err(|e| format!("read Cargo.toml: {e}"))?;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with("rosace ") || t.starts_with("rosace=") {
            // Path form: `rosace = { path = "…" }` → add features.
            if let Some(path) = t.split_once("path").and_then(|(_, r)| {
                r.trim_start_matches([' ', '=']).split('"').nth(1)
            }) {
                return Ok(format!(
                    "rosace = {{ path = \"{path}\", features = [\"rsc-hot\"] }}"
                ));
            }
            // Version form: `rosace = "x.y"` → add features.
            if let Some(v) = toml_str_value(t) {
                return Ok(format!(
                    "rosace = {{ version = \"{v}\", features = [\"rsc-hot\"] }}"
                ));
            }
        }
    }
    Err("no `rosace` dependency found in the app's Cargo.toml".into())
}

/// Run a metadata build and harvest every `cargo:rustc-link-search` /
/// `-link-lib` a build script emitted, as `-L …` / `-l …` rustc flags. This is
/// the generic fix for the native-lib re-link problem (sqlite was the first
/// case): a dylib must resolve native symbols that dependencies' inline code
/// re-monomorphizes into it.
fn collect_native_link_flags(app_dir: &Path, target: &Path) -> Result<String, String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(app_dir).args([
        "build", "--lib", "--features", "rosace/rsc-hot",
        "--message-format=json-render-diagnostics",
    ]);
    hot_env(&mut cmd, target);
    let out = cmd
        .output()
        .map_err(|e| format!("cargo metadata build failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "metadata build failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let mut flags: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if !line.contains("\"build-script-executed\"") {
            continue;
        }
        // Minimal, dependency-free JSON field scraping for the two arrays.
        for path in json_string_array(line, "linked_paths") {
            let flag = format!("-L {}", normalize_link_kv(&path, "native"));
            if seen.insert(flag.clone()) { flags.push(flag); }
        }
        for lib in json_string_array(line, "linked_libs") {
            let flag = format!("-l {}", lib);
            if seen.insert(flag.clone()) { flags.push(flag); }
        }
    }
    Ok(flags.join(" "))
}

/// Extract the string elements of a `"key":[ "a", "b" ]` array from one JSON
/// line. Good enough for cargo's build-script message shape (no nested quotes
/// inside these particular values).
fn json_string_array(line: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\":[");
    let Some(start) = line.find(&needle) else { return Vec::new() };
    let rest = &line[start + needle.len()..];
    let Some(end) = rest.find(']') else { return Vec::new() };
    rest[..end]
        .split(',')
        .filter_map(|s| {
            let s = s.trim().trim_matches('"');
            if s.is_empty() { None } else { Some(s.to_string()) }
        })
        .collect()
}

/// cargo gives `linked_paths` like `native=/p` or bare `/p`; ensure a kind
/// prefix so `-L native=/p` is unambiguous.
fn normalize_link_kv(v: &str, default_kind: &str) -> String {
    if v.contains('=') { v.to_string() } else { format!("{default_kind}={v}") }
}

/// Build the app lib as a `dylib` (crate-type overridden at the command line so
/// the app's own manifest — which needs `cdylib`/`staticlib` for mobile — is
/// left untouched). The per-crate flags go AFTER `--` so they hit only this
/// crate, not its dependencies:
///   - `-Z share-generics=off` → the module instantiates its own generics
///     locally instead of expecting an upstream dylib to provide them (the
///     `<&T as Debug>::fmt` "symbol not found" at dlopen);
///   - the harvested `-L/-l` native link flags → resolve native symbols
///     (sqlite) that dependency inline code re-monomorphizes into the module.
fn build_module(app_dir: &Path, target: &Path, link_flags: &str) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(app_dir).args([
        "rustc", "--lib", "--crate-type", "dylib",
        "--features", "rosace/rsc-hot",
        "--", "-Zshare-generics=off",
    ]);
    // Append each harvested native link flag as its own arg.
    for tok in link_flags.split_whitespace() {
        cmd.arg(tok);
    }
    hot_env(&mut cmd, target);
    let status = cmd
        .status()
        .map_err(|e| format!("cargo rustc failed to start: {e}"))?;
    if !status.success() {
        return Err("building the app module dylib failed".into());
    }
    Ok(())
}

/// Generate (once) and build the tiny host binary that drives the reload loop.
fn build_host(app_dir: &Path, target: &Path) -> Result<PathBuf, String> {
    let host_dir = target.join("host");
    std::fs::create_dir_all(host_dir.join("src")).map_err(|e| e.to_string())?;

    let rosace_dep = rosace_dep_for_host(app_dir)?;
    let cargo_toml = format!(
        "[package]\nname = \"rsc-dev-host\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\
         [workspace]\n\n[[bin]]\nname = \"rsc-dev-host\"\npath = \"src/main.rs\"\n\n\
         [dependencies]\n{rosace_dep}\n"
    );
    std::fs::write(host_dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;
    std::fs::write(
        host_dir.join("src").join("main.rs"),
        "fn main() {\n    \
            let module = std::env::args().nth(1).expect(\"usage: rsc-dev-host <module.dylib>\");\n    \
            rosace::dev_host::run(\"ROSACE dev (hot)\", 960, 640, module.into());\n}\n",
    )
    .map_err(|e| e.to_string())?;

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&host_dir).arg("build");
    hot_env(&mut cmd, target); // SAME env+target → shared librosace.dylib
    let status = cmd
        .status()
        .map_err(|e| format!("cargo build (host) failed to start: {e}"))?;
    if !status.success() {
        return Err("building the dev host failed".into());
    }
    Ok(target.join("debug").join("rsc-dev-host"))
}

/// `target/debug` + `target/debug/deps` + the rust std libdir — what dyld needs
/// to resolve `librosace.dylib` and `libstd-*.dylib` for a spawned binary.
fn dyld_search_path(target: &Path) -> Result<String, String> {
    let std_dir = Command::new("rustc")
        .args(["--print", "target-libdir"])
        .output()
        .map_err(|e| format!("rustc --print target-libdir: {e}"))?;
    let std_dir = String::from_utf8_lossy(&std_dir.stdout).trim().to_string();
    let dbg = target.join("debug");
    Ok(format!(
        "{}:{}:{std_dir}",
        dbg.display(),
        dbg.join("deps").display()
    ))
}

/// Watch `src/` and rebuild the module dylib on edits — the host hot-swaps the
/// result. This is the **coalescing rebuild pipeline**: a burst of edits (a
/// refactor touching many files, or rapid saves) collapses into ONE rebuild,
/// and edits made *during* a rebuild trigger exactly ONE follow-up rebuild on
/// the latest source. Never a backlog of N builds for N edits. Blocks until the
/// app window closes.
///
/// Shape (trailing debounce + single-slot):
///   1. Block until the first edit.
///   2. Keep draining events until it's been quiet for `debounce` — this
///      batches a multi-file save into one unit and settles rapid typing-saves.
///   3. Build once.
///   4. Any edits that arrived *while building* are still queued in the channel,
///      so the next loop iteration picks them up immediately → exactly one more
///      build on the newest bytes. (This is what makes it lossless AND bounded.)
fn watch_and_rebuild(
    app_dir: &Path,
    target: &Path,
    link_flags: &str,
    child: &mut std::process::Child,
    debounce: std::time::Duration,
) {
    use rosace_hot_reload::FileWatcher;
    use std::sync::mpsc::RecvTimeoutError;

    let (watcher, rx) = FileWatcher::new();
    watcher.watch(app_dir.join("src"));
    let _watcher = watcher; // keep the watcher thread alive for the loop's life
    println!(
        "  [hot-reload] watching src/ — edit any screen and save (Ctrl-C to stop; debounce {}ms)\n",
        debounce.as_millis()
    );

    loop {
        // (1) Wait for the first edit of a new burst — polling so a window close
        // (no more edits will ever come) is noticed promptly instead of hanging.
        let first = loop {
            match rx.recv_timeout(std::time::Duration::from_millis(300)) {
                Ok(ev) => break ev,
                Err(RecvTimeoutError::Timeout) => {
                    if app_closed(child) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return, // watcher gone
            }
        };
        if app_closed(child) {
            return;
        }

        // (2) Trailing debounce: reset the quiet window on every further edit, so
        // a stream of saves (a refactor) is treated as ONE change once it stops.
        let mut touched = file_name(&first.path);
        let mut burst = 1usize;
        loop {
            match rx.recv_timeout(debounce) {
                Ok(ev) => {
                    burst += 1;
                    touched = file_name(&ev.path);
                }
                Err(RecvTimeoutError::Timeout) => break,       // quiet → build
                Err(RecvTimeoutError::Disconnected) => return, // watcher gone
            }
        }

        // (3) Build once for the whole burst.
        if app_closed(child) {
            return;
        }
        if burst > 1 {
            println!("  [hot-reload] {burst} edits (…{touched}) → rebuilding module…");
        } else {
            println!("  [hot-reload] {touched} changed → rebuilding module…");
        }
        match build_module(app_dir, target, link_flags) {
            Ok(()) => println!("  [hot-reload] rebuilt — host will swap it live"),
            Err(e) => eprintln!("  [hot-reload] rebuild failed, kept previous version: {e}"),
        }

        // (4) Edits made DURING the build are still queued in `rx`; the next
        // iteration's `rx.recv()` returns one immediately → exactly one more
        // coalesced rebuild. Nothing is lost; nothing piles up.
    }

    let _ = child.wait();
}

/// True once the app process has ended — the signal to stop supervising. Also
/// reports HOW it ended: a clean close, a non-zero exit, or (the important one)
/// a crash by signal — so a silent segfault becomes a labeled "app crashed"
/// line instead of a mysterious disappearance.
fn app_closed(child: &mut std::process::Child) -> bool {
    match child.try_wait() {
        Ok(Some(status)) => {
            report_app_exit(status);
            true
        }
        _ => false,
    }
}

/// Print a clear message describing how the app process ended.
fn report_app_exit(status: std::process::ExitStatus) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            let name = match sig {
                11 => "SIGSEGV (segfault — invalid memory access)",
                6 => "SIGABRT (abort — e.g. an unwrap/assert or double-panic)",
                10 => "SIGBUS (bad memory access)",
                4 => "SIGILL (illegal instruction)",
                9 => "SIGKILL (killed)",
                15 => "SIGTERM (terminated)",
                2 => "SIGINT (interrupted)",
                _ => "signal",
            };
            eprintln!("  💥 app CRASHED — killed by {name} [{sig}]. Stopping dev server.");
            eprintln!("     (see any ROSACE panic/crash report above for the cause)");
            return;
        }
    }
    match status.code() {
        Some(0) | None => println!("  App closed. Stopping dev server."),
        Some(code) => eprintln!("  ⚠️  app exited with code {code}. Stopping dev server."),
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
