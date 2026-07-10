//! demo_app — a ROSACE app.
//!
//! `launch()` is shared by every platform. The native binary calls it from
//! `main`; the web build calls it from a `wasm-bindgen(start)` entry.

pub mod app;
mod ffi;
mod screens;
pub mod theme;

use rosace::prelude::*;

/// Start the app. Runs the winit event loop on native; hands off to the
/// browser's requestAnimationFrame loop on web.
pub fn launch() {
    // Window size applies on desktop; mobile is always fullscreen.
    App::new()
        .title("demo_app")
        .size(960, 640)
        .themes(theme::themes())
        .launch(app::AppRoot);
}

/// Web (wasm) entry — invoked automatically when the module is instantiated.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    launch();
}
