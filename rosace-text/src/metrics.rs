use rosace_render::FontCache;

/// Measure the pixel width of a string using fontdue glyph advance widths.
/// Falls back to the monospace heuristic if no font is available.
pub fn measure_text(text: &str, font_size: f32, font: &FontCache) -> f32 {
    // fontdue uses float px sizes — ceil so small sizes still rasterize
    let px = font_size.ceil();
    text.chars()
        .map(|ch| {
            let (metrics, _) = font.rasterize(ch, px);
            metrics.advance_width
        })
        .sum()
}

/// Heuristic fallback (no font required).
pub fn measure_text_heuristic(text: &str, font_size: f32) -> f32 {
    text.len() as f32 * font_size * 0.55
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- heuristic tests ----

    #[test]
    fn measure_text_heuristic_scales_with_length() {
        let short = measure_text_heuristic("Hi", 14.0);
        let long = measure_text_heuristic("Hello!", 14.0);
        // "Hello!" has 3× as many chars as "Hi" — width should be 3× as wide
        assert!((long - 3.0 * short).abs() < 1e-5);
    }

    #[test]
    fn measure_text_heuristic_scales_with_size() {
        let small = measure_text_heuristic("test", 10.0);
        let large = measure_text_heuristic("test", 20.0);
        assert!((large - 2.0 * small).abs() < 1e-5);
    }

    #[test]
    fn measure_text_heuristic_empty() {
        let w = measure_text_heuristic("", 14.0);
        assert_eq!(w, 0.0);
    }

    #[test]
    fn measure_text_heuristic_single_char() {
        let w = measure_text_heuristic("A", 14.0);
        assert!((w - 14.0 * 0.55).abs() < 1e-5);
    }

    // ---- real-font tests (require a system font) ----

    #[test]
    fn measure_text_with_font_gt_zero() {
        if let Some(font) = FontCache::system_mono() {
            let w = measure_text("hello", 16.0, &font);
            assert!(w > 0.0, "expected positive width, got {w}");
        }
        // If no system font is available on this machine, skip silently.
    }

    #[test]
    fn measure_text_with_font_longer_wider() {
        if let Some(font) = FontCache::system_mono() {
            let short = measure_text("hi", 16.0, &font);
            let long = measure_text("hello world", 16.0, &font);
            assert!(
                long > short,
                "expected longer string to be wider: long={long} short={short}"
            );
        }
    }
}
