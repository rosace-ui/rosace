//! Image widget — renders a PNG image (or a placeholder) inside a fixed-size box.
//!
//! # Example
//! ```rust,ignore
//! let img = ImageWidget::new()
//!     .file("assets/photo.png")
//!     .fit(ImageFit::Cover)
//!     .width(320.0)
//!     .height(200.0);
//! img.render(&mut canvas, &font_cache, 10.0, 10.0, &theme);
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, FontCache, SkiaCanvas};
use rosace_theme::ThemeData;

/// Controls how the image is scaled to fit the widget's bounding box.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageFit {
    /// Stretch to fill the target rect exactly (may distort aspect ratio).
    Fill,
    /// Scale uniformly to fit within the rect, preserving aspect ratio (letterbox).
    Contain,
    /// Scale uniformly to cover the rect, cropping edges (center crop).
    Cover,
    /// Natural size — no scaling (may be clipped if larger than the widget).
    None,
}

/// The data source for an [`ImageWidget`].
#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    /// Load the image from a file path.
    File(PathBuf),
    /// Use already-loaded PNG bytes.
    Bytes(Vec<u8>),
    /// Show a colored placeholder rectangle (no image data).
    Placeholder,
}

/// A widget that renders an image from a file path or raw bytes.
///
/// Falls back to a colored placeholder when the image cannot be decoded.
pub struct ImageWidget {
    pub source: ImageSource,
    pub fit: ImageFit,
    pub width: f32,
    pub height: f32,
    pub placeholder_color: Color,
    /// Accessible/SEO alt text (D107/Phase 25) — `None` produces no
    /// semantics entry at all (an image with no `.alt(...)` call is
    /// decorative, matching HTML's own convention of an empty/absent
    /// `alt` attribute), not an empty-string placeholder.
    pub alt: Option<String>,
}

impl ImageWidget {
    /// Create a new `ImageWidget` with placeholder defaults.
    pub fn new() -> Self {
        Self {
            source: ImageSource::Placeholder,
            fit: ImageFit::Contain,
            width: 200.0,
            height: 200.0,
            placeholder_color: Color::rgb(60, 65, 90),
            alt: None,
        }
    }

    /// Sets the accessible/SEO alt text.
    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }

    /// Set the image source to a file path.
    pub fn file(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = ImageSource::File(path.into());
        self
    }

    /// Set the image source to raw PNG bytes.
    pub fn bytes(mut self, data: Vec<u8>) -> Self {
        self.source = ImageSource::Bytes(data);
        self
    }

    /// Set the fit mode.
    pub fn fit(mut self, f: ImageFit) -> Self {
        self.fit = f;
        self
    }

    /// Set the widget width in pixels.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Set the widget height in pixels.
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// Set the placeholder background color shown when no image is available.
    pub fn placeholder_color(mut self, c: Color) -> Self {
        self.placeholder_color = c;
        self
    }

    /// Render the image at `(x, y)`.
    ///
    /// Falls back to a colored placeholder with a simple icon if the image
    /// cannot be loaded or decoded.
    pub fn render(
        &self,
        canvas: &mut SkiaCanvas,
        font: &FontCache,
        x: f32,
        y: f32,
        _theme: &ThemeData,
    ) {
        let pixmap_bytes = match &self.source {
            ImageSource::Placeholder => None,
            ImageSource::File(path) => std::fs::read(path).ok(),
            ImageSource::Bytes(b) => Some(b.clone()),
        };

        let loaded = pixmap_bytes.and_then(|bytes| tiny_skia::Pixmap::decode_png(&bytes).ok());

        match loaded {
            Some(pixmap) => {
                self.blit_pixmap(canvas, x, y, &pixmap);
            }
            None => {
                // Placeholder rectangle.
                canvas.fill_rect(
                    Rect {
                        origin: Point { x, y },
                        size: Size {
                            width: self.width,
                            height: self.height,
                        },
                    },
                    self.placeholder_color,
                );
                // Simple camera / image icon.
                let cx = x + self.width / 2.0;
                let cy = y + self.height / 2.0;
                canvas.fill_circle(
                    Point { x: cx, y: cy - 15.0 },
                    12.0,
                    Color::rgb(100, 110, 140),
                );
                canvas.fill_rect(
                    Rect {
                        origin: Point {
                            x: cx - 20.0,
                            y: cy + 5.0,
                        },
                        size: Size {
                            width: 40.0,
                            height: 20.0,
                        },
                    },
                    Color::rgb(80, 90, 120),
                );
                if let ImageSource::File(path) = &self.source {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    canvas.draw_text(
                        &name,
                        Point {
                            x: x + 4.0,
                            y: y + self.height - 18.0,
                        },
                        Color::rgb(140, 145, 175),
                        font,
                        10.0,
                    );
                }
            }
        }
    }

    /// Blit a decoded pixmap onto the canvas using the widget's fit mode.
    fn blit_pixmap(&self, canvas: &mut SkiaCanvas, x: f32, y: f32, pixmap: &tiny_skia::Pixmap) {
        let src_w = pixmap.width() as f32;
        let src_h = pixmap.height() as f32;

        let (draw_x, draw_y, draw_w, draw_h, src_crop_x, src_crop_y, src_crop_w, src_crop_h) =
            self.compute_fit(src_w, src_h, x, y);

        let dst_w = draw_w as u32;
        let dst_h = draw_h as u32;
        if dst_w == 0 || dst_h == 0 {
            return;
        }

        let scale_x = src_crop_w / draw_w;
        let scale_y = src_crop_h / draw_h;

        let canvas_w = canvas.width() as i32;
        let canvas_h = canvas.height() as i32;
        let pixels = canvas.pixels_mut();

        for dy in 0..dst_h {
            for dx in 0..dst_w {
                let px = draw_x as i32 + dx as i32;
                let py = draw_y as i32 + dy as i32;
                if px < 0 || py < 0 || px >= canvas_w || py >= canvas_h {
                    continue;
                }

                let sx = (src_crop_x + dx as f32 * scale_x) as u32;
                let sy = (src_crop_y + dy as f32 * scale_y) as u32;
                let sx = sx.min(pixmap.width() - 1);
                let sy = sy.min(pixmap.height() - 1);

                let src_idx = (sy * pixmap.width() + sx) as usize * 4;
                let dst_idx = (py * canvas_w + px) as usize * 4;

                if let (Some(src), Some(dst)) = (
                    pixmap.data().get(src_idx..src_idx + 4),
                    pixels.get_mut(dst_idx..dst_idx + 4),
                ) {
                    // tiny-skia outputs premultiplied RGBA; alpha-blend onto canvas.
                    let alpha = src[3];
                    if alpha == 255 {
                        dst[0] = src[0];
                        dst[1] = src[1];
                        dst[2] = src[2];
                        dst[3] = src[3];
                    } else if alpha > 0 {
                        let a = alpha as u32;
                        let ia = 255 - a;
                        dst[0] = ((src[0] as u32 * a + dst[0] as u32 * ia) / 255) as u8;
                        dst[1] = ((src[1] as u32 * a + dst[1] as u32 * ia) / 255) as u8;
                        dst[2] = ((src[2] as u32 * a + dst[2] as u32 * ia) / 255) as u8;
                        dst[3] = 255;
                    }
                }
            }
        }
    }

    /// Compute `(draw_x, draw_y, draw_w, draw_h, src_crop_x, src_crop_y, src_crop_w, src_crop_h)`.
    fn compute_fit(
        &self,
        src_w: f32,
        src_h: f32,
        x: f32,
        y: f32,
    ) -> (f32, f32, f32, f32, f32, f32, f32, f32) {
        match self.fit {
            ImageFit::Fill => (x, y, self.width, self.height, 0.0, 0.0, src_w, src_h),
            ImageFit::None => (
                x,
                y,
                src_w.min(self.width),
                src_h.min(self.height),
                0.0,
                0.0,
                src_w.min(self.width),
                src_h.min(self.height),
            ),
            ImageFit::Contain => {
                let scale = (self.width / src_w).min(self.height / src_h);
                let dw = src_w * scale;
                let dh = src_h * scale;
                let dx = x + (self.width - dw) / 2.0;
                let dy = y + (self.height - dh) / 2.0;
                (dx, dy, dw, dh, 0.0, 0.0, src_w, src_h)
            }
            ImageFit::Cover => {
                let scale = (self.width / src_w).max(self.height / src_h);
                let scaled_w = src_w * scale;
                let scaled_h = src_h * scale;
                let crop_x = (scaled_w - self.width) / 2.0 / scale;
                let crop_y = (scaled_h - self.height) / 2.0 / scale;
                let crop_w = self.width / scale;
                let crop_h = self.height / scale;
                (x, y, self.width, self.height, crop_x, crop_y, crop_w, crop_h)
            }
        }
    }
}

impl Default for ImageWidget {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ImageCache
// ---------------------------------------------------------------------------

/// A decoded image: shared premultiplied-RGBA pixels + dimensions. The
/// `Arc` is what `DrawCommand::BlitRgba` carries, so a cached image costs
/// zero copies per frame — and its stable content makes the compositor's
/// GPU texture key (a content hash) stable across frames too.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width:  u32,
    pub height: u32,
    pub pixels: std::sync::Arc<Vec<u8>>,
}

/// Caches decoded images to avoid re-decoding on every frame — keyed by
/// file path (or a content hash for byte sources). Wired for real in
/// Phase 27 (`Image::paint` previously did `fs::read` + PNG decode on
/// EVERY paint — the former Known-Issues "orphaned ImageCache" entry);
/// paint-time access goes through [`ImageCache::global`].
///
/// Unbounded by design for now: it holds each distinct image an app ever
/// shows, which is the same bound the old decode-per-paint had on peak
/// memory. A byte budget + eviction is real follow-up work, tracked with
/// the compositor's image-texture eviction.
pub struct ImageCache {
    cache: HashMap<PathBuf, DecodedImage>,
    by_bytes: HashMap<u64, DecodedImage>,
}

impl ImageCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            by_bytes: HashMap::new(),
        }
    }

    /// The process-wide cache used by `Image::paint` — same global-service
    /// pattern as the scroll-offset channel.
    pub fn global() -> &'static std::sync::Mutex<ImageCache> {
        use std::sync::{Mutex, OnceLock};
        static CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(ImageCache::new()))
    }

    /// Return the cached image for `path`, loading and decoding it on first
    /// access. Returns `None` if the file cannot be read or decoded as PNG.
    pub fn get_or_load(&mut self, path: impl Into<PathBuf>) -> Option<DecodedImage> {
        let path = path.into();
        if !self.cache.contains_key(&path) {
            let bytes = std::fs::read(&path).ok()?;
            let pixmap = tiny_skia::Pixmap::decode_png(&bytes).ok()?;
            self.cache.insert(path.clone(), DecodedImage {
                width:  pixmap.width(),
                height: pixmap.height(),
                pixels: std::sync::Arc::new(pixmap.data().to_vec()),
            });
        }
        self.cache.get(&path).cloned()
    }

    /// Decode-once for byte sources, keyed by a content hash (dims + len +
    /// sampled windows — same scheme as the compositor's texture key).
    pub fn get_or_decode_bytes(&mut self, bytes: &[u8]) -> Option<DecodedImage> {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut eat = |b: u8| {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        };
        for b in (bytes.len() as u64).to_le_bytes() { eat(b); }
        let n = bytes.len();
        for &start in &[0usize, n / 2, n.saturating_sub(32)] {
            for &b in &bytes[start..(start + 32).min(n)] { eat(b); }
        }
        if let std::collections::hash_map::Entry::Vacant(e) = self.by_bytes.entry(h) {
            let pixmap = tiny_skia::Pixmap::decode_png(bytes).ok()?;
            e.insert(DecodedImage {
                width:  pixmap.width(),
                height: pixmap.height(),
                pixels: std::sync::Arc::new(pixmap.data().to_vec()),
            });
        }
        self.by_bytes.get(&h).cloned()
    }

    /// Number of cached entries (path + byte sources).
    pub fn len(&self) -> usize {
        self.cache.len() + self.by_bytes.len()
    }

    /// Returns `true` if the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty() && self.by_bytes.is_empty()
    }

    /// Remove all cached entries.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.by_bytes.clear();
    }

    /// Returns `true` if the given path is already in the cache.
    pub fn contains(&self, path: impl Into<PathBuf>) -> bool {
        self.cache.contains_key(&path.into())
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── ImageWidget construction ──────────────────────────────────────────────

    #[test]
    fn image_widget_new_defaults() {
        let w = ImageWidget::new();
        assert_eq!(w.source, ImageSource::Placeholder);
        assert_eq!(w.fit, ImageFit::Contain);
        assert_eq!(w.width, 200.0);
        assert_eq!(w.height, 200.0);
    }

    #[test]
    fn image_widget_file_source() {
        let w = ImageWidget::new().file("/tmp/photo.png");
        assert_eq!(w.source, ImageSource::File(PathBuf::from("/tmp/photo.png")));
    }

    #[test]
    fn image_widget_bytes_source() {
        let data = vec![0u8, 1, 2, 3];
        let w = ImageWidget::new().bytes(data.clone());
        assert_eq!(w.source, ImageSource::Bytes(data));
    }

    #[test]
    fn image_widget_fit_setter() {
        let w = ImageWidget::new().fit(ImageFit::Cover);
        assert_eq!(w.fit, ImageFit::Cover);
    }

    #[test]
    fn image_widget_size_setters() {
        let w = ImageWidget::new().width(640.0).height(480.0);
        assert_eq!(w.width, 640.0);
        assert_eq!(w.height, 480.0);
    }

    // ── compute_fit geometry ──────────────────────────────────────────────────

    #[test]
    fn image_fit_contain() {
        // 100×50 image into 200×200 widget — should scale uniformly (scale=2)
        // → drawn at 200×100, offset to centre vertically.
        let w = ImageWidget::new().fit(ImageFit::Contain).width(200.0).height(200.0);
        let (dx, dy, dw, dh, scx, scy, scw, sch) = w.compute_fit(100.0, 50.0, 0.0, 0.0);
        assert!((dw - 200.0).abs() < 0.01, "dw={dw}");
        assert!((dh - 100.0).abs() < 0.01, "dh={dh}");
        assert!((dx - 0.0).abs() < 0.01, "dx={dx}");
        assert!((dy - 50.0).abs() < 0.01, "dy={dy}");
        // source crop covers full image
        assert!((scx).abs() < 0.01);
        assert!((scy).abs() < 0.01);
        assert!((scw - 100.0).abs() < 0.01);
        assert!((sch - 50.0).abs() < 0.01);
    }

    #[test]
    fn image_fit_fill() {
        let w = ImageWidget::new().fit(ImageFit::Fill).width(300.0).height(150.0);
        let (dx, dy, dw, dh, scx, scy, scw, sch) = w.compute_fit(100.0, 100.0, 5.0, 5.0);
        assert_eq!(dx, 5.0);
        assert_eq!(dy, 5.0);
        assert_eq!(dw, 300.0);
        assert_eq!(dh, 150.0);
        assert_eq!(scx, 0.0);
        assert_eq!(scy, 0.0);
        assert_eq!(scw, 100.0);
        assert_eq!(sch, 100.0);
    }

    #[test]
    fn image_fit_cover() {
        // 100×100 image into 200×100 widget — scale=2 to cover width,
        // crop 50px from top and bottom in source space (25px each side).
        let w = ImageWidget::new().fit(ImageFit::Cover).width(200.0).height(100.0);
        let (dx, dy, dw, dh, scx, scy, scw, sch) = w.compute_fit(100.0, 100.0, 0.0, 0.0);
        assert!((dw - 200.0).abs() < 0.01, "dw={dw}");
        assert!((dh - 100.0).abs() < 0.01, "dh={dh}");
        assert!((dx).abs() < 0.01, "dx={dx}");
        assert!((dy).abs() < 0.01, "dy={dy}");
        // crop: source crop_w = widget_w / scale = 200/2 = 100 ✓ (full width)
        assert!((scx).abs() < 0.01, "scx={scx}");
        assert!((scy - 25.0).abs() < 0.01, "scy={scy}");
        assert!((scw - 100.0).abs() < 0.01, "scw={scw}");
        assert!((sch - 50.0).abs() < 0.01, "sch={sch}");
    }

    // ── ImageCache ────────────────────────────────────────────────────────────

    #[test]
    fn image_cache_new_empty() {
        let cache = ImageCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn image_cache_contains_after_miss() {
        let cache = ImageCache::new();
        // A non-existent path should NOT end up cached.
        assert!(!cache.contains("/no/such/file.png"));
    }
}
