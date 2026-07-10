use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use super::workspace::CommandResult;

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotOptions {
    /// The example binary to run (e.g. "phase10_demo").
    pub example: String,
    /// Directory to copy the snapshot PNG into.
    pub out_dir: String,
    /// Cargo package that owns the example (default: "rosace-examples").
    pub package: String,
}

impl SnapshotOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }
        let mut example = None;
        let mut out_dir = "snapshots".to_string();
        let mut package = "rosace-examples".to_string();

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--example" if i + 1 < args.len() => {
                    example = Some(args[i + 1].clone());
                    i += 2;
                }
                "--out" if i + 1 < args.len() => {
                    out_dir = args[i + 1].clone();
                    i += 2;
                }
                "--package" if i + 1 < args.len() => {
                    package = args[i + 1].clone();
                    i += 2;
                }
                other if other.starts_with("--example=") => {
                    example = Some(other.trim_start_matches("--example=").to_string());
                    i += 1;
                }
                other if other.starts_with("--out=") => {
                    out_dir = other.trim_start_matches("--out=").to_string();
                    i += 1;
                }
                _ => { i += 1; }
            }
        }

        let example = example.ok_or_else(|| "missing --example <name>".to_string())?;
        Ok(Self { example, out_dir, package })
    }
}

pub fn print_help() {
    println!("rsc snapshot — run an example binary and save its PNG output");
    println!();
    println!("USAGE:");
    println!("  rsc snapshot --example <name> [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --example <name>   Example binary to run (required)");
    println!("  --out <dir>        Directory to copy the snapshot PNG into (default: snapshots)");
    println!("  --package <name>   Cargo package that owns the example (default: rosace-examples)");
    println!("  -h, --help         Print this message");
}

pub fn run_snapshot(opts: &SnapshotOptions) -> CommandResult {
    let label = format!("rsc snapshot --example {}", opts.example);
    let start = Instant::now();

    // Run the example binary
    let status = Command::new("cargo")
        .args(["run", "-p", &opts.package, "--bin", &opts.example, "--release"])
        .status();

    let success = status.map(|s| s.success()).unwrap_or(false);
    let duration_ms = start.elapsed().as_millis() as u64;

    if !success {
        return CommandResult {
            command: label,
            exit_code: 1,
            stdout: String::new(),
            stderr: format!("cargo run failed for example '{}'", opts.example),
            duration_ms,
            success: false,
        };
    }

    // Copy the PNG to out_dir
    let png_name = format!("{}.png", opts.example);
    let src = Path::new(&png_name);
    let copy_result = if src.exists() {
        fs::create_dir_all(&opts.out_dir)
            .and_then(|_| fs::copy(src, Path::new(&opts.out_dir).join(&png_name)))
            .map(|_| ())
    } else {
        Ok(()) // example may not produce a PNG — not an error
    };

    let (exit_code, stdout, stderr) = match copy_result {
        Ok(_) => (
            0,
            format!("Saved: {}/{}\n", opts.out_dir, png_name),
            String::new(),
        ),
        Err(e) => (1, String::new(), format!("copy failed: {}", e)),
    };

    CommandResult {
        command: label,
        exit_code,
        stdout,
        stderr,
        duration_ms,
        success: exit_code == 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_options_from_args_example() {
        let args: Vec<String> = vec!["--example".to_string(), "phase10_demo".to_string()];
        let opts = SnapshotOptions::from_args(&args).unwrap();
        assert_eq!(opts.example, "phase10_demo");
    }

    #[test]
    fn snapshot_options_default_out_dir() {
        let args: Vec<String> = vec!["--example".to_string(), "phase10_demo".to_string()];
        let opts = SnapshotOptions::from_args(&args).unwrap();
        assert_eq!(opts.out_dir, "snapshots");
    }

    #[test]
    fn snapshot_options_custom_out_dir() {
        let args: Vec<String> = vec![
            "--example".to_string(), "phase10_demo".to_string(),
            "--out".to_string(), "golden/".to_string(),
        ];
        let opts = SnapshotOptions::from_args(&args).unwrap();
        assert_eq!(opts.out_dir, "golden/");
    }

    #[test]
    fn snapshot_options_eq_syntax() {
        let args: Vec<String> = vec!["--example=phase9_demo".to_string()];
        let opts = SnapshotOptions::from_args(&args).unwrap();
        assert_eq!(opts.example, "phase9_demo");
    }

    #[test]
    fn snapshot_options_missing_example_errors() {
        let result = SnapshotOptions::from_args(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("--example"));
    }

    #[test]
    fn snapshot_options_default_package() {
        let args: Vec<String> = vec!["--example".to_string(), "demo".to_string()];
        let opts = SnapshotOptions::from_args(&args).unwrap();
        assert_eq!(opts.package, "rosace-examples");
    }

    #[test]
    fn snapshot_options_custom_package() {
        let args: Vec<String> = vec![
            "--example".to_string(), "demo".to_string(),
            "--package".to_string(), "my-pkg".to_string(),
        ];
        let opts = SnapshotOptions::from_args(&args).unwrap();
        assert_eq!(opts.package, "my-pkg");
    }
}
