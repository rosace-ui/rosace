use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand};

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

    /// Create an image from a bundled **asset** by logical name — resolved
    /// per-platform via [`rosace_core::asset`] (dev: `assets/<name>`; mobile:
    /// the app bundle). Portable across platforms and hot-reloads under
    /// `rsc dev`. Prefer this over [`file`](Self::file) for shipped images.
    pub fn asset(name: impl rosace_core::asset::AssetRef) -> Self {
        Self { inner: ImageWidgetImpl::new().asset(name).fit(ImageFit::Contain) }
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
            ctx.semantics(super::Semantics::new(rosace_core::Role::Image).label(alt));
        }
        let x = ctx.rect.origin.x;
        let y = ctx.rect.origin.y;
        let w = self.inner.width;
        let h = self.inner.height;
        let dest_rect = Rect { origin: Point { x, y }, size: Size { width: w, height: h } };

        // Decode ONCE via the global cache (Phase 27 — this paint
        // previously did `fs::read` + PNG decode on EVERY frame, the
        // "orphaned ImageCache" known issue). The cached `Arc` is recorded
        // directly: zero pixel copies per frame, and its stable content
        // gives the compositor a stable GPU-texture key.
        let decoded = {
            let mut cache = crate::ImageCache::global().lock().unwrap_or_else(|e| e.into_inner());
            match &self.inner.source {
                ImageSource::Placeholder => None,
                ImageSource::File(path) => cache.get_or_load(path),
                ImageSource::Bytes(b) => cache.get_or_decode_bytes(b),
            }
        };

        if let Some(img) = decoded {
            // No default load-in fade (D111 corrects D108/Phase 26 Step
            // 4): this widget has no stable per-image identity inside a
            // virtualized list (`ListView` allocates render-tree nodes
            // positionally by viewport slot, not by data index — see
            // D111), so an `animate_to`-driven fade here would bind its
            // animated opacity to whichever image currently occupies a
            // given on-screen slot, not to the image itself. Full
            // opacity, always, is the only default that's correct in
            // every context. `opacity` stays a real per-call parameter
            // on `DrawCommand::BlitRgba` for callers with real identity
            // (e.g. Hero transitions) to use deliberately.
            ctx.record(DrawCommand::BlitRgba {
                pixels: img.pixels,
                src_width: img.width,
                src_height: img.height,
                dest_rect,
                opacity: 1.0,
            });
            return;
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
    use rosace_render::FontCache;
    use rosace_theme::built_in;

    fn make_ctx(_c: rosace_layout::Constraints) -> (FontCache, rosace_theme::ThemeData) {
        let font = FontCache::system_ui()
            .or_else(FontCache::system_mono)
            .expect("no system font");
        (font, built_in::dark_theme())
    }

    #[test]
    fn image_placeholder_layout() {
        let img = Image::placeholder(Color::rgb(128, 128, 128)).width(320.0).height(200.0);
        let c = rosace_layout::Constraints::loose(800.0, 600.0);
        let (font, theme) = make_ctx(c);
        let ctx = LayoutCtx::new(c, &font, &theme);
        let size = img.layout(&ctx);
        assert_eq!(size.width, 320.0);
        assert_eq!(size.height, 200.0);
    }

    #[test]
    fn image_file_layout() {
        let img = Image::file("assets/photo.png").width(100.0).height(80.0);
        let c = rosace_layout::Constraints::loose(800.0, 600.0);
        let (font, theme) = make_ctx(c);
        let ctx = LayoutCtx::new(c, &font, &theme);
        let size = img.layout(&ctx);
        assert_eq!(size.width, 100.0);
        assert_eq!(size.height, 80.0);
    }
}
