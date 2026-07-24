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
type ColorGlyphKey = (char, u32); // (char, px.to_bits())

/// Shared rasterized glyph: metrics + coverage bitmap.
pub type CachedGlyph = Arc<(fontdue::Metrics, Vec<u8>)>;

enum Fallback {
    Untried(&'static str),
    Missing,
    Loaded(Font),
}

/// Color-emoji fallback face (Phase 32 Step 4, D115): raw bytes retained
/// (unlike the plain-text `Fallback` above, which discards them once
/// `fontdue::Font` parses them) — `fontdue` only rasterizes vector
/// OUTLINES, but color emoji glyphs live in a bitmap table (`sbix` on
/// macOS: literally an embedded PNG per glyph per size, "up to the caller
/// to decode" per `ttf-parser`'s own doc comment), so decoding needs
/// `ttf_parser::Face` directly, re-parsed from these bytes on each lookup
/// (parsing itself is cheap — no re-reading the outline tables fontdue
/// already indexed).
enum EmojiFallback {
    Untried,
    Missing,
    Loaded(Arc<Vec<u8>>),
}

/// One decoded color glyph: real advance width (font units, ttf_parser's
/// own metric — NOT approximated from bitmap size) + a premultiplied RGBA8
/// bitmap (`tiny_skia::Pixmap::decode_png`'s own convention — the SAME one
/// `rosace-render::image`'s `Image` widget already decodes PNGs with, so
/// this reuses an already-consistent pixel-format contract, not a new one)
/// at whatever `sbix` strike size `glyph_raster_image` picked (nearest
/// available, not rescaled to the exact requested px — a named
/// simplification; see `color_glyph_rgba`'s doc).
pub struct ColorGlyph {
    pub advance: f32,
    pub width: u32,
    pub height: u32,
    /// `Arc`, not a plain `Vec` — matches `rosace_render::canvas::ImagePixels`
    /// (the `Image` widget's own blit-source wrapper), so the GPU-shapes path
    /// clones a refcount instead of the pixel bytes every repaint frame.
    pub rgba: Arc<Vec<u8>>,
}

/// Emoji fallback candidates per platform, in priority order — mirrors
/// `FALLBACK_PATHS`'s own per-platform-paths convention. Only macOS is
/// covered today (Apple Color Emoji, `sbix`); Windows (Segoe UI Emoji,
/// COLR/CPAL — a different table `ttf-parser` also supports via
/// `paint_color_glyph`, not wired here) and Linux (Noto Color Emoji, CBDT)
/// are a named, honest gap, not silently assumed to work.
const EMOJI_FALLBACK_PATHS: &[&str] = &[
    "/System/Library/Fonts/Apple Color Emoji.ttc",
];

/// Common emoji Unicode blocks — used to decide whether a character should
/// even ATTEMPT the color-glyph path (most text never does, so this check
/// must be cheap and must not itself trigger loading the emoji font).
/// Deliberately covers the well-known blocks, not a byte-for-byte match of
/// Unicode's own emoji-data.txt (that table also includes plain digits/`#`
/// as "emoji-capable" via keycap sequences — out of scope for this pass).
fn is_emoji_codepoint(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1FAFF // Misc Symbols&Pictographs, Emoticons, Transport, Supplemental Symbols&Pictographs, Symbols&Pictographs Ext-A
        | 0x2600..=0x27BF // Misc Symbols, Dingbats
        | 0x2190..=0x21FF // Arrows (subset render as emoji with presentation)
        | 0x2B00..=0x2BFF // Misc Symbols and Arrows
        | 0x1F1E6..=0x1F1FF // Regional indicators (flags)
    )
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
    /// Color-emoji fallback face (Phase 32 Step 4) — raw bytes, loaded
    /// lazily on the first emoji-range character (same "don't pay for it
    /// until needed" principle as `fallbacks` above).
    emoji: RefCell<EmojiFallback>,
    /// Decoded color glyphs, keyed like `glyph_cache` — PNG decode is real
    /// work (unlike a cached fontdue rasterize, which is already cheap),
    /// so this cache matters more, not less.
    color_glyph_cache: RefCell<HashMap<ColorGlyphKey, Option<Arc<ColorGlyph>>>>,
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
            emoji: RefCell::new(EmojiFallback::Untried),
            color_glyph_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let font = Font::from_bytes(bytes, FontSettings::default())
            .expect("invalid font bytes");
        Self::build(font, None)
    }

    /// Load a font from a bundled **asset** by logical name — resolved
    /// per-platform via [`rosace_core::asset`] (dev: `assets/<name>`; mobile:
    /// the app bundle). Returns `None` if the asset is missing or not a valid
    /// font, so callers can fall back to [`default`](Self::default)/[`embedded`].
    ///
    /// ```ignore
    /// let brand = FontCache::from_asset("fonts/Brand.ttf")
    ///     .unwrap_or_else(FontCache::default);
    /// ```
    pub fn from_asset(name: impl rosace_core::asset::AssetRef) -> Option<Self> {
        let bytes = rosace_core::asset::bytes(name)?;
        let font = Font::from_bytes(bytes.as_slice(), FontSettings::default()).ok()?;
        Some(Self::build(font, None))
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

    /// Lazily load the color-emoji fallback face's raw bytes (first
    /// emoji-range character only — same principle as `fallbacks`).
    fn emoji_bytes(&self) -> Option<Arc<Vec<u8>>> {
        {
            match &*self.emoji.borrow() {
                EmojiFallback::Loaded(b) => return Some(Arc::clone(b)),
                EmojiFallback::Missing => return None,
                EmojiFallback::Untried => {}
            }
        }
        let found = EMOJI_FALLBACK_PATHS.iter()
            .find_map(|p| std::fs::read(p).ok())
            .map(Arc::new);
        *self.emoji.borrow_mut() = match &found {
            Some(b) => EmojiFallback::Loaded(Arc::clone(b)),
            None => EmojiFallback::Missing,
        };
        found
    }

    /// Real color glyph for `c` at `px`, if `c` is in an emoji range AND the
    /// emoji fallback face actually has a color bitmap for it (`sbix` only
    /// today — see `EMOJI_FALLBACK_PATHS`'s doc for the Windows/Linux gap).
    /// `None` for anything else, including a plain character that happens
    /// to fail this lookup — callers fall through to the normal fontdue path.
    pub fn color_glyph_rgba(&self, c: char, px: f32) -> Option<Arc<ColorGlyph>> {
        if !is_emoji_codepoint(c) { return None; }

        let cache_key = (c, px.to_bits());
        if let Some(hit) = self.color_glyph_cache.borrow().get(&cache_key) {
            return hit.clone();
        }

        let result = (|| {
            let bytes = self.emoji_bytes()?;
            let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
            let gid = face.glyph_index(c)?;
            // NOT `face.is_color_glyph(gid)` — that method checks ONLY the
            // `COLR`/`CPAL` layered-vector table (confirmed by reading
            // ttf-parser's own source: `self.tables().colr...`), never
            // `sbix`. Apple Color Emoji uses `sbix` exclusively, so that
            // gate was always false here and this function always bailed —
            // a real bug caught only by noticing the LIVE app rendered tofu
            // boxes for real emoji despite an isolated unit test "passing"
            // (its own graceful-skip-if-font-missing branch silently
            // absorbed the same bug as a false "not installed" negative,
            // instead of catching it — a real lesson, not just a fix).
            // `glyph_raster_image` returning `Some` IS already proof this
            // glyph has a real color bitmap; no separate gate is needed.
            let img = face.glyph_raster_image(gid, px.round().clamp(1.0, u16::MAX as f32) as u16)?;
            if img.format != ttf_parser::RasterImageFormat::PNG { return None; }
            let pixmap = tiny_skia::Pixmap::decode_png(img.data).ok()?;
            let units_per_em = face.units_per_em() as f32;
            let advance = face.glyph_hor_advance(gid)
                .map(|a| a as f32 / units_per_em * px)
                .unwrap_or(pixmap.width() as f32);
            Some(Arc::new(ColorGlyph {
                advance,
                width: pixmap.width(),
                height: pixmap.height(),
                rgba: Arc::new(pixmap.data().to_vec()),
            }))
        })();

        self.color_glyph_cache.borrow_mut().insert(cache_key, result.clone());
        result
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
    /// `Some` for a color-emoji glyph (Phase 32 Step 4) — `glyph` above is
    /// then a cheap zero-size placeholder (never read) and consumers must
    /// blit this RGBA bitmap directly instead of using `glyph`'s coverage
    /// mask. Reuses the same premultiplied-RGBA-quad pipeline
    /// `DrawCommand::BlitRgba` (the `Image` widget) already established in
    /// both the CPU and GPU-shapes paths, rather than adding a second
    /// coverage-atlas page — real color rendering with no new render
    /// primitive.
    pub color_rgba: Option<Arc<ColorGlyph>>,
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
        // Variation selectors (U+FE00-U+FE0F, e.g. the "emoji presentation"
        // VS-16 that commonly follows a symbol like U+2600 SUN to request
        // its color form — as in "☀️" — real bug found live: this demo's
        // own "☀️ sunny" text rendered a stray tofu box for the selector
        // itself, since it has no visible glyph in ANY face and wasn't
        // being recognized as a zero-width modifier). Invisible by
        // definition — skip entirely, no glyph lookup, no cursor advance.
        if matches!(ch as u32, 0xFE00..=0xFE0F) {
            continue; // invisible modifier — `prev` stays the last REAL glyph for correct kerning after it
        }
        if let Some(p) = prev {
            cursor_x += font.kern_weighted(p, ch, px, weight);
        }
        prev = Some(ch);

        // Color-emoji check first: a real emoji codepoint should never fall
        // through to fontdue's outline rasterizer (the primary UI font has
        // no glyph for it at all, or — worse — a plain monochrome fallback
        // shape that isn't the real emoji).
        if let Some(cg) = font.color_glyph_rgba(ch, px) {
            let gx = cursor_x.round() as i32;
            let gy = base_y - cg.height as i32; // bottom-aligned to baseline, left-aligned to cursor
            let key = ((px.to_bits() as u64) << 32) | ((ch as u64) << 1) | bold | (1 << 63);
            let placeholder: CachedGlyph = Arc::new((fontdue::Metrics::default(), Vec::new()));
            let advance = cg.advance;
            out.push(PlacedGlyph { glyph: placeholder, x: gx, y: gy, key, color_rgba: Some(cg) });
            cursor_x += advance;
            continue;
        }

        let glyph = font.glyph_weighted(ch, px, weight);
        let advance = glyph.0.advance_width;
        if glyph.0.width != 0 && glyph.0.height != 0 {
            let gx = cursor_x.round() as i32 + glyph.0.xmin;
            let gy = base_y - glyph.0.ymin - glyph.0.height as i32;
            let key = ((px.to_bits() as u64) << 32) | ((ch as u64) << 1) | bold;
            out.push(PlacedGlyph { glyph, x: gx, y: gy, key, color_rgba: None });
        }
        cursor_x += advance;
    }
    out
}

#[cfg(test)]
mod color_glyph_tests {
    use super::*;

    #[test]
    fn is_emoji_codepoint_covers_common_emoji_but_not_plain_text() {
        assert!(is_emoji_codepoint('😀')); // U+1F600, Emoticons block
        assert!(is_emoji_codepoint('🎉')); // U+1F389, Misc Symbols & Pictographs
        assert!(is_emoji_codepoint('☀'));  // U+2600, Misc Symbols
        assert!(!is_emoji_codepoint('a'));
        assert!(!is_emoji_codepoint('#'));
        assert!(!is_emoji_codepoint(' '));
    }

    /// Real integration test, not a mock: decodes an ACTUAL emoji glyph from
    /// whatever color-emoji font this machine has installed (this repo's
    /// dev machines are macOS, where `/System/Library/Fonts/Apple Color
    /// Emoji.ttc` is always present) — the exit bar this Phase 32 Step 4
    /// task is actually held to ("a real running app renders a string
    /// containing at least one emoji correctly, real color").
    ///
    /// The skip path checks the font FILE's existence directly, separately
    /// from whether decode succeeded — a lesson from a real bug this test
    /// almost hid: an earlier version skipped whenever `color_glyph_rgba`
    /// returned `None`, which is EXACTLY what it did (every time) while a
    /// real bug (`is_color_glyph` checking the wrong table) was silently
    /// swallowing every lookup on this very machine, where the font
    /// genuinely IS installed. A skip must only fire for the environment
    /// gap it claims to be about, or it stops being a skip and becomes a
    /// blindfold.
    #[test]
    fn color_glyph_rgba_decodes_a_real_emoji_on_this_machine() {
        if EMOJI_FALLBACK_PATHS.iter().all(|p| !std::path::Path::new(p).exists()) {
            eprintln!("no color-emoji font file on this machine — skipping (not a failure)");
            return;
        }
        let font = FontCache::embedded();
        let cg = font.color_glyph_rgba('😀', 32.0)
            .expect("font file exists but color_glyph_rgba returned None — a real bug, not an environment gap");
        assert!(cg.width > 0 && cg.height > 0, "decoded bitmap must have real dimensions");
        assert_eq!(cg.rgba.len(), (cg.width * cg.height * 4) as usize, "RGBA8 buffer must match width*height*4");
        assert!(cg.advance > 0.0, "a real emoji must have a positive advance width");
        // At least one non-transparent, non-black pixel — a real decoded
        // photo/icon, not an all-zero buffer silently accepted as "success".
        let has_real_color = cg.rgba.chunks_exact(4).any(|p| p[3] > 0 && (p[0] > 20 || p[1] > 20 || p[2] > 20));
        assert!(has_real_color, "decoded emoji must contain real non-black visible pixels");
    }

    #[test]
    fn color_glyph_rgba_returns_none_for_plain_text() {
        let font = FontCache::embedded();
        assert!(font.color_glyph_rgba('a', 16.0).is_none());
    }

    #[test]
    fn layout_glyphs_places_a_real_emoji_with_color_rgba_set() {
        // px=16.0 deliberately: the exact size that exposed the
        // `is_color_glyph` bug live (the earlier isolated test used 32.0,
        // which happens to be an exact sbix strike — this one must NOT
        // rely on that coincidence, since the real app renders at 16/24px).
        if EMOJI_FALLBACK_PATHS.iter().all(|p| !std::path::Path::new(p).exists()) {
            eprintln!("no color-emoji font file on this machine — skipping (not a failure)");
            return;
        }
        let font = FontCache::embedded();
        let placed = layout_glyphs(&font, "hi 😀 there", 0.0, 0.0, 16.0, FontWeight::Regular);
        let pg = placed.iter().find(|pg| pg.color_rgba.is_some())
            .expect("font file exists but no placed glyph had color_rgba set — a real bug, not an environment gap");
        let cg = pg.color_rgba.as_ref().unwrap();
        assert!(cg.width > 0 && cg.height > 0);
        // Regardless of emoji decode availability, the surrounding plain
        // text must still be placed normally.
        assert!(placed.iter().any(|pg| pg.color_rgba.is_none()), "plain characters must still be placed");
    }

    #[test]
    fn variation_selector_16_produces_no_placed_glyph() {
        // Real bug found live: "☀️" (U+2600 SUN + U+FE0F VARIATION
        // SELECTOR-16, requesting the emoji/color presentation) rendered a
        // stray tofu box for the selector itself — it has no visible glyph
        // in any face and wasn't recognized as a zero-width modifier.
        // A bare selector with nothing else in the string must place NOTHING.
        let font = FontCache::embedded();
        let placed = layout_glyphs(&font, "\u{FE0F}", 0.0, 0.0, 16.0, FontWeight::Regular);
        assert!(placed.is_empty(), "a lone variation selector must never produce a placed glyph");

        // "☀️" must place AT MOST one glyph (the sun itself, color or
        // monochrome depending on font availability) — never two, which
        // would mean the selector also got its own tofu-box glyph.
        let placed = layout_glyphs(&font, "\u{2600}\u{FE0F}", 0.0, 0.0, 16.0, FontWeight::Regular);
        assert!(placed.len() <= 1, "the selector must not add a second placed glyph, got {}", placed.len());
    }
}
