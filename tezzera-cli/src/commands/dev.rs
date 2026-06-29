use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;

/// Target platform for the dev server.
#[derive(Debug, Clone, PartialEq)]
pub enum DevTarget {
    Desktop,
    Web,
}

/// Options parsed from `tzr dev [--target <target>] [--port <n>] [--watch] [--bin <name>]`.
#[derive(Debug, Clone)]
pub struct DevOptions {
    pub target: DevTarget,
    pub port: u16,
    pub watch: bool,
    /// Optional binary name (for workspaces with multiple `[[bin]]` entries).
    pub bin: Option<String>,
}

impl DevOptions {
    /// Build `DevOptions` from the CLI arguments that follow `dev`.
    ///
    /// Accepts both space-separated (`--target web`) and equals-separated
    /// (`--target=web`) forms.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let mut target = DevTarget::Desktop;
        let mut port = 3000u16;
        let mut watch = false;
        let mut bin: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--target" if i + 1 < args.len() => {
                    target = match args[i + 1].as_str() {
                        "desktop" => DevTarget::Desktop,
                        "web" => DevTarget::Web,
                        other => {
                            return Err(format!(
                                "unknown target '{}'. Supported: desktop, web",
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
                other if other.starts_with("--target=") => {
                    let t = other.trim_start_matches("--target=");
                    target = match t {
                        "desktop" => DevTarget::Desktop,
                        "web" => DevTarget::Web,
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

        Ok(Self { target, port, watch, bin })
    }
}

/// Run the dev command.
pub fn run(opts: DevOptions) -> Result<(), String> {
    if opts.watch {
        start_watcher(&opts);
    }
    match opts.target {
        DevTarget::Desktop => run_desktop(opts.bin.as_deref()),
        DevTarget::Web => run_web(opts.port),
    }
}

fn start_watcher(opts: &DevOptions) {
    use tezzera_hot_reload::{FileWatcher, RebuildRunner, RebuildTarget};

    let rebuild_target = match opts.target {
        DevTarget::Desktop => RebuildTarget::Desktop,
        DevTarget::Web => RebuildTarget::Web,
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
    println!("Starting TEZZERA dev server (desktop)...");
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
    let status = cmd
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;
    if !status.success() {
        return Err("cargo run failed".to_string());
    }
    Ok(())
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
            eprintln!("  Example: tzr dev --bin {}", extract_first_bin_name(&cargo_toml).unwrap_or("my-app".to_string()));
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
            if let Some(val) = t.splitn(2, '=').nth(1) {
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
    println!("Starting TEZZERA dev server (web)...");
    println!();

    // Step 1: build WASM
    println!("  Building WASM...");
    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-unknown-unknown"])
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

    // Step 3: start HTTP server
    let addr = format!("127.0.0.1:{}", port);
    let listener =
        TcpListener::bind(&addr).map_err(|e| format!("cannot bind to {}: {}", addr, e))?;

    println!();
    println!("  TEZZERA dev server running at http://localhost:{}", port);
    println!("  Serving: dist/");
    println!("  Press Ctrl+C to stop.");
    println!();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(e) = handle_request(stream) {
                    eprintln!("  request error: {}", e);
                }
            }
            Err(e) => eprintln!("  connection error: {}", e),
        }
    }

    Ok(())
}

fn handle_request(mut stream: TcpStream) -> Result<(), String> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| format!("read: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse: "GET /path HTTP/1.1"
    let path = parse_path(&request);
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
  <title>TEZZERA App</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { background: #12121c; display: flex; align-items: center; justify-content: center; min-height: 100vh; }
    canvas { display: block; image-rendering: pixelated; }
    .hint { color: #6750A4; font-family: monospace; margin-top: 16px; text-align: center; }
  </style>
</head>
<body>
  <div>
    <canvas id="tezzera-canvas"></canvas>
    <p class="hint">Run <code>tzr build --target web</code> to compile your app.</p>
  </div>
</body>
</html>"#
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
        let args = vec!["--target".to_string(), "ios".to_string()];
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
        assert!(default_index_html().contains("tezzera-canvas"));
    }
}
