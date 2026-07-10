//! The root component: owns navigation, app-wide state, and the theme.

use rosace::prelude::*;
use rosace::theme::set_theme;

use crate::screens::{
    buttons_screen, counter_screen, gallery_screen, hero_detail_screen, home_screen,
    inputs_screen, overlays_screen, scroll_screen,
};

/// Every screen in the app. Add a variant + a match arm to add a route.
#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Home,
    Counter,
    Buttons,
    Gallery,
    Inputs,
    Overlays,
    Scroll,
    HeroDetail,
}

impl Screen {
    fn title(&self) -> &'static str {
        match self {
            Screen::Home => "demo_app",
            Screen::Counter => "Counter",
            Screen::Buttons => "Buttons",
            Screen::Gallery => "Gallery",
            Screen::Inputs => "Inputs",
            Screen::Overlays => "Overlays",
            Screen::Scroll => "Scroll",
            Screen::HeroDetail => "Photo",
        }
    }
}

pub struct AppRoot;

impl Component for AppRoot {
    fn build(&self, ctx: &mut Context) -> Element {
        // Hooks — declared unconditionally, in a stable order, every frame,
        // regardless of which screen is active. `build_screen` below is
        // called up to twice per frame (current + outgoing, mid-transition)
        // but never calls `ctx.state(...)` itself — every screen's local
        // widget state is a pre-created `Atom` handle threaded down as a
        // plain parameter, so hook order never depends on which screen is
        // showing.
        let nav = ScreenNav::new(ctx, Screen::Home);
        let count = ctx.state(0i32);
        let is_dark = ctx.state(true);
        let button_taps = ctx.state(0i32);
        let chip_selected = ctx.state(false);
        let progress = ctx.state(0.35f32);
        let switch_on = ctx.state(true);
        let checkbox_checked = ctx.state(false);
        let radio_selected = ctx.state(false);
        let slider_value = ctx.state(40.0f32);
        let segmented_selected = ctx.state(0usize);
        let dropdown_open = ctx.state(false);
        let dropdown_selected = ctx.state(0usize);
        let dialog_open = ctx.state(false);
        let sheet_open = ctx.state(false);
        let toast_open = ctx.state(false);
        let menu_open = ctx.state(false);

        // Same match arms build both the current and (if mid-transition)
        // previous screen, so ScreenTransitionView can animate between
        // them — see nav.push/pop's docs (default-on, theme-governed).
        let build_screen = {
            let nav = nav.clone();
            let count = count.clone();
            let button_taps = button_taps.clone();
            let chip_selected = chip_selected.clone();
            let progress = progress.clone();
            let switch_on = switch_on.clone();
            let checkbox_checked = checkbox_checked.clone();
            let radio_selected = radio_selected.clone();
            let slider_value = slider_value.clone();
            let segmented_selected = segmented_selected.clone();
            let dropdown_open = dropdown_open.clone();
            let dropdown_selected = dropdown_selected.clone();
            let dialog_open = dialog_open.clone();
            let sheet_open = sheet_open.clone();
            let toast_open = toast_open.clone();
            let menu_open = menu_open.clone();
            move |s: Screen| -> BoxedWidget {
                match s {
                    Screen::Home => Box::new(home_screen(&nav)),
                    Screen::Counter => Box::new(counter_screen(&count)),
                    Screen::Buttons => Box::new(buttons_screen(&button_taps)),
                    Screen::Gallery => Box::new(gallery_screen(&chip_selected, &progress)),
                    Screen::Inputs => Box::new(inputs_screen(
                        &switch_on, &checkbox_checked, &radio_selected, &slider_value,
                        &segmented_selected, &dropdown_open, &dropdown_selected,
                    )),
                    Screen::Overlays => Box::new(overlays_screen(
                        &dialog_open, &sheet_open, &toast_open, &menu_open,
                    )),
                    Screen::Scroll => Box::new(scroll_screen()),
                    Screen::HeroDetail => Box::new(hero_detail_screen()),
                }
            }
        };
        let screen = nav.current().unwrap_or(Screen::Home);
        let body = build_screen(screen);
        let outgoing = nav.previous().map(build_screen);
        let view = ScreenTransitionView::new(body, outgoing, nav.transition_handle());

        // App bar: a back button appears off Home; a theme toggle on the right.
        let mut bar = AppBar::new(screen.title()).back_button(&nav);
        let label = if is_dark.get() { "\u{2600} Light" } else { "\u{263e} Dark" };
        let d = is_dark.clone();
        bar = bar.action(Button::new(label).on_press(move || {
            let next = !d.get();
            d.set(next);
            set_theme(if next { crate::theme::dark() } else { crate::theme::light() });
        }));

        Scaffold::new(view).app_bar(bar).into_element()
    }
}
