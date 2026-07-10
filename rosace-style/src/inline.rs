use std::collections::HashMap;
use crate::property::StyleProperty;
use crate::value::StyleValue;

/// Per-widget inline styles (highest specificity, applied last).
#[derive(Debug, Clone, Default)]
pub struct InlineStyle {
    properties: HashMap<StyleProperty, StyleValue>,
}

impl InlineStyle {
    pub fn new() -> Self { Self::default() }

    pub fn set(&mut self, prop: StyleProperty, value: StyleValue) -> &mut Self {
        self.properties.insert(prop, value);
        self
    }

    pub fn get(&self, prop: StyleProperty) -> Option<&StyleValue> {
        self.properties.get(&prop)
    }

    pub fn builder(self) -> InlineStyleBuilder { InlineStyleBuilder(self) }

    pub fn len(&self) -> usize { self.properties.len() }
    pub fn is_empty(&self) -> bool { self.properties.is_empty() }
    pub fn properties(&self) -> &HashMap<StyleProperty, StyleValue> { &self.properties }
}

/// Fluent builder for `InlineStyle`.
pub struct InlineStyleBuilder(InlineStyle);

impl InlineStyleBuilder {
    pub fn set(mut self, prop: StyleProperty, value: StyleValue) -> Self {
        self.0.set(prop, value);
        self
    }
    pub fn build(self) -> InlineStyle { self.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_style_set_get() {
        let mut style = InlineStyle::new();
        style.set(StyleProperty::FontSize, StyleValue::px(14.0));
        assert_eq!(style.get(StyleProperty::FontSize), Some(&StyleValue::px(14.0)));
    }

    #[test]
    fn inline_style_is_empty() {
        let style = InlineStyle::new();
        assert!(style.is_empty());
    }

    #[test]
    fn inline_style_len() {
        let mut style = InlineStyle::new();
        style.set(StyleProperty::Color, StyleValue::keyword("red"));
        style.set(StyleProperty::Padding, StyleValue::px(8.0));
        assert_eq!(style.len(), 2);
    }

    #[test]
    fn inline_style_builder() {
        let style = InlineStyle::new()
            .builder()
            .set(StyleProperty::BorderRadius, StyleValue::px(4.0))
            .set(StyleProperty::Opacity, StyleValue::Number(0.8))
            .build();
        assert_eq!(style.len(), 2);
        assert_eq!(style.get(StyleProperty::BorderRadius), Some(&StyleValue::px(4.0)));
    }
}
