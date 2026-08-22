/// Merged from the former `rosace-gesture` crate (D131).
pub mod gesture;

/// Merged from the former `rosace-web-seo` crate (D131).
pub mod web_seo;

/// Merged from the former `rosace-ime` crate (D131).
pub mod ime;

pub mod app;
pub mod event;
/// Hand a URL to the OS's default handler (D116 context-menu items).
pub mod open_url;
pub mod scroll_layer;

#[cfg(target_arch = "wasm32")]
pub mod web_seo_sync;

/// Platform accessibility bridge (D132) — the desktop counterpart to
/// `web_seo_sync`: same semantic tree, published to NSAccessibility /
/// UI Automation / AT-SPI via AccessKit instead of to a DOM shadow tree.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub mod a11y_bridge;

pub use app::PlatformWindow;
pub use event::{InputEvent, MouseButton, Key};
pub use open_url::{encode_query, open_url};
pub use scroll_layer::{ScrollLayer, publish_scroll_layers, take_scroll_layers};
