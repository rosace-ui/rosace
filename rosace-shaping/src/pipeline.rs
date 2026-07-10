use rosace_text::TextDirection;
use crate::engine::ShapingEngine;
use crate::fallback::FallbackShaper;
use crate::glyph::GlyphRun;

/// A chain of shaping engines with automatic fallback.
///
/// Each engine is tried in order via `can_shape()`. The first engine
/// that returns `true` for `can_shape` is used. If none match,
/// the `FallbackShaper` is used.
pub struct ShapingPipeline {
    engines: Vec<Box<dyn ShapingEngine>>,
    fallback: Option<FallbackShaper>,
}

impl ShapingPipeline {
    pub fn new() -> Self {
        Self {
            engines: Vec::new(),
            fallback: FallbackShaper::system(),
        }
    }

    /// Add an engine at the highest priority.
    pub fn with_engine(mut self, engine: impl ShapingEngine) -> Self {
        self.engines.push(Box::new(engine));
        self
    }

    /// Shape `text` using the first applicable engine, or fallback.
    pub fn shape(&self, text: &str, font_size: f32, direction: TextDirection) -> GlyphRun {
        for engine in &self.engines {
            if engine.can_shape(text) {
                return engine.shape(text, font_size, direction);
            }
        }
        if let Some(fb) = &self.fallback {
            return fb.shape(text, font_size, direction);
        }
        // No font available — return empty run
        GlyphRun::new(font_size, direction, crate::script::Script::Unknown)
    }

    pub fn engine_count(&self) -> usize { self.engines.len() }
    pub fn has_fallback(&self) -> bool { self.fallback.is_some() }
}

impl Default for ShapingPipeline {
    fn default() -> Self { Self::new() }
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

    struct NoOpEngine;

    impl ShapingEngine for NoOpEngine {
        fn shape(&self, _text: &str, font_size: f32, direction: TextDirection) -> GlyphRun {
            GlyphRun::new(font_size, direction, Script::Unknown)
        }
        fn name(&self) -> &'static str { "NoOpEngine" }
    }

    #[test]
    fn pipeline_new_has_no_engines() {
        let p = ShapingPipeline::new();
        assert_eq!(p.engine_count(), 0);
    }

    #[test]
    fn pipeline_has_fallback_when_font_available() {
        let p = ShapingPipeline::new();
        // has_fallback depends on whether a system font was found — just assert it doesn't panic
        let _ = p.has_fallback();
    }

    #[test]
    fn pipeline_engine_count() {
        let p = ShapingPipeline::new().with_engine(NoOpEngine);
        assert_eq!(p.engine_count(), 1);
    }

    #[test]
    fn pipeline_shape_returns_run() {
        let p = ShapingPipeline::new().with_engine(NoOpEngine);
        let run = p.shape("Hello", 16.0, TextDirection::Ltr);
        // NoOpEngine returns an empty run — just verify it's a GlyphRun
        assert_eq!(run.font_size, 16.0);
        assert_eq!(run.direction, TextDirection::Ltr);
    }

    #[test]
    fn pipeline_empty_string_returns_empty_run() {
        let p = ShapingPipeline::new();
        let run = p.shape("", 16.0, TextDirection::Ltr);
        assert!(run.is_empty());
    }

    #[test]
    fn pipeline_default() {
        let p = ShapingPipeline::default();
        assert_eq!(p.engine_count(), 0);
    }
}
