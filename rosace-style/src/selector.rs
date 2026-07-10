/// A CSS-like selector for matching elements.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selector {
    /// Matches any element.
    Any,
    /// Matches elements with the given tag/element name (e.g. "button").
    Element(String),
    /// Matches elements with the given class (e.g. "btn-primary").
    Class(String),
    /// Matches the element with the given ID.
    Id(String),
    /// Matches elements with the given pseudo-class (e.g. "hover", "focus").
    Pseudo(String),
}

impl Selector {
    pub fn element(name: impl Into<String>) -> Self { Selector::Element(name.into()) }
    pub fn class(name: impl Into<String>) -> Self { Selector::Class(name.into()) }
    pub fn id(name: impl Into<String>) -> Self { Selector::Id(name.into()) }
    pub fn pseudo(name: impl Into<String>) -> Self { Selector::Pseudo(name.into()) }

    /// Returns true if this selector matches `other`.
    /// `Any` matches everything. Exact variant equality for the rest.
    pub fn matches(&self, other: &Selector) -> bool {
        match self {
            Selector::Any => true,
            _ => self == other,
        }
    }

    pub fn specificity(&self) -> u32 {
        match self {
            Selector::Any        => 0,
            Selector::Element(_) => 1,
            Selector::Pseudo(_)  => 1,
            Selector::Class(_)   => 10,
            Selector::Id(_)      => 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_any_matches_all() {
        let any = Selector::Any;
        assert!(any.matches(&Selector::class("foo")));
        assert!(any.matches(&Selector::id("bar")));
        assert!(any.matches(&Selector::element("div")));
        assert!(any.matches(&Selector::Any));
    }

    #[test]
    fn selector_class_matches_same() {
        let s = Selector::class("btn");
        assert!(s.matches(&Selector::class("btn")));
    }

    #[test]
    fn selector_class_no_match_different() {
        let s = Selector::class("btn");
        assert!(!s.matches(&Selector::class("other")));
    }

    #[test]
    fn selector_id_matches_same() {
        let s = Selector::id("header");
        assert!(s.matches(&Selector::id("header")));
    }

    #[test]
    fn selector_element_matches_same() {
        let s = Selector::element("button");
        assert!(s.matches(&Selector::element("button")));
    }

    #[test]
    fn selector_specificity_id_highest() {
        assert!(Selector::id("x").specificity() > Selector::class("x").specificity());
        assert!(Selector::id("x").specificity() > Selector::element("x").specificity());
    }

    #[test]
    fn selector_specificity_class_mid() {
        let class_spec = Selector::class("x").specificity();
        let element_spec = Selector::element("x").specificity();
        assert!(class_spec > element_spec);
        assert!(class_spec < Selector::id("x").specificity());
    }

    #[test]
    fn selector_pseudo_specificity() {
        assert_eq!(Selector::pseudo("hover").specificity(), 1);
        assert_eq!(Selector::Any.specificity(), 0);
    }
}
