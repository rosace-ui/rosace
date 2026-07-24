//! Dev-only hot-reload driver (D103 / D102 Tier 1). Compiled ONLY under the
//! `rsc-hot` feature (which `rsc dev` turns on).
//!
//! The **reload handling** is platform-agnostic: [`apply_source_edit`] takes an
//! edited file's path + source, applies safe `view!` swaps to the live registry
//! (`apply_reload`), and forces a rebuild + repaint. Only the **transport** —
//! how the edited source reaches the running app — differs per platform:
//!   - **desktop**: a filesystem watcher thread ([`spawn_dev_watcher`]);
//!   - **web**: a WebSocket from the dev server pushes source text (calls
//!     [`apply_source_edit`]);
//!   - **Android/iOS device**: a socket over `adb`/`devicectl` delivers text.
//! All of them funnel through the same `apply_source_edit`.

/// Apply one edited file's source to the running app: swap what's safe, force a
/// repaint, and report what couldn't reload as data. Platform-agnostic — this
/// is the single entry point every transport calls.
pub fn apply_source_edit(file: &str, src: &str) {
    let report = rosace_widgets::template::apply_reload(file, src);

    // Unparseable mid-edit → wait for the next save; say nothing.
    if report.ignored() {
        return;
    }

    // Any site that swapped is already live in the registry — force a full
    // rebuild + repaint so the next frame re-inflates the new shape. (`paint()`
    // rebuilds only when dirty, so the global-dirty is required.)
    if report.applied > 0 {
        log(&format!("⚡ swapped {} view! site(s) in {file}", report.applied));
        rosace_state::reset_to_global_dirty();
        rosace_state::request_frame();
    }

    // Edits needing compiled code can't reload as data.
    if report.needs_restart() {
        log(&format!(
            "↻ {file} changed in a way that needs a recompile \
             (new hole/logic or a new view!) — rebuild to apply"
        ));
    }
}

/// Dev-log a line to the platform's console.
fn log(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    println!("  [hot-reload] {msg}");
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&format!("[hot-reload] {msg}").into());
}

/// The dev hot-reload wire message is `"<file path>\n<source text>"`. Split it
/// into `(file, source)`. Platform-agnostic + tested; used by every socket
/// transport (web WebSocket, Android/iOS device socket).
pub fn parse_reload_message(msg: &str) -> Option<(&str, &str)> {
    msg.split_once('\n')
}

// ── Desktop transport: filesystem watcher ───────────────────────────────────

/// Start watching `src/` for `view!` edits on a background thread (desktop).
/// Called from `App::launch` under `rsc-hot`. The browser has no filesystem, so
/// this is native-only; web is driven by a WebSocket instead.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_dev_watcher() {
    use rosace_hot_reload::FileWatcher;

    let (watcher, rx) = FileWatcher::new();
    let root = if std::path::Path::new("src").exists() { "src" } else { "." };
    watcher.watch(root);
    log(&format!("watching {root}/ for view! edits (Tier 1 data reload)"));

    // Also watch the app's asset dir: an edited image/font/data file hot-swaps
    // by invalidating the decode cache + repainting (no recompile needed).
    if std::path::Path::new("assets").exists() {
        watcher.watch("assets");
        log("watching assets/ for asset hot-reload");
    }

    std::thread::spawn(move || {
        let _watcher = watcher; // keep alive for the thread's life
        for event in rx {
            // Branch on file type: Rust source drives a `view!` data reload;
            // anything else is an asset — invalidate + repaint.
            let is_rust = event.path.extension().map(|e| e == "rs").unwrap_or(false);
            if is_rust {
                let Ok(src) = std::fs::read_to_string(&event.path) else {
                    continue; // deleted / unreadable mid-edit
                };
                apply_source_edit(&event.path.to_string_lossy(), &src);
            } else {
                apply_asset_change(&event.path.to_string_lossy());
            }
        }
    });
}

/// A file under `assets/` changed: drop every decoded image so the next paint
/// re-reads it from disk, then force a rebuild + repaint. Platform-agnostic so
/// the mobile/web transports can call it too once they push asset bytes.
pub fn apply_asset_change(file: &str) {
    rosace_widgets::ImageCache::global().lock().unwrap().clear();
    rosace_state::reset_to_global_dirty();
    rosace_state::request_frame();
    log(&format!("🖼  reloaded asset {file}"));
}

// ── Mobile transport: length-framed TCP socket (Android / iOS device+sim) ────
//
// The device app can't watch the dev machine's filesystem, so the dev machine
// pushes edited source over a socket. `adb forward` / `devicectl` maps a host
// port to this listener on the device; `rsc dev` connects and pushes frames.
// This is the same `apply_source_edit` as every other platform — only the
// delivery differs. Verifiable on localhost (it's plain `std::net`).

/// Default port the device app listens on for pushed edits (host maps to it via
/// `adb forward tcp:9765 tcp:9765`). Referenced only on mobile launch.
#[allow(dead_code)]
pub const DEFAULT_HOT_RELOAD_PORT: u16 = 9765;

/// Wire framing: a 4-byte big-endian length prefix followed by the UTF-8
/// `"<file>\n<source>"` payload. Length-framed because raw TCP is a byte stream
/// and the source itself contains newlines (so line-reads won't do).
#[cfg(not(target_arch = "wasm32"))]
pub fn write_frame(w: &mut impl std::io::Write, payload: &str) -> std::io::Result<()> {
    w.write_all(&(payload.len() as u32).to_be_bytes())?;
    w.write_all(payload.as_bytes())?;
    w.flush()
}

#[cfg(not(target_arch = "wasm32"))]
fn read_frame(r: &mut impl std::io::Read) -> std::io::Result<Option<String>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let n = u32::from_be_bytes(len) as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

/// Listen on `127.0.0.1:port` for pushed edits and apply each. Called on the
/// device (Android/iOS) under `rsc-hot`. Spawns a background acceptor thread.
/// (Unused on desktop, which uses the filesystem watcher.)
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn serve_hot_reload_socket(port: u16) {
    match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            log(&format!("hot-reload socket listening on 127.0.0.1:{port}"));
            std::thread::spawn(move || serve_on_listener(listener));
        }
        Err(e) => log(&format!("hot-reload socket disabled ({e})")),
    }
}

/// Accept connections and apply each length-framed edit. Split out so tests can
/// drive it with a bound ephemeral listener.
#[cfg(not(target_arch = "wasm32"))]
fn serve_on_listener(listener: std::net::TcpListener) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        std::thread::spawn(move || loop {
            match read_frame(&mut stream) {
                Ok(Some(msg)) => {
                    if let Some((file, src)) = parse_reload_message(&msg) {
                        apply_source_edit(file, src);
                    }
                }
                Ok(None) | Err(_) => break, // connection closed / error
            }
        });
    }
}

// ── Web transport: WebSocket from the dev server ─────────────────────────────

/// Open the dev hot-reload WebSocket (web). The dev server pushes
/// `"<file>\n<source>"` on each edit; we parse + [`apply_source_edit`]. Called
/// from the wasm launch path under `rsc-hot`. Best-effort: if the socket can't
/// open (e.g. plain `cargo`-served page with no dev server) it just logs.
#[cfg(target_arch = "wasm32")]
pub fn connect_hot_reload_socket() {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let url = match hot_reload_ws_url() {
        Some(u) => u,
        None => return,
    };
    let ws = match web_sys::WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(_) => {
            log("no dev hot-reload socket (open via `rsc dev`)");
            return;
        }
    };
    log(&format!("connecting hot-reload socket {url}"));

    let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
        if let Some(text) = e.data().as_string() {
            if let Some((file, src)) = parse_reload_message(&text) {
                apply_source_edit(file, src);
            }
        }
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget(); // keep the callback alive for the socket's life
}

/// Derive `ws://<host>/__rosace_hot` from the page's location.
#[cfg(target_arch = "wasm32")]
fn hot_reload_ws_url() -> Option<String> {
    let loc = web_sys::window()?.location();
    let host = loc.host().ok()?;
    let scheme = if loc.protocol().ok()?.starts_with("https") { "wss" } else { "ws" };
    Some(format!("{scheme}://{host}/__rosace_hot"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_reload_message_into_file_and_source() {
        let msg = "src/app.rs\nfn main() {\n    view! { Column { spacing: 8.0 } }\n}\n";
        let (file, src) = parse_reload_message(msg).unwrap();
        assert_eq!(file, "src/app.rs");
        assert!(src.starts_with("fn main()"));
        assert!(src.contains("spacing: 8.0"));
    }

    #[test]
    fn a_message_without_a_newline_is_rejected() {
        assert_eq!(parse_reload_message("no-newline"), None);
    }

    // The mobile socket transport, end-to-end on localhost: a pushed, framed
    // edit hot-swaps a registered site — exactly what runs on-device over an
    // `adb`/`devicectl`-forwarded port.
    #[test]
    fn socket_transport_applies_a_pushed_edit() {
        use rosace_widgets::template::{self, PropValue, StaticValue, Template, TemplateKey, TemplateNode};
        use std::net::{TcpListener, TcpStream};

        // Baseline: a Column at src/sock_demo.rs:2 with static spacing 4.0.
        let mut root = TemplateNode::new("Column");
        root.props.push(("spacing".into(), PropValue::Static(StaticValue::Float(4.0))));
        let key = TemplateKey::new("src/sock_demo.rs", 2, 1);
        template::register(Template::new(key.clone(), root));

        // Start the device-side listener on an ephemeral port.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || serve_on_listener(listener));

        // Push a framed edit: the file path + full source (view! on line 2),
        // spacing 4.0 → 40.0.
        let src = "// line 1\nfn v() { let _ = view! { Column { spacing: 40.0 } }; }\n";
        let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write_frame(&mut client, &format!("src/sock_demo.rs\n{src}")).unwrap();

        // Wait for the swap to land in the live registry.
        let mut applied = false;
        for _ in 0..100 {
            if let Some(t) = template::get(&key) {
                if t.root.props[0].1 == PropValue::Static(StaticValue::Float(40.0)) {
                    applied = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(applied, "a socket-pushed edit should hot-swap the registered site");
    }
}
