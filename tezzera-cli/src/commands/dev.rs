use std::sync::Arc;

use tezzera_trace::bus::TRACING_BUS;
use tezzera_trace::subscribers::console::{ConsoleFilter, ConsoleSubscriber};

/// Options parsed from `tzr dev [--trace=<filter>]`.
pub struct DevOptions {
    /// Which event category to display in the terminal.
    pub trace_filter: ConsoleFilter,
}

impl DevOptions {
    /// Build `DevOptions` from the CLI arguments that follow `dev`.
    pub fn from_args(args: &[String]) -> Self {
        let filter = args
            .iter()
            .find(|a| a.starts_with("--trace="))
            .map(|a| parse_filter(a.trim_start_matches("--trace=")))
            .unwrap_or(ConsoleFilter::All);
        Self { trace_filter: filter }
    }
}

fn parse_filter(s: &str) -> ConsoleFilter {
    match s {
        "state" => ConsoleFilter::State,
        "network" => ConsoleFilter::Network,
        "performance" => ConsoleFilter::Performance,
        "all" => ConsoleFilter::All,
        name => ConsoleFilter::Component(name.to_string()),
    }
}

/// Run the dev server.
///
/// Registers a `ConsoleSubscriber` with the chosen filter, prints a startup
/// banner, then blocks until the process is interrupted (Ctrl+C).  This
/// function intentionally never calls `process::exit` — the caller controls
/// process lifetime.
pub fn run(opts: DevOptions) {
    // Register console subscriber with the chosen filter.
    let subscriber = Arc::new(ConsoleSubscriber::with_filter(opts.trace_filter));
    TRACING_BUS.add_subscriber(subscriber);

    println!("╔══════════════════════════════════════╗");
    println!("║  TEZZERA dev  —  Fast by nature.     ║");
    println!("╚══════════════════════════════════════╝");
    println!();
    println!("  Trace output active. Waiting for app events...");
    println!();
    println!("  Phase 1: pixel buffer rendering (no window yet).");
    println!("  Run the counter example with: cargo run -p tezzera-widgets");
    println!();
    println!("  Press Ctrl+C to stop.");

    // In Phase 1, there is no event loop. Block until Ctrl+C.
    // Use a simple loop with a sleep so the process stays alive and
    // trace events from other threads can be observed.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_trace::subscribers::console::ConsoleFilter;

    #[test]
    fn dev_opts_default_filter_is_all() {
        let opts = DevOptions::from_args(&[]);
        assert_eq!(opts.trace_filter, ConsoleFilter::All);
    }

    #[test]
    fn dev_opts_parses_trace_state() {
        let args = vec!["--trace=state".to_string()];
        let opts = DevOptions::from_args(&args);
        assert_eq!(opts.trace_filter, ConsoleFilter::State);
    }

    #[test]
    fn dev_opts_parses_trace_performance() {
        let args = vec!["--trace=performance".to_string()];
        let opts = DevOptions::from_args(&args);
        assert_eq!(opts.trace_filter, ConsoleFilter::Performance);
    }
}
