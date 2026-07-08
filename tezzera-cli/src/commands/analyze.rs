use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzeOptions {
    pub verbose: bool,
    /// Path to workspace root (default: current dir).
    pub workspace_dir: String,
}

impl AnalyzeOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }
        let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
        let workspace_dir = args.windows(2)
            .find(|w| w[0] == "--dir")
            .map(|w| w[1].clone())
            .unwrap_or_else(|| ".".to_string());
        Ok(Self { verbose, workspace_dir })
    }
}

pub fn print_help() {
    println!("tzr analyze — workspace health: crate count, member list");
    println!();
    println!("USAGE:");
    println!("  tzr analyze [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  --verbose, -v   Also print the member crate list");
    println!("  --dir <path>    Workspace root (default: current directory)");
    println!("  -h, --help      Print this message");
}

/// Workspace analysis report.
#[derive(Debug, Clone)]
pub struct AnalyzeReport {
    pub workspace_name: String,
    pub member_count:   usize,
    pub members:        Vec<String>,
}

impl AnalyzeReport {
    pub fn summary(&self) -> String {
        format!(
            "Workspace: {}\n  Crates:  {}\n  Status:  OK",
            self.workspace_name, self.member_count
        )
    }

    pub fn member_list(&self) -> String {
        self.members.join(", ")
    }
}

/// Parse `members = [...]` from a Cargo.toml string.
/// Returns crate names (path basenames) in order.
pub fn parse_members(toml: &str) -> Vec<String> {
    let start = match toml.find("members") {
        Some(i) => i,
        None    => return vec![],
    };
    let rest = &toml[start..];
    let open = match rest.find('[') {
        Some(i) => i + 1,
        None    => return vec![],
    };
    let close = match rest[open..].find(']') {
        Some(i) => open + i,
        None    => return vec![],
    };
    let inner = &rest[open..close];
    inner
        .split(',')
        .filter_map(|s| {
            let s = s.trim().trim_matches('"').trim_matches('\'').trim();
            if s.is_empty() { return None; }
            // Extract basename from path like "tezzera-core" or "../tezzera-core"
            Some(s.trim_start_matches("../").split('/').next_back().unwrap_or(s).to_string())
        })
        .collect()
}

pub fn run_analyze(opts: &AnalyzeOptions) -> Result<AnalyzeReport, String> {
    let cargo_path = Path::new(&opts.workspace_dir).join("Cargo.toml");
    let toml = fs::read_to_string(&cargo_path)
        .map_err(|e| format!("cannot read {}: {}", cargo_path.display(), e))?;

    let members = parse_members(&toml);
    let name = extract_workspace_name(&toml)
        .unwrap_or_else(|| "tezzera".to_string());

    Ok(AnalyzeReport {
        workspace_name: name,
        member_count:   members.len(),
        members,
    })
}

fn extract_workspace_name(toml: &str) -> Option<String> {
    // Look for [workspace.package] name = "..."
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with("name") && line.contains('=') {
            let val = line.split_once('=')?.1
                .trim()
                .trim_matches('"')
                .to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
[workspace]
members = [
    "tezzera-core",
    "tezzera-state",
    "tezzera-render",
]
resolver = "2"

[workspace.package]
name = "tezzera"
version = "0.1.0"
"#;

    #[test]
    fn parse_members_extracts_three() {
        let members = parse_members(FIXTURE);
        assert_eq!(members.len(), 3);
    }

    #[test]
    fn parse_members_correct_names() {
        let members = parse_members(FIXTURE);
        assert!(members.contains(&"tezzera-core".to_string()));
        assert!(members.contains(&"tezzera-state".to_string()));
        assert!(members.contains(&"tezzera-render".to_string()));
    }

    #[test]
    fn parse_members_empty_when_no_members_key() {
        let members = parse_members("[workspace]\nresolver=\"2\"");
        assert!(members.is_empty());
    }

    #[test]
    fn parse_members_single() {
        let members = parse_members("members = [\"tezzera-core\"]");
        assert_eq!(members, vec!["tezzera-core"]);
    }

    #[test]
    fn parse_members_strips_path_prefix() {
        let members = parse_members("members = [\"../tezzera-core\"]");
        assert_eq!(members, vec!["tezzera-core"]);
    }

    #[test]
    fn analyze_report_summary_contains_name() {
        let report = AnalyzeReport {
            workspace_name: "tezzera".to_string(),
            member_count:   3,
            members:        vec!["a".to_string()],
        };
        assert!(report.summary().contains("tezzera"));
        assert!(report.summary().contains("3"));
    }

    #[test]
    fn analyze_report_member_list() {
        let report = AnalyzeReport {
            workspace_name: "tezzera".to_string(),
            member_count:   2,
            members:        vec!["a".to_string(), "b".to_string()],
        };
        assert_eq!(report.member_list(), "a, b");
    }

    #[test]
    fn analyze_options_verbose_flag() {
        let args: Vec<String> = vec!["--verbose".to_string()];
        let opts = AnalyzeOptions::from_args(&args).unwrap();
        assert!(opts.verbose);
    }

    #[test]
    fn analyze_options_default_not_verbose() {
        let opts = AnalyzeOptions::from_args(&[]).unwrap();
        assert!(!opts.verbose);
    }

    #[test]
    fn analyze_options_short_verbose() {
        let args: Vec<String> = vec!["-v".to_string()];
        let opts = AnalyzeOptions::from_args(&args).unwrap();
        assert!(opts.verbose);
    }
}
