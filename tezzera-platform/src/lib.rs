pub mod app;
pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(target_arch = "wasm32")]
pub use web::web_app::run_web;

pub use app::{TezzeraApp, AppConfig};
pub use event::{InputEvent, MouseButton, Key};
