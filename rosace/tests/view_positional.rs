//! The positional-constructor-args unblock (no feature gate → runs in RELEASE
//! too). Before this, `view! { Text("Hi") }` couldn't compile in release
//! because the builder path emitted `Text::new().content(..)`. Now positional
//! args go to `new(...)`, so constructor-arg widgets (Text, Button, …) work in
//! `view!` in BOTH release and dev.

use rosace::prelude::*;
use rosace::{Constraints, FontCache};

#[test]
fn view_with_positional_args_compiles_and_builds() {
    let title = String::from("Count: 0");

    // Static + dynamic positional args, nested in a setter-style container.
    let widget = view! {
        Column {
            spacing: 12.0
            Text("Hello")     // static positional constructor arg
            Text(title)       // dynamic positional constructor arg (a hole in dev)
        }
    };

    // It's a real widget either way (release: a Column; dev: an inflated tree).
    // Compiling this file at all is the proof the release builder path handles
    // positional args; measuring confirms it's a usable widget.
    let font = FontCache::embedded();
    let theme = rosace::prelude::dark_theme();
    let ctx = rosace::widgets::tree::LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
    let size = rosace::widgets::tree::Widget::layout(&widget, &ctx);
    assert!(size.height > 0.0, "the built tree should lay out to a non-empty size");
}

#[test]
fn view_with_a_button_handler_compiles_and_builds() {
    // A Button with an `on_press` closure — the A4 interactive case. Release:
    // `Button::new("Click").on_press(closure)`. Dev: the closure is wrapped as
    // Arc<dyn Fn()> in the hole array and the inflater binds it. This test
    // passing in BOTH `cargo test -p rosace` and `--features rsc-hot` is the
    // proof handlers work in `view!` in both paths.
    let widget = view! {
        Column {
            spacing: 8.0
            Text("Tap the button")
            Button("Click me") { on_press: || {} }
        }
    };

    let font = FontCache::embedded();
    let theme = rosace::prelude::dark_theme();
    let ctx = rosace::widgets::tree::LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
    let size = rosace::widgets::tree::Widget::layout(&widget, &ctx);
    assert!(size.height > 0.0);
}
