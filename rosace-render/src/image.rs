/// Image cache policy controlling when the local cache is consulted.
#[derive(Debug, Clone, Copy)]
pub enum CachePolicy {
    /// Fetch from the network; fall back to cache on failure.
    NetworkFirst,
    /// Use the cache if available; fetch from network otherwise.
    CacheFirst,
    /// Never use the cache; always fetch from the network.
    NoCache,
}

/// Controls how an image fills its layout bounds.
#[derive(Debug, Clone, Copy)]
pub enum ImageFit {
    /// Stretch to fill the bounds, ignoring aspect ratio.
    Fill,
    /// Scale uniformly to fit within the bounds, preserving aspect ratio.
    Contain,
    /// Scale uniformly to cover the bounds, clipping if necessary.
    Cover,
    /// Like [`Contain`], but never upscales below natural size.
    ///
    /// [`Contain`]: ImageFit::Contain
    ScaleDown,
    /// Render at the image's natural pixel size; no scaling.
    None,
}

/// A decoded image ready for rendering.
///
/// Full decode and cache support will be implemented in Phase 2.  Phase 1
/// supports PNG decoding via `tiny-skia`.
#[derive(Debug, Clone)]
pub struct ImageHandle {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Raw RGBA pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

impl ImageHandle {
    /// Decode a PNG image from raw bytes.
    ///
    /// Returns `None` if the bytes are not valid PNG data.
    pub fn from_png_bytes(bytes: &[u8]) -> Option<Self> {
        let pixmap = tiny_skia::Pixmap::decode_png(bytes).ok()?;
        Some(Self {
            width: pixmap.width(),
            height: pixmap.height(),
            pixels: pixmap.data().to_vec(),
        })
    }
}
