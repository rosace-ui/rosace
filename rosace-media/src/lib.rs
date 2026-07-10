//! Media (audio/video) stubs for ROSACE.
//!
//! All operations return `MediaError::PlatformUnavailable` — real
//! decode via rodio/cpal (audio) and platform APIs (video) is planned for v1.0.
//!
//! These types provide the data model so widget code can reference
//! `AudioHandle` and `VideoFrame` now and compile correctly.
//!
//! # Example
//! ```rust,ignore
//! use rosace_media::{AudioPlayer, MediaError};
//!
//! let mut player = AudioPlayer::new();
//! match player.load("music.mp3") {
//!     Ok(handle) => { /* play */ }
//!     Err(MediaError::PlatformUnavailable) => {
//!         println!("audio not available yet (v1.0)");
//!     }
//!     Err(e) => eprintln!("{e}"),
//! }
//! ```

pub mod audio;
pub mod error;
pub mod format;
pub mod video;

pub use audio::{AudioHandle, AudioPlayer};
pub use error::MediaError;
pub use format::{AudioFormat, VideoFormat};
pub use video::{VideoDecoder, VideoFrame};
