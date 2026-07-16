//! `RatingBar` (D115/Phase 32 Step 1) — a row of stars showing a rating,
//! tappable to set one (whole stars only; a tap rounds to the star under
//! the pointer — half-star display is named follow-up work, not silently
//! attempted).
//!
//! Stars render through [`super::Icon`]'s `Star` (same vector-icon pipeline
//! as everything else): filled stars in `.color()`, the rest dimmed in
//! `.empty_color()`. Read-only without `.on_change` — the tap region still
//! absorbs (interactive-by-identity), it just changes nothing.

use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;

use super::{LayoutCtx, PaintCtx, Widget};

/// Default number of stars.
const DEFAULT_COUNT: u8 = 5;
/// Default star box size (logical px).
const DEFAULT_SIZE: f32 = 20.0;
/// Default gap between stars (logical px).
const DEFAULT_SPACING: f32 = 4.0;

/// Pure tap-position → rating mapping: which whole-star rating a press at
/// `local_x` px (from the widget's left edge) selects. Each star owns its
/// box plus the trailing gap; the result is always `1..=count` (`0.0` only
/// for an empty bar).
pub(crate) fn rating_at(local_x: f32, count: u8, star_size: f32, spacing: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let slot = (star_size + spacing).max(1.0);
    let idx = (local_x / slot).floor().clamp(0.0, count as f32 - 1.0);
    idx + 1.0
}

/// A star-rating display/input row.
pub struct RatingBar {
    /// Current rating in `0.0..=count` (rendered rounded to whole stars).
    value: f32,
    count: u8,
    size: f32,
    spacing: f32,
    color: Option<Color>,
    empty_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(f32) + Send + Sync>>,
}

impl RatingBar {
    /// A rating bar showing `value` stars (of [`RatingBar::count`], default 5).
    pub fn new(value: f32) -> Self {
        Self {
            value: value.max(0.0),
            count: DEFAULT_COUNT,
            size: DEFAULT_SIZE,
            spacing: DEFAULT_SPACING,
            color: None,
            empty_color: None,
            on_change: None,
        }
    }
    /// Number of stars (default `5`).
    pub fn count(mut self, n: u8) -> Self { self.count = n; self }
    /// Star box size in logical px (default `20.0`).
    pub fn size(mut self, s: f32) -> Self { self.size = s.max(1.0); self }
    /// Gap between stars in logical px (default `4.0`).
    pub fn spacing(mut self, s: f32) -> Self { self.spacing = s.max(0.0); self }
    /// Filled-star tint — defaults to the theme's `primary`.
    pub fn color(mut self, c: Color) -> Self { self.color = Some(c); self }
    /// Empty-star tint — defaults to the theme's `on_surface`, dimmed.
    pub fn empty_color(mut self, c: Color) -> Self { self.empty_color = Some(c); self }
    /// Called with the new rating (`1.0..=count`, whole stars) on tap/drag.
    /// Without it the bar is read-only (taps absorb, nothing changes).
    pub fn on_change(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
}

impl Widget for RatingBar {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let n = self.count as f32;
        let w = n * self.size + (n - 1.0).max(0.0) * self.spacing;
        ctx.constraints.constrain(Size { width: w, height: self.size })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (the borrow must end before mutable painting).
        let (filled, empty) = {
            let t = &ctx.theme.colors;
            let on_surface = ctx.tc(t.on_surface);
            (
                self.color.unwrap_or_else(|| ctx.tc(t.primary)),
                self.empty_color.unwrap_or(Color::rgba(
                    on_surface.r, on_surface.g, on_surface.b, 70,
                )),
            )
        };

        let r = ctx.rect;
        ctx.semantics(
            super::Semantics::new(rosace_core::Role::Slider)
                .label("rating")
                .value(format!("{:.0} of {}", self.value.round(), self.count)),
        );

        // Tap/drag sets the rating — always registered
        // (interactive-by-identity): read-only bars absorb the press.
        match &self.on_change {
            Some(cb) => {
                let cb = Arc::clone(cb);
                let (left, count, size, spacing) =
                    (r.origin.x, self.count, self.size, self.spacing);
                ctx.on_press_at(move |x, _y| cb(rating_at(x - left, count, size, spacing)));
            }
            None => ctx.on_press_at(|_, _| {}),
        }

        let lit = self.value.round().clamp(0.0, self.count as f32) as u8;
        for i in 0..self.count {
            let star_rect = Rect {
                origin: Point {
                    x: r.origin.x + i as f32 * (self.size + self.spacing),
                    y: r.origin.y,
                },
                size: Size { width: self.size, height: self.size },
            };
            let tint = if i < lit { filled } else { empty };
            super::Icon::new(super::IconKind::Star)
                .size(self.size)
                .color(tint)
                .paint(&mut ctx.child(star_rect));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    fn test_env() -> (rosace_render::FontCache, rosace_theme::ThemeData) {
        (rosace_render::FontCache::embedded(), rosace_theme::built_in::dark_theme())
    }

    #[test]
    fn width_is_count_times_size_plus_gaps() {
        let bar = RatingBar::new(3.0).count(5).size(20.0).spacing(4.0);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = bar.layout(&ctx);
        // 5 × 20 + 4 × 4 gaps = 116.
        assert_eq!((size.width, size.height), (116.0, 20.0));
    }

    #[test]
    fn single_star_bar_has_no_gap() {
        let bar = RatingBar::new(1.0).count(1).size(24.0);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        assert_eq!(bar.layout(&ctx).width, 24.0);
    }

    #[test]
    fn tap_position_maps_to_the_star_under_it() {
        // count 5, size 20, spacing 4 → 24px slots.
        assert_eq!(rating_at(0.0, 5, 20.0, 4.0), 1.0);
        assert_eq!(rating_at(10.0, 5, 20.0, 4.0), 1.0);
        assert_eq!(rating_at(25.0, 5, 20.0, 4.0), 2.0);
        assert_eq!(rating_at(100.0, 5, 20.0, 4.0), 5.0);
    }

    #[test]
    fn tap_mapping_clamps_outside_the_bar() {
        assert_eq!(rating_at(-30.0, 5, 20.0, 4.0), 1.0);
        assert_eq!(rating_at(10_000.0, 5, 20.0, 4.0), 5.0);
        assert_eq!(rating_at(50.0, 0, 20.0, 4.0), 0.0);
    }
}
