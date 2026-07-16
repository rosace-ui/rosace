//! Build-time semantic HTML/SEO export (D107 Phase 25 Step 3) — run by
//! `rsc build --target web` via `cargo run --example seo_extract`, NEVER
//! compiled to wasm or shipped to a browser. See `rosace-web-seo`'s
//! module doc for why this mapping lives in its own crate rather than
//! `rosace-core` (platform isolation — verified via `cargo tree`, not
//! assumed).
//!
//! A Cargo example is its own crate root — `crate::` here would NOT reach
//! this package's own `src/lib.rs` modules, so `app`/`theme` are addressed
//! by this crate's own library name instead (`my_app::...`), same as
//! any other external dependent would reach them. `lib_rs`'s codegen
//! widens `app`/`theme` to `pub mod` specifically so this resolves.

use rosace::{FontCache, FrameEngine, SkiaCanvas};

use my_app::app::AppRoot;

/// Matches `web_index_html`'s marker comment in `build_web`.
const SPLIT_MARKER: &str = "\n---RSC-SEO-TEXT---\n";

fn main() {
    rosace::theme::set_theme(my_app::theme::light());

    let font = FontCache::system_ui()
        .or_else(FontCache::system_mono)
        .unwrap_or_else(FontCache::embedded);

    let mut engine = FrameEngine::new(Box::new(AppRoot), font);

    // A representative desktop-web viewport — the semantic tree (roles/
    // labels/structure) doesn't meaningfully depend on the exact size for
    // typical layouts, so this doesn't need to match any real device.
    let mut canvas = SkiaCanvas::new_hidpi(1280, 800, 1.0);
    let mut overlay = SkiaCanvas::new_hidpi(1280, 800, 1.0);
    engine.paint(&mut canvas, &mut overlay, &[]);

    let tree = engine.semantics();
    let html = rosace_web_seo::render_shadow_dom_template(&tree);
    let text = rosace_web_seo::render_text(&tree);

    print!("{html}{SPLIT_MARKER}{text}");
}
