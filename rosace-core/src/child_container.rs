use crate::element::Element;

/// Implemented by builder types that accumulate child `Element`s.
///
/// All methods consume `self` and return `Self` to support chained builder syntax.
pub trait ChildContainer: Sized {
    /// Appends a single child element.
    fn child(self, element: impl Into<Element>) -> Self;

    /// Appends a homogeneous collection of child elements.
    fn children<E: Into<Element>>(self, elements: Vec<E>) -> Self;

    /// Appends a child element only if `element` is `Some`.
    fn child_if(self, element: Option<impl Into<Element>>) -> Self {
        match element {
            Some(e) => self.child(e),
            None => self,
        }
    }

    /// Inserts a child element at the front of the child list.
    fn prepend(self, element: impl Into<Element>) -> Self;
}
