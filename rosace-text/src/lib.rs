//! Rich text layout for ROSACE.
//!
//! Provides styled text spans, paragraph layout with word wrapping,
//! and a text cursor for editable fields.
//!
//! # Example
//! ```rust,ignore
//! use rosace_text::{RichText, TextStyle, TextLayout};
//! use rosace_theme::Color;
//!
//! let rt = RichText::new()
//!     .text("Hello ", 16.0, Color::WHITE)
//!     .bold("world", 16.0, Color::from_hex(0x6750A4));
//!
//! let layout = TextLayout::layout(&rt.spans, 300.0);
//! // layout.render(&mut canvas, &font, 20.0, 40.0);
//! ```

pub mod cursor;
pub mod direction;
pub mod layout;
pub mod metrics;
pub mod rich_text;
pub mod span;

pub use cursor::{TextCursor, TextSelection};
pub use direction::{detect_direction, reverse_words, TextDirection};
pub use layout::{word_wrap, word_wrap_simple, TextLayout, TextLine};
pub use metrics::{measure_text, measure_text_heuristic};
pub use rich_text::RichText;
pub use span::{TextSpan, TextStyle};
