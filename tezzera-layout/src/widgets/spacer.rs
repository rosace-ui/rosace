//! [`Spacer`] — flexible empty space inside a flex container.

use tezzera_core::element::{Element, NativeElement};

/// A flexible empty widget that expands to fill remaining main-axis space
/// inside a [`Column`] or [`Row`].
///
/// The `flex` factor controls how much space this spacer claims relative to
/// other [`Expanded`] / `Spacer` widgets in the same container.
///
/// [`Column`]: crate::widgets::column::Column
/// [`Row`]: crate::widgets::row::Row
/// [`Expanded`]: crate::widgets::expanded::Expanded
#[derive(Debug, Clone)]
pub struct Spacer {
    /// Flex factor — higher values claim proportionally more space.
    pub flex: u32,
}

impl Spacer {
    /// Create a `Spacer` with flex factor 1.
    pub fn new() -> Self {
        Self { flex: 1 }
    }

    /// Override the flex factor.
    pub fn flex(mut self, f: u32) -> Self {
        self.flex = f;
        self
    }
}

impl Default for Spacer {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Spacer> for Element {
    fn from(_s: Spacer) -> Self {
        Element::Native(NativeElement {
            tag: "Spacer",
            children: vec![],
        })
    }
}
