//! Core theme types: `ThemeData` and the `TezzeraTheme` trait.

use crate::color::ColorScheme;
use crate::radius::BorderRadius;
use crate::spacing::Spacing;
use crate::typography::Typography;

/// Global animation policy (theme-level). Toggle-style widgets
/// (Switch, Checkbox, Radio) and other transitions read this: when
/// `enabled` is false everything snaps; otherwise they ease over
/// `duration_ms`. Set it once on the theme and every widget follows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub duration_ms: f32,
}

impl Default for AnimationConfig {
    fn default() -> Self { Self { enabled: true, duration_ms: 160.0 } }
}

/// All design tokens bundled together as a single snapshot.
///
/// `ThemeData` is `Clone` so it can be cheaply shared via the global atom.
#[derive(Debug, Clone)]
pub struct ThemeData {
    pub colors: ColorScheme,
    /// Global animation policy — see [`AnimationConfig`].
    pub animation: AnimationConfig,
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

impl ThemeData {
    /// Enable or disable global animation (theme-level).
    pub fn animations(mut self, enabled: bool) -> Self {
        self.animation.enabled = enabled; self
    }
    /// Set the global animation duration in milliseconds.
    pub fn animation_ms(mut self, ms: f32) -> Self {
        self.animation.duration_ms = ms; self
    }
}
