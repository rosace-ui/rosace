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

/// Which face a glyph resolved to: primary regular/bold, the registered
/// icon face, or a Unicode fallback face by index.
type FaceKey = u8;
const FACE_REGULAR: FaceKey = 0;
const FACE_BOLD: FaceKey = 1;
const FACE_ICON: FaceKey = 2;
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
    /// In-memory icon face (D115/Phase 32 Step 2) — registered once by the
    /// widget layer, consulted when the primary faces miss a codepoint and
    /// BEFORE the disk fallback chain: icon fonts live in the Private Use
    /// Area, where system fallback faces (Apple Symbols et al.) carry their
    /// own unrelated glyphs.
    icon: RefCell<Option<Arc<Font>>>,
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
            icon: RefCell::new(None),
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

    /// A fallback font compiled into the binary — DejaVu Sans (permissive
    /// Bitstream Vera license). Used when no system font is available, most
    /// importantly on the web/wasm target where `system_ui()` finds nothing.
    /// Guarantees text always renders on every platform.
    pub fn embedded() -> Self {
        const DEJAVU_SANS: &[u8] =
            include_bytes!("../../assets/fonts/DejaVuSans.ttf");
        Self::from_bytes(DEJAVU_SANS)
    }

    /// The DEFAULT app font (Phase 32, user-decided): bundled Inter (SIL
    /// OFL — `assets/fonts/inter/LICENSE-OFL.txt`), the same pleasant,
    /// screen-tuned face on EVERY platform with clearly differentiable
    /// weights — Regular for body, real Bold (700) for emphasis. Replaces
    /// "whatever the OS ships" as the default (`system_ui()` remains
    /// available as an opt-in); also replaces the short-lived
    /// Medium-by-default experiment, which read slightly bold.
    ///
    /// Italic faces (`Inter-Italic`/`Inter-BoldItalic`) are bundled
    /// alongside but not yet wired — the text pipeline has no italic
    /// axis yet (tracked in `PHASE_32.md`).
    pub fn bundled() -> Self {
        const INTER_REGULAR: &[u8] =
            include_bytes!("../../assets/fonts/inter/Inter-Regular.ttf");
        const INTER_BOLD: &[u8] =
            include_bytes!("../../assets/fonts/inter/Inter-Bold.ttf");
        let regular = Font::from_bytes(INTER_REGULAR, FontSettings::default())
            .expect("bundled Inter Regular is valid");
        let bold = Font::from_bytes(INTER_BOLD, FontSettings::default())
            .expect("bundled Inter Bold is valid");
        Self::build(regular, Some(bold))
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

    /// Score how well `name` (a face's full/typographic name) matches the
    /// weight we're looking for. Higher is better; `None` means "not a
    /// candidate at all" for this weight.
    ///
    /// This exists because `.ttc` collections (how macOS ships every UI
    /// family — Avenir Next, Helvetica Neue, ...) do NOT put the Regular
    /// face at index 0. `fontdue::Font::from_bytes` defaults to index 0,
    /// so naively loading a `.ttc` silently picks WHATEVER face happens to
    /// be first — on Avenir Next.ttc that's actually "Avenir Next Bold".
    /// Loading that as "regular" and then falling back to an unrelated
    /// Arial Bold for "bold" produces two different type families where
    /// the nominal "bold" face is visually THINNER than the nominal
    /// "regular" one — bold becomes visually indistinguishable (or
    /// reversed) from regular. Real fix: read the name table and pick the
    /// actual matching face for each weight, from the same family when
    /// possible.
    fn weight_score(name: &str, want_bold: bool) -> Option<i32> {
        let n = name.to_ascii_lowercase();
        if n.contains("italic") || n.contains("oblique") {
            return None;
        }
        if want_bold {
            if n.ends_with("bold") && !n.contains("semi") && !n.contains("demi")
                && !n.contains("ultra") && !n.contains("extra")
            {
                return Some(3);
            }
            if n.contains("bold") { return Some(2); }
            if n.contains("heavy") || n.contains("black") { return Some(1); }
            None
        } else {
            if n.ends_with("regular") || n == "regular" { return Some(3); }
            if !n.contains("bold") && !n.contains("black") && !n.contains("heavy")
                && !n.contains("light") && !n.contains("thin") && !n.contains("medium")
                && !n.contains("demi") && !n.contains("semi") && !n.contains("condensed")
                && !n.contains("narrow") && !n.contains("ultra") && !n.contains("extra")
            {
                return Some(2);
            }
            Some(0)
        }
    }

    fn face_name(bytes: &[u8], index: u32) -> Option<String> {
        let face = ttf_parser::Face::parse(bytes, index).ok()?;
        face.names().into_iter()
            .find(|n| n.name_id == 4 && n.is_unicode())
            .and_then(|n| n.to_string())
    }

    /// Pick the best-matching face index in `bytes` for `want_bold`, or
    /// `None` if it isn't a (usable) collection / no good candidate exists.
    fn best_face_index(bytes: &[u8], want_bold: bool) -> Option<u32> {
        let n = ttf_parser::fonts_in_collection(bytes).unwrap_or(1);
        let mut best: Option<(i32, u32)> = None;
        for i in 0..n {
            let Some(name) = Self::face_name(bytes, i) else { continue };
            let Some(score) = Self::weight_score(&name, want_bold) else { continue };
            if best.map(|(s, _)| score > s).unwrap_or(true) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    }

    fn load_face(bytes: &[u8], index: u32) -> Option<Font> {
        Font::from_bytes(bytes, FontSettings { collection_index: index, ..FontSettings::default() }).ok()
    }

    /// Load a system proportional / UI font plus (when available) a real
    /// bold face and the Unicode fallback chain. Prefers a same-family
    /// bold face found inside the regular candidate's own file (see
    /// [`Self::weight_score`]); only falls back to the unrelated
    /// `BOLD_PATHS` standalone files when the chosen family has no bold
    /// member of its own (e.g. plain `Arial.ttf`, which IS the regular
    /// face and needs the separate `Arial Bold.ttf`).
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
        for path in candidates {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let reg_idx = Self::best_face_index(&bytes, false).unwrap_or(0);
            let Some(regular) = Self::load_face(&bytes, reg_idx) else { continue };
            let bold = Self::best_face_index(&bytes, true)
                .and_then(|i| Self::load_face(&bytes, i))
                .or_else(|| Self::load_first(BOLD_PATHS));
            return Some(Self::build(regular, bold));
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
        let regular = Self::load_first(&candidates)?;
        Some(Self::build(regular, None))
    }

    // ── Icon face (D115/Phase 32 Step 2) ─────────────────────────────────

    /// Install an in-memory icon face — glyphs the primary faces miss route
    /// to it before the disk fallback chain, so icon-font codepoints (PUA)
    /// flow through the ordinary text path: physical-px rasterization,
    /// glyph cache, and the GPU glyph atlas, with zero new draw commands.
    ///
    /// Idempotent: the first registration wins; later calls are no-ops.
    /// Registration clears the route cache so codepoints resolved earlier
    /// (as tofu) re-route to the new face.
    pub fn set_icon_face(&self, font: Arc<Font>) {
        {
            let mut slot = self.icon.borrow_mut();
            if slot.is_some() {
                return;
            }
            *slot = Some(font);
        }
        self.route_cache.borrow_mut().clear();
    }

    /// True once an icon face is installed — lets callers skip
    /// re-registration on every paint.
    pub fn has_icon_face(&self) -> bool {
        self.icon.borrow().is_some()
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
        } else if self
            .icon
            .borrow()
            .as_ref()
            .is_some_and(|f| f.lookup_glyph_index(c) != 0)
        {
            FACE_ICON
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
        } else if face == FACE_ICON {
            let icon = self.icon.borrow();
            if let Some(i) = icon.as_ref() {
                return f(i);
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

/// One glyph placed by [`layout_glyphs`]: the cached rasterization plus its
/// top-left pixel position and a stable atlas key (D109/Phase 27 Step 4).
pub struct PlacedGlyph {
    pub glyph: CachedGlyph,
    /// Top-left of the glyph bitmap, physical px.
    pub x: i32,
    pub y: i32,
    /// Stable across frames: `px_bits << 32 | char << 1 | wants_bold`.
    /// Face routing is deterministic per `(char, bold)`, so this fully
    /// identifies the rasterization without exposing `FaceKey`.
    pub key: u64,
}

/// The one glyph-placement walk (kerning, baseline, bearing) shared by the
/// CPU blit path (`SkiaCanvas::draw_text_weighted`) and the GPU atlas
/// collect path — they MUST agree glyph-for-glyph, so the math lives once.
///
/// `origin` is the line box's top-left in physical px (the baseline is
/// derived via [`FontCache::ascender`]); zero-size glyphs (spaces) advance
/// the cursor but emit nothing.
pub fn layout_glyphs(
    font: &FontCache,
    text: &str,
    origin_x: f32,
    origin_y: f32,
    px: f32,
    weight: FontWeight,
) -> Vec<PlacedGlyph> {
    let base_y = origin_y.round() as i32 + font.ascender(px);
    let mut cursor_x = origin_x;
    let mut prev: Option<char> = None;
    let mut out = Vec::with_capacity(text.len());
    let bold = weight.wants_bold() as u64;

    for ch in text.chars() {
        if let Some(p) = prev {
            cursor_x += font.kern_weighted(p, ch, px, weight);
        }
        prev = Some(ch);

        let glyph = font.glyph_weighted(ch, px, weight);
        let advance = glyph.0.advance_width;
        if glyph.0.width != 0 && glyph.0.height != 0 {
            let gx = cursor_x.round() as i32 + glyph.0.xmin;
            let gy = base_y - glyph.0.ymin - glyph.0.height as i32;
            let key = ((px.to_bits() as u64) << 32) | ((ch as u64) << 1) | bold;
            out.push(PlacedGlyph { glyph, x: gx, y: gy, key });
        }
        cursor_x += advance;
    }
    out
}
