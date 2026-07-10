use std::process::Command;
use std::time::Instant;

/// Result of running a rsc workspace command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub success: bool,
}

impl CommandResult {
    pub fn summary(&self) -> String {
        if self.success {
            format!("{} completed in {}ms", self.command, self.duration_ms)
        } else {
            format!("{} failed (exit {})", self.command, self.exit_code)
        }
    }
}

fn run_cargo(args: &[&str], label: &str) -> CommandResult {
    let start = Instant::now();
    let mut cmd = Command::new("cargo");
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().unwrap_or_else(|e| {
        panic!("failed to run cargo: {e}");
    });

    let duration_ms = start.elapsed().as_millis() as u64;
    let exit_code = output.status.code().unwrap_or(-1);

    CommandResult {
        command: label.to_string(),
        exit_code,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        duration_ms,
        success: output.status.success(),
    }
}

/// `rsc check` — runs `cargo check --workspace`
pub fn run_check(verbose: bool) -> CommandResult {
    let args = if verbose {
        vec!["check", "--workspace", "--message-format=short"]
    } else {
        vec!["check", "--workspace"]
    };
    run_cargo(&args, "rsc check")
}

/// `rsc test` — runs `cargo test --workspace`
pub fn run_test(filter: Option<&str>) -> CommandResult {
    let mut args = vec!["test", "--workspace"];
    if let Some(f) = filter {
        args.push("--");
        args.push(f);
    }
    run_cargo(&args, "rsc test")
}

/// `rsc lint` — runs `cargo clippy --workspace -- -D warnings`
pub fn run_lint() -> CommandResult {
    run_cargo(&["clippy", "--workspace", "--", "-D", "warnings"], "rsc lint")
}

/// `rsc fmt` — runs `cargo fmt --workspace --check`
pub fn run_fmt_check() -> CommandResult {
    run_cargo(&["fmt", "--workspace", "--check"], "rsc fmt")
}

// ---------------------------------------------------------------------------
// Tests (unit tests that don't actually run cargo — they test the data model)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(success: bool, code: i32, cmd: &str) -> CommandResult {
        CommandResult {
            command: cmd.to_string(),
            exit_code: code,
            stdout: "stdout".to_string(),
            stderr: "stderr".to_string(),
            duration_ms: 42,
            success,
        }
    }

    #[test]
    fn command_result_success_summary() {
        let r = make_result(true, 0, "rsc check");
        assert!(r.summary().contains("completed"));
        assert!(r.summary().contains("rsc check"));
    }

    #[test]
    fn command_result_failure_summary() {
        let r = make_result(false, 1, "rsc lint");
        assert!(r.summary().contains("failed"));
        assert!(r.summary().contains("rsc lint"));
    }

    #[test]
    fn command_result_fields() {
        let r = make_result(true, 0, "test");
        assert_eq!(r.exit_code, 0);
        assert_eq!(r.duration_ms, 42);
        assert!(r.success);
    }

    #[test]
    fn command_result_failure_fields() {
        let r = make_result(false, 101, "test");
        assert!(!r.success);
        assert_eq!(r.exit_code, 101);
    }
}
