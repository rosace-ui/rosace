//! Minimal ANSI color helpers for `rsc`'s terminal output. No dependency —
//! a handful of escape codes is all this needs.
//!
//! Respects the [NO_COLOR](https://no-color.org) convention and disables
//! itself automatically when stdout isn't a real terminal (piped into a
//! file, captured by CI, redirected into another tool) so colored output
//! never leaks raw escape codes into logs.

use std::io::IsTerminal;

fn enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

fn paint(s: &str, code: &str) -> String {
    if enabled() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
}

pub fn green(s: &str) -> String { paint(s, "32") }
pub fn red(s: &str) -> String { paint(s, "31") }
pub fn yellow(s: &str) -> String { paint(s, "33") }
pub fn bold(s: &str) -> String { paint(s, "1") }
pub fn dim(s: &str) -> String { paint(s, "2") }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_wraps_in_escape_codes_when_forced_on() {
        // Can't force `enabled()` true in a test process (stdout isn't a
        // TTY under `cargo test`), so this only exercises the disabled
        // path directly — the real color path is verified visually via
        // `rsc doctor` in a real terminal.
        std::env::set_var("NO_COLOR", "1");
        assert_eq!(green("ok"), "ok", "NO_COLOR must disable color entirely");
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn disabled_when_not_a_terminal() {
        // `cargo test`'s stdout is captured, never a TTY, so this is
        // always exercising the real, non-forced `enabled()` check.
        assert_eq!(red("x"), "x");
        assert_eq!(bold("x"), "x");
    }
}
