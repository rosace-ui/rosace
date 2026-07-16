//! The root component: owns navigation, app-wide state, and the theme.

use rosace::prelude::*;
use rosace::theme::set_theme;

use crate::screens::{counter_screen, home_screen};

/// Every screen in the app. Add a variant + a match arm to add a route.
#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Home,
    Counter,
}

impl Screen {
    fn title(&self) -> &'static str {
        match self {
            Screen::Home => "my_app",
            Screen::Counter => "Counter",
        }
    }
}

pub struct AppRoot;

impl Component for AppRoot {
    fn build(&self, ctx: &mut Context) -> Element {
        // Hooks — declared unconditionally, in a stable order.
        let nav = ScreenNav::new(ctx, Screen::Home);
        let count = ctx.state(0i32);
        let is_dark = ctx.state(false);

        // Same match arms build both the current and (if mid-transition)
        // previous screen, so ScreenTransitionView can animate between
        // them — see nav.push/pop's docs (default-on, theme-governed).
        let build_screen = {
            let nav = nav.clone();
            let count = count.clone();
            move |s: Screen| -> BoxedWidget {
                match s {
                    Screen::Home => Box::new(home_screen(&nav)),
                    Screen::Counter => Box::new(counter_screen(&count)),
                }
            }
        };
        let screen = nav.current().unwrap_or(Screen::Home);
        let body = build_screen(screen);
        let outgoing = nav.previous().map(build_screen);
        let view = ScreenTransitionView::new(body, outgoing, nav.transition_handle());

        // App bar: a back button appears off Home; a theme toggle on the right.
        let mut bar = AppBar::new(screen.title()).back_button(&nav);
        let label = if is_dark.get() { "\u{2600} Dark" } else { "\u{263e} Light" };
        let d = is_dark.clone();
        bar = bar.action(Button::new(label).on_press(move || {
            let next = !d.get();
            d.set(next);
            set_theme(if next { crate::theme::dark() } else { crate::theme::light() });
        }));

        Scaffold::new(view).app_bar(bar).into_element()
    }
}
