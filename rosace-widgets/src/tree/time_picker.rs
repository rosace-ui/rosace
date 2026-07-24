//! `TimePicker` — an Android-Material **clock-dial** hour:minute picker
//! (D115/Phase 32; dial per `.steering/PICKERS_SPEC.md`, 2026-07-24).
//!
//! A circular dial with an animated hand + thumb, an AM/PM toggle, and a
//! header whose hour/minute switch which the dial edits. No seconds (removed
//! by design — see the spec). Every component is stylable via a builder and
//! theme-defaulted. Controlled: the app owns `value` (+ optionally the edit
//! unit) and gets changes back through callbacks.

use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{LayoutCtx, PaintCtx, Widget, vcenter_text_y};
use super::container::draw_rounded_rect_pub;

/// A plain wall-clock time — hour (0-23) and minute (0-59). No seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimpleTime {
    pub hour: u8,
    pub minute: u8,
}

impl SimpleTime {
    pub fn new(hour: u8, minute: u8) -> Self { Self { hour: hour.min(23), minute: minute.min(59) } }
    /// `(1-12, is_pm)` — the 12-hour display form.
    pub fn hour_12(self) -> (u8, bool) {
        let is_pm = self.hour >= 12;
        (match self.hour % 12 { 0 => 12, h => h }, is_pm)
    }
    pub fn with_hour_12(self, h12: u8, is_pm: bool) -> Self {
        let h12 = h12.clamp(1, 12);
        let hour = match (h12, is_pm) { (12, false) => 0, (12, true) => 12, (h, false) => h, (h, true) => h + 12 };
        Self::new(hour, self.minute)
    }
}

/// Which unit the dial currently edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit { Hour, Minute }

const DIAL_D: f32 = 240.0;
const HEADER_H: f32 = 64.0;
const PAD: f32 = 16.0;

/// A Material clock-dial time picker.
pub struct TimePicker {
    value: SimpleTime,
    editing: TimeUnit,
    minute_step: u8,
    accent: Option<Color>,
    dial_color: Option<Color>,
    hand_color: Option<Color>,
    thumb_color: Option<Color>,
    number_color: Option<Color>,
    selected_number_color: Option<Color>,
    on_change: Option<Arc<dyn Fn(SimpleTime) + Send + Sync>>,
    on_unit_change: Option<Arc<dyn Fn(TimeUnit) + Send + Sync>>,
}

impl TimePicker {
    pub fn new(value: SimpleTime) -> Self {
        Self {
            value, editing: TimeUnit::Hour, minute_step: 1,
            accent: None, dial_color: None, hand_color: None, thumb_color: None,
            number_color: None, selected_number_color: None,
            on_change: None, on_unit_change: None,
        }
    }
    /// Which unit the dial edits (controlled; pair with `.on_unit_change`).
    pub fn editing(mut self, u: TimeUnit) -> Self { self.editing = u; self }
    pub fn minute_step(mut self, s: u8) -> Self { self.minute_step = s.max(1); self }
    pub fn accent(mut self, c: Color) -> Self { self.accent = Some(c); self }
    /// Dial face fill (default: a faint `surface_variant`).
    pub fn dial_color(mut self, c: Color) -> Self { self.dial_color = Some(c); self }
    /// The hand line color (default: accent).
    pub fn hand_color(mut self, c: Color) -> Self { self.hand_color = Some(c); self }
    /// The thumb disc at the hand's tip (default: accent).
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    /// Unselected number color (default: `on_surface`).
    pub fn number_color(mut self, c: Color) -> Self { self.number_color = Some(c); self }
    /// The number sitting on the thumb (default: bright/`on_primary`).
    pub fn selected_number_color(mut self, c: Color) -> Self { self.selected_number_color = Some(c); self }
    pub fn on_change(mut self, f: impl Fn(SimpleTime) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f)); self
    }
    pub fn on_unit_change(mut self, f: impl Fn(TimeUnit) + Send + Sync + 'static) -> Self {
        self.on_unit_change = Some(Arc::new(f)); self
    }

    fn target_angle(&self) -> f32 {
        match self.editing {
            TimeUnit::Hour => (self.value.hour_12().0 as f32 % 12.0) * 30.0,
            TimeUnit::Minute => self.value.minute as f32 * 6.0,
        }
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}
/// Point on a circle at `deg` clockwise from 12-o'clock.
fn on_circle(cx: f32, cy: f32, r: f32, deg: f32) -> (f32, f32) {
    let a = deg.to_radians();
    (cx + r * a.sin(), cy - r * a.cos())
}

impl Widget for TimePicker {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: DIAL_D + PAD * 2.0, height: HEADER_H + DIAL_D + PAD * 2.0 }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let colors = ctx.theme.colors.clone();
        let accent = self.accent.unwrap_or_else(|| ctx.tc(colors.primary));
        let on_surface = ctx.tc(colors.on_surface);
        let dial_fill = self.dial_color.unwrap_or_else(|| with_alpha(ctx.tc(colors.surface_variant), 0.55));
        let hand_c = self.hand_color.unwrap_or(accent);
        let thumb_c = self.thumb_color.unwrap_or(accent);
        let num_c = self.number_color.unwrap_or(on_surface);
        let sel_num_c = self.selected_number_color.unwrap_or(Color::rgb(252, 252, 255));

        let r = ctx.rect;
        let (h12, is_pm) = self.value.hour_12();

        // ── Header: HH : MM  (tappable to switch unit) + AM/PM ───────────────
        let hy = r.origin.y + PAD;
        let big = 34.0;
        let hh = format!("{h12:02}");
        let mm = format!("{:02}", self.value.minute);
        let hw = ctx.font.measure_text(&hh, big);
        let cw = ctx.font.measure_text(":", big);
        let mw = ctx.font.measure_text(&mm, big);
        let group_w = hw + 8.0 + cw + 8.0 + mw;
        let hx = r.origin.x + (r.size.width - group_w) / 2.0 - 10.0;
        let hour_sel = matches!(self.editing, TimeUnit::Hour);
        ctx.draw_text_at(&hh, Point { x: hx, y: vcenter_text_y(hy, big, ctx.font, big) },
            if hour_sel { accent } else { with_alpha(on_surface, 0.55) }, big);
        ctx.draw_text_at(":", Point { x: hx + hw + 8.0, y: vcenter_text_y(hy, big, ctx.font, big) }, with_alpha(on_surface, 0.55), big);
        ctx.draw_text_at(&mm, Point { x: hx + hw + 8.0 + cw + 8.0, y: vcenter_text_y(hy, big, ctx.font, big) },
            if !hour_sel { accent } else { with_alpha(on_surface, 0.55) }, big);
        let hour_hit = Rect { origin: Point { x: hx - 4.0, y: hy }, size: Size { width: hw + 8.0, height: big } };
        let min_hit = Rect { origin: Point { x: hx + hw + 8.0 + cw + 4.0, y: hy }, size: Size { width: mw + 8.0, height: big } };
        if let Some(uc) = &self.on_unit_change {
            { let uc = uc.clone(); ctx.child(hour_hit).register_hit(Arc::new(move || uc(TimeUnit::Hour))); }
            { let uc = uc.clone(); ctx.child(min_hit).register_hit(Arc::new(move || uc(TimeUnit::Minute))); }
        }

        // AM/PM pill (top-right).
        let ampm_label = if is_pm { "PM" } else { "AM" };
        let ap_w = 44.0;
        let ap_rect = Rect { origin: Point { x: r.origin.x + r.size.width - ap_w - PAD, y: hy + 4.0 }, size: Size { width: ap_w, height: 30.0 } };
        draw_rounded_rect_pub(ctx, ap_rect, with_alpha(accent, 0.9), 8.0);
        let apw = ctx.font.measure_text(ampm_label, 14.0);
        ctx.draw_text_at(ampm_label, Point { x: ap_rect.origin.x + (ap_w - apw) / 2.0, y: vcenter_text_y(ap_rect.origin.y, 30.0, ctx.font, 14.0) }, sel_num_c, 14.0);
        if let Some(oc) = &self.on_change {
            let oc = oc.clone(); let v = self.value;
            ctx.child(ap_rect).register_hit(Arc::new(move || { let (h, pm) = v.hour_12(); oc(v.with_hour_12(h, !pm)); }));
        }

        // ── Dial ─────────────────────────────────────────────────────────────
        let cx = r.origin.x + r.size.width / 2.0;
        let cy = r.origin.y + HEADER_H + PAD + DIAL_D / 2.0;
        let dial_r = DIAL_D / 2.0;
        let num_r = dial_r - 22.0; // ring the numbers sit on

        ctx.fill_circle(Point { x: cx, y: cy }, dial_r, dial_fill);

        // Animated hand angle: seed at 12-o'clock so it sweeps to the target on
        // first launch, then eases on every value/unit change.
        ctx.seed_channel_if_unset(0, 0.0);
        let angle = ctx.animate_channel(0, self.target_angle(), 0.0);
        let (tx, ty) = on_circle(cx, cy, num_r, angle);

        // Hand: a SOLID line drawn as densely-overlapping circles (there is no
        // line primitive). Step < radius so it reads as one continuous stroke,
        // not dots. Stops short of the thumb so it doesn't overdraw the number.
        let hand_len = ((tx - cx).powi(2) + (ty - cy).powi(2)).sqrt().max(1.0);
        let stroke_r = 2.2;
        let steps = (hand_len / (stroke_r * 0.7)).ceil() as i32;
        let start = (10.0 / hand_len).clamp(0.0, 1.0); // leave the hub
        let end = ((hand_len - 16.0) / hand_len).clamp(0.0, 1.0); // stop at thumb
        for i in 0..=steps {
            let t = start + (end - start) * (i as f32 / steps as f32);
            ctx.fill_circle(Point { x: cx + (tx - cx) * t, y: cy + (ty - cy) * t }, stroke_r, hand_c);
        }
        ctx.fill_circle(Point { x: cx, y: cy }, 4.5, hand_c);          // centre hub
        ctx.fill_circle(Point { x: tx, y: ty }, 18.0, thumb_c);        // thumb disc

        // Numbers around the ring.
        let sel_index = match self.editing {
            TimeUnit::Hour => (self.value.hour_12().0 % 12) as i32,
            TimeUnit::Minute => (self.value.minute / 5) as i32 * if self.value.minute % 5 == 0 { 1 } else { 1 },
        };
        for i in 0..12 {
            let deg = i as f32 * 30.0;
            let (nx, ny) = on_circle(cx, cy, num_r, deg);
            let label = match self.editing {
                TimeUnit::Hour => if i == 0 { "12".to_string() } else { i.to_string() },
                TimeUnit::Minute => format!("{:02}", i * 5),
            };
            let is_sel = match self.editing {
                TimeUnit::Hour => (self.value.hour_12().0 % 12) as i32 == i,
                TimeUnit::Minute => (self.value.minute as i32 / 5) == i && self.value.minute % 5 == 0,
            };
            let _ = sel_index;
            let nw = ctx.font.measure_text(&label, 15.0);
            let nh = ctx.font.line_height(15.0);
            ctx.draw_text_at(&label, Point { x: nx - nw / 2.0, y: ny - nh / 2.0 },
                if is_sel { sel_num_c } else { num_c }, 15.0);
        }

        // Dial hit: tap/drag anywhere on the face selects (draggable via the
        // engine's positional press). Round to the nearest hour / stepped minute.
        let dial_rect = Rect { origin: Point { x: cx - dial_r, y: cy - dial_r }, size: Size { width: DIAL_D, height: DIAL_D } };
        if let Some(oc) = &self.on_change {
            let oc = oc.clone();
            let (unit, step, v) = (self.editing, self.minute_step, self.value);
            let dial = ctx.child(dial_rect);
            dial.on_press_at(move |px, py| {
                let dx = px - cx; let dy = py - cy;
                let mut deg = dx.atan2(-dy).to_degrees();
                if deg < 0.0 { deg += 360.0; }
                match unit {
                    TimeUnit::Hour => {
                        let h = (((deg / 30.0).round() as i32) % 12 + 12) % 12;
                        let h12 = if h == 0 { 12 } else { h as u8 };
                        oc(v.with_hour_12(h12, v.hour_12().1));
                    }
                    TimeUnit::Minute => {
                        let m = (((deg / 6.0).round() as i32) % 60 + 60) % 60;
                        let snapped = ((m as f32 / step as f32).round() as i32 * step as i32).rem_euclid(60) as u8;
                        oc(SimpleTime::new(v.hour, snapped));
                    }
                }
            });
        } else {
            ctx.child(dial_rect).on_press_at(|_, _| {});
        }

        ctx.semantics(super::Semantics::new(rosace_core::Role::Unknown)
            .label(format!("Time picker, {h12:02}:{:02} {}", self.value.minute, ampm_label)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    #[ignore] // TIME_PNG=/path cargo test -p rosace-widgets clock_showcase -- --ignored --nocapture
    fn clock_showcase() {
        use super::super::app::WidgetApp;
        let out = std::env::var("TIME_PNG").unwrap_or_else(|_| "clock.png".to_string());
        let w = TimePicker::new(SimpleTime::new(9, 30));
        // Settled frame (animation off) so the hand rests at the real position.
        let mut theme = rosace_theme::built_in::dark_theme();
        theme.animation.enabled = false;
        std::fs::write(&out, WidgetApp::new((DIAL_D + PAD * 2.0) as u32, (HEADER_H + DIAL_D + PAD * 2.0) as u32).theme(theme).render_png(&w)).unwrap();
        println!("wrote {out}");
    }

    #[test]
    fn hour_12_conversion_round_trips() {
        assert_eq!(SimpleTime::new(0, 0).hour_12(), (12, false));
        assert_eq!(SimpleTime::new(12, 0).hour_12(), (12, true));
        assert_eq!(SimpleTime::new(13, 30).hour_12(), (1, true));
    }

    #[test]
    fn with_hour_12_reconstructs_24_hour() {
        let base = SimpleTime::new(0, 45);
        assert_eq!(base.with_hour_12(12, false).hour, 0);
        assert_eq!(base.with_hour_12(1, true).hour, 13);
    }

    #[test]
    fn target_angle_maps_hour_and_minute() {
        // 3 o'clock → 90°, 9 o'clock → 270°, :30 → 180°.
        assert_eq!(TimePicker::new(SimpleTime::new(3, 0)).target_angle(), 90.0);
        assert_eq!(TimePicker::new(SimpleTime::new(9, 0)).target_angle(), 270.0);
        assert_eq!(TimePicker::new(SimpleTime::new(0, 30)).editing(TimeUnit::Minute).target_angle(), 180.0);
    }

    #[test]
    fn layout_is_dial_plus_header() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 500.0), &font, &theme);
        let size = TimePicker::new(SimpleTime::new(9, 30)).layout(&ctx);
        assert_eq!(size.width, DIAL_D + PAD * 2.0);
    }
}
