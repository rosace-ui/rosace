//! The reload runtime (D103 / D102 Tier 1 — rollout step 4, the watcher's core).
//!
//! Given an edited `.rs` file's path + source, [`apply_reload`] re-parses every
//! `view!` in it, matches each to the running site (line + path-suffix — see
//! [`registry::find_running`], tolerating the macro-vs-scanner key differences),
//! diffs it, and applies safe swaps to the registry. It returns a
//! [`ReloadReport`] the watcher uses to decide: keep running (all swapped),
//! ignore (unparseable mid-edit), or escalate to a Tier 0 restart (a change
//! touched compiled logic, or a brand-new `view!` site appeared).
//!
//! This is the whole in-process reload decision — pure over `(file, src)`, so
//! it is fully testable without a window. Wiring it to a file watcher +
//! repaint is the only remaining integration.

use super::{apply_swap, parse_file_templates, registry, EscalationReason, SwapOutcome, Template};

/// Outcome of reloading one edited file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReloadReport {
    /// Sites hot-swapped (shape changed, safe).
    pub applied: usize,
    /// Sites whose shape did not change.
    pub unchanged: usize,
    /// `view!` sites with no matching running site — a new `view!` (new
    /// compiled code) → needs a restart to take effect.
    pub unknown: usize,
    /// Sites that changed in a way needing compiled code (Tier 2/0).
    pub escalations: Vec<EscalationReason>,
    /// The file didn't parse (mid-edit) — do nothing, wait for the next save.
    pub parse_error: Option<String>,
}

impl ReloadReport {
    /// A safe data reload happened — repaint, don't restart.
    pub fn hot_swapped(&self) -> bool {
        self.applied > 0 && !self.needs_restart() && self.parse_error.is_none()
    }
    /// Something needs compiled code — the watcher should fall back to a Tier 0
    /// rebuild + restart.
    pub fn needs_restart(&self) -> bool {
        !self.escalations.is_empty() || self.unknown > 0
    }
    /// The edit couldn't be parsed (likely mid-typing) — ignore it.
    pub fn ignored(&self) -> bool {
        self.parse_error.is_some()
    }
}

/// Re-parse an edited file and apply safe swaps to the running app's registry.
pub fn apply_reload(file: &str, src: &str) -> ReloadReport {
    let mut report = ReloadReport::default();

    let scanned = match parse_file_templates(src, file) {
        Ok(v) => v,
        Err(e) => {
            report.parse_error = Some(e.to_string());
            return report;
        }
    };

    for site in scanned {
        match registry::find_running(file, site.key.line) {
            None => report.unknown += 1,
            Some(running) => {
                // Re-key the edited template to the running site's key so the
                // diff/registry operate on the same entry.
                let candidate = Template::new(running.key.clone(), site.root);
                match apply_swap(candidate) {
                    SwapOutcome::Applied => report.applied += 1,
                    SwapOutcome::Unchanged => report.unchanged += 1,
                    SwapOutcome::Escalate(reason) => report.escalations.push(reason),
                    SwapOutcome::UnknownSite => report.unknown += 1,
                }
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{registry, PropValue, StaticValue, Template, TemplateKey, TemplateNode};

    fn baseline(file: &str, line: u32, col: u32, spacing: PropValue) -> Template {
        let mut root = TemplateNode::new("Column");
        root.props.push(("spacing".into(), spacing));
        Template::new(TemplateKey::new(file, line, col), root)
    }

    #[test]
    fn safe_edit_swaps_despite_different_path_form_and_column() {
        // Baseline as the macro would register it: package-relative file, a
        // column! column. The view! sits on line 3.
        registry::register(baseline(
            "reload_pkg/src/app.rs",
            3,
            17,
            PropValue::Static(StaticValue::Float(4.0)),
        ));

        // Edited file: ABSOLUTE path, and the scanner's own column — must still
        // match by (line, path-suffix). view! is on line 3.
        let src = "// l1\n// l2\nfn v() { let _ = view! { Column { spacing: 40.0 } }; }\n";
        let report = apply_reload("/Users/x/reload_pkg/src/app.rs", src);

        assert_eq!(report.applied, 1, "should hot-swap the matched site: {report:?}");
        assert!(report.hot_swapped());
        assert!(!report.needs_restart());
        // Registry entry (keyed by the BASELINE key) now holds the edit.
        let now = registry::get(&TemplateKey::new("reload_pkg/src/app.rs", 3, 17)).unwrap();
        assert_eq!(now.root.props[0].1, PropValue::Static(StaticValue::Float(40.0)));
    }

    #[test]
    fn a_logic_change_reports_needs_restart() {
        registry::register(baseline(
            "reload_pkg/src/b.rs",
            1,
            9,
            PropValue::Static(StaticValue::Float(4.0)),
        ));
        // spacing becomes a hole → hole count 0→1 → escalation.
        let src = "fn v() { let _ = view! { Column { spacing: g } }; }\n";
        let report = apply_reload("reload_pkg/src/b.rs", src);
        assert_eq!(report.applied, 0);
        assert!(report.needs_restart(), "a new hole must force a restart: {report:?}");
    }

    #[test]
    fn a_new_view_site_is_unknown_and_needs_restart() {
        // Nothing registered for this file/line.
        let src = "fn v() { let _ = view! { Row { } }; }\n";
        let report = apply_reload("reload_pkg/src/brand_new.rs", src);
        assert_eq!(report.unknown, 1);
        assert!(report.needs_restart());
    }

    #[test]
    fn an_unparseable_edit_is_ignored_not_restarted() {
        let src = "fn v() { let _ = view! { Column { : : : } }; }\n";
        let report = apply_reload("reload_pkg/src/c.rs", src);
        assert!(report.ignored());
        assert!(!report.needs_restart(), "mid-edit garbage should be ignored, not restarted");
    }
}
