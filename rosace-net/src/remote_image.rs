use crate::load_state::LoadState;
use crate::loader::ImageLoader;

/// Widget stub for a remotely-loaded image.
/// Tracks the URL and load state; rendering is delegated to the caller.
pub struct RemoteImage {
    pub url: String,
    pub width: f32,
    pub height: f32,
    pub fit: RemoteImageFit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteImageFit { Fill, Contain, Cover, None }

impl RemoteImage {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into(), width: 200.0, height: 200.0, fit: RemoteImageFit::Contain }
    }
    pub fn width(mut self, w: f32) -> Self { self.width = w; self }
    pub fn height(mut self, h: f32) -> Self { self.height = h; self }
    pub fn fit(mut self, f: RemoteImageFit) -> Self { self.fit = f; self }

    /// Register this image with a loader (begins loading if idle).
    pub fn register(&self, loader: &mut ImageLoader) {
        loader.load(&self.url);
    }

    /// Get current state from a loader.
    pub fn state<'a>(&self, loader: &'a ImageLoader) -> &'a LoadState<Vec<u8>> {
        loader.state(&self.url)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_image_new() {
        let img = RemoteImage::new("http://example.com/photo.png");
        assert_eq!(img.url, "http://example.com/photo.png");
        assert_eq!(img.width, 200.0);
        assert_eq!(img.height, 200.0);
        assert_eq!(img.fit, RemoteImageFit::Contain);
    }

    #[test]
    fn remote_image_width_setter() {
        let img = RemoteImage::new("http://example.com/photo.png")
            .width(640.0)
            .height(480.0);
        assert_eq!(img.width, 640.0);
        assert_eq!(img.height, 480.0);
    }

    #[test]
    fn remote_image_fit_setter() {
        let img = RemoteImage::new("http://example.com/photo.png")
            .fit(RemoteImageFit::Cover);
        assert_eq!(img.fit, RemoteImageFit::Cover);
    }

    #[test]
    fn remote_image_register_starts_loading() {
        let mut loader = ImageLoader::new();
        let img = RemoteImage::new("http://localhost:19999/img.png");
        img.register(&mut loader);
        assert!(loader.state("http://localhost:19999/img.png").is_loading());
    }

    #[test]
    fn remote_image_state_delegates_to_loader() {
        let mut loader = ImageLoader::new();
        let url = "http://example.com/banner.png";
        loader.inject_loaded(url, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        let img = RemoteImage::new(url);
        assert!(img.state(&loader).is_loaded());
        assert_eq!(img.state(&loader).value(), Some(&vec![0xDE, 0xAD, 0xBE, 0xEF]));
    }
}
