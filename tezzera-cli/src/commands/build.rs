use std::process::Command;

/// Target platform for the build.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildTarget {
    /// Native desktop build (release mode).
    Desktop,
    /// WebAssembly build for the browser.
    Web,
}

/// Options parsed from `tzr build --target <target>`.
#[derive(Debug)]
pub struct BuildOptions {
    /// Which platform to build for.
    pub target: BuildTarget,
}

impl BuildOptions {
    /// Build `BuildOptions` from the CLI arguments that follow `build`.
    ///
    /// Accepts both `--target desktop` (space-separated) and
    /// `--target=desktop` (equals-separated) forms.
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }

        let target_str = args
            .iter()
            .find(|a| a.starts_with("--target"))
            .and_then(|a| {
                // Handle "--target=desktop"
                if a.contains('=') {
                    a.split('=').nth(1).map(str::to_string)
                } else {
                    None
                }
            })
            .or_else(|| {
                // "--target desktop" (space-separated)
                let pos = args.iter().position(|a| *a == "--target")?;
                args.get(pos + 1).cloned()
            });

        let target = match target_str.as_deref() {
            Some("desktop") => BuildTarget::Desktop,
            Some("web") => BuildTarget::Web,
            Some(other) => {
                return Err(format!(
                    "unknown target '{}'. Supported: desktop, web",
                    other
                ))
            }
            None => {
                return Err(
                    "--target is required. Usage: tzr build --target desktop | web".to_string(),
                )
            }
        };

        Ok(Self { target })
    }
}

pub fn print_help() {
    println!("tzr build — release build for a target platform");
    println!();
    println!("USAGE:");
    println!("  tzr build --target <target>");
    println!();
    println!("OPTIONS:");
    println!("  --target <target>   desktop | web");
    println!("  -h, --help          Print this message");
}

/// Run the build for the given options.
///
/// Returns `Ok(())` on success, or an error message on failure.
pub fn run(opts: BuildOptions) -> Result<(), String> {
    match opts.target {
        BuildTarget::Desktop => build_desktop(),
        BuildTarget::Web => build_web(),
    }
}

fn build_desktop() -> Result<(), String> {
    println!("Building TEZZERA for desktop (release)...");

    let status = Command::new("cargo")
        .args(["build", "--release", "--workspace"])
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;

    if status.success() {
        println!();
        println!("Build complete.");
        println!("Binary: target/release/tzr");
        Ok(())
    } else {
        Err("cargo build --release failed".to_string())
    }
}

fn build_web() -> Result<(), String> {
    println!("Building TEZZERA for web (wasm32)...");
    println!();

    // Step 1: ensure wasm32-unknown-unknown target is installed
    let check = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map_err(|e| format!("failed to run rustup: {}", e))?;
    let installed = String::from_utf8_lossy(&check.stdout);
    if !installed.contains("wasm32-unknown-unknown") {
        println!("  Installing wasm32-unknown-unknown target...");
        let status = Command::new("rustup")
            .args(["target", "add", "wasm32-unknown-unknown"])
            .status()
            .map_err(|e| format!("failed to run rustup: {}", e))?;
        if !status.success() {
            return Err("failed to install wasm32 target".to_string());
        }
    }

    // Step 2: cargo build --target wasm32-unknown-unknown --release
    println!("  Compiling to WebAssembly...");
    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-unknown-unknown", "--release"])
        .status()
        .map_err(|e| format!("failed to invoke cargo: {}", e))?;
    if !status.success() {
        return Err("cargo build for wasm32 failed".to_string());
    }

    // Step 3: check whether wasm-bindgen-cli is available
    let wasm_bindgen_available = Command::new("wasm-bindgen")
        .arg("--version")
        .output()
        .is_ok();

    // Step 4: ensure dist/ exists
    std::fs::create_dir_all("dist")
        .map_err(|e| format!("failed to create dist/: {}", e))?;

    if wasm_bindgen_available {
        println!("  Running wasm-bindgen...");
        let wasm_files = glob_wasm_files("target/wasm32-unknown-unknown/release")?;
        if let Some(wasm_path) = wasm_files.first() {
            let status = Command::new("wasm-bindgen")
                .args([
                    wasm_path.as_str(),
                    "--out-dir",
                    "dist",
                    "--target",
                    "web",
                    "--no-typescript",
                ])
                .status()
                .map_err(|e| format!("wasm-bindgen failed: {}", e))?;
            if !status.success() {
                return Err("wasm-bindgen failed".to_string());
            }
        }
    } else {
        println!("  Note: wasm-bindgen not found — copying raw .wasm to dist/");
        println!("  Install with: cargo install wasm-bindgen-cli");
        let wasm_files = glob_wasm_files("target/wasm32-unknown-unknown/release")?;
        if let Some(src) = wasm_files.first() {
            std::fs::copy(src, "dist/app.wasm")
                .map_err(|e| format!("failed to copy wasm: {}", e))?;
        }
    }

    // Step 5: write dist/index.html
    let html = generate_index_html();
    std::fs::write("dist/index.html", html)
        .map_err(|e| format!("failed to write dist/index.html: {}", e))?;

    println!();
    println!("  Build complete → dist/");
    println!("  Serve with:  npx serve dist  or  python3 -m http.server -d dist");
    Ok(())
}

/// Return paths to non-deps `.wasm` files in the given directory.
fn glob_wasm_files(dir: &str) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {}", dir, e))?;
    Ok(entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "wasm").unwrap_or(false))
        .filter(|p| {
            !p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .contains("deps")
        })
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

fn generate_index_html() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>TEZZERA App</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      background: #12121c;
      display: flex;
      align-items: center;
      justify-content: center;
      min-height: 100vh;
    }
    canvas {
      display: block;
      image-rendering: pixelated;
    }
  </style>
</head>
<body>
  <canvas id="tezzera-canvas"></canvas>
  <script type="module">
    // If wasm-bindgen output exists, use it; otherwise load raw wasm
    try {
      const { default: init } = await import('./app.js');
      await init();
    } catch (e) {
      // Fallback: raw wasm (no wasm-bindgen)
      const response = await fetch('app.wasm');
      const bytes = await response.arrayBuffer();
      await WebAssembly.instantiate(bytes, {});
    }
  </script>
</body>
</html>
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_opts_parses_desktop_target() {
        let args = vec!["--target".to_string(), "desktop".to_string()];
        let opts = BuildOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, BuildTarget::Desktop);
    }

    #[test]
    fn build_opts_parses_desktop_target_eq_form() {
        let args = vec!["--target=desktop".to_string()];
        let opts = BuildOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, BuildTarget::Desktop);
    }

    #[test]
    fn build_opts_errors_on_missing_target() {
        let args = vec![];
        assert!(BuildOptions::from_args(&args).is_err());
    }

    #[test]
    fn build_opts_errors_on_unknown_target() {
        let args = vec!["--target=ios".to_string()];
        assert!(BuildOptions::from_args(&args).is_err());
    }

    #[test]
    fn build_opts_parses_web_target() {
        let args = vec!["--target".to_string(), "web".to_string()];
        let opts = BuildOptions::from_args(&args).unwrap();
        assert_eq!(opts.target, BuildTarget::Web);
    }

    #[test]
    fn build_opts_errors_on_unknown_target_mentions_web() {
        let args = vec!["--target=ios".to_string()];
        let err = BuildOptions::from_args(&args).unwrap_err();
        assert!(err.contains("web"), "error should mention web: {}", err);
    }

    #[test]
    fn index_html_contains_canvas() {
        let html = generate_index_html();
        assert!(html.contains("tezzera-canvas"));
        assert!(html.contains("<canvas"));
    }
}
