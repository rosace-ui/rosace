use rosace_theme::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Px,
    Em,
    Rem,
    Vw,
    Vh,
    Percent,
}

impl std::fmt::Display for LengthUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LengthUnit::Px      => write!(f, "px"),
            LengthUnit::Em      => write!(f, "em"),
            LengthUnit::Rem     => write!(f, "rem"),
            LengthUnit::Vw      => write!(f, "vw"),
            LengthUnit::Vh      => write!(f, "vh"),
            LengthUnit::Percent => write!(f, "%"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue {
    Color(Color),
    Length(f32, LengthUnit),
    Percent(f32),
    Number(f32),
    Keyword(String),
    None,
    Inherit,
    Auto,
}

impl StyleValue {
    /// Resolve to pixel value. Returns None for non-length values or relative units
    /// that can't be resolved without context.
    pub fn to_px(&self) -> Option<f32> {
        match self {
            StyleValue::Length(v, LengthUnit::Px) => Some(*v),
            StyleValue::Number(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        if let StyleValue::Color(c) = self { Some(*c) } else { None }
    }

    pub fn as_keyword(&self) -> Option<&str> {
        if let StyleValue::Keyword(k) = self { Some(k) } else { None }
    }

    pub fn is_none(&self) -> bool { matches!(self, StyleValue::None) }
    pub fn is_auto(&self) -> bool { matches!(self, StyleValue::Auto) }
    pub fn is_inherit(&self) -> bool { matches!(self, StyleValue::Inherit) }

    // Convenience constructors
    pub fn px(v: f32) -> Self { StyleValue::Length(v, LengthUnit::Px) }
    pub fn em(v: f32) -> Self { StyleValue::Length(v, LengthUnit::Em) }
    pub fn percent(v: f32) -> Self { StyleValue::Percent(v) }
    pub fn keyword(k: impl Into<String>) -> Self { StyleValue::Keyword(k.into()) }
    pub fn color(c: Color) -> Self { StyleValue::Color(c) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_value_px_to_px() {
        let v = StyleValue::px(42.0);
        assert_eq!(v.to_px(), Some(42.0));
    }

    #[test]
    fn style_value_em_to_px_none() {
        let v = StyleValue::em(2.0);
        assert_eq!(v.to_px(), None);
    }

    #[test]
    fn style_value_color() {
        let c = Color::rgb(1.0, 0.0, 0.0);
        let v = StyleValue::color(c);
        assert_eq!(v.as_color(), Some(c));
    }

    #[test]
    fn style_value_keyword() {
        let v = StyleValue::keyword("flex");
        assert_eq!(v.as_keyword(), Some("flex"));
    }

    #[test]
    fn style_value_is_none() {
        let v = StyleValue::None;
        assert!(v.is_none());
        assert!(!StyleValue::Auto.is_none());
    }

    #[test]
    fn style_value_is_auto() {
        let v = StyleValue::Auto;
        assert!(v.is_auto());
        assert!(!StyleValue::None.is_auto());
    }

    #[test]
    fn style_value_px_constructor() {
        let v = StyleValue::px(16.0);
        assert_eq!(v, StyleValue::Length(16.0, LengthUnit::Px));
    }

    #[test]
    fn style_value_color_as_color() {
        let c = Color::WHITE;
        let v = StyleValue::color(c);
        let got = v.as_color().unwrap();
        assert_eq!(got.r, 1.0);
        assert_eq!(got.g, 1.0);
        assert_eq!(got.b, 1.0);
        assert_eq!(got.a, 1.0);
    }
}
