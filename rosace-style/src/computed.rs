use std::collections::HashMap;
use rosace_theme::Color;
use crate::property::StyleProperty;
use crate::selector::Selector;
use crate::sheet::StyleSheet;
use crate::inline::InlineStyle;
use crate::value::StyleValue;

/// The final resolved style for a widget — stylesheet rules + inline overrides.
#[derive(Debug, Clone, Default)]
pub struct ComputedStyle {
    properties: HashMap<StyleProperty, StyleValue>,
}

impl ComputedStyle {
    pub fn new() -> Self { Self::default() }

    /// Build from a stylesheet (for the given selector) + optional inline style.
    pub fn resolve(sheet: &StyleSheet, selector: &Selector, inline: Option<&InlineStyle>) -> Self {
        let mut properties = HashMap::new();

        // Apply matching stylesheet rules (later rules win)
        for rule in sheet.rules_for(selector) {
            for (prop, val) in &rule.properties {
                properties.insert(*prop, val.clone());
            }
        }

        // Inline styles override everything
        if let Some(inline) = inline {
            for (prop, val) in inline.properties() {
                properties.insert(*prop, val.clone());
            }
        }

        Self { properties }
    }

    pub fn get(&self, prop: StyleProperty) -> Option<&StyleValue> {
        self.properties.get(&prop)
    }

    /// Resolved foreground color.
    pub fn color(&self) -> Option<Color> {
        self.get(StyleProperty::Color)?.as_color()
    }

    /// Resolved background color.
    pub fn background(&self) -> Option<Color> {
        self.get(StyleProperty::Background)?.as_color()
    }

    /// Resolved font size in px.
    pub fn font_size(&self) -> Option<f32> {
        self.get(StyleProperty::FontSize)?.to_px()
    }

    /// Resolved uniform padding in px (from `Padding` property).
    pub fn padding_px(&self) -> Option<f32> {
        self.get(StyleProperty::Padding)?.to_px()
    }

    /// Resolved border radius in px.
    pub fn border_radius(&self) -> Option<f32> {
        self.get(StyleProperty::BorderRadius)?.to_px()
    }

    /// Resolved opacity (0.0–1.0).
    pub fn opacity(&self) -> f32 {
        self.get(StyleProperty::Opacity)
            .and_then(|v| v.to_px())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0)
    }

    pub fn property_count(&self) -> usize { self.properties.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::StyleRule;

    #[test]
    fn computed_resolve_from_sheet() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::class("btn"))
                .set(StyleProperty::Padding, StyleValue::px(12.0)),
        );
        let computed = ComputedStyle::resolve(&sheet, &Selector::class("btn"), None);
        assert_eq!(computed.padding_px(), Some(12.0));
    }

    #[test]
    fn computed_inline_overrides_sheet() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::class("btn"))
                .set(StyleProperty::Padding, StyleValue::px(12.0)),
        );
        let mut inline = InlineStyle::new();
        inline.set(StyleProperty::Padding, StyleValue::px(20.0));
        let computed = ComputedStyle::resolve(&sheet, &Selector::class("btn"), Some(&inline));
        assert_eq!(computed.padding_px(), Some(20.0));
    }

    #[test]
    fn computed_color() {
        let mut sheet = StyleSheet::new();
        let c = Color::rgb(1.0, 0.0, 0.0);
        sheet.add_rule(
            StyleRule::new(Selector::Any).set(StyleProperty::Color, StyleValue::color(c)),
        );
        let computed = ComputedStyle::resolve(&sheet, &Selector::Any, None);
        assert_eq!(computed.color(), Some(c));
    }

    #[test]
    fn computed_background() {
        let mut sheet = StyleSheet::new();
        let bg = Color::BLACK;
        sheet.add_rule(
            StyleRule::new(Selector::class("card"))
                .set(StyleProperty::Background, StyleValue::color(bg)),
        );
        let computed = ComputedStyle::resolve(&sheet, &Selector::class("card"), None);
        assert_eq!(computed.background(), Some(bg));
    }

    #[test]
    fn computed_font_size() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::element("body"))
                .set(StyleProperty::FontSize, StyleValue::px(16.0)),
        );
        let computed = ComputedStyle::resolve(&sheet, &Selector::element("body"), None);
        assert_eq!(computed.font_size(), Some(16.0));
    }

    #[test]
    fn computed_padding_px() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::class("box"))
                .set(StyleProperty::Padding, StyleValue::px(24.0)),
        );
        let computed = ComputedStyle::resolve(&sheet, &Selector::class("box"), None);
        assert_eq!(computed.padding_px(), Some(24.0));
    }

    #[test]
    fn computed_opacity_default_1() {
        let sheet = StyleSheet::new();
        let computed = ComputedStyle::resolve(&sheet, &Selector::Any, None);
        assert!((computed.opacity() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn computed_property_count() {
        let mut sheet = StyleSheet::new();
        sheet.add_rule(
            StyleRule::new(Selector::class("card"))
                .set(StyleProperty::Background, StyleValue::keyword("white"))
                .set(StyleProperty::Padding, StyleValue::px(8.0))
                .set(StyleProperty::BorderRadius, StyleValue::px(4.0)),
        );
        let computed = ComputedStyle::resolve(&sheet, &Selector::class("card"), None);
        assert_eq!(computed.property_count(), 3);
    }
}
