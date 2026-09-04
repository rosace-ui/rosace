//! `#[routes]` gives an enum of screens a URL path form, in both directions.
//!
//! D026 locked "`#[routes]` enum with `#[route("/path")]`, type-safe, auto
//! deep link" and only the enum half ever shipped: `Route` was a bare marker
//! trait, nothing parsed or formatted a path, and
//! `NavigationDecision::RedirectTo(String)` carried a path string that nothing
//! could consume.
//!
//! The point of deriving both directions from one declaration is that they
//! cannot drift. A hand-written formatter gains a segment its parser does not
//! know about, and a link the app produced itself stops resolving — so the
//! round-trip is what these mostly assert.

use rosace::nav::{Navigator, RoutePath, ScreenNav};
use rosace::routes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind { Slider, Button }

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self { Kind::Slider => "slider", Kind::Button => "button" })
    }
}
impl std::str::FromStr for Kind {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s { "slider" => Ok(Kind::Slider), "button" => Ok(Kind::Button), _ => Err(()) }
    }
}

#[routes]
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    #[route("/")]
    Home,
    #[route("/settings")]
    Settings,
    #[route("/user/:id")]
    User { id: u64 },
    #[route("/widget/:kind")]
    Widget(Kind),
    #[route("/post/:year/:slug")]
    Post { year: u16, slug: String },
}

#[test]
fn every_variant_survives_a_round_trip() {
    let all = [
        Screen::Home,
        Screen::Settings,
        Screen::User { id: 7 },
        Screen::Widget(Kind::Slider),
        Screen::Post { year: 2026, slug: "hello".into() },
    ];
    for route in all {
        let path = route.to_path();
        assert_eq!(
            Screen::from_path(&path),
            Some(route.clone()),
            "{route:?} formatted to {path:?} and did not parse back"
        );
    }
}

#[test]
fn paths_look_the_way_they_were_declared() {
    assert_eq!(Screen::Home.to_path(), "/");
    assert_eq!(Screen::Settings.to_path(), "/settings");
    assert_eq!(Screen::User { id: 7 }.to_path(), "/user/7");
    assert_eq!(Screen::Widget(Kind::Slider).to_path(), "/widget/slider");
    assert_eq!(
        Screen::Post { year: 2026, slug: "hello".into() }.to_path(),
        "/post/2026/hello"
    );
}

/// Typed parameters are the whole claim. A path that cannot produce the
/// declared type is not a match — it must not arrive as a half-built screen.
#[test]
fn a_parameter_that_does_not_parse_is_not_a_match() {
    assert_eq!(Screen::from_path("/user/seven"), None, "`id` is a u64");
    assert_eq!(Screen::from_path("/widget/nonesuch"), None, "not a Kind");
    assert_eq!(Screen::from_path("/post/notayear/x"), None, "`year` is a u16");
}

#[test]
fn unknown_and_malformed_paths_are_refused() {
    assert_eq!(Screen::from_path("/nope"), None);
    assert_eq!(Screen::from_path("/user"), None, "missing the parameter");
    assert_eq!(Screen::from_path("/user/7/extra"), None, "too many segments");
    assert_eq!(Screen::from_path(""), Some(Screen::Home), "empty is the root");
}

/// Trailing and doubled slashes are the same route. A link pasted with a
/// trailing slash is the same link.
#[test]
fn slashes_are_forgiving() {
    assert_eq!(Screen::from_path("/settings/"), Some(Screen::Settings));
    assert_eq!(Screen::from_path("settings"), Some(Screen::Settings));
    assert_eq!(Screen::from_path("//settings//"), Some(Screen::Settings));
}

/// A query string names the same screen — it carries filters, not identity.
#[test]
fn a_query_string_does_not_change_which_route_matches() {
    assert_eq!(Screen::from_path("/settings?tab=general"), Some(Screen::Settings));
    assert_eq!(Screen::from_path("/user/7?ref=email"), Some(Screen::User { id: 7 }));
}

#[test]
fn push_path_deep_links_into_the_navigator() {
    let nav: Navigator<Screen> = Navigator::new(Screen::Home);
    assert!(nav.push_path("/user/7"));
    assert_eq!(nav.current(), Some(Screen::User { id: 7 }));
    assert_eq!(nav.current_path().as_deref(), Some("/user/7"));
    assert_eq!(nav.depth(), 2);
}

#[test]
fn push_path_refuses_a_path_that_names_nothing() {
    let nav: Navigator<Screen> = Navigator::new(Screen::Home);
    assert!(!nav.push_path("/does/not/exist"));
    assert_eq!(nav.current(), Some(Screen::Home), "a bad link must not navigate");
    assert_eq!(nav.depth(), 1, "and must not touch the stack");
}

#[test]
fn screen_nav_deep_links_too() {
    let mut ctx = rosace::Context::new(rosace_trace::event::ComponentId(0));
    let nav: ScreenNav<Screen> = ScreenNav::new(&mut ctx, Screen::Home);
    assert!(nav.push_path("/widget/button"));
    assert_eq!(nav.current(), Some(Screen::Widget(Kind::Button)));
    assert!(!nav.push_path("/widget/nope"));
    assert_eq!(
        nav.current(),
        Some(Screen::Widget(Kind::Button)),
        "a refused deep link leaves the screen alone"
    );
}
