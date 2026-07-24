//! `TimePicker` (D115/Phase 32 Step 1) — spinner/column style
//! hour : minute AM/PM control. Simpler and more cross-platform-consistent
//! than Material's clock-dial picker (a dial variant is future work, named
//! in `.steering/PHASE_32.md`).

use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{LayoutCtx, PaintCtx, Widget, vcenter_text_y};
use super::container::draw_rounded_rect_pub;

/// A plain wall-clock time — hour (0-23) and minute (0-59), no date/timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimpleTime {
    /// 0-23 (24-hour, internal storage).
    pub hour: u8,
    pub minute: u8,
}

impl SimpleTime {
    pub fn new(hour: u8, minute: u8) -> Self {
        Self { hour: hour.min(23), minute: minute.min(59) }
    }

    /// `(1-12, is_pm)` — the 12-hour display form.
    pub fn hour_12(self) -> (u8, bool) {
        let is_pm = self.hour >= 12;
        let h12 = match self.hour % 12 { 0 => 12, h => h };
        (h12, is_pm)
    }

    pub fn with_hour_12(self, h12: u8, is_pm: bool) -> Self {
        let h12 = h12.clamp(1, 12);
        let hour = match (h12, is_pm) {
            (12, false) => 0,
            (12, true) => 12,
            (h, false) => h,
            (h, true) => h + 12,
        };
        Self::new(hour, self.minute)
    }
}

/// One spinner column: a value + up/down arrows.
struct SpinnerColumn<'a> {
    label: String,
    rect: Rect,
    on_up: Option<&'a Arc<dyn Fn() + Send + Sync>>,
    on_down: Option<&'a Arc<dyn Fn() + Send + Sync>>,
}

const COLUMN_W: f32 = 56.0;
const ARROW_H: f32 = 24.0;
const VALUE_H: f32 = 40.0;
const GAP: f32 = 8.0;

/// Hour:minute AM/PM spinner. Controlled — the app owns `value`.
/// Composable inside a `Dialog` for a modal time-selection flow, or used
/// inline.
pub struct TimePicker {
    value: SimpleTime,
    minute_step: u8,
    accent: Option<Color>,
    on_change: Option<Arc<dyn Fn(SimpleTime) + Send + Sync>>,
}

impl TimePicker {
    pub fn new(value: SimpleTime) -> Self {
        Self { value, minute_step: 1, accent: None, on_change: None }
    }

    /// Minute increment/decrement step (e.g. `5` for 5-minute snapping).
    pub fn minute_step(mut self, step: u8) -> Self { self.minute_step = step.max(1); self }
    pub fn accent(mut self, c: Color) -> Self { self.accent = Some(c); self }

    pub fn on_change(mut self, f: impl Fn(SimpleTime) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }
}

impl Widget for TimePicker {
    fn layout(&self, _ctx: &LayoutCtx) -> Size {
        Size { width: COLUMN_W * 3.0 + GAP * 2.0, height: ARROW_H * 2.0 + VALUE_H }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let (bg, muted, accent) = {
            let t = &ctx.theme.colors;
            (ctx.tc(t.surface), ctx.tc(t.outline),
             self.accent.unwrap_or_else(|| ctx.tc(t.primary)))
        };
        let r = ctx.rect;
        let (h12, is_pm) = self.value.hour_12();

        let value = self.value;
        let step = self.minute_step;
        let on_change = self.on_change.clone();

        let hour_up: Option<Arc<dyn Fn() + Send + Sync>> = on_change.clone().map(|f| {
            Arc::new(move || {
                let (h, pm) = value.hour_12();
                f(value.with_hour_12(if h == 12 { 1 } else { h + 1 }, pm));
            }) as Arc<dyn Fn() + Send + Sync>
        });
        let hour_down: Option<Arc<dyn Fn() + Send + Sync>> = on_change.clone().map(|f| {
            Arc::new(move || {
                let (h, pm) = value.hour_12();
                f(value.with_hour_12(if h == 1 { 12 } else { h - 1 }, pm));
            }) as Arc<dyn Fn() + Send + Sync>
        });
        let minute_up: Option<Arc<dyn Fn() + Send + Sync>> = on_change.clone().map(|f| {
            Arc::new(move || f(SimpleTime::new(value.hour, (value.minute + step) % 60))) as Arc<dyn Fn() + Send + Sync>
        });
        let minute_down: Option<Arc<dyn Fn() + Send + Sync>> = on_change.clone().map(|f| {
            Arc::new(move || {
                let m = (value.minute as i16 - step as i16).rem_euclid(60) as u8;
                f(SimpleTime::new(value.hour, m));
            }) as Arc<dyn Fn() + Send + Sync>
        });
        let ampm_toggle: Option<Arc<dyn Fn() + Send + Sync>> = on_change.map(|f| {
            Arc::new(move || {
                let (h, pm) = value.hour_12();
                f(value.with_hour_12(h, !pm));
            }) as Arc<dyn Fn() + Send + Sync>
        });

        let hour_rect = Rect { origin: r.origin, size: Size { width: COLUMN_W, height: r.size.height } };
        let minute_rect = Rect { origin: Point { x: r.origin.x + COLUMN_W + GAP, y: r.origin.y }, size: Size { width: COLUMN_W, height: r.size.height } };
        let ampm_rect = Rect { origin: Point { x: r.origin.x + (COLUMN_W + GAP) * 2.0, y: r.origin.y }, size: Size { width: COLUMN_W, height: r.size.height } };

        self.paint_column(ctx, SpinnerColumn {
            label: format!("{h12:02}"), rect: hour_rect, on_up: hour_up.as_ref(), on_down: hour_down.as_ref(),
        }, bg, muted, accent);
        self.paint_column(ctx, SpinnerColumn {
            label: format!("{:02}", self.value.minute), rect: minute_rect, on_up: minute_up.as_ref(), on_down: minute_down.as_ref(),
        }, bg, muted, accent);

        // AM/PM: a single toggle button rather than up/down (only 2 states).
        // Same accent-pill treatment as the hour/minute values, so all three
        // columns read as one consistent "current selection", not a mix of
        // plain labels and highlighted ones.
        let ampm_label = if is_pm { "PM" } else { "AM" };
        let value_rect = Rect {
            origin: Point { x: ampm_rect.origin.x, y: ampm_rect.origin.y + ARROW_H },
            size: Size { width: COLUMN_W, height: VALUE_H },
        };
        let pill = Rect {
            origin: Point { x: value_rect.origin.x + 4.0, y: value_rect.origin.y + 4.0 },
            size: Size { width: value_rect.size.width - 8.0, height: value_rect.size.height - 8.0 },
        };
        draw_rounded_rect_pub(ctx, pill, accent, 8.0);
        let tw = ctx.font.measure_text(ampm_label, 16.0);
        ctx.draw_text_at(ampm_label, Point {
            x: value_rect.origin.x + (COLUMN_W - tw) / 2.0,
            y: vcenter_text_y(value_rect.origin.y, VALUE_H, ctx.font, 16.0),
        }, bg, 16.0);
        let node = ctx.node;
        match ampm_toggle {
            Some(f) => { ctx.tree.borrow_mut().node_mut(node).hits.push((value_rect, f)); }
            None => { ctx.tree.borrow_mut().node_mut(node).hits.push((value_rect, Arc::new(|| {}))); }
        }

        ctx.semantics(super::Semantics::new(rosace_core::Role::Unknown)
            .label(format!("Time picker, {h12:02}:{:02} {}", self.value.minute, ampm_label)));
    }
}

impl TimePicker {
    /// `bg` is the text color drawn ON TOP of the accent pill (theme surface,
    /// for contrast — same convention as `DatePicker`'s selected-day circle).
    fn paint_column(&self, ctx: &mut PaintCtx, col: SpinnerColumn, bg: Color, muted: Color, accent: Color) {
        let up_rect = Rect { origin: col.rect.origin, size: Size { width: COLUMN_W, height: ARROW_H } };
        let value_rect = Rect { origin: Point { x: col.rect.origin.x, y: col.rect.origin.y + ARROW_H }, size: Size { width: COLUMN_W, height: VALUE_H } };
        let down_rect = Rect { origin: Point { x: col.rect.origin.x, y: col.rect.origin.y + ARROW_H + VALUE_H }, size: Size { width: COLUMN_W, height: ARROW_H } };

        let up_w = ctx.font.measure_text("^", 12.0);
        ctx.draw_text_at("^", Point { x: up_rect.origin.x + (COLUMN_W - up_w) / 2.0, y: vcenter_text_y(up_rect.origin.y, ARROW_H, ctx.font, 12.0) }, muted, 12.0);
        let down_w = ctx.font.measure_text("v", 12.0);
        ctx.draw_text_at("v", Point { x: down_rect.origin.x + (COLUMN_W - down_w) / 2.0, y: vcenter_text_y(down_rect.origin.y, ARROW_H, ctx.font, 12.0) }, muted, 12.0);

        // Accent pill behind the current value — marks it as the live
        // selection (mirrors DatePicker's filled circle on the selected day),
        // instead of rendering as an indistinguishable plain label.
        let pill = Rect {
            origin: Point { x: value_rect.origin.x + 4.0, y: value_rect.origin.y + 4.0 },
            size: Size { width: value_rect.size.width - 8.0, height: value_rect.size.height - 8.0 },
        };
        draw_rounded_rect_pub(ctx, pill, accent, 8.0);
        let vw = ctx.font.measure_text(&col.label, 18.0);
        ctx.draw_text_at(&col.label, Point { x: value_rect.origin.x + (COLUMN_W - vw) / 2.0, y: vcenter_text_y(value_rect.origin.y, VALUE_H, ctx.font, 18.0) }, bg, 18.0);

        let node = ctx.node;
        match col.on_up {
            Some(f) => { let f = f.clone(); ctx.tree.borrow_mut().node_mut(node).hits.push((up_rect, f)); }
            None => { ctx.tree.borrow_mut().node_mut(node).hits.push((up_rect, Arc::new(|| {}))); }
        }
        match col.on_down {
            Some(f) => { let f = f.clone(); ctx.tree.borrow_mut().node_mut(node).hits.push((down_rect, f)); }
            None => { ctx.tree.borrow_mut().node_mut(node).hits.push((down_rect, Arc::new(|| {}))); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn hour_12_conversion_round_trips() {
        assert_eq!(SimpleTime::new(0, 0).hour_12(), (12, false));  // midnight
        assert_eq!(SimpleTime::new(12, 0).hour_12(), (12, true));  // noon
        assert_eq!(SimpleTime::new(13, 30).hour_12(), (1, true));
        assert_eq!(SimpleTime::new(23, 0).hour_12(), (11, true));
        assert_eq!(SimpleTime::new(1, 0).hour_12(), (1, false));
    }

    #[test]
    fn with_hour_12_reconstructs_24_hour() {
        let base = SimpleTime::new(0, 45);
        assert_eq!(base.with_hour_12(12, false).hour, 0);
        assert_eq!(base.with_hour_12(12, true).hour, 12);
        assert_eq!(base.with_hour_12(1, true).hour, 13);
        assert_eq!(base.with_hour_12(11, true).hour, 23);
    }

    #[test]
    fn layout_is_fixed_three_column_size() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = TimePicker::new(SimpleTime::new(9, 30)).layout(&ctx);
        assert_eq!(size.width, COLUMN_W * 3.0 + GAP * 2.0);
        assert_eq!(size.height, ARROW_H * 2.0 + VALUE_H);
    }
}
