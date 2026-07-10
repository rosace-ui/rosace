//! CSS-like style system for ROSACE.
//!
//! Provides `StyleSheet`, `StyleRule`, `InlineStyle`, and `ComputedStyle`
//! for decoupling widget appearance from widget structure.
//!
//! # Example
//! ```rust,ignore
//! use rosace_style::{StyleSheet, StyleRule, StyleValue, StyleProperty, Selector, ComputedStyle};
//! use rosace_theme::Color;
//!
//! let mut sheet = StyleSheet::new();
//! sheet.add_rule(
//!     StyleRule::new(Selector::class("btn"))
//!         .set(StyleProperty::Background, StyleValue::color(Color::rgb(0.42, 0.31, 0.78)))
//!         .set(StyleProperty::Padding, StyleValue::px(12.0))
//!         .set(StyleProperty::BorderRadius, StyleValue::px(6.0))
//! );
//!
//! let computed = ComputedStyle::resolve(&sheet, &Selector::class("btn"), None);
//! assert_eq!(computed.padding_px(), Some(12.0));
//! ```

pub mod computed;
pub mod inline;
pub mod property;
pub mod rule;
pub mod selector;
pub mod sheet;
pub mod value;

pub use computed::ComputedStyle;
pub use inline::{InlineStyle, InlineStyleBuilder};
pub use property::StyleProperty;
pub use rule::StyleRule;
pub use selector::Selector;
pub use sheet::StyleSheet;
pub use value::{LengthUnit, StyleValue};
