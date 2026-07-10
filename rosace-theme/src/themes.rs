//! Platform-keyed theme bundle (D105 Phase 23 Step 2).
//!
//! `Themes` lets an app hand the framework a different [`ThemeData`] per
//! platform, with a required fallback. The framework resolves the active
//! theme ONCE at startup from the detected/overridden running platform
//! (`rosace_core::use_platform()`) — widgets never see this bundle, only
//! the single resolved `ThemeData`, exactly as `set_theme`/`use_theme` work
//! today. Apps that don't use `Themes` are unaffected (`App` with a single
//! `.theme(..)` keeps working — see `App::launch`).

use std::collections::HashMap;

use rosace_core::Platform;

use crate::theme::ThemeData;

/// A platform-keyed set of themes plus a required fallback.
///
/// ```rust,ignore
/// let themes = Themes::new(light_theme())
///     .platform(Platform::Ios, cupertino())
///     .platform(Platform::Android, material());
/// App::new().themes(themes).launch(MyApp);
/// ```
#[derive(Clone)]
pub struct Themes {
    fallback: ThemeData,
    per_platform: HashMap<Platform, ThemeData>,
}

impl Themes {
    /// Starts a bundle with the theme used for any platform that doesn't
    /// get its own entry via [`Themes::platform`].
    pub fn new(fallback: ThemeData) -> Self {
        Self { fallback, per_platform: HashMap::new() }
    }

    /// Registers `theme` for `platform`. Chain multiple calls for multiple
    /// platforms.
    pub fn platform(mut self, platform: Platform, theme: ThemeData) -> Self {
        self.per_platform.insert(platform, theme);
        self
    }

    /// Resolves the theme for `platform` — the registered one, or the
    /// fallback if none was registered for it.
    pub fn resolve(&self, platform: Platform) -> ThemeData {
        self.per_platform.get(&platform).cloned().unwrap_or_else(|| self.fallback.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::built_in::{cupertino, light_theme, material};

    #[test]
    fn resolve_returns_registered_theme_for_platform() {
        let themes = Themes::new(light_theme())
            .platform(Platform::Ios, cupertino())
            .platform(Platform::Android, material());
        assert_eq!(themes.resolve(Platform::Ios).app_bar.height, cupertino().app_bar.height);
        assert_eq!(themes.resolve(Platform::Android).app_bar.height, material().app_bar.height);
    }

    #[test]
    fn resolve_falls_back_for_unregistered_platform() {
        let themes = Themes::new(light_theme()).platform(Platform::Ios, cupertino());
        // macOS was never registered — must fall back, not panic or default-construct.
        let resolved = themes.resolve(Platform::MacOs);
        assert_eq!(resolved.app_bar.height, light_theme().app_bar.height);
    }
}
