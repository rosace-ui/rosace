use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Connected browser hot-reload WebSocket clients (web dev). The file watcher
/// pushes edited source to each; the serve loop registers new ones.
type HotClients = Arc<Mutex<Vec<TcpStream>>>;

/// The path the wasm hot-reload client connects to (matches
/// `rosace::dev_reload::connect_hot_reload_socket`).
const WEB_HOT_RELOAD_PATH: &str = "/__rosace_hot";

/// Target platform for the dev server.
#[derive(Debug, Clone, PartialEq)]
pub enum DevTarget {
    Desktop,
    Web,
    /// Android device/emulator — pushes edits over an `adb forward`ed socket.
    Android,
    /// iOS simulator — shares the host's localhost, so pushes directly.
    Ios,
}

/// Options parsed from `rsc dev [--target <target>] [--port <n>] [--watch] [--bin <name>]`.
#[derive(Debug, Clone)]
pub struct DevOptions {
    pub target: DevTarget,
    pub port: u16,
    pub watch: bool,
    /// Optional binary name (for workspaces with multiple `[[bin]]` entries).
    pub bin: Option<String>,
    /// Trailing-debounce window (ms) for the Tier-2 rebuild pipeline: a burst of
    /// edits within this quiet window coalesces into ONE rebuild. Default 300.
    pub debounce_ms: u64,
}

impl DevOptions {
    /// Build `DevOptions` from the CLI arguments that follow `dev`.
    ///
    /// Accepts both space-separated (`--target web`) and equals-separated
    /// (`--target=web`) forms.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }

        let mut target = DevTarget::Desktop;
        let mut port = 3000u16;
        let mut watch = false;
        let mut bin: Option<String> = None;
        let mut debounce_ms = 300u64;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--target" if i + 1 < args.len() => {
                    target = match args[i + 1].as_str() {
                        "desktop" => DevTarget::Desktop,
                        "web" => DevTarget::Web,
                        "android" => DevTarget::Android,
                        "ios" => DevTarget::Ios,
                        other => {
                            return Err(format!(
                                "unknown target '{}'. Supported: desktop, web, android, ios",
                                other
                            ))
                        }
                    };
                    i += 2;
                }
                "--port" if i + 1 < args.len() => {
                    port = args[i + 1]
                        .parse::<u16>()
                        .map_err(|_| format!("invalid port: {}", args[i + 1]))?;
                    i += 2;
                }
                "--bin" if i + 1 < args.len() => {
                    bin = Some(args[i + 1].clone());
                    i += 2;
                }
                "--watch" => {
                    watch = true;
                    i += 1;
                }
                "--debounce" if i + 1 < args.len() => {
                    debounce_ms = args[i + 1]
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --debounce ms: {}", args[i + 1]))?;
                    i += 2;
                }
                other if other.starts_with("--debounce=") => {
                    let d = other.trim_start_matches("--debounce=");
                    debounce_ms = d
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --debounce ms: {}", d))?;
                    i += 1;
                }
                other if other.starts_with("--target=") => {
                    let t = other.trim_start_matches("--target=");
                    target = match t {
                        "desktop" => DevTarget::Desktop,
                        "web" => DevTarget::Web,
                        "android" => DevTarget::Android,
                        "ios" => DevTarget::Ios,
                        _ => return Err(format!("unknown target '{}'", t)),
                    };
                    i += 1;
                }
                other if other.starts_with("--port=") => {
                    let p = other.trim_start_matches("--port=");
                    port = p
                        .parse::<u16>()
                        .map_err(|_| format!("invalid port: {}", p))?;
                    i += 1;
                }
                other if other.starts_with("--bin=") => {
                    bin = Some(other.trim_start_matches("--bin=").to_string());
                    i += 1;
                }
                other => return Err(format!("unknown flag: {}", other)),
            }
        }

        Ok(Self { target, port, watch, bin, debounce_ms })
    }
}

pub fn print_help() {
    println!("rsc dev — start the desktop app in dev mode (cargo run)");
    println!();
    println!("USAGE:");
    println!("  rsc dev [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --target <t>   desktop (default) | web | android | ios");
    println!("  --port <n>     Dev server port for web (default: 3000)");
    println!("  --watch        Rebuild on source changes");
    println!("  --bin <name>   Binary name, for workspaces with multiple [[bin]] entries");
    println!("  --debounce <ms> Coalesce edit bursts into one rebuild (default: 300)");
    println!("  -h, --help     Print this message");
}

/// Run the dev command.
pub fn run(opts: DevOptions) -> Result<(), String> {
    match opts.target {
        // Desktop + watch is a supervised build→run→relaunch loop (Tier 0
        // hot restart): it owns the app child process so it can restart it on
        // a successful rebuild. Persistent (`state_permanent`) state survives
        // because it lives on disk in the platform app-data dir.
        DevTarget::Desktop if opts.watch => run_desktop_watch(opts.bin.as_deref()),
        // Default desktop dev = Tier 2 dylib hot-reload (D102): edit ANY code and
        // it swaps live, no restart. If the app isn't Tier-2-ready (no
        // `__rsc_dev_root`), fall back to the Tier-1 `cargo run` path.
        DevTarget::Desktop => match crate::commands::tier2::run(opts.debounce_ms) {
            Err(e) if e.starts_with("NOT_TIER2_READY:") => {
                println!("  {}", e.trim_start_matches("NOT_TIER2_READY:").trim());
                println!("  Falling back to Tier 1 (view! edits only).\n");
                run_desktop(opts.bin.as_deref())
            }
            other => other,
        },
        DevTarget::Web => {
            // Web has no native process to hot-restart yet; watch just rebuilds.
            if opts.watch {
                start_watcher(&opts);
            }
            run_web(opts.port)
        }
        // Mobile: the app already runs on the device (deploy via `rsc run
        // --android/--ios`); here we push edited source to its hot-reload
        // socket. `port` reuses the default 9765 unless overridden.
        DevTarget::Android => run_mobile_hot_reload(DevTarget::Android, opts.port),
        DevTarget::Ios => run_mobile_hot_reload(DevTarget::Ios, opts.port),
    }
}

fn start_watcher(opts: &DevOptions) {
    use rosace_hot_reload::{FileWatcher, RebuildRunner, RebuildTarget};

    let rebuild_target = match opts.target {
        DevTarget::Web => RebuildTarget::Web,
        _ => RebuildTarget::Desktop,
    };

    let (watcher, rx) = FileWatcher::new();
    // Watch src/ if it exists, else current directory
    if std::path::Path::new("src").exists() {
        watcher.watch("src");
    } else {
        watcher.watch(".");
    }

    println!("  [watch] Watching src/ for changes...");

    let runner = RebuildRunner::new().target(rebuild_target);
    std::thread::spawn(move || {
        // Keep watcher alive by moving it into the thread
        let _watcher = watcher;
        runner.run_loop(rx);
    });
}

fn run_desktop(bin: Option<&str>) -> Result<(), String> {
    println!("Starting ROSACE dev server (desktop)...");
    println!();

    // Auto-detect the binary name when none is specified.
    let bin_name: Option<String> = match bin {
        Some(name) => Some(name.to_string()),
        None => detect_single_bin(),
    };

    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if let Some(ref name) = bin_name {
        cmd.args(["--bin", name]);
        println!("  Binary: {}", name);
    }
    // Enable Tier 1 hot reload: the app is built in dev mode so `view!`
    // registers descriptors and the in-app watcher can swap them live.
    cmd.args(["--features", "rosace/rsc-hot"]);
    println!("  Hot reload: on (view! edits reload live; recompiles when logic changes)");
    let status = cmd
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;
    if !status.success() {
        return Err("cargo run failed".to_string());
    }
    Ok(())
}

/// Desktop dev with `--watch`: the Tier 0 hot-restart supervise loop.
///
/// Owns the app as a child process so it can relaunch it on each successful
/// rebuild. A failed build leaves the running app untouched (you don't lose a
/// working session to a typo). Transient in-memory atoms reset on restart;
/// `state_permanent` values are restored automatically because they persist to
/// disk (the `reload`/`session` in-memory tiers wait on D008's deferred serde
/// snapshot — named, not silently skipped).
fn run_desktop_watch(bin: Option<&str>) -> Result<(), String> {
    use rosace_hot_reload::{FileWatcher, RebuildRunner, RebuildTarget};

    let bin_name: Option<String> = match bin {
        Some(name) => Some(name.to_string()),
        None => detect_single_bin(),
    };

    println!("Starting ROSACE dev server (desktop, hot restart)...");
    if let Some(ref name) = bin_name {
        println!("  Binary: {}", name);
    }

    // Build once up front so the first launch is a run of an already-built
    // binary and a broken tree fails before we spawn anything.
    let runner = RebuildRunner::new().target(RebuildTarget::Desktop);
    println!("  Building...");
    runner.rebuild().map_err(|e| format!("initial build failed: {}", e))?;

    let mut child = spawn_app(bin_name.as_deref())?;
    println!("  App running (pid {}). Persistent state is preserved across reloads.", child.id());

    let (watcher, rx) = FileWatcher::new();
    if std::path::Path::new("src").exists() {
        watcher.watch("src");
    } else {
        watcher.watch(".");
    }
    println!("  [watch] Watching for changes... (Ctrl-C to stop)");

    for event in rx {
        // The user may have quit the app themselves; if so, stop supervising.
        if let Ok(Some(status)) = child.try_wait() {
            println!("  App exited ({}). Stopping dev server.", status);
            return Ok(());
        }

        println!("  [hot-restart] {} changed, rebuilding...", event.path.display());
        match runner.rebuild() {
            Ok(()) => {
                println!("  [hot-restart] rebuild OK — relaunching");
                // Kill the old process, then start the freshly built binary.
                let _ = child.kill();
                let _ = child.wait();
                child = spawn_app(bin_name.as_deref())?;
                println!("  [hot-restart] reloaded (pid {}) — persistent state restored", child.id());
            }
            Err(e) => {
                // Keep the running app alive; a broken build shouldn't cost
                // the developer their live session.
                eprintln!("  [hot-restart] rebuild FAILED: {} — keeping current app running", e);
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

/// Spawn the app as a child process via `cargo run`. Assumes a prior build has
/// already succeeded, so this is effectively just a run.
fn spawn_app(bin: Option<&str>) -> Result<std::process::Child, String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if let Some(name) = bin {
        cmd.args(["--bin", name]);
    }
    cmd.spawn().map_err(|e| format!("failed to launch app: {}", e))
}

/// Read Cargo.toml and return the binary name only when exactly one [[bin]]
/// is defined. Returns `None` when there are zero or multiple binaries so
/// Cargo's own error (or the `--bin` flag) handles the ambiguity.
fn detect_single_bin() -> Option<String> {
    let cargo_toml = std::fs::read_to_string("Cargo.toml").ok()?;
    let bins: Vec<&str> = cargo_toml
        .lines()
        .filter_map(|line| {
            let t = line.trim();
            if t.starts_with("name") && t.contains('=') {
                // Only collect names that appear after a [[bin]] header, which
                // we do by counting [[bin]] sections below.
                Some(t)
            } else {
                None
            }
        })
        .collect();
    // Count [[bin]] sections — a simpler marker than full TOML parsing.
    let bin_sections = cargo_toml.matches("[[bin]]").count();
    if bin_sections != 1 {
        if bin_sections > 1 {
            eprintln!(
                "  Hint: this package has {} binaries. Use --bin <name> to pick one.",
                bin_sections
            );
            eprintln!("  Example: rsc dev --bin {}", extract_first_bin_name(&cargo_toml).unwrap_or("my-app".to_string()));
        }
        return None;
    }
    // Single binary — extract its name.
    let _ = bins; // suppress unused warning
    extract_first_bin_name(&cargo_toml)
}

fn extract_first_bin_name(toml: &str) -> Option<String> {
    let mut in_bin = false;
    for line in toml.lines() {
        let t = line.trim();
        if t == "[[bin]]" { in_bin = true; continue; }
        if in_bin && t.starts_with("name") {
            if let Some((_, val)) = t.split_once('=') {
                let name = val.trim().trim_matches('"').trim_matches('\'').to_string();
                if !name.is_empty() { return Some(name); }
            }
        }
        // Another section starts — stop scanning
        if in_bin && t.starts_with('[') && t != "[[bin]]" { break; }
    }
    None
}

fn run_web(port: u16) -> Result<(), String> {
    println!("Starting ROSACE dev server (web)...");
    println!();

    // Step 1: build WASM with hot reload on (the wasm app opens the reload
    // WebSocket back to this dev server).
    println!("  Building WASM (hot reload on)...");
    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-unknown-unknown", "--features", "rosace/rsc-hot"])
        .status()
        .map_err(|e| format!("cargo build wasm32 failed: {}", e))?;
    if !status.success() {
        println!("  Warning: wasm32 build failed (target may not be installed)");
        println!("  Run: rustup target add wasm32-unknown-unknown");
        println!("  Serving existing dist/ if available...");
    }

    // Step 2: ensure dist/ exists with at least index.html
    fs::create_dir_all("dist").map_err(|e| format!("cannot create dist/: {}", e))?;

    let index_path = "dist/index.html";
    if !Path::new(index_path).exists() {
        fs::write(index_path, default_index_html())
            .map_err(|e| format!("cannot write index.html: {}", e))?;
    }

    // `rsc dev` serves WITH the hot-reload WebSocket push; `rsc run --target
    // web` uses the plain `serve_dist` (no dev socket).
    serve_dist_hot(port)
}

/// Serve the `dist/` directory over HTTP on `127.0.0.1:port` (blocks). Shared
/// by `rsc dev` and `rsc run --target web`.
pub fn serve_dist(port: u16) -> Result<(), String> {
    serve_dist_inner(port, None)
}

/// Serve `dist/` AND push `view!` edits to the browser over a WebSocket (web
/// hot reload). Watches `src/`; on a change, sends `"<file>\n<source>"` to every
/// connected client, which re-parses and hot-swaps on device.
fn serve_dist_hot(port: u16) -> Result<(), String> {
    let clients: HotClients = Arc::new(Mutex::new(Vec::new()));
    spawn_web_hot_watcher(clients.clone());
    serve_dist_inner(port, Some(clients))
}

fn serve_dist_inner(port: u16, hot_clients: Option<HotClients>) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port);
    let listener =
        TcpListener::bind(&addr).map_err(|e| format!("cannot bind to {}: {}", addr, e))?;

    println!();
    println!("  ROSACE app running at http://localhost:{}", port);
    println!("  Serving: dist/");
    if hot_clients.is_some() {
        println!("  Hot reload: on ({WEB_HOT_RELOAD_PATH}) — edit a view! and save");
    }
    println!("  Press Ctrl+C to stop.");
    println!();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Handle each connection on its own thread — a hot-reload
                // WebSocket stays open indefinitely and must not block serving.
                let clients = hot_clients.clone();
                std::thread::spawn(move || {
                    if let Err(e) = handle_request(stream, clients) {
                        eprintln!("  request error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("  connection error: {}", e),
        }
    }

    Ok(())
}

/// Watch `src/` and push each edited file to all connected browsers.
fn spawn_web_hot_watcher(clients: HotClients) {
    let (watcher, rx) = rosace_hot_reload::FileWatcher::new();
    let root = if Path::new("src").exists() { "src" } else { "." };
    watcher.watch(root);
    std::thread::spawn(move || {
        let _watcher = watcher; // keep alive
        for event in rx {
            let Ok(src) = fs::read_to_string(&event.path) else { continue };
            let frame = crate::commands::hot_ws::text_frame(&format!("{}\n{}", event.path.display(), src));
            let mut list = clients.lock().unwrap_or_else(|e| e.into_inner());
            // Push to each client; drop any that have disconnected.
            list.retain_mut(|c| c.write_all(&frame).and_then(|_| c.flush()).is_ok());
            println!("  → pushed {} to {} browser(s)", event.path.display(), list.len());
        }
    });
}

fn handle_request(mut stream: TcpStream, hot_clients: Option<HotClients>) -> Result<(), String> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| format!("read: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse: "GET /path HTTP/1.1"
    let path = parse_path(&request);

    // WebSocket upgrade on the hot-reload path → handshake + register the
    // client; the watcher thread pushes edits to it. Keep the socket open.
    if path == WEB_HOT_RELOAD_PATH {
        if let (Some(clients), Some(key)) = (hot_clients, crate::commands::hot_ws::websocket_key(&request)) {
            stream
                .write_all(crate::commands::hot_ws::handshake_response(&key).as_bytes())
                .map_err(|e| format!("ws handshake: {}", e))?;
            clients.lock().unwrap_or_else(|e| e.into_inner()).push(stream);
            return Ok(());
        }
    }

    let file_path = map_path_to_file(&path);

    let (status, content_type, body) = serve_file(&file_path);

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\n\r\n",
        status,
        content_type,
        body.len()
    );

    stream
        .write_all(response.as_bytes())
        .map_err(|e| format!("write headers: {}", e))?;
    stream
        .write_all(&body)
        .map_err(|e| format!("write body: {}", e))?;
    Ok(())
}

fn parse_path(request: &str) -> String {
    request
        .lines()
        .next()
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.get(1).map(|s| s.to_string())
        })
        .unwrap_or_else(|| "/".to_string())
}

fn map_path_to_file(url_path: &str) -> String {
    // Strip query string
    let path = url_path.split('?').next().unwrap_or(url_path);
    // Normalize to dist/ relative path, prevent path traversal
    let safe = path.trim_start_matches('/').replace("..", "");
    if safe.is_empty() || safe == "/" {
        "dist/index.html".to_string()
    } else {
        format!("dist/{}", safe)
    }
}

fn serve_file(file_path: &str) -> (&'static str, &'static str, Vec<u8>) {
    match fs::read(file_path) {
        Ok(bytes) => {
            let ct = content_type_for(file_path);
            ("200 OK", ct, bytes)
        }
        Err(_) => {
            // Try appending index.html for directory-like paths
            let index = format!("{}/index.html", file_path.trim_end_matches('/'));
            match fs::read(&index) {
                Ok(bytes) => ("200 OK", "text/html; charset=utf-8", bytes),
                Err(_) => ("404 Not Found", "text/plain", b"404 Not Found".to_vec()),
            }
        }
    }
}

fn content_type_for(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

fn default_index_html() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>ROSACE App</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { background: #12121c; display: flex; align-items: center; justify-content: center; min-height: 100vh; }
    canvas { display: block; image-rendering: pixelated; }
    .hint { color: #6750A4; font-family: monospace; margin-top: 16px; text-align: center; }
  </style>
</head>
<body>
  <div>
    <canvas id="rosace-canvas"></canvas>
    <p class="hint">Run <code>rsc build --target web</code> to compile your app.</p>
  </div>
</body>
</html>"#
}

// ── Mobile hot-reload sender (Tier 1) ───────────────────────────────────────
//
// The app runs on the device/simulator (deployed via `rsc run --android/--ios`)
// and listens on a hot-reload socket (`rosace-ffi` starts it in `Engine::init`).
// Here on the dev machine we watch `src/` and push edited source to it. Android
// needs an `adb forward`; the iOS simulator shares the host's localhost.

/// Default port matching `rosace::dev_reload::DEFAULT_HOT_RELOAD_PORT`.
const MOBILE_HOT_RELOAD_PORT: u16 = 9765;

fn run_mobile_hot_reload(target: DevTarget, port: u16) -> Result<(), String> {
    // `port` defaults to the web default (3000) when unset; use the mobile port.
    let port = if port == 3000 { MOBILE_HOT_RELOAD_PORT } else { port };
    let tflag = if target == DevTarget::Android { "android" } else { "ios" };
    println!("ROSACE mobile hot reload ({target:?}) on port {port}");
    println!("  First deploy a hot-reload build:  RSC_HOT=1 rsc run --target {tflag}");
    println!("  (that builds the app with rosace-ffi/rsc-hot so it opens the reload socket)");

    // Android device/emulator needs a host→device port forward; the iOS
    // simulator runs on the host and shares its localhost, so none is needed.
    if target == DevTarget::Android {
        adb_forward(port)?;
    }

    let (watcher, rx) = rosace_hot_reload::FileWatcher::new();
    let root = if std::path::Path::new("src").exists() { "src" } else { "." };
    watcher.watch(root);
    println!("  Watching {root}/ — edit a view! and save to reload on device");

    let _watcher = watcher; // keep alive
    for event in rx {
        let path = event.path.to_string_lossy().to_string();
        let Ok(src) = std::fs::read_to_string(&event.path) else { continue };
        match push_source_edit(port, &path, &src) {
            Ok(()) => println!("  → pushed {path}"),
            Err(e) => eprintln!("  push failed (is the app running?): {e}"),
        }
    }
    Ok(())
}

fn adb_forward(port: u16) -> Result<(), String> {
    let status = Command::new("adb")
        .args(["forward", &format!("tcp:{port}"), &format!("tcp:{port}")])
        .status()
        .map_err(|e| format!("adb not found ({e}); install Android platform-tools"))?;
    if !status.success() {
        return Err("adb forward failed (is a device/emulator connected?)".to_string());
    }
    println!("  adb forward tcp:{port} tcp:{port}");
    Ok(())
}

/// Push a framed `"<file>\n<source>"` edit to the device app's hot-reload
/// socket. Framing matches `rosace::dev_reload` (4-byte BE length + payload).
fn push_source_edit(port: u16, file: &str, src: &str) -> std::io::Result<()> {
    let payload = format!("{file}\n{src}");
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
    stream.write_all(payload.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_opts_defaults_to_desktop() {
        let opts = DevOptions::from_args(&[]).unwrap();
        assert_eq!(opts.target, DevTarget::Desktop);
        assert_eq!(opts.port, 3000);
    }

    #[test]
    fn dev_opts_web_target() {
        let args = vec!["--target".to_string(), "web".to_string()];
        let opts = DevOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, DevTarget::Web);
    }

    #[test]
    fn dev_opts_mobile_targets() {
        for (name, want) in [("android", DevTarget::Android), ("ios", DevTarget::Ios)] {
            let opts = DevOptions::from_args(&["--target".to_string(), name.to_string()]).unwrap();
            assert_eq!(opts.target, want);
            let opts = DevOptions::from_args(&[format!("--target={name}")]).unwrap();
            assert_eq!(opts.target, want);
        }
    }

    #[test]
    fn push_source_edit_writes_a_length_framed_message() {
        use std::io::Read;
        // Verify the sender's wire format matches what `rosace::dev_reload`
        // `read_frame` expects (4-byte BE length + "<file>\n<source>").
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut len = [0u8; 4];
            s.read_exact(&mut len).unwrap();
            let n = u32::from_be_bytes(len) as usize;
            let mut buf = vec![0u8; n];
            s.read_exact(&mut buf).unwrap();
            String::from_utf8(buf).unwrap()
        });
        push_source_edit(port, "src/x.rs", "fn main(){}").unwrap();
        assert_eq!(server.join().unwrap(), "src/x.rs\nfn main(){}");
    }

    #[test]
    fn dev_opts_desktop_explicit() {
        let args = vec!["--target".to_string(), "desktop".to_string()];
        let opts = DevOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, DevTarget::Desktop);
    }

    #[test]
    fn dev_opts_custom_port() {
        let args = vec!["--port".to_string(), "8080".to_string()];
        let opts = DevOptions::from_args(&args).unwrap();
        assert_eq!(opts.port, 8080);
    }

    #[test]
    fn dev_opts_equals_syntax() {
        let args = vec!["--target=web".to_string(), "--port=9000".to_string()];
        let opts = DevOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, DevTarget::Web);
        assert_eq!(opts.port, 9000);
    }

    #[test]
    fn dev_opts_invalid_port() {
        let args = vec!["--port".to_string(), "99999".to_string()];
        assert!(DevOptions::from_args(&args).is_err());
    }

    #[test]
    fn dev_opts_watch_flag() {
        let args = vec!["--watch".to_string()];
        let opts = DevOptions::from_args(&args).unwrap();
        assert!(opts.watch);
    }

    #[test]
    fn dev_opts_default_no_watch() {
        let opts = DevOptions::from_args(&[]).unwrap();
        assert!(!opts.watch);
    }

    #[test]
    fn dev_opts_watch_with_target() {
        let args = vec!["--target".to_string(), "web".to_string(), "--watch".to_string()];
        let opts = DevOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, DevTarget::Web);
        assert!(opts.watch);
    }

    #[test]
    fn dev_opts_unknown_flag_errors() {
        let args = vec!["--foo".to_string()];
        assert!(DevOptions::from_args(&args).is_err());
    }

    #[test]
    fn dev_opts_unknown_target_errors() {
        let args = vec!["--target".to_string(), "playstation".to_string()];
        assert!(DevOptions::from_args(&args).is_err());
    }

    #[test]
    fn parse_path_extracts_route() {
        assert_eq!(
            parse_path("GET /app.js HTTP/1.1\r\nHost: localhost"),
            "/app.js"
        );
        assert_eq!(parse_path("GET / HTTP/1.1"), "/");
    }

    #[test]
    fn map_path_root_returns_index() {
        assert_eq!(map_path_to_file("/"), "dist/index.html");
        assert_eq!(map_path_to_file(""), "dist/index.html");
    }

    #[test]
    fn map_path_prevents_traversal() {
        let result = map_path_to_file("/../etc/passwd");
        assert!(!result.contains(".."));
    }

    #[test]
    fn map_path_file() {
        assert_eq!(map_path_to_file("/app.js"), "dist/app.js");
        assert_eq!(map_path_to_file("/app.wasm"), "dist/app.wasm");
    }

    #[test]
    fn content_type_html() {
        assert_eq!(
            content_type_for("index.html"),
            "text/html; charset=utf-8"
        );
    }

    #[test]
    fn content_type_js() {
        assert_eq!(content_type_for("app.js"), "application/javascript");
    }

    #[test]
    fn content_type_wasm() {
        assert_eq!(content_type_for("app.wasm"), "application/wasm");
    }

    #[test]
    fn content_type_unknown() {
        assert_eq!(content_type_for("file.bin"), "application/octet-stream");
    }

    #[test]
    fn default_index_contains_canvas() {
        assert!(default_index_html().contains("rosace-canvas"));
    }
}
