//! Unicode Bidirectional Algorithm subset for ROSACE (TR#9 simplified).
//!
//! Provides BidiClass classification, paragraph embedding level detection,
//! per-character level resolution, and visual reordering.
//!
//! Full ICU/unicode-bidi crate integration is deferred to v1.0.
//! This crate covers the most common cases: Latin + Arabic + Hebrew mixed text.
//!
//! # Example
//! ```rust,ignore
//! use rosace_bidi::BidiParagraph;
//!
//! let para = BidiParagraph::new("Hello مرحبا World");
//! println!("base level: {}", para.base_level);  // 0 (LTR — first strong char is 'H')
//! println!("visual:     {}", para.visual);
//! ```

pub mod class;
pub mod level;
pub mod paragraph;
pub mod reorder;

pub use class::{bidi_class, BidiClass};
pub use level::{paragraph_level, resolve_levels};
pub use paragraph::BidiParagraph;
pub use reorder::{reorder, reorder_line};
