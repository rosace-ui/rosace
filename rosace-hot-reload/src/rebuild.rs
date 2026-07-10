use std::process::Command;
use std::sync::mpsc::Receiver;
use crate::event::ChangeEvent;

/// Listens for `ChangeEvent`s and triggers `cargo build` on each one.
pub struct RebuildRunner {
    pub target: RebuildTarget,
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RebuildTarget {
    Desktop,
    Web,
}

impl Default for RebuildRunner {
    fn default() -> Self {
        Self { target: RebuildTarget::Desktop, package: None }
    }
}

impl RebuildRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn target(mut self, t: RebuildTarget) -> Self { self.target = t; self }
    pub fn package(mut self, p: impl Into<String>) -> Self { self.package = Some(p.into()); self }

    /// Run the rebuild loop — blocks until the receiver is closed.
    pub fn run_loop(&self, rx: Receiver<ChangeEvent>) {
        for event in rx {
            println!("  [hot-reload] {} changed, rebuilding...", event.path.display());
            match self.rebuild() {
                Ok(()) => println!("  [hot-reload] rebuild OK"),
                Err(e) => eprintln!("  [hot-reload] rebuild failed: {}", e),
            }
        }
    }

    fn rebuild(&self) -> Result<(), String> {
        let mut cmd = Command::new("cargo");
        match self.target {
            RebuildTarget::Desktop => {
                cmd.arg("build");
            }
            RebuildTarget::Web => {
                cmd.args(["build", "--target", "wasm32-unknown-unknown"]);
            }
        }
        if let Some(pkg) = &self.package {
            cmd.args(["--package", pkg]);
        }
        let status = cmd.status()
            .map_err(|e| format!("cargo: {}", e))?;
        if status.success() { Ok(()) } else { Err("non-zero exit".to_string()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_runner_default_target_is_desktop() {
        let runner = RebuildRunner::new();
        assert_eq!(runner.target, RebuildTarget::Desktop);
    }

    #[test]
    fn rebuild_runner_target_setter() {
        let runner = RebuildRunner::new().target(RebuildTarget::Web);
        assert_eq!(runner.target, RebuildTarget::Web);
    }

    #[test]
    fn rebuild_runner_package_setter() {
        let runner = RebuildRunner::new().package("rosace-core");
        assert_eq!(runner.package, Some("rosace-core".to_string()));
    }

    #[test]
    fn rebuild_target_eq() {
        assert_eq!(RebuildTarget::Desktop, RebuildTarget::Desktop);
        assert_eq!(RebuildTarget::Web, RebuildTarget::Web);
        assert_ne!(RebuildTarget::Desktop, RebuildTarget::Web);
    }
}
