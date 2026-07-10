use crate::script::Script;
use rosace_text::TextDirection;

/// A single shaped glyph with positioning info.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    /// Font-internal glyph ID (Unicode codepoint in our stub).
    pub glyph_id: u32,
    /// Horizontal advance (how far to move the pen after this glyph).
    pub x_advance: f32,
    /// Vertical advance (0 for horizontal text).
    pub y_advance: f32,
    /// Horizontal offset from pen position.
    pub x_offset: f32,
    /// Vertical offset from pen position.
    pub y_offset: f32,
    /// Index into the source text (byte offset).
    pub cluster: u32,
    /// The original character (for debugging and fallback rendering).
    pub ch: char,
}

impl ShapedGlyph {
    pub fn new(ch: char, glyph_id: u32, x_advance: f32, cluster: u32) -> Self {
        Self { glyph_id, x_advance, y_advance: 0.0, x_offset: 0.0, y_offset: 0.0, cluster, ch }
    }

    pub fn total_advance(&self) -> f32 { self.x_advance + self.x_offset }
}

/// A sequence of shaped glyphs for a text run.
#[derive(Debug, Clone, Default)]
pub struct GlyphRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub font_size: f32,
    pub direction: TextDirection,
    pub script: Script,
}

impl GlyphRun {
    pub fn new(font_size: f32, direction: TextDirection, script: Script) -> Self {
        Self { glyphs: Vec::new(), font_size, direction, script }
    }

    pub fn push(&mut self, glyph: ShapedGlyph) { self.glyphs.push(glyph); }

    /// Sum of all x_advance values — total pixel width of the run.
    pub fn total_advance(&self) -> f32 {
        self.glyphs.iter().map(|g| g.x_advance).sum()
    }

    pub fn glyph_count(&self) -> usize { self.glyphs.len() }
    pub fn is_empty(&self) -> bool { self.glyphs.is_empty() }

    /// Reverse glyph order (for RTL visual rendering).
    pub fn reversed(mut self) -> Self {
        self.glyphs.reverse();
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_text::TextDirection;
    use crate::script::Script;

    #[test]
    fn shaped_glyph_new() {
        let g = ShapedGlyph::new('A', 65, 10.0, 0);
        assert_eq!(g.glyph_id, 65);
        assert_eq!(g.x_advance, 10.0);
        assert_eq!(g.cluster, 0);
        assert_eq!(g.y_advance, 0.0);
        assert_eq!(g.x_offset, 0.0);
        assert_eq!(g.y_offset, 0.0);
    }

    #[test]
    fn shaped_glyph_total_advance() {
        let mut g = ShapedGlyph::new('B', 66, 10.0, 1);
        g.x_offset = 2.0;
        assert_eq!(g.total_advance(), 12.0);
    }

    #[test]
    fn shaped_glyph_ch_field() {
        let g = ShapedGlyph::new('Z', 90, 8.0, 25);
        assert_eq!(g.ch, 'Z');
    }

    #[test]
    fn glyph_run_new_empty() {
        let run = GlyphRun::new(16.0, TextDirection::Ltr, Script::Latin);
        assert!(run.is_empty());
        assert_eq!(run.glyph_count(), 0);
    }

    #[test]
    fn glyph_run_push() {
        let mut run = GlyphRun::new(16.0, TextDirection::Ltr, Script::Latin);
        run.push(ShapedGlyph::new('a', 97, 9.0, 0));
        assert_eq!(run.glyph_count(), 1);
        assert!(!run.is_empty());
    }

    #[test]
    fn glyph_run_glyph_count() {
        let mut run = GlyphRun::new(16.0, TextDirection::Ltr, Script::Latin);
        run.push(ShapedGlyph::new('a', 97, 9.0, 0));
        run.push(ShapedGlyph::new('b', 98, 9.0, 1));
        assert_eq!(run.glyph_count(), 2);
    }

    #[test]
    fn glyph_run_total_advance() {
        let mut run = GlyphRun::new(16.0, TextDirection::Ltr, Script::Latin);
        run.push(ShapedGlyph::new('a', 97, 9.0, 0));
        run.push(ShapedGlyph::new('b', 98, 11.0, 1));
        assert_eq!(run.total_advance(), 20.0);
    }

    #[test]
    fn glyph_run_is_empty() {
        let run = GlyphRun::new(16.0, TextDirection::Ltr, Script::Latin);
        assert!(run.is_empty());
    }

    #[test]
    fn glyph_run_reversed() {
        let mut run = GlyphRun::new(16.0, TextDirection::Rtl, Script::Arabic);
        run.push(ShapedGlyph::new('a', 97, 9.0, 0));
        run.push(ShapedGlyph::new('b', 98, 9.0, 1));
        let rev = run.reversed();
        assert_eq!(rev.glyphs[0].ch, 'b');
        assert_eq!(rev.glyphs[1].ch, 'a');
    }

    #[test]
    fn glyph_run_font_size() {
        let run = GlyphRun::new(24.0, TextDirection::Ltr, Script::Latin);
        assert_eq!(run.font_size, 24.0);
    }

    #[test]
    fn glyph_run_direction() {
        let run = GlyphRun::new(16.0, TextDirection::Rtl, Script::Arabic);
        assert_eq!(run.direction, TextDirection::Rtl);
    }

    #[test]
    fn glyph_run_script() {
        let run = GlyphRun::new(16.0, TextDirection::Ltr, Script::Greek);
        assert_eq!(run.script, Script::Greek);
    }
}
