//! Icon widget backed by the bundled Material Symbols Outlined font
//! (D115/Phase 32 Step 2).
//!
//! Icons are font glyphs rendered through the ordinary text pipeline: the
//! icon face is registered on the app's `FontCache` as an in-memory
//! fallback ([`rosace_render::FontCache::set_icon_face`]), so a plain
//! `DrawText` command carries each icon — physical-pixel rasterization on
//! HiDPI and the Phase 27 GPU glyph atlas come for free. (The alternative —
//! rasterizing here and blitting via `BlitRgba` — was rejected: blits are
//! recorded in logical pixels and bilinearly rescaled on Retina, i.e.
//! blurry.)
//!
//! Extensibility (the D115 exit bar): downstream crates call
//! [`register_icon`] to bind a name to any codepoint in the icon font and
//! render it with [`Icon::named`], or use [`Icon::glyph`] directly —
//! no edits to `rosace-widgets` required. Every Material Symbols name is
//! pre-registered from the bundled `.codepoints` table, so
//! `Icon::named("wifi")` works out of the box.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use rosace_core::types::{Point, Rect, Size};
use rosace_render::{fontdue, Color};
use super::{Widget, LayoutCtx, PaintCtx};

/// Bundled Material Symbols Outlined variable font (Apache 2.0 — see
/// `assets/LICENSE-MaterialSymbols.txt`). fontdue renders its default
/// instance (FILL 0, wght 400): the standard outlined style.
const ICON_FONT_BYTES: &[u8] =
    include_bytes!("../../assets/MaterialSymbolsOutlined.ttf");

/// The `name codepoint` table shipped alongside the font — seeds the
/// registry so every Material Symbols name resolves without registration.
const ICON_CODEPOINTS: &str =
    include_str!("../../assets/MaterialSymbolsOutlined.codepoints");

/// The parsed icon face, shared process-wide (parsed once — the variable
/// font is ~10 MB; every `FontCache` gets a clone of this one `Arc`).
fn icon_font() -> &'static Arc<fontdue::Font> {
    static FONT: OnceLock<Arc<fontdue::Font>> = OnceLock::new();
    FONT.get_or_init(|| {
        Arc::new(
            fontdue::Font::from_bytes(
                ICON_FONT_BYTES,
                fontdue::FontSettings::default(),
            )
            .expect("bundled Material Symbols font parses"),
        )
    })
}

/// name → codepoint registry, pre-seeded with the full Material Symbols
/// table on first use.
fn registry() -> &'static RwLock<HashMap<String, char>> {
    static REG: OnceLock<RwLock<HashMap<String, char>>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut map = HashMap::new();
        for line in ICON_CODEPOINTS.lines() {
            if let Some((name, hex)) = line.split_once(' ') {
                if let Some(c) = u32::from_str_radix(hex.trim(), 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    map.insert(name.to_string(), c);
                }
            }
        }
        RwLock::new(map)
    })
}

/// Register (or override) a named icon codepoint — the D115 extension
/// point. A downstream crate binds a name to any glyph in the icon font
/// and renders it with [`Icon::named`], without editing `rosace-widgets`:
///
/// ```ignore
/// rosace_widgets::register_icon("acme_logo", '\u{f0d3}');
/// Icon::named("acme_logo").size(24.0)
/// ```
pub fn register_icon(name: impl Into<String>, codepoint: char) {
    registry()
        .write()
        .expect("icon registry poisoned")
        .insert(name.into(), codepoint);
}

/// Resolve a registered icon name to its codepoint. All Material Symbols
/// names (from the bundled `.codepoints` table) are pre-registered.
pub fn resolve_icon(name: &str) -> Option<char> {
    registry()
        .read()
        .expect("icon registry poisoned")
        .get(name)
        .copied()
}

/// Built-in icon names (backward compatible — D115's Migration Rule keeps
/// every pre-Phase-32 variant working; they now render Material Symbols
/// glyphs instead of the old hand-drawn primitive approximations).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Check,
    Close,
    Add,
    Remove,
    Search,
    Menu,
    Arrow,
    ChevronRight,
    ChevronDown,
    Settings,
    User,
    Home,
    Inbox,
    Calendar,
    Star,
    Heart,
    Bell,
    Edit,
    Trash,
    Upload,
    Download,
    Filter,
    Sort,
    Grid,
    List,
    Circle,
    Dot,
}

impl IconKind {
    /// Every variant, in declaration order — for galleries and tests.
    pub const ALL: [IconKind; 27] = [
        IconKind::Check, IconKind::Close, IconKind::Add, IconKind::Remove,
        IconKind::Search, IconKind::Menu, IconKind::Arrow,
        IconKind::ChevronRight, IconKind::ChevronDown, IconKind::Settings,
        IconKind::User, IconKind::Home, IconKind::Inbox, IconKind::Calendar,
        IconKind::Star, IconKind::Heart, IconKind::Bell, IconKind::Edit,
        IconKind::Trash, IconKind::Upload, IconKind::Download,
        IconKind::Filter, IconKind::Sort, IconKind::Grid, IconKind::List,
        IconKind::Circle, IconKind::Dot,
    ];

    /// The Material Symbols codepoint this kind renders as (looked up in
    /// the bundled `.codepoints` table — this variable font's assignments
    /// differ from the legacy Material Icons set).
    ///
    /// `None` for the two geometric kinds: `Circle`/`Dot` mean a *filled*
    /// disc, but the variable font's default instance is FILL=0, where
    /// `circle` and `fiber_manual_record` rasterize as hollow rings
    /// (verified pixel-level). Those two keep their exact primitive
    /// `fill_circle` rendering — no visual regression.
    fn codepoint(self) -> Option<char> {
        Some(match self {
            IconKind::Check        => '\u{e668}', // check
            IconKind::Close        => '\u{e5cd}', // close
            IconKind::Add          => '\u{e145}', // add
            IconKind::Remove       => '\u{e15b}', // remove
            IconKind::Search       => '\u{ef7a}', // search
            IconKind::Menu         => '\u{e5d2}', // menu
            IconKind::Arrow        => '\u{e5c8}', // arrow_forward
            IconKind::ChevronRight => '\u{e5cc}', // chevron_right
            IconKind::ChevronDown  => '\u{e5cf}', // expand_more
            IconKind::Settings     => '\u{e8b8}', // settings
            IconKind::User         => '\u{f0d3}', // person
            IconKind::Home         => '\u{e9b2}', // home
            IconKind::Inbox        => '\u{e156}', // inbox
            IconKind::Calendar     => '\u{ebcc}', // calendar_month
            IconKind::Star         => '\u{f09a}', // star
            IconKind::Heart        => '\u{e87e}', // favorite
            IconKind::Bell         => '\u{e7f5}', // notifications
            IconKind::Edit         => '\u{f097}', // edit
            IconKind::Trash        => '\u{e92e}', // delete
            IconKind::Upload       => '\u{f09b}', // upload
            IconKind::Download     => '\u{f090}', // download
            IconKind::Filter       => '\u{e152}', // filter_list
            IconKind::Sort         => '\u{e164}', // sort
            IconKind::Grid         => '\u{e9b0}', // grid_view
            IconKind::List         => '\u{e896}', // list
            IconKind::Circle | IconKind::Dot => return None,
        })
    }
}

/// Where an [`Icon`]'s glyph comes from.
enum IconSource {
    Kind(IconKind),
    Glyph(char),
    Named(String),
}

/// A Material Symbols icon glyph rendered through the text pipeline at any
/// size. `Circle`/`Dot` stay primitive-drawn filled discs (see
/// [`IconKind::codepoint`]).
pub struct Icon {
    source: IconSource,
    pub size: f32,
    pub color: Color,
}

impl Icon {
    /// A built-in icon.
    pub fn new(kind: IconKind) -> Self {
        Self::from_source(IconSource::Kind(kind))
    }

    /// Any codepoint from the icon font — for glyphs without a built-in
    /// [`IconKind`] or registered name.
    pub fn glyph(codepoint: char) -> Self {
        Self::from_source(IconSource::Glyph(codepoint))
    }

    /// An icon by registered name ([`register_icon`]); all Material
    /// Symbols names are pre-registered. Resolved at paint time, so
    /// registration order doesn't matter; unresolved names paint a hollow
    /// placeholder box.
    pub fn named(name: impl Into<String>) -> Self {
        Self::from_source(IconSource::Named(name.into()))
    }

    fn from_source(source: IconSource) -> Self {
        Self { source, size: 16.0, color: Color::rgb(180, 184, 210) }
    }

    pub fn size(mut self, s: f32) -> Self { self.size = s; self }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }
}

impl Widget for Icon {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: self.size, height: self.size }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let r = ctx.rect;
        let cx = r.origin.x + r.size.width / 2.0;
        let cy = r.origin.y + r.size.height / 2.0;
        let s = self.size;
        let c = self.color;

        let glyph = match &self.source {
            IconSource::Kind(k)  => k.codepoint(),
            IconSource::Glyph(g) => Some(*g),
            IconSource::Named(n) => resolve_icon(n),
        };

        let Some(ch) = glyph else {
            match &self.source {
                IconSource::Kind(IconKind::Circle) => {
                    ctx.fill_circle(Point { x: cx, y: cy }, s * 0.4, c);
                }
                IconSource::Kind(IconKind::Dot) => {
                    ctx.fill_circle(Point { x: cx, y: cy }, s * 0.2, c);
                }
                // Unresolved name: hollow placeholder box (visible in dev,
                // unlike silently painting nothing).
                _ => {
                    let half = s * 0.35;
                    ctx.stroke_rect(
                        Rect {
                            origin: Point { x: cx - half, y: cy - half },
                            size: Size { width: half * 2.0, height: half * 2.0 },
                        },
                        c,
                        1.0,
                    );
                }
            }
            return;
        };

        // Route the icon face into this FontCache once (idempotent) so the
        // DrawText below resolves the PUA codepoint to the icon font.
        if !ctx.font.has_icon_face() {
            ctx.font.set_icon_face(Arc::clone(icon_font()));
        }

        // Center the glyph in the icon box. `layout_glyphs` places a glyph's
        // top-left at `origin + (xmin, ascender - ymin - height)`; solve for
        // the text origin that puts the glyph's raster center on (cx, cy).
        // Metrics here are at logical px; the canvas re-rasterizes at
        // physical px, which scales linearly — placement agrees within a
        // physical pixel.
        let m = ctx.font.glyph(ch, s).0;
        let asc = ctx.font.ascender(s) as f32;
        let origin = Point {
            x: cx - m.width as f32 / 2.0 - m.xmin as f32,
            y: cy - m.height as f32 / 2.0
                - (asc - m.ymin as f32 - m.height as f32),
        };
        let mut buf = [0u8; 4];
        ctx.draw_text_at(ch.encode_utf8(&mut buf), origin, c, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn lays_out_at_requested_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = Icon::new(IconKind::Star).size(24.0).layout(&ctx);
        assert_eq!(size.width, 24.0);
        assert_eq!(size.height, 24.0);
    }

    #[test]
    fn bundled_font_parses_and_covers_every_mapped_kind() {
        let font = icon_font();
        for kind in IconKind::ALL {
            let Some(cp) = kind.codepoint() else { continue };
            assert_ne!(
                font.lookup_glyph_index(cp), 0,
                "{kind:?} ({cp:?}) missing from the bundled icon font"
            );
            let (metrics, bitmap) = font.rasterize(cp, 24.0);
            assert!(
                metrics.width > 0 && bitmap.iter().any(|&b| b > 0),
                "{kind:?} ({cp:?}) rasterized empty"
            );
        }
    }

    #[test]
    fn downstream_crates_register_custom_icons_by_name() {
        // The D115 exit bar: a name NOT in the built-in set, bound to a
        // codepoint without editing rosace-widgets.
        assert_eq!(resolve_icon("acme_rocket"), None);
        register_icon("acme_rocket", '\u{eb9b}'); // rocket_launch
        assert_eq!(resolve_icon("acme_rocket"), Some('\u{eb9b}'));
        let _widget = Icon::named("acme_rocket").size(24.0);
    }

    #[test]
    fn material_names_are_preregistered_from_the_codepoints_table() {
        assert_eq!(resolve_icon("search"), Some('\u{ef7a}'));
        assert_eq!(resolve_icon("wifi"), Some('\u{e63e}'));
        assert_eq!(resolve_icon("not_a_real_icon_name"), None);
    }

    #[test]
    fn font_cache_routes_icon_codepoints_to_the_icon_face() {
        let cache = rosace_render::FontCache::embedded();
        cache.set_icon_face(Arc::clone(icon_font()));
        // 'search' is a PUA codepoint DejaVu lacks — with the icon face
        // installed it must rasterize with real coverage, through the same
        // glyph API the canvas text path uses.
        let glyph = cache.glyph('\u{ef7a}', 24.0);
        assert!(glyph.0.width > 0, "icon glyph resolved to tofu");
        assert!(glyph.1.iter().any(|&b| b > 0), "icon glyph has no coverage");
    }
}
