//! Web (wasm) MVP entry — the same Counter component as the desktop demo,
//! running in the browser. Build:
//!   cargo build --example web --target wasm32-unknown-unknown
//!   wasm-bindgen target/wasm32-unknown-unknown/debug/examples/web.wasm \
//!     --out-dir dist --target web
//! then serve dist/ with an index.html that imports it.

use tezzera::prelude::*;

struct Counter;

impl Component for Counter {
    fn build(&self, ctx: &mut Context) -> Element {
        let count = ctx.state(0i32);
        Scaffold::new(
            Column::new()
                .child(Spacer::gap(0.0, 60.0))
                .child(Text::display(count.get().to_string()).align(TextAlign::Center))
                .child(Spacer::gap(0.0, 8.0))
                .child(Text::new("click to increment").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 32.0))
                .child(
                    Row::new()
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .spacing(12.0)
                        .child(Button::new("−").variant(ButtonVariant::Ghost).width(44.0)
                            .on_press({ let c = count.clone(); move || c.set(c.get() - 1) }))
                        .child(Button::new("Increment").width(140.0)
                            .on_press({ let c = count.clone(); move || c.set(c.get() + 1) }))
                        .child(Button::new("+").variant(ButtonVariant::Ghost).width(44.0)
                            .on_press({ let c = count.clone(); move || c.set(c.get() + 1) })),
                ),
        )
        .app_bar(AppBar::new("Tezzera on Web"))
        .into_element()
    }
}

/// Browser entry — winit hands the app to `requestAnimationFrame` and returns.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    App::new().title("Tezzera Web").size(900, 640).launch(Counter);
}

/// Native entry so the example also builds/runs on desktop.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    App::new().title("Tezzera Web").size(900, 640).launch(Counter);
}
