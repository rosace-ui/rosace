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
            ctx.semantics(super::SemanticsProps::new(rosace_core::Role::Image).label(alt));
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

        // Real image data was requested (file/bytes/asset) but decode came
        // back empty (missing file, corrupt data) — a BROKEN state,
        // distinct from an intentional `Image::placeholder()`. Both used to
        // render identically (found live: no way to tell "no image was
        // ever asked for" from "the image failed to load").
        let broken = decoded.is_none() && !matches!(self.inner.source, ImageSource::Placeholder);

        if let Some(img) = decoded {
            // Honour `.fit(..)`. This used to blit straight into `dest_rect`
            // (the widget box) and never consult `self.inner.fit` at all, so
            // EVERY image was stretched to its box and the aspect ratio the
            // API promised was silently ignored. `compute_fit` lived only on
            // the legacy non-tree paint path, which nothing calls any more.
            //
            // `BlitRgba` carries no source-crop rect, so `Cover` and `None`
            // are expressed as an oversized/undersized dest rect bracketed by
            // a clip to the widget box rather than by cropping the source.
            let (src_w, src_h) = (img.width as f32, img.height as f32);
            let (dest_rect, clip) = fit_rect(self.inner.fit, src_w, src_h, dest_rect);
            if clip {
                ctx.record(DrawCommand::PushClip { rect: Rect {
                    origin: Point { x, y }, size: Size { width: w, height: h },
                } });
            }
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
            if clip {
                ctx.record(DrawCommand::PopClip);
            }
            return;
        }

        if broken {
            // Broken-image fallback: a red-tinted box with an X icon,
            // visually distinct from the neutral "intentional placeholder"
            // box below so a failed load doesn't masquerade as a
            // deliberate one.
            let err = ctx.tc(ctx.theme.colors.error);
            // A tint of the error colour, not a fixed maroon — the box has to
            // read as "failed" against whatever surface it lands on.
            ctx.fill_rect(dest_rect, Color::rgba(err.r, err.g, err.b, 60));
            let icon_size = (w.min(h) * 0.4).clamp(16.0, 32.0);
            let icon_rect = Rect {
                origin: Point { x: x + (w - icon_size) / 2.0, y: y + (h - icon_size) / 2.0 },
                size: Size { width: icon_size, height: icon_size },
            };
            ctx.paint_child(icon_rect, &super::Icon::new(super::IconKind::Close)
                .size(icon_size)
                .color(err));
            return;
        }

        // Placeholder: colored box + icon.
        ctx.fill_rect(dest_rect, self.inner.placeholder_color);
        let deco = ctx.tc(ctx.theme.colors.outline);
        ctx.fill_circle(Point { x: x + w / 2.0, y: y + h / 2.0 - 15.0 }, 12.0, deco);
        ctx.fill_rect(Rect {
            origin: Point { x: x + w / 2.0 - 20.0, y: y + h / 2.0 + 5.0 },
            size: Size { width: 40.0, height: 20.0 },
        }, deco);
    }
}

/// Place a `src_w` x `src_h` image inside the widget `box_rect` per `fit`.
///
/// Returns the destination rect and whether the caller must clip to
/// `box_rect` — true exactly when the destination can fall outside it
/// (`Cover` scales up and overflows; `None` draws at natural size, which may
/// be larger than the box). `Fill` and `Contain` are always contained, so
/// they skip the clip and the two draw commands it costs.
fn fit_rect(fit: ImageFit, src_w: f32, src_h: f32, box_rect: Rect) -> (Rect, bool) {
    let (x, y) = (box_rect.origin.x, box_rect.origin.y);
    let (w, h) = (box_rect.size.width, box_rect.size.height);
    // A zero-dimension source has no aspect ratio to preserve; every mode
    // degenerates to the box, and dividing by it would yield NaN rects.
    if src_w <= 0.0 || src_h <= 0.0 {
        return (box_rect, false);
    }
    let at = |dx: f32, dy: f32, dw: f32, dh: f32| Rect {
        origin: Point { x: dx, y: dy },
        size: Size { width: dw, height: dh },
    };
    match fit {
        ImageFit::Fill => (box_rect, false),
        ImageFit::Contain => {
            let s = (w / src_w).min(h / src_h);
            let (dw, dh) = (src_w * s, src_h * s);
            (at(x + (w - dw) / 2.0, y + (h - dh) / 2.0, dw, dh), false)
        }
        ImageFit::Cover => {
            let s = (w / src_w).max(h / src_h);
            let (dw, dh) = (src_w * s, src_h * s);
            (at(x + (w - dw) / 2.0, y + (h - dh) / 2.0, dw, dh), true)
        }
        ImageFit::None => {
            let (dw, dh) = (src_w, src_h);
            (at(x + (w - dw) / 2.0, y + (h - dh) / 2.0, dw, dh), dw > w || dh > h)
        }
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

    fn boxr(w: f32, h: f32) -> Rect {
        Rect { origin: Point { x: 10.0, y: 20.0 }, size: Size { width: w, height: h } }
    }

    #[test]
    fn fill_stretches_to_the_box_and_needs_no_clip() {
        let (r, clip) = fit_rect(ImageFit::Fill, 100.0, 50.0, boxr(200.0, 200.0));
        assert_eq!((r.size.width, r.size.height), (200.0, 200.0));
        assert!(!clip);
    }

    #[test]
    fn contain_letterboxes_and_preserves_aspect_ratio() {
        // 100x50 (2:1) into a 200x200 box -> 200x100, centred vertically.
        let (r, clip) = fit_rect(ImageFit::Contain, 100.0, 50.0, boxr(200.0, 200.0));
        assert_eq!((r.size.width, r.size.height), (200.0, 100.0));
        assert_eq!(r.origin.x, 10.0, "no horizontal letterbox — it fills the width");
        assert_eq!(r.origin.y, 20.0 + 50.0, "centred in the leftover 100px");
        assert!(!clip, "Contain never leaves the box");
        assert!((r.size.width / r.size.height - 2.0).abs() < 1e-6);
    }

    #[test]
    fn cover_fills_the_box_overflows_and_clips() {
        // 100x50 (2:1) into a 200x200 box -> 400x200, centred horizontally.
        let (r, clip) = fit_rect(ImageFit::Cover, 100.0, 50.0, boxr(200.0, 200.0));
        assert_eq!((r.size.width, r.size.height), (400.0, 200.0));
        assert_eq!(r.origin.x, 10.0 - 100.0, "overflow split evenly on both sides");
        assert!(clip, "Cover overflows, so the caller MUST clip");
        assert!((r.size.width / r.size.height - 2.0).abs() < 1e-6);
    }

    #[test]
    fn none_draws_natural_size_and_clips_only_when_it_overflows() {
        let (small, clip) = fit_rect(ImageFit::None, 40.0, 30.0, boxr(200.0, 200.0));
        assert_eq!((small.size.width, small.size.height), (40.0, 30.0));
        assert!(!clip, "smaller than the box — nothing to clip");

        let (big, clip) = fit_rect(ImageFit::None, 400.0, 30.0, boxr(200.0, 200.0));
        assert_eq!(big.size.width, 400.0);
        assert!(clip, "wider than the box — must clip");
    }

    #[test]
    fn zero_sized_source_degenerates_to_the_box_without_nan() {
        // Guards the divide-by-zero: every mode divides by src_w/src_h.
        for fit in [ImageFit::Fill, ImageFit::Contain, ImageFit::Cover, ImageFit::None] {
            let (r, _) = fit_rect(fit, 0.0, 0.0, boxr(200.0, 200.0));
            assert!(r.size.width.is_finite() && r.size.height.is_finite());
            assert_eq!((r.size.width, r.size.height), (200.0, 200.0));
        }
    }
}
