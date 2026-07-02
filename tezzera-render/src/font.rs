use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use fontdue::{Font, FontSettings};

type GlyphKey = (char, u32); // (char, px.to_bits())
/// Shared rasterized glyph: metrics + coverage bitmap.
pub type CachedGlyph = Arc<(fontdue::Metrics, Vec<u8>)>;

pub struct FontCache {
    pub(crate) font: Font,
    // Caches CachedGlyph keyed by (char, px.to_bits()).
    // Arc so draw_text gets a cheap handle instead of cloning the bitmap
    // Vec on every glyph draw. RefCell so callers only need &self.
    glyph_cache: RefCell<HashMap<GlyphKey, CachedGlyph>>,
    // Caches advance_width per (char, px.to_bits()).
    metrics_cache: RefCell<HashMap<GlyphKey, f32>>,
}

impl FontCache {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let font = Font::from_bytes(bytes, FontSettings::default())
            .expect("invalid font bytes");
        Self {
            font,
            glyph_cache: RefCell::new(HashMap::new()),
            metrics_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Load a system proportional / UI font (Avenir Next, Helvetica, Arial).
    pub fn system_ui() -> Option<Self> {
        let candidates = [
            // macOS — clean proportional faces
            "/System/Library/Fonts/Avenir Next.ttc",
            "/System/Library/Fonts/HelveticaNeue.ttc",
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            // Linux
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            // Windows
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\arial.ttf",
        ];
        for path in &candidates {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(Self::from_bytes(&bytes));
            }
        }
        None
    }

    /// Load a system monospace font (Menlo, Courier, DejaVu Mono, etc.).
    pub fn system_mono() -> Option<Self> {
        let candidates = [
            "/System/Library/Fonts/Menlo.ttc",
            "/System/Library/Fonts/Monaco.ttf",
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
            "C:\\Windows\\Fonts\\consola.ttf",
        ];
        for path in &candidates {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(Self::from_bytes(&bytes));
            }
        }
        None
    }

    /// Shared handle to the cached glyph for `c` at `px` size.
    ///
    /// Rasterizes on first use; afterwards returns an `Arc` clone (no bitmap
    /// copy). This is the hot path used by `SkiaCanvas::draw_text`.
    pub fn glyph(&self, c: char, px: f32) -> CachedGlyph {
        let key = (c, px.to_bits());
        {
            let cache = self.glyph_cache.borrow();
            if let Some(entry) = cache.get(&key) {
                return Arc::clone(entry);
            }
        }
        let entry = Arc::new(self.font.rasterize(c, px));
        self.glyph_cache.borrow_mut().insert(key, Arc::clone(&entry));
        entry
    }

    /// Rasterize a single character at the given px size.
    /// Returns (metrics, coverage_bitmap). Result is cached after first call.
    ///
    /// Copies the bitmap — prefer [`FontCache::glyph`] in hot paths.
    pub fn rasterize(&self, c: char, px: f32) -> (fontdue::Metrics, Vec<u8>) {
        let glyph = self.glyph(c, px);
        (glyph.0, glyph.1.clone())
    }

    /// Kerning adjustment in pixels between `left` and `right` at `px` size.
    /// Zero when the font defines no kerning pair.
    pub fn kern(&self, left: char, right: char, px: f32) -> f32 {
        self.font.horizontal_kern(left, right, px).unwrap_or(0.0)
    }

    /// Pixel advance width of a single character at `px` size. Cached.
    pub fn advance_width(&self, c: char, px: f32) -> f32 {
        let key = (c, px.to_bits());
        {
            let cache = self.metrics_cache.borrow();
            if let Some(&w) = cache.get(&key) {
                return w;
            }
        }
        let w = self.font.metrics(c, px).advance_width;
        self.metrics_cache.borrow_mut().insert(key, w);
        w
    }

    /// Total pixel width of a string at `px` size.
    ///
    /// Sums advance widths plus kerning pairs — must stay in lockstep with
    /// `SkiaCanvas::draw_text` so measured and painted widths agree.
    pub fn measure_text(&self, text: &str, px: f32) -> f32 {
        let mut width = 0.0;
        let mut prev: Option<char> = None;
        for c in text.chars() {
            if let Some(p) = prev {
                width += self.kern(p, c, px);
            }
            width += self.advance_width(c, px);
            prev = Some(c);
        }
        width
    }

    /// Distance from the top of the line box to the text baseline, in pixels.
    pub fn ascender(&self, px: f32) -> i32 {
        self.font
            .horizontal_line_metrics(px)
            .map(|m| m.ascent.round() as i32)
            .unwrap_or((px * 0.78) as i32)
    }

    /// Full line height (ascender + descender + gap) in pixels.
    pub fn line_height(&self, px: f32) -> f32 {
        self.font
            .horizontal_line_metrics(px)
            .map(|m| m.new_line_size)
            .unwrap_or(px * 1.2)
    }
}
