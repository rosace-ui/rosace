//! Network image loading for ROSACE.
//!
//! Uses `std::thread` + `mpsc` for non-blocking HTTP GET.
//! No async runtime required.
//!
//! # Example
//! ```rust,ignore
//! use rosace_net::{ImageLoader, RemoteImage, LoadState};
//!
//! let mut loader = ImageLoader::new();
//! let img = RemoteImage::new("http://example.com/photo.png").width(200.0);
//! img.register(&mut loader);
//!
//! // Each frame:
//! loader.poll();
//! match img.state(&loader) {
//!     LoadState::Loading => { /* draw spinner */ }
//!     LoadState::Loaded(bytes) => { /* decode and blit */ }
//!     LoadState::Failed(e) => { /* show error */ }
//!     LoadState::Idle => {}
//! }
//! ```

pub mod http;
pub mod load_state;
pub mod loader;
pub mod remote_image;

pub use load_state::LoadState;
pub use loader::ImageLoader;
pub use remote_image::{RemoteImage, RemoteImageFit};
