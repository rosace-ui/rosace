//! Applying a hot-swap (D103 / D102 Tier 1 — rollout step 4).
//!
//! The watcher hands an edited [`Template`] (from `parse_file_templates`) to
//! [`apply_swap`], which is the one place the diff safety-gate meets the live
//! registry. If the edit is a safe data swap, it REPLACES the site's registry
//! entry; the next frame's `view!` inflates the new descriptor with that
//! frame's compiled holes (no in-place tree surgery — the reactive rebuild does
//! the work). If the edit touched compiled logic, it escalates instead, leaving
//! the running descriptor untouched so nothing breaks before a Tier 0 restart.

use super::diff::{diff, EscalationReason, TemplateDiff};
use super::registry;
use super::Template;

/// The result of trying to apply an edited template to the running app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapOutcome {
    /// Safe data edit — the registry entry was replaced; next frame shows it.
    Applied,
    /// The edit didn't change the shape — nothing to do.
    Unchanged,
    /// The edit touched compiled logic — registry left as-is; the caller must
    /// escalate (Tier 2 dylib swap or Tier 0 restart).
    Escalate(EscalationReason),
    /// No running template for this site key — nothing to swap against (a new
    /// `view!`, or a key that never registered). Also an escalation.
    UnknownSite,
}

/// Diff an edited template against the running one and, if it is a safe data
/// swap, install it. Keyed by `new.key`.
pub fn apply_swap(new: Template) -> SwapOutcome {
    match registry::get(&new.key) {
        None => SwapOutcome::UnknownSite,
        Some(running) => match diff(&running, &new) {
            TemplateDiff::Unchanged => SwapOutcome::Unchanged,
            TemplateDiff::Swappable => {
                registry::register(new);
                SwapOutcome::Applied
            }
            TemplateDiff::Escalate(reason) => SwapOutcome::Escalate(reason),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{registry, PropValue, StaticValue, Template, TemplateKey, TemplateNode};

    fn col(key: &TemplateKey, spacing: PropValue) -> Template {
        Template::new(key.clone(), TemplateNode::new("Column").with_prop("spacing", spacing))
    }

    // Small helper on TemplateNode-by-value for the tests.
    trait WithProp {
        fn with_prop(self, k: &str, v: PropValue) -> Self;
    }
    impl WithProp for TemplateNode {
        fn with_prop(mut self, k: &str, v: PropValue) -> Self {
            self.props.push((k.to_string(), v));
            self
        }
    }

    #[test]
    fn safe_static_edit_is_applied_and_replaces_the_registry_entry() {
        let key = TemplateKey::new("src/swap_a.rs", 1, 1);
        registry::register(col(&key, PropValue::Static(StaticValue::Float(4.0))));

        let edited = col(&key, PropValue::Static(StaticValue::Float(40.0)));
        assert_eq!(apply_swap(edited), SwapOutcome::Applied);

        // The registry now holds the edited value.
        let now = registry::get(&key).unwrap();
        assert_eq!(now.root.props[0].1, PropValue::Static(StaticValue::Float(40.0)));
    }

    #[test]
    fn adding_a_hole_escalates_and_leaves_the_registry_untouched() {
        let key = TemplateKey::new("src/swap_b.rs", 2, 1);
        registry::register(col(&key, PropValue::Static(StaticValue::Float(4.0))));

        // Edit turns spacing into a hole → hole count 0→1 → escalate.
        let edited = col(&key, PropValue::Hole(0));
        assert_eq!(
            apply_swap(edited),
            SwapOutcome::Escalate(EscalationReason::HoleCountChanged { old: 0, new: 1 })
        );
        // Running descriptor unchanged.
        assert_eq!(
            registry::get(&key).unwrap().root.props[0].1,
            PropValue::Static(StaticValue::Float(4.0))
        );
    }

    #[test]
    fn unknown_site_is_reported() {
        let key = TemplateKey::new("src/never_registered_swap.rs", 99, 1);
        assert_eq!(apply_swap(col(&key, PropValue::Static(StaticValue::Float(1.0)))), SwapOutcome::UnknownSite);
    }

    #[test]
    fn identical_edit_is_unchanged() {
        let key = TemplateKey::new("src/swap_c.rs", 3, 1);
        registry::register(col(&key, PropValue::Static(StaticValue::Float(4.0))));
        assert_eq!(apply_swap(col(&key, PropValue::Static(StaticValue::Float(4.0)))), SwapOutcome::Unchanged);
    }
}
