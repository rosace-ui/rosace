//! [`Expanded`] — fills remaining flex space in a [`Column`] or [`Row`].
//!
//! [`Column`]: crate::widgets::column::Column
//! [`Row`]: crate::widgets::row::Row

use tezzera_core::element::{Element, NativeElement};

/// A widget that expands to fill the remaining main-axis space allocated by
/// a parent [`Column`] or [`Row`].
///
/// The `flex` factor controls how space is shared among multiple `Expanded`
/// widgets in the same container.
///
/// [`Column`]: crate::widgets::column::Column
/// [`Row`]: crate::widgets::row::Row
#[derive(Debug, Clone)]
pub struct Expanded {
    /// Flex factor — higher values claim proportionally more remaining space.
    pub flex: u32,
    child: Option<Element>,
}

impl Expanded {
    /// Create an `Expanded` with flex factor 1 and no child.
    pub fn new() -> Self {
        Self { flex: 1, child: None }
    }

    /// Override the flex factor.
    pub fn flex(mut self, f: u32) -> Self {
        self.flex = f;
        self
    }

    /// Set the single child element that fills the expanded region.
    pub fn child(mut self, e: impl Into<Element>) -> Self {
        self.child = Some(e.into());
        self
    }
}

impl Default for Expanded {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Expanded> for Element {
    fn from(e: Expanded) -> Self {
        let children = e.child.map(|c| vec![c]).unwrap_or_default();
        Element::Native(NativeElement {
            tag: "Expanded",
            children,
        })
    }
}
