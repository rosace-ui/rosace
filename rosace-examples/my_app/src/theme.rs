//! App theme. Edit these to customize colors, or build a `ThemeData` from
//! scratch — the built-ins are just a convenient starting point.

use rosace::prelude::ThemeData;

/// The app's dark theme.
pub fn dark() -> ThemeData {
    rosace::prelude::dark_theme()
}

/// The app's light theme.
pub fn light() -> ThemeData {
    rosace::prelude::light_theme()
}

/// Per-platform look (D105): iOS gets Cupertino chrome, Android gets
/// Material chrome; every other platform (desktop, web) falls back to
/// `light()`. Passed to `App::themes(..)` in `lib.rs`.
pub fn themes() -> rosace::prelude::Themes {
    rosace::prelude::Themes::new(light())
        .platform(rosace::prelude::Platform::MacOs, rosace::prelude::material())
}
