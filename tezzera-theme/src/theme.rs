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

/// Where an [`AppBar`](tezzera_widgets equivalent — see `tezzera-widgets`)
/// positions its title relative to the bar, per D105.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleAlign {
    /// Centered within the space between the leading widget and the actions
    /// (falls back to left-aligned if the title doesn't fit) — today's
    /// existing behavior, kept as the default so converting AppBar to read
    /// this doesn't change any app that hasn't opted into a platform theme.
    Leading,
    /// Centered in the FULL bar width regardless of leading/actions — the
    /// iOS convention.
    Center,
}

/// Per-widget platform styling for `AppBar` (D105 Phase 23 Step 3 — the
/// proof-of-concept widget for the whole per-widget-Style-struct model).
/// Per-instance builder calls on the widget itself (`.height(..)`,
/// `.traffic_lights()`) override these theme defaults; a widget that
/// doesn't set them falls back to whatever the active theme says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppBarStyle {
    pub title_align: TitleAlign,
    /// Draw faux macOS traffic-light dots. Stays `false` on every built-in
    /// theme, including macOS — a real window already has real OS traffic
    /// lights; this is decorative mockup chrome for docs/screenshots only
    /// (see the widget's own doc comment), never something a theme should
    /// silently turn on for real apps.
    pub show_traffic_lights: bool,
    pub height: f32,
    /// > 0 draws the bar's separating edge (today's flat bottom border);
    /// `0.0` omits it. Not yet a real elevation/shadow effect — a coarser
    /// "on/off" proxy for the proof, real elevation rendering is later work.
    pub elevation: f32,
}

impl Default for AppBarStyle {
    fn default() -> Self {
        Self { title_align: TitleAlign::Leading, show_traffic_lights: false, height: 44.0, elevation: 1.0 }
    }
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
    /// Platform-adaptive AppBar defaults (D105). The first of what will
    /// become several per-widget Style fields — see Phase 23.
    pub app_bar: AppBarStyle,
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
