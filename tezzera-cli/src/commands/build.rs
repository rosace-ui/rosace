use std::process::Command;

/// Target platform for the build.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildTarget {
    /// Native desktop build (release mode).
    Desktop,
    // Web, iOS, Android — Phase 2+
}

/// Options parsed from `tzr build --target <target>`.
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
            Some(other) => {
                return Err(format!(
                    "unknown target: '{}'. Phase 1 supports: desktop",
                    other
                ))
            }
            None => {
                return Err(
                    "--target is required. Usage: tzr build --target desktop".to_string(),
                )
            }
        };

        Ok(Self { target })
    }
}

/// Run the build for the given options.
///
/// Returns `Ok(())` on success, or an error message on failure.
pub fn run(opts: BuildOptions) -> Result<(), String> {
    match opts.target {
        BuildTarget::Desktop => build_desktop(),
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
        let args = vec!["--target=wasm".to_string()];
        assert!(BuildOptions::from_args(&args).is_err());
    }
}
