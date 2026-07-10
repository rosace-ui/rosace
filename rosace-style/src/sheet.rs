use crate::rule::StyleRule;
use crate::selector::Selector;
use crate::property::StyleProperty;
use crate::value::StyleValue;

/// A collection of `StyleRule`s — the equivalent of a CSS stylesheet.
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    rules: Vec<StyleRule>,
}

impl StyleSheet {
    pub fn new() -> Self { Self::default() }

    pub fn add_rule(&mut self, rule: StyleRule) { self.rules.push(rule); }

    /// Return all rules whose selector matches `selector`.
    pub fn rules_for(&self, selector: &Selector) -> Vec<&StyleRule> {
        self.rules.iter().filter(|r| r.matches(selector)).collect()
    }

    /// Merge another stylesheet's rules into this one (appended, lower priority).
    pub fn merge(&mut self, other: &StyleSheet) {
        self.rules.extend(other.rules.iter().cloned());
    }

    /// Look up a specific property across all matching rules (last match wins).
    pub fn resolve(&self, selector: &Selector, prop: StyleProperty) -> Option<&StyleValue> {
        self.rules_for(selector)
            .into_iter()
            .filter_map(|r| r.get(prop))
            .next_back()
    }

    pub fn rule_count(&self) -> usize { self.rules.len() }
    pub fn is_empty(&self) -> bool { self.rules.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_new_empty() {
        let sheet = StyleSheet::new();
        assert!(sheet.is_empty());
        assert_eq!(sheet.rule_count(), 0);
    }

    #[test]
    fn sheet_add_rule() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(StyleRule::new(Selector::class("btn")));
        assert_eq!(sheet.rule_count(), 1);
        assert!(!sheet.is_empty());
    }

    #[test]
    fn sheet_rules_for_matching() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::class("btn"))
                .set(StyleProperty::Padding, StyleValue::px(8.0)),
        );
        let matches = sheet.rules_for(&Selector::class("btn"));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn sheet_rules_for_no_match() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(StyleRule::new(Selector::class("btn")));
        let matches = sheet.rules_for(&Selector::class("card"));
        assert!(matches.is_empty());
    }

    #[test]
    fn sheet_resolve_last_wins() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::class("btn"))
                .set(StyleProperty::Padding, StyleValue::px(4.0)),
        );
        sheet.add_rule(
            StyleRule::new(Selector::class("btn"))
                .set(StyleProperty::Padding, StyleValue::px(12.0)),
        );
        let resolved = sheet.resolve(&Selector::class("btn"), StyleProperty::Padding);
        assert_eq!(resolved, Some(&StyleValue::px(12.0)));
    }

    #[test]
    fn sheet_merge() {
        let mut sheet_a = StyleSheet::new();
        sheet_a.add_rule(StyleRule::new(Selector::class("a")));

        let mut sheet_b = StyleSheet::new();
        sheet_b.add_rule(StyleRule::new(Selector::class("b")));

        sheet_a.merge(&sheet_b);
        assert_eq!(sheet_a.rule_count(), 2);
    }

    #[test]
    fn sheet_rule_count() {
        let mut sheet = StyleSheet::new();
        for i in 0..5 {
            sheet.add_rule(StyleRule::new(Selector::class(&format!("cls-{}", i))));
        }
        assert_eq!(sheet.rule_count(), 5);
    }

    #[test]
    fn sheet_is_empty() {
        let sheet = StyleSheet::new();
        assert!(sheet.is_empty());
    }
}
