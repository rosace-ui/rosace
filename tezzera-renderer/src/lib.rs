//! Renderer abstraction layer for TEZZERA (D032 prep).
//!
//! Provides the `Renderer` trait so all widget draw calls are
//! decoupled from the concrete backend. Swapping tiny-skia for
//! skia-safe in v1.0 will only require a new `Renderer` impl —
//! no widget code changes needed.
//!
//! # Example
//! ```rust,ignore
//! use tezzera_renderer::{SkiaRenderer, Renderer};
//! use tezzera_render::{Color, FontCache};
//! use tezzera_core::types::{Point, Rect, Size};
//!
//! let mut r: Box<dyn Renderer> = Box::new(SkiaRenderer::new(800, 600));
//! r.clear(Color::BLACK);
//! r.fill_rect(Rect { origin: Point { x: 10.0, y: 10.0 }, size: Size { width: 100.0, height: 50.0 } }, Color::WHITE);
//! let png = r.encode_png();
//! ```

pub mod backend;
pub mod renderer;
pub mod skia;

pub use backend::RendererBackend;
pub use renderer::Renderer;
pub use skia::SkiaRenderer;
