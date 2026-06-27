//! Core theme types: `ThemeData` and the `TezzeraTheme` trait.

use crate::color::ColorScheme;
use crate::radius::BorderRadius;
use crate::spacing::Spacing;
use crate::typography::Typography;

/// All design tokens bundled together as a single snapshot.
///
/// `ThemeData` is `Clone` so it can be cheaply shared via the global atom.
#[derive(Debug, Clone)]
pub struct ThemeData {
    pub colors: ColorScheme,
    pub typography: Typography,
    pub spacing: Spacing,
    pub radius: BorderRadius,
    /// `true` for dark themes; `false` for light themes.
    pub is_dark: bool,
}

/// Implement this trait to supply a custom theme to the framework.
///
/// The bound `Send + Sync + 'static` ensures that theme objects can be stored
/// in global statics and shared across threads.
pub trait TezzeraTheme: Send + Sync + 'static {
    fn theme_data(&self) -> &ThemeData;
}
