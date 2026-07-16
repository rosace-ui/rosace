//! The home screen — an index of the app's routes.

use rosace::prelude::*;

use crate::app::Screen;

pub fn home_screen(nav: &ScreenNav<Screen>) -> impl Widget {
    let nav = nav.clone();
    Column::new()
        .padding(EdgeInsets::all(16.0))
        .children(vec![
            Box::new(ListTile::new("Counter")
                .subtitle("A simple counter with + / \u{2212}")
                .on_press(move || {
                    nav.push(Screen::Counter);
                })
            ),
            Box::new(ListTile::new("Counter")
                .subtitle("A simple counter with + / \u{2212}")
                .on_press(move || {
                    nav.push(Screen::Counter);
                })
            )])
}
