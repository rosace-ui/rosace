pub mod app;
pub mod event;
pub mod scroll_layer;

#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub mod web_seo_sync;

#[cfg(target_arch = "wasm32")]
pub use web::web_app::run_web;

pub use app::PlatformWindow;
pub use event::{InputEvent, MouseButton, Key};
pub use scroll_layer::{ScrollLayer, publish_scroll_layers, take_scroll_layers};
