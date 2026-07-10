//! Text shaping for ROSACE — HarfBuzz prep stub.
//!
//! Provides the `ShapingEngine` trait and `FallbackShaper` (fontdue-backed,
//! one glyph per character). In v1.0, a `HarfBuzzShaper` implementing
//! `ShapingEngine` will be slotted in to provide ligatures, kerning, and
//! GSUB/GPOS OpenType features without changing any call sites.
//!
//! # Example
//! ```rust,ignore
//! use rosace_shaping::{FallbackShaper, ShapingEngine};
//! use rosace_text::TextDirection;
//!
//! if let Some(shaper) = FallbackShaper::system() {
//!     let run = shaper.shape("Hello", 16.0, TextDirection::Ltr);
//!     println!("glyphs: {}, advance: {:.1}px", run.glyph_count(), run.total_advance());
//! }
//! ```

pub mod engine;
pub mod fallback;
pub mod glyph;
pub mod pipeline;
pub mod script;

pub use engine::ShapingEngine;
pub use fallback::FallbackShaper;
pub use glyph::{GlyphRun, ShapedGlyph};
pub use pipeline::ShapingPipeline;
pub use script::Script;
