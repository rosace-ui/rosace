use crate::glyph::GlyphRun;
use rosace_text::TextDirection;

/// A text shaping engine — converts a Unicode string into a `GlyphRun`.
///
/// In v1.0, a `HarfBuzzShaper` will implement this trait and provide
/// full OpenType shaping (ligatures, kerning, GSUB/GPOS).
pub trait ShapingEngine: Send + 'static {
    /// Shape `text` at `font_size` px in the given direction.
    fn shape(&self, text: &str, font_size: f32, direction: TextDirection) -> GlyphRun;

    /// Human-readable name for this engine.
    fn name(&self) -> &'static str;

    /// Whether this engine can shape `text` (returns false to defer to fallback).
    fn can_shape(&self, _text: &str) -> bool { true }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::GlyphRun;
    use crate::script::Script;
    use rosace_text::TextDirection;

    struct DummyEngine;

    impl ShapingEngine for DummyEngine {
        fn shape(&self, text: &str, font_size: f32, direction: TextDirection) -> GlyphRun {
            GlyphRun::new(font_size, direction, Script::detect(text))
        }

        fn name(&self) -> &'static str { "DummyEngine" }
    }

    #[test]
    fn shaping_engine_is_object_safe() {
        let engine: Box<dyn ShapingEngine> = Box::new(DummyEngine);
        assert_eq!(engine.name(), "DummyEngine");
    }

    #[test]
    fn shaping_engine_can_shape_default_true() {
        let engine = DummyEngine;
        assert!(engine.can_shape("hello"));
        assert!(engine.can_shape(""));
    }
}
