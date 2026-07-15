//! Networking for ROSACE: a general HTTP client (D113/Phase 30 — `ureq` +
//! `rustls`, so `https://` works for real) plus the original non-blocking
//! remote-image loading, now rebuilt on that same client.
//!
//! Uses `std::thread` + `mpsc` for non-blocking requests.
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

pub mod client;
pub mod load_state;
pub mod loader;
pub mod network_status;
pub mod query;
pub mod remote_image;

pub use client::{HttpClient, HttpHandle, HttpMethod, HttpRequest, HttpResponse};
pub use load_state::LoadState;
pub use loader::ImageLoader;
pub use network_status::{network_status, set_network_status, use_network_status, NetworkStatus};
pub use query::{use_query, QueryState};
pub use remote_image::{RemoteImage, RemoteImageFit};
