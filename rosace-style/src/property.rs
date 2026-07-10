/// CSS-like style property identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleProperty {
    Background,
    Color,
    FontSize,
    FontWeight,
    Padding,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    Margin,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    BorderRadius,
    BorderWidth,
    BorderColor,
    Opacity,
    Display,
    FlexDirection,
    Gap,
    AlignItems,
    JustifyContent,
    Overflow,
}

impl StyleProperty {
    pub fn is_inherited(&self) -> bool {
        matches!(
            self,
            StyleProperty::Color | StyleProperty::FontSize | StyleProperty::FontWeight
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            StyleProperty::Background     => "background",
            StyleProperty::Color          => "color",
            StyleProperty::FontSize       => "font-size",
            StyleProperty::FontWeight     => "font-weight",
            StyleProperty::Padding        => "padding",
            StyleProperty::PaddingTop     => "padding-top",
            StyleProperty::PaddingRight   => "padding-right",
            StyleProperty::PaddingBottom  => "padding-bottom",
            StyleProperty::PaddingLeft    => "padding-left",
            StyleProperty::Margin         => "margin",
            StyleProperty::MarginTop      => "margin-top",
            StyleProperty::MarginRight    => "margin-right",
            StyleProperty::MarginBottom   => "margin-bottom",
            StyleProperty::MarginLeft     => "margin-left",
            StyleProperty::Width          => "width",
            StyleProperty::Height         => "height",
            StyleProperty::MinWidth       => "min-width",
            StyleProperty::MinHeight      => "min-height",
            StyleProperty::MaxWidth       => "max-width",
            StyleProperty::MaxHeight      => "max-height",
            StyleProperty::BorderRadius   => "border-radius",
            StyleProperty::BorderWidth    => "border-width",
            StyleProperty::BorderColor    => "border-color",
            StyleProperty::Opacity        => "opacity",
            StyleProperty::Display        => "display",
            StyleProperty::FlexDirection  => "flex-direction",
            StyleProperty::Gap            => "gap",
            StyleProperty::AlignItems     => "align-items",
            StyleProperty::JustifyContent => "justify-content",
            StyleProperty::Overflow       => "overflow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_is_inherited_color() {
        assert!(StyleProperty::Color.is_inherited());
    }

    #[test]
    fn property_is_inherited_font_size() {
        assert!(StyleProperty::FontSize.is_inherited());
    }

    #[test]
    fn property_not_inherited_padding() {
        assert!(!StyleProperty::Padding.is_inherited());
    }

    #[test]
    fn property_name_background() {
        assert_eq!(StyleProperty::Background.name(), "background");
    }

    #[test]
    fn property_name_font_size() {
        assert_eq!(StyleProperty::FontSize.name(), "font-size");
    }

    #[test]
    fn property_count_reasonable() {
        // Verify the enum has at least 20 variants by checking known ones exist.
        let props = [
            StyleProperty::Background,
            StyleProperty::Color,
            StyleProperty::FontSize,
            StyleProperty::FontWeight,
            StyleProperty::Padding,
            StyleProperty::PaddingTop,
            StyleProperty::PaddingRight,
            StyleProperty::PaddingBottom,
            StyleProperty::PaddingLeft,
            StyleProperty::Margin,
            StyleProperty::MarginTop,
            StyleProperty::MarginRight,
            StyleProperty::MarginBottom,
            StyleProperty::MarginLeft,
            StyleProperty::Width,
            StyleProperty::Height,
            StyleProperty::MinWidth,
            StyleProperty::MinHeight,
            StyleProperty::MaxWidth,
            StyleProperty::MaxHeight,
        ];
        assert!(props.len() >= 20);
    }
}
