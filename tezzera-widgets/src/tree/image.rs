use std::sync::Arc;

use tezzera_core::types::{Point, Rect, Size};
use tezzera_render::{Color, DrawCommand};

use super::{Widget, LayoutCtx, PaintCtx};
use crate::{ImageWidget as ImageWidgetImpl, ImageFit, ImageSource};

/// A tree-compatible image widget that blits a PNG file or bytes onto the canvas.
///
/// Wraps [`crate::ImageWidget`] as a [`Widget`] so it can be used as a child
/// of `Column`, `Row`, `Stack`, etc.
pub struct Image {
    inner: ImageWidgetImpl,
}

impl Image {
    /// Create an image from a file path.
    pub fn file(path: impl Into<std::path::PathBuf>) -> Self {
        Self { inner: ImageWidgetImpl::new().file(path).fit(ImageFit::Contain) }
    }

    /// Create an image from raw PNG bytes.
    pub fn bytes(data: Vec<u8>) -> Self {
        Self { inner: ImageWidgetImpl::new().bytes(data).fit(ImageFit::Contain) }
    }

    /// Show a colored placeholder rectangle (no image data).
    pub fn placeholder(color: Color) -> Self {
        Self { inner: ImageWidgetImpl::new().placeholder_color(color) }
    }

    pub fn fit(mut self, f: ImageFit) -> Self { self.inner = self.inner.fit(f); self }
    pub fn width(mut self, w: f32) -> Self    { self.inner = self.inner.width(w); self }
    pub fn height(mut self, h: f32) -> Self   { self.inner = self.inner.height(h); self }
    /// Accessible/SEO alt text (D107/Phase 25).
    pub fn alt(mut self, alt: impl Into<String>) -> Self { self.inner = self.inner.alt(alt); self }
}

impl Widget for Image {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: self.inner.width, height: self.inner.height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // No entry at all for a decorative image (no `.alt(...)` set) —
        // matches HTML's own convention (see the `alt` field doc).
        if let Some(alt) = &self.inner.alt {
            ctx.semantics(super::Semantics::new(tezzera_core::Role::Image).label(alt));
        }
        let x = ctx.rect.origin.x;
        let y = ctx.rect.origin.y;
        let w = self.inner.width;
        let h = self.inner.height;
        let dest_rect = Rect { origin: Point { x, y }, size: Size { width: w, height: h } };

        // Try to load and decode the source pixels.
        let raw_bytes = match &self.inner.source {
            ImageSource::Placeholder => None,
            ImageSource::File(path) => std::fs::read(path).ok(),
            ImageSource::Bytes(b) => Some(b.clone()),
        };

        if let Some(bytes) = raw_bytes {
            if let Ok(pixmap) = tiny_skia::Pixmap::decode_png(&bytes) {
                // Load-in fade (D108/Phase 26 Step 4): decoding is fully
                // synchronous in this crate today (no async pipeline, no
                // cache — re-decoded every paint, see the crate's own
                // orphaned `ImageCache`, flagged in CRATE_CONTRACTS.md),
                // so there's no real async gap to fade across. What this
                // fades is the honest thing available: the first frame
                // THIS node has real decoded content, opacity starts at 0
                // (via `seed_anim_if_unset`, which opts out of
                // `animate_to`'s normal "first observation snaps"
                // behavior) and eases to 1 — not a placeholder-to-loaded
                // crossfade, since there is no tracked "was a placeholder"
                // state to cross-fade from.
                ctx.seed_anim_if_unset(0.0);
                let opacity = ctx.animate_to(1.0, 0.0);
                ctx.record(DrawCommand::BlitRgba {
                    pixels: Arc::new(pixmap.data().to_vec()),
                    src_width: pixmap.width(),
                    src_height: pixmap.height(),
                    dest_rect,
                    opacity,
                });
                return;
            }
        }

        // Placeholder: colored box + icon.
        ctx.fill_rect(dest_rect, self.inner.placeholder_color);
        ctx.fill_circle(Point { x: x + w / 2.0, y: y + h / 2.0 - 15.0 }, 12.0, Color::rgb(100, 110, 140));
        ctx.fill_rect(Rect {
            origin: Point { x: x + w / 2.0 - 20.0, y: y + h / 2.0 + 5.0 },
            size: Size { width: 40.0, height: 20.0 },
        }, Color::rgb(80, 90, 120));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tezzera_render::FontCache;
    use tezzera_theme::built_in;

    fn make_ctx(c: tezzera_layout::Constraints) -> (FontCache, tezzera_theme::ThemeData) {
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("no system font");
        (font, built_in::dark_theme())
    }

    #[test]
    fn image_placeholder_layout() {
        let img = Image::placeholder(Color::rgb(128, 128, 128)).width(320.0).height(200.0);
        let c = tezzera_layout::Constraints::loose(800.0, 600.0);
        let (font, theme) = make_ctx(c);
        let ctx = LayoutCtx::new(c, &font, &theme);
        let size = img.layout(&ctx);
        assert_eq!(size.width, 320.0);
        assert_eq!(size.height, 200.0);
    }

    #[test]
    fn image_file_layout() {
        let img = Image::file("assets/photo.png").width(100.0).height(80.0);
        let c = tezzera_layout::Constraints::loose(800.0, 600.0);
        let (font, theme) = make_ctx(c);
        let ctx = LayoutCtx::new(c, &font, &theme);
        let size = img.layout(&ctx);
        assert_eq!(size.width, 100.0);
        assert_eq!(size.height, 80.0);
    }
}
