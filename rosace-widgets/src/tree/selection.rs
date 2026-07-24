//! Theme-driven text-selection styling (D105 ext pattern, D124 follow-up).
//!
//! Registered ONCE on the theme — never per-widget:
//!
//! ```rust,ignore
//! let theme = dark_theme().with_ext(SelectionStyle::glass());
//! ```
//!
//! `TextInput`/`TextArea` resolve it from `ctx.theme.ext::<SelectionStyle>()`
//! at paint time; no style registered means [`SelectionStyle::flat`] — the
//! exact pre-existing look, so apps that never touch this see zero change.
//!
//! Two built-in kinds:
//! - [`SelectionKind::Flat`] — Material-style: flat tint band behind the
//!   glyphs + round drag grips below each endpoint.
//! - [`SelectionKind::Glass`] — Liquid-Glass-style: softer tint, iOS-style
//!   lollipop handles (vertical bar through the line, grip at the bottom
//!   anchor the engine's handle-drag already targets), and a
//!   backdrop-sampling MAGNIFIER pill over the selected text — the Phase
//!   28 Step 7 magnifier, finally landed. Single-line fields only; a
//!   multi-line lens (TextArea) is deferred with the same GPU-path notes
//!   as every backdrop material.

use rosace_render::Color;

/// Which selection look to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionKind {
    /// Flat tint + round grips (the Material/default look).
    Flat,
    /// Glass: soft tint, lollipop handles, magnifier lens over the
    /// selection (GPU/backdrop path; degrades to the tint alone elsewhere).
    Glass,
}

/// Theme-extension value carrying the selection look — see the module doc.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectionStyle {
    pub kind: SelectionKind,
    /// Tint painted behind the selected glyphs.
    pub highlight: Color,
    /// Handle grip/bar color.
    pub handle: Color,
    /// Glass only: lens magnification (1.0 = no zoom).
    pub zoom: f32,
}

impl SelectionStyle {
    /// The Material/default look — exactly the colors the widgets used
    /// before selection became themeable.
    pub fn flat() -> Self {
        Self {
            kind: SelectionKind::Flat,
            highlight: Color::rgba(110, 75, 210, 90),
            handle: Color::rgb(180, 160, 255),
            zoom: 1.0,
        }
    }

    /// The Liquid-Glass look: cooler, softer tint; near-white glass
    /// handles; a magnifier lens over the selected run. Zoom is a subtle
    /// 1.15x "lift" — a stronger lens crops the ends of the selection out
    /// of the pill (physically correct, but wrong as selection UX; found
    /// live at 1.35x).
    pub fn glass() -> Self {
        Self {
            kind: SelectionKind::Glass,
            // A STRONG dark band, on purpose: the lens magnifies whatever
            // sits behind the glyphs, and the backdrop under a glass
            // surface can be arbitrarily bright (found live — a bright
            // aurora blob drifting behind the input washed the magnified
            // text out with the earlier subtle tints). A near-opaque dark
            // band guarantees light-theme glyph contrast inside the lens
            // no matter what the scene does, and reads clearly as the
            // selection on paths where the lens can't render.
            highlight: Color::rgba(28, 36, 110, 120),
            handle: Color::rgba(235, 240, 255, 230),
            zoom: 1.25,
        }
    }

    /// Pack the lens uniforms (`radius`, `zoom` + two pad scalars) in the
    /// WGSL layout `builtin::SELECTION_LENS` declares — four tightly
    /// packed f32s, one 16-byte uniform row.
    pub fn lens_uniforms(radius: f32, zoom: f32) -> Vec<u8> {
        let mut out = Vec::with_capacity(16);
        for v in [radius, zoom, 0.0f32, 0.0f32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// The glass lens geometry for a single-line selection spanning
    /// `x0..x1` on a line at `y_top` with height `line_h` — the pill rect,
    /// the end-bar x positions, and the grip anchor y. Used by BOTH
    /// `TextInput`'s painting AND the engine's handle-drag grab, so the
    /// visible grips and the draggable anchors can never drift apart
    /// (found live: grips pinned at the unzoomed endpoints floated far
    /// inside the pill on wide selections, disconnected from the bars).
    pub fn glass_lens(&self, x0: f32, x1: f32, y_top: f32, line_h: f32) -> GlassLens {
        let cx = (x0 + x1) * 0.5;
        let cy = y_top + line_h * 0.5;
        let w = (x1 - x0) * self.zoom + 8.0;
        let h = line_h * self.zoom + 6.0;
        let rect = (cx - w / 2.0, cy - h / 2.0, w, h);
        GlassLens {
            rect,
            bar_x: (rect.0 + 3.5, rect.0 + w - 3.5),
            grip_y: cy + h / 2.0,
        }
    }
}

/// See [`SelectionStyle::glass_lens`]. Plain data, `(x, y, w, h)` rects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassLens {
    pub rect: (f32, f32, f32, f32),
    /// Left and right end-bar center x — the visual cursors, at the pill's
    /// outer edges, always bounding every magnified glyph.
    pub bar_x: (f32, f32),
    /// Grip circle center y — hanging at the pill's bottom edge, directly
    /// under each bar (the lollipop). The engine grabs here too.
    pub grip_y: f32,
}

impl Default for SelectionStyle {
    fn default() -> Self {
        Self::flat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_the_flat_pre_themeable_look() {
        let s = SelectionStyle::default();
        assert_eq!(s.kind, SelectionKind::Flat);
        assert_eq!(s.highlight, Color::rgba(110, 75, 210, 90));
        assert_eq!(s.handle, Color::rgb(180, 160, 255));
    }

    #[test]
    fn glass_kind_carries_a_real_zoom() {
        let s = SelectionStyle::glass();
        assert_eq!(s.kind, SelectionKind::Glass);
        assert!(s.zoom > 1.0, "a lens that doesn't magnify isn't a lens");
    }

    #[test]
    fn glass_lens_bounds_the_magnified_selection_with_connected_lollipops() {
        let st = SelectionStyle::glass();
        let g = st.glass_lens(100.0, 200.0, 50.0, 20.0);
        // Pill spans the MAGNIFIED selection (plus padding), centered.
        assert!(g.rect.2 > (200.0 - 100.0) * st.zoom, "pill must fit sel*zoom");
        assert!((g.rect.0 + g.rect.2 / 2.0 - 150.0).abs() < 0.01, "centered on selection");
        // Bars sit INSIDE the pill, grips hang at its bottom edge.
        assert!(g.bar_x.0 > g.rect.0 && g.bar_x.1 < g.rect.0 + g.rect.2);
        assert!((g.grip_y - (g.rect.1 + g.rect.3)).abs() < 0.01, "grip at pill bottom");
    }

    #[test]
    fn lens_uniforms_pack_radius_then_zoom_in_sixteen_bytes() {
        let b = SelectionStyle::lens_uniforms(12.0, 1.35);
        assert_eq!(b.len(), 16);
        assert_eq!(&b[0..4], &12.0f32.to_le_bytes());
        assert_eq!(&b[4..8], &1.35f32.to_le_bytes());
        assert_eq!(&b[8..16], &[0u8; 8]);
    }
}
