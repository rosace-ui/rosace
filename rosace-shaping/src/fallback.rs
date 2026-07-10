use rosace_render::FontCache;
use rosace_text::TextDirection;
use crate::engine::ShapingEngine;
use crate::glyph::{GlyphRun, ShapedGlyph};
use crate::script::Script;

/// Fallback shaper backed by fontdue.
///
/// Produces one glyph per character using `FontCache::rasterize()` advance widths.
/// No ligature substitution, no kerning — pure character-to-glyph mapping.
pub struct FallbackShaper {
    font: FontCache,
}

impl FallbackShaper {
    pub fn new(font: FontCache) -> Self { Self { font } }

    /// Try to create with system monospace font.
    pub fn system() -> Option<Self> {
        FontCache::system_mono().map(Self::new)
    }
}

impl ShapingEngine for FallbackShaper {
    fn name(&self) -> &'static str { "FallbackShaper (fontdue)" }

    fn shape(&self, text: &str, font_size: f32, direction: TextDirection) -> GlyphRun {
        let script = Script::detect(text);
        let mut run = GlyphRun::new(font_size, direction, script);
        let px = font_size.ceil();
        let mut byte_offset: u32 = 0;

        for ch in text.chars() {
            let (metrics, _) = self.font.rasterize(ch, px);
            let glyph = ShapedGlyph::new(ch, ch as u32, metrics.advance_width, byte_offset);
            run.push(glyph);
            byte_offset += ch.len_utf8() as u32;
        }

        if direction == TextDirection::Rtl {
            run = run.reversed();
        }

        run
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_text::TextDirection;

    fn get_shaper() -> Option<FallbackShaper> {
        FallbackShaper::system()
    }

    #[test]
    fn fallback_shaper_system_may_return_some() {
        // On machines without any system font this may be None — that is acceptable.
        // We just ensure it doesn't panic.
        let _shaper = FallbackShaper::system();
    }

    #[test]
    fn fallback_name() {
        if let Some(shaper) = get_shaper() {
            assert_eq!(shaper.name(), "FallbackShaper (fontdue)");
        }
    }

    #[test]
    fn fallback_shape_hello_has_5_glyphs() {
        if let Some(shaper) = get_shaper() {
            let run = shaper.shape("Hello", 16.0, TextDirection::Ltr);
            assert_eq!(run.glyph_count(), 5);
        }
    }

    #[test]
    fn fallback_shape_total_advance_positive() {
        if let Some(shaper) = get_shaper() {
            let run = shaper.shape("Hello", 16.0, TextDirection::Ltr);
            assert!(run.total_advance() > 0.0);
        }
    }

    #[test]
    fn fallback_shape_clusters_monotonic() {
        if let Some(shaper) = get_shaper() {
            let run = shaper.shape("Hi", 16.0, TextDirection::Ltr);
            // LTR: clusters should be in increasing order
            let clusters: Vec<u32> = run.glyphs.iter().map(|g| g.cluster).collect();
            for w in clusters.windows(2) {
                assert!(w[0] < w[1], "clusters not monotonically increasing: {:?}", clusters);
            }
        }
    }

    #[test]
    fn fallback_shape_rtl_reverses() {
        if let Some(shaper) = get_shaper() {
            let ltr = shaper.shape("ab", 16.0, TextDirection::Ltr);
            let rtl = shaper.shape("ab", 16.0, TextDirection::Rtl);
            if ltr.glyph_count() == 2 && rtl.glyph_count() == 2 {
                // RTL run should have glyphs in reversed order compared to LTR
                assert_eq!(ltr.glyphs[0].ch, rtl.glyphs[1].ch);
                assert_eq!(ltr.glyphs[1].ch, rtl.glyphs[0].ch);
            }
        }
    }

    #[test]
    fn fallback_shape_empty_string() {
        if let Some(shaper) = get_shaper() {
            let run = shaper.shape("", 16.0, TextDirection::Ltr);
            assert!(run.is_empty());
            assert_eq!(run.total_advance(), 0.0);
        }
    }

    #[test]
    fn fallback_glyph_ids_match_codepoints() {
        if let Some(shaper) = get_shaper() {
            let run = shaper.shape("AB", 16.0, TextDirection::Ltr);
            for glyph in &run.glyphs {
                assert_eq!(glyph.glyph_id, glyph.ch as u32);
            }
        }
    }
}
