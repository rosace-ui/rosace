/// Merged from the former `rosace-gesture` crate (D131).
pub mod gesture;

/// Merged from the former `rosace-web-seo` crate (D131).
pub mod web_seo;

/// Merged from the former `rosace-ime` crate (D131).
pub mod ime;

pub mod app;
pub mod event;
pub mod scroll_layer;

#[cfg(target_arch = "wasm32")]
pub mod web_seo_sync;

pub use app::PlatformWindow;
pub use event::{InputEvent, MouseButton, Key};
pub use scroll_layer::{ScrollLayer, publish_scroll_layers, take_scroll_layers};
