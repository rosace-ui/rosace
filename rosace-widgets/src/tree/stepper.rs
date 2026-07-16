//! `Stepper` (D115/Phase 32 Step 1) — the numeric −/+ control:
//! `[−] value [+]` with Button-style hover/press feedback per segment.
//!
//! Controlled, like every input widget: the app owns the value — pass it
//! to [`Stepper::new`] and write the new value back in `.on_change`.
//! At an end stop the corresponding button dims and absorbs the click
//! without firing (interactive-by-identity: it still owns its hit region).

use std::sync::Arc;

use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;

use super::button::lighten;
use super::container::draw_rounded_rect_pub;
use super::{LayoutCtx, PaintCtx, Widget};

/// Minimum width of the central value segment (logical px).
const MIN_VALUE_WIDTH: f32 = 48.0;

/// Clamped stepper arithmetic: `cur + delta × step`, saturating on
/// overflow, clamped into `[min, max]` (normalized if given reversed).
/// `delta` is the direction: `-1` for the − button, `+1` for the +.
pub(crate) fn next_value(cur: i64, delta: i64, min: i64, max: i64, step: i64) -> i64 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    cur.saturating_add(delta.saturating_mul(step)).clamp(lo, hi)
}

/// A numeric stepper: decrement button, current value, increment button.
pub struct Stepper {
    value: i64,
    min: i64,
    max: i64,
    step: i64,
    height: f32,
    font_size: f32,
    radius: f32,
    background: Option<Color>,
    foreground: Option<Color>,
    border: Option<(Color, f32)>,
    on_change: Option<Arc<dyn Fn(i64) + Send + Sync>>,
}

impl Stepper {
    /// A stepper showing `value` (unbounded range, step 1 by default).
    pub fn new(value: i64) -> Self {
        Self {
            value,
            min: i64::MIN,
            max: i64::MAX,
            step: 1,
            height: 32.0,
            font_size: 13.0,
            radius: 6.0,
            background: None,
            foreground: None,
            border: None,
            on_change: None,
        }
    }
    /// Lower bound (inclusive). The − button dims and stops firing there.
    pub fn min(mut self, v: i64) -> Self { self.min = v; self }
    /// Upper bound (inclusive). The + button dims and stops firing there.
    pub fn max(mut self, v: i64) -> Self { self.max = v; self }
    /// Increment per press (default `1`; clamped to at least `1`).
    pub fn step(mut self, s: i64) -> Self { self.step = s.max(1); self }
    /// Control height in logical px (default `32.0`).
    pub fn height(mut self, h: f32) -> Self { self.height = h.max(0.0); self }
    /// Value/glyph text size (default `13.0`).
    pub fn font_size(mut self, s: f32) -> Self { self.font_size = s; self }
    /// Corner radius of the track (default `6.0`).
    pub fn radius(mut self, r: f32) -> Self { self.radius = r.max(0.0); self }
    /// Track fill — defaults to the theme's `surface_variant`.
    pub fn background(mut self, c: Color) -> Self { self.background = Some(c); self }
    /// Text/glyph tint — defaults to the theme's `on_surface`.
    pub fn color(mut self, c: Color) -> Self { self.foreground = Some(c); self }
    /// Outline stroke around the track (color, width).
    pub fn border(mut self, c: Color, w: f32) -> Self { self.border = Some((c, w)); self }
    /// Called with the new (already clamped) value on every effective press.
    pub fn on_change(mut self, f: impl Fn(i64) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    /// Width of one −/+ button segment: square, matching the height.
    fn button_width(&self) -> f32 { self.height }

    /// Width of the central value segment for the current value's text.
    fn value_width(&self, ctx_font: &rosace_render::FontCache) -> f32 {
        (ctx_font.measure_text(&self.value.to_string(), self.font_size) + 16.0)
            .max(MIN_VALUE_WIDTH)
    }
}

impl Widget for Stepper {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let w = self.button_width() * 2.0 + self.value_width(ctx.font);
        ctx.constraints.constrain(Size { width: w, height: self.height })
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        // Hoisted theme reads (the borrow must end before mutable painting).
        let (bg, fg) = {
            let t = &ctx.theme.colors;
            (
                self.background.unwrap_or_else(|| ctx.tc(t.surface_variant)),
                self.foreground.unwrap_or_else(|| ctx.tc(t.on_surface)),
            )
        };
        let dim = Color::rgba(fg.r, fg.g, fg.b, 90);

        let r = ctx.rect;
        ctx.semantics(
            super::Semantics::new(rosace_core::Role::Slider)
                .label("stepper")
                .value(self.value.to_string()),
        );

        draw_rounded_rect_pub(ctx, r, bg, self.radius);
        if let Some((bc, bw)) = self.border {
            ctx.stroke_rrect(r, self.radius, bc, bw);
        }

        let bw = self.button_width().min(r.size.width / 2.0);
        let at_min = self.value <= self.min;
        let at_max = self.value >= self.max;

        // The two button segments + the centered value.
        let minus_rect = Rect { origin: r.origin, size: Size { width: bw, height: r.size.height } };
        let plus_rect = Rect {
            origin: Point { x: r.origin.x + r.size.width - bw, y: r.origin.y },
            size: Size { width: bw, height: r.size.height },
        };

        let value_text = self.value.to_string();
        let tw = ctx.font.measure_text(&value_text, self.font_size);
        let lh = ctx.font.line_height(self.font_size);
        ctx.draw_text_at(
            &value_text,
            Point {
                x: r.origin.x + (r.size.width - tw) / 2.0,
                y: r.origin.y + (r.size.height - lh) / 2.0,
            },
            fg,
            self.font_size,
        );

        for (rect, glyph, disabled, delta, sem_label) in [
            (minus_rect, "\u{2212}", at_min, -1i64, "decrement"),
            (plus_rect, "+", at_max, 1i64, "increment"),
        ] {
            let mut slot = ctx.child(rect);
            slot.semantics(super::Semantics::new(rosace_core::Role::Button).label(sem_label));

            // Hover/press lift, the Button/FAB convention (D108 Step 1).
            let target = if disabled { 0.0 } else if slot.pressed() { 1.0 } else if slot.hovered() { 0.5 } else { 0.0 };
            let emphasis = slot.animate_to(target, 0.0);
            if emphasis > 0.0 {
                draw_rounded_rect_pub(
                    &mut slot,
                    rect,
                    lighten(bg, (0.12 * emphasis * 2.0).min(1.0)),
                    self.radius,
                );
            }

            let glyph_color = if disabled { dim } else { fg };
            let gw = slot.font.measure_text(glyph, self.font_size);
            let gh = slot.font.line_height(self.font_size);
            slot.draw_text_at(
                glyph,
                Point {
                    x: rect.origin.x + (rect.size.width - gw) / 2.0,
                    y: rect.origin.y + (rect.size.height - gh) / 2.0,
                },
                glyph_color,
                self.font_size,
            );

            // Interactive-by-identity: the segment ALWAYS owns its hit
            // region — disabled/unwired presses absorb and do nothing.
            match (&self.on_change, disabled) {
                (Some(cb), false) => {
                    let cb = Arc::clone(cb);
                    let (cur, min, max, step) = (self.value, self.min, self.max, self.step);
                    slot.register_hit(Arc::new(move || cb(next_value(cur, delta, min, max, step))));
                }
                _ => slot.register_hit(Arc::new(|| {})),
            }
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
    fn stepper_is_two_buttons_plus_the_value_segment_wide() {
        let s = Stepper::new(5).height(32.0);
        let (font, theme) = test_env();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = s.layout(&ctx);
        assert_eq!(size.height, 32.0);
        // 2 square buttons (32 each) + the value segment (≥ MIN_VALUE_WIDTH).
        assert!(size.width >= 64.0 + MIN_VALUE_WIDTH, "got {}", size.width);
    }

    #[test]
    fn next_value_steps_and_clamps() {
        assert_eq!(next_value(5, 1, 0, 10, 1), 6);
        assert_eq!(next_value(5, -1, 0, 10, 1), 4);
        assert_eq!(next_value(5, 1, 0, 10, 3), 8);
        // Clamps at both ends (including a partial last step).
        assert_eq!(next_value(10, 1, 0, 10, 1), 10);
        assert_eq!(next_value(0, -1, 0, 10, 1), 0);
        assert_eq!(next_value(9, 1, 0, 10, 5), 10);
    }

    #[test]
    fn next_value_saturates_instead_of_overflowing() {
        assert_eq!(next_value(i64::MAX, 1, i64::MIN, i64::MAX, 1), i64::MAX);
        assert_eq!(next_value(i64::MIN, -1, i64::MIN, i64::MAX, 1), i64::MIN);
        assert_eq!(next_value(0, 1, i64::MIN, i64::MAX, i64::MAX), i64::MAX);
    }

    #[test]
    fn next_value_normalizes_a_reversed_range() {
        assert_eq!(next_value(5, 1, 10, 0, 1), 6); // min/max swapped by caller
    }
}
