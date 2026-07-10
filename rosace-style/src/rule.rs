use std::collections::HashMap;
use crate::property::StyleProperty;
use crate::selector::Selector;
use crate::value::StyleValue;

/// A single CSS-like rule: a selector + a block of property declarations.
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub properties: HashMap<StyleProperty, StyleValue>,
}

impl StyleRule {
    pub fn new(selector: Selector) -> Self {
        Self { selector, properties: HashMap::new() }
    }

    pub fn set(mut self, prop: StyleProperty, value: StyleValue) -> Self {
        self.properties.insert(prop, value);
        self
    }

    pub fn get(&self, prop: StyleProperty) -> Option<&StyleValue> {
        self.properties.get(&prop)
    }

    pub fn property_count(&self) -> usize { self.properties.len() }

    pub fn matches(&self, selector: &Selector) -> bool {
        self.selector.matches(selector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_new() {
        let rule = StyleRule::new(Selector::class("btn"));
        assert_eq!(rule.property_count(), 0);
    }

    #[test]
    fn rule_set_and_get() {
        let rule = StyleRule::new(Selector::class("btn"))
            .set(StyleProperty::Padding, StyleValue::px(8.0));
        assert_eq!(rule.get(StyleProperty::Padding), Some(&StyleValue::px(8.0)));
    }

    #[test]
    fn rule_property_count() {
        let rule = StyleRule::new(Selector::Any)
            .set(StyleProperty::Color, StyleValue::keyword("red"))
            .set(StyleProperty::Padding, StyleValue::px(4.0));
        assert_eq!(rule.property_count(), 2);
    }

    #[test]
    fn rule_matches_selector() {
        let rule = StyleRule::new(Selector::class("card"));
        assert!(rule.matches(&Selector::class("card")));
    }

    #[test]
    fn rule_no_match_wrong_selector() {
        let rule = StyleRule::new(Selector::class("card"));
        assert!(!rule.matches(&Selector::class("btn")));
    }

    #[test]
    fn rule_chaining() {
        let rule = StyleRule::new(Selector::id("header"))
            .set(StyleProperty::Background, StyleValue::keyword("blue"))
            .set(StyleProperty::Height, StyleValue::px(60.0))
            .set(StyleProperty::Width, StyleValue::percent(100.0));
        assert_eq!(rule.property_count(), 3);
        assert!(rule.get(StyleProperty::Height).is_some());
    }
}
