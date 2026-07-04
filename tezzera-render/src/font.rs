use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use fontdue::{Font, FontSettings};

/// Text weight. Maps onto real font faces: `SemiBold`/`Bold` use the bold
/// face when one was found; `Light`/`Regular`/`Medium` use the regular face.
/// Before this existed the field was silently ignored — headings were never
/// actually bold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontWeight {
    Light,
    #[default]
    Regular,
    Medium,
    SemiBold,
    Bold,
}

impl FontWeight {
    #[inline]
    fn wants_bold(self) -> bool {
        matches!(self, FontWeight::SemiBold | FontWeight::Bold)
    }
}

/// Which face a glyph resolved to: primary regular/bold, or a Unicode
/// fallback face by index.
type FaceKey = u8;
const FACE_REGULAR: FaceKey = 0;
const FACE_BOLD: FaceKey = 1;
const FACE_FALLBACK_BASE: FaceKey = 128;

type GlyphKey = (FaceKey, char, u32); // (face, char, px.to_bits())

/// Shared rasterized glyph: metrics + coverage bitmap.
pub type CachedGlyph = Arc<(fontdue::Metrics, Vec<u8>)>;

enum Fallback {
    Untried(&'static str),
    Missing,
    Loaded(Font),
}

pub struct FontCache {
    pub(crate) font: Font,
    /// Real bold face when the platform provides one; None → bold renders
    /// with the regular face (as before).
    bold: Option<Font>,
    /// Unicode fallback faces, loaded lazily on the first glyph miss —
    /// Arial Unicode alone is ~20 MB, so we don't parse it until a CJK or
    /// symbol codepoint actually appears.
    fallbacks: RefCell<Vec<Fallback>>,
    /// (char, wants_bold) → resolved face. Routing is per-character.
    route_cache: RefCell<HashMap<(char, bool), FaceKey>>,
    glyph_cache: RefCell<HashMap<GlyphKey, CachedGlyph>>,
    metrics_cache: RefCell<HashMap<GlyphKey, f32>>,
}

/// Unicode fallback candidates per platform. Order = priority. Coverage:
/// Arial Unicode (huge BMP incl. CJK), Apple Symbols (arrows, misc),
/// Noto (Linux), Segoe Symbol / MS Gothic (Windows).
const FALLBACK_PATHS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/System/Library/Fonts/Apple Symbols.ttf",
    "/System/Library/Fonts/Supplemental/Zapf Dingbats.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "C:\\Windows\\Fonts\\seguisym.ttf",
    "C:\\Windows\\Fonts\\msgothic.ttc",
];

/// Bold-face candidates paired with nothing in particular — the first that
/// exists wins. (macOS ships most UI families as .ttc collections without a
/// reliable index → member mapping, so we use the standalone bold files.)
const BOLD_PATHS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "C:\\Windows\\Fonts\\segoeuib.ttf",
    "C:\\Windows\\Fonts\\arialbd.ttf",
];

impl FontCache {
    fn build(font: Font, bold: Option<Font>) -> Self {
        Self {
            font,
            bold,
            fallbacks: RefCell::new(
                FALLBACK_PATHS.iter().map(|p| Fallback::Untried(p)).collect(),
            ),
            route_cache: RefCell::new(HashMap::new()),
            glyph_cache: RefCell::new(HashMap::new()),
            metrics_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let font = Font::from_bytes(bytes, FontSettings::default())
            .expect("invalid font bytes");
        Self::build(font, None)
    }

    fn load_first(paths: &[&str]) -> Option<Font> {
        for path in paths {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(f) = Font::from_bytes(bytes.as_slice(), FontSettings::default()) {
                    return Some(f);
                }
            }
        }
        None
    }

    /// Load a system proportional / UI font plus (when available) a real
    /// bold face and the Unicode fallback chain.
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
        let regular = Self::load_first(&candidates)?;
        let bold = Self::load_first(BOLD_PATHS);
        Some(Self::build(regular, bold))
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
        let regular = Self::load_first(&candidates)?;
        Some(Self::build(regular, None))
    }

    // ── Face routing (Unicode fallback, D-text) ──────────────────────────

    /// Resolve which face renders `c` at `weight`: bold face when requested
    /// and it has the glyph; else regular; else the first fallback face that
    /// covers the codepoint (loaded lazily); else regular (tofu).
    fn resolve(&self, c: char, weight: FontWeight) -> FaceKey {
        let wants_bold = weight.wants_bold() && self.bold.is_some();
        let key = (c, wants_bold);
        if let Some(&f) = self.route_cache.borrow().get(&key) {
            return f;
        }

        let face = if wants_bold && self.bold.as_ref().unwrap().lookup_glyph_index(c) != 0 {
            FACE_BOLD
        } else if self.font.lookup_glyph_index(c) != 0 {
            FACE_REGULAR
        } else {
            let mut found = FACE_REGULAR; // tofu in the primary face
            let mut fallbacks = self.fallbacks.borrow_mut();
            for (i, slot) in fallbacks.iter_mut().enumerate() {
                if let Fallback::Untried(path) = slot {
                    *slot = match std::fs::read(&path)
                        .ok()
                        .and_then(|b| Font::from_bytes(b.as_slice(), FontSettings::default()).ok())
                    {
                        Some(f) => Fallback::Loaded(f),
                        None => Fallback::Missing,
                    };
                }
                if let Fallback::Loaded(f) = slot {
                    if f.lookup_glyph_index(c) != 0 {
                        found = FACE_FALLBACK_BASE + i as FaceKey;
                        break;
                    }
                }
            }
            found
        };

        self.route_cache.borrow_mut().insert(key, face);
        face
    }

    /// Run `f` with the resolved face's Font.
    fn with_face<R>(&self, face: FaceKey, f: impl FnOnce(&Font) -> R) -> R {
        if face == FACE_BOLD {
            if let Some(b) = &self.bold {
                return f(b);
            }
        } else if face >= FACE_FALLBACK_BASE {
            let fallbacks = self.fallbacks.borrow();
            if let Some(Fallback::Loaded(fb)) = fallbacks.get((face - FACE_FALLBACK_BASE) as usize) {
                return f(fb);
            }
        }
        f(&self.font)
    }

    // ── Glyphs ────────────────────────────────────────────────────────────

    /// Shared handle to the cached glyph for `c` at `px`/`weight` —
    /// routed through the bold face and Unicode fallbacks.
    pub fn glyph_weighted(&self, c: char, px: f32, weight: FontWeight) -> CachedGlyph {
        let face = self.resolve(c, weight);
        let key = (face, c, px.to_bits());
        {
            let cache = self.glyph_cache.borrow();
            if let Some(entry) = cache.get(&key) {
                return Arc::clone(entry);
            }
        }
        let entry = Arc::new(self.with_face(face, |f| f.rasterize(c, px)));
        self.glyph_cache.borrow_mut().insert(key, Arc::clone(&entry));
        entry
    }

    /// Regular-weight glyph (hot path for plain text).
    pub fn glyph(&self, c: char, px: f32) -> CachedGlyph {
        self.glyph_weighted(c, px, FontWeight::Regular)
    }

    /// Rasterize a single character (copies the bitmap — prefer
    /// [`FontCache::glyph`] in hot paths).
    pub fn rasterize(&self, c: char, px: f32) -> (fontdue::Metrics, Vec<u8>) {
        let glyph = self.glyph(c, px);
        (glyph.0, glyph.1.clone())
    }

    /// Kerning between `left` and `right` at `px`/`weight`. Zero when the
    /// pair spans different faces (fallback boundaries have no kern data).
    pub fn kern_weighted(&self, left: char, right: char, px: f32, weight: FontWeight) -> f32 {
        let fl = self.resolve(left, weight);
        if fl != self.resolve(right, weight) {
            return 0.0;
        }
        self.with_face(fl, |f| f.horizontal_kern(left, right, px).unwrap_or(0.0))
    }

    pub fn kern(&self, left: char, right: char, px: f32) -> f32 {
        self.kern_weighted(left, right, px, FontWeight::Regular)
    }

    /// Pixel advance width at `px`/`weight`. Cached, fallback-routed.
    pub fn advance_width_weighted(&self, c: char, px: f32, weight: FontWeight) -> f32 {
        let face = self.resolve(c, weight);
        let key = (face, c, px.to_bits());
        {
            let cache = self.metrics_cache.borrow();
            if let Some(&w) = cache.get(&key) {
                return w;
            }
        }
        let w = self.with_face(face, |f| f.metrics(c, px).advance_width);
        self.metrics_cache.borrow_mut().insert(key, w);
        w
    }

    pub fn advance_width(&self, c: char, px: f32) -> f32 {
        self.advance_width_weighted(c, px, FontWeight::Regular)
    }

    /// Total pixel width of a string at `px`/`weight` — advances plus
    /// kerning, in lockstep with `SkiaCanvas::draw_text_weighted` so
    /// measured and painted widths agree.
    pub fn measure_text_weighted(&self, text: &str, px: f32, weight: FontWeight) -> f32 {
        let mut width = 0.0;
        let mut prev: Option<char> = None;
        for c in text.chars() {
            if let Some(p) = prev {
                width += self.kern_weighted(p, c, px, weight);
            }
            width += self.advance_width_weighted(c, px, weight);
            prev = Some(c);
        }
        width
    }

    pub fn measure_text(&self, text: &str, px: f32) -> f32 {
        self.measure_text_weighted(text, px, FontWeight::Regular)
    }

    /// Distance from the top of the line box to the baseline, in pixels.
    /// Always from the primary face — mixed-face runs share one baseline.
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
