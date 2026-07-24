//! `DatePicker` (D115/Phase 32 Step 1) — a month calendar grid with
//! year/month navigation. Pure-Rust date math (no `chrono` dependency,
//! matching the workspace's no-new-deps bias) — just enough calendar
//! arithmetic (leap years, days-in-month, day-of-week via Zeller's
//! congruence) to lay out a correct grid; not a general date library.

use std::sync::Arc;
use rosace_core::types::{Point, Rect, Size};
use rosace_render::Color;
use super::{LayoutCtx, PaintCtx, Widget, vcenter_text_y};

/// A plain calendar date — year/month/day, no time-of-day or timezone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SimpleDate {
    pub year: i32,
    /// 1-12.
    pub month: u8,
    /// 1-31.
    pub day: u8,
}

impl SimpleDate {
    pub fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month: month.clamp(1, 12), day: day.clamp(1, 31) }
    }

    pub fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }

    pub fn days_in_month(year: i32, month: u8) -> u8 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => if Self::is_leap_year(year) { 29 } else { 28 },
            _ => 30,
        }
    }

    /// 0 = Sunday .. 6 = Saturday, via Zeller's congruence (Gregorian).
    pub fn day_of_week(year: i32, month: u8, day: u8) -> u8 {
        let (y, m) = if month < 3 { (year - 1, month as i32 + 12) } else { (year, month as i32) };
        let k = y % 100;
        let j = y / 100;
        let h = (day as i32 + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
        // Zeller's h: 0 = Saturday .. rotate so 0 = Sunday.
        ((h + 6) % 7).rem_euclid(7) as u8
    }

    pub fn prev_month(self) -> Self {
        if self.month == 1 { Self::new(self.year - 1, 12, self.day) } else { Self::new(self.year, self.month - 1, self.day) }
    }

    pub fn next_month(self) -> Self {
        if self.month == 12 { Self::new(self.year + 1, 1, self.day) } else { Self::new(self.year, self.month + 1, self.day) }
    }

    pub fn prev_year(self) -> Self { Self::new(self.year - 1, self.month, self.day) }
    pub fn next_year(self) -> Self { Self::new(self.year + 1, self.month, self.day) }

    fn month_name(month: u8) -> &'static str {
        const NAMES: [&str; 12] = ["January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December"];
        NAMES[(month.clamp(1, 12) - 1) as usize]
    }
}

const WEEKDAY_LABELS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];

/// A month calendar grid: header with prev/next month nav, weekday labels,
/// a 7-column day grid. Controlled — the app owns `viewed_month`/`selected`.
pub struct DatePicker {
    /// The month currently displayed (day component is ignored for display).
    viewed_month: SimpleDate,
    selected: Option<SimpleDate>,
    today: Option<SimpleDate>,
    min: Option<SimpleDate>,
    max: Option<SimpleDate>,
    accent: Option<Color>,
    on_change: Option<Arc<dyn Fn(SimpleDate) + Send + Sync>>,
    on_month_change: Option<Arc<dyn Fn(SimpleDate) + Send + Sync>>,
}

const HEADER_H: f32 = 36.0;
const WEEKDAY_ROW_H: f32 = 24.0;
const CELL_H: f32 = 36.0;
const GRID_ROWS: usize = 6;

impl DatePicker {
    pub fn new(viewed_month: SimpleDate) -> Self {
        Self {
            viewed_month,
            selected: None,
            today: None,
            min: None,
            max: None,
            accent: None,
            on_change: None,
            on_month_change: None,
        }
    }

    pub fn selected(mut self, d: SimpleDate) -> Self { self.selected = Some(d); self }
    pub fn today(mut self, d: SimpleDate) -> Self { self.today = Some(d); self }
    pub fn min_date(mut self, d: SimpleDate) -> Self { self.min = Some(d); self }
    pub fn max_date(mut self, d: SimpleDate) -> Self { self.max = Some(d); self }
    pub fn accent(mut self, c: Color) -> Self { self.accent = Some(c); self }

    /// Called with the clicked date when a non-disabled day is pressed.
    pub fn on_change(mut self, f: impl Fn(SimpleDate) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Arc::new(f));
        self
    }

    /// Called with the new viewed month when the prev/next nav is pressed.
    pub fn on_month_change(mut self, f: impl Fn(SimpleDate) + Send + Sync + 'static) -> Self {
        self.on_month_change = Some(Arc::new(f));
        self
    }

    fn is_disabled(&self, d: SimpleDate) -> bool {
        self.min.is_some_and(|m| d < m) || self.max.is_some_and(|m| d > m)
    }
}

impl Widget for DatePicker {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let width = super::avail_w(ctx.constraints).clamp(7.0 * CELL_H, 320.0);
        let height = HEADER_H + WEEKDAY_ROW_H + GRID_ROWS as f32 * CELL_H;
        Size { width, height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let (bg, on_bg, muted, accent, disabled_fg) = {
            let t = &ctx.theme.colors;
            (ctx.tc(t.surface), ctx.tc(t.on_surface), ctx.tc(t.outline),
             self.accent.unwrap_or_else(|| ctx.tc(t.primary)),
             ctx.tc(t.outline))
        };
        let r = ctx.rect;
        let cell_w = r.size.width / 7.0;

        // Header: "«  ‹  Month Year  ›  »" — «/» jump a YEAR, ‹/› a month.
        // (Original report: only month nav existed, no way to change year.)
        let header_rect = Rect { origin: r.origin, size: Size { width: r.size.width, height: HEADER_H } };
        let label = format!("{} {}", SimpleDate::month_name(self.viewed_month.month), self.viewed_month.year);
        let text_w = ctx.font.measure_text(&label, 15.0);
        ctx.draw_text_at(&label, Point {
            x: header_rect.origin.x + (header_rect.size.width - text_w) / 2.0,
            y: vcenter_text_y(header_rect.origin.y, HEADER_H, ctx.font, 15.0),
        }, on_bg, 15.0);

        let nav_w = 28.0;
        let nav_y_text = vcenter_text_y(header_rect.origin.y, HEADER_H, ctx.font, 15.0);
        let prev_year_rect = Rect { origin: header_rect.origin, size: Size { width: nav_w, height: HEADER_H } };
        let prev_month_rect = Rect {
            origin: Point { x: header_rect.origin.x + nav_w, y: header_rect.origin.y },
            size: Size { width: nav_w, height: HEADER_H },
        };
        let next_month_rect = Rect {
            origin: Point { x: header_rect.origin.x + header_rect.size.width - nav_w * 2.0, y: header_rect.origin.y },
            size: Size { width: nav_w, height: HEADER_H },
        };
        let next_year_rect = Rect {
            origin: Point { x: header_rect.origin.x + header_rect.size.width - nav_w, y: header_rect.origin.y },
            size: Size { width: nav_w, height: HEADER_H },
        };
        for (glyph, rect) in [("<<", prev_year_rect), ("<", prev_month_rect), (">", next_month_rect), (">>", next_year_rect)] {
            let w = ctx.font.measure_text(glyph, 15.0);
            ctx.draw_text_at(glyph, Point { x: rect.origin.x + (nav_w - w) / 2.0, y: nav_y_text }, on_bg, 15.0);
        }

        let node = ctx.node;
        if let Some(f) = &self.on_month_change {
            let month = self.viewed_month;
            for (rect, next) in [
                (prev_year_rect, month.prev_year()),
                (prev_month_rect, month.prev_month()),
                (next_month_rect, month.next_month()),
                (next_year_rect, month.next_year()),
            ] {
                let f = f.clone();
                ctx.tree.borrow_mut().node_mut(node).hits.push((rect, Arc::new(move || f(next))));
            }
        } else {
            // Interactive-by-identity: absorb even when unwired.
            for rect in [prev_year_rect, prev_month_rect, next_month_rect, next_year_rect] {
                ctx.tree.borrow_mut().node_mut(node).hits.push((rect, Arc::new(|| {})));
            }
        }

        // Weekday labels.
        let weekday_y = r.origin.y + HEADER_H;
        for (i, label) in WEEKDAY_LABELS.iter().enumerate() {
            let w = ctx.font.measure_text(label, 12.0);
            ctx.draw_text_at(label, Point {
                x: r.origin.x + i as f32 * cell_w + (cell_w - w) / 2.0,
                y: vcenter_text_y(weekday_y, WEEKDAY_ROW_H, ctx.font, 12.0),
            }, muted, 12.0);
        }

        // Day grid.
        let first_weekday = SimpleDate::day_of_week(self.viewed_month.year, self.viewed_month.month, 1);
        let days = SimpleDate::days_in_month(self.viewed_month.year, self.viewed_month.month);
        let grid_top = weekday_y + WEEKDAY_ROW_H;

        for day in 1..=days {
            let slot = first_weekday as usize + (day - 1) as usize;
            let (col, row) = (slot % 7, slot / 7);
            let cell_rect = Rect {
                origin: Point { x: r.origin.x + col as f32 * cell_w, y: grid_top + row as f32 * CELL_H },
                size: Size { width: cell_w, height: CELL_H },
            };
            let date = SimpleDate::new(self.viewed_month.year, self.viewed_month.month, day);
            let is_selected = self.selected == Some(date);
            let is_today = self.today == Some(date);
            let disabled = self.is_disabled(date);

            if is_selected {
                let dot_r = (cell_w.min(CELL_H) * 0.36).min(16.0);
                ctx.fill_circle(Point { x: cell_rect.origin.x + cell_w / 2.0, y: cell_rect.origin.y + CELL_H / 2.0 }, dot_r, accent);
            } else if is_today {
                let dot_r = (cell_w.min(CELL_H) * 0.36).min(16.0);
                ctx.stroke_rrect(Rect {
                    origin: Point { x: cell_rect.origin.x + cell_w / 2.0 - dot_r, y: cell_rect.origin.y + CELL_H / 2.0 - dot_r },
                    size: Size { width: dot_r * 2.0, height: dot_r * 2.0 },
                }, dot_r, accent, 1.5);
            }

            let day_str = day.to_string();
            let dw = ctx.font.measure_text(&day_str, 13.0);
            let fg = if disabled { disabled_fg } else if is_selected { bg } else { on_bg };
            ctx.draw_text_at(&day_str, Point {
                x: cell_rect.origin.x + (cell_w - dw) / 2.0,
                y: vcenter_text_y(cell_rect.origin.y, CELL_H, ctx.font, 13.0),
            }, fg, 13.0);

            if !disabled {
                if let Some(f) = &self.on_change {
                    let f = f.clone();
                    ctx.tree.borrow_mut().node_mut(node).hits.push((cell_rect, Arc::new(move || f(date))));
                } else {
                    ctx.tree.borrow_mut().node_mut(node).hits.push((cell_rect, Arc::new(|| {})));
                }
            }
        }

        ctx.semantics(super::Semantics::new(rosace_core::Role::Unknown)
            .label(format!("Date picker, {}", label)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    fn leap_year_math_is_correct() {
        assert!(SimpleDate::is_leap_year(2024));
        assert!(!SimpleDate::is_leap_year(2023));
        assert!(!SimpleDate::is_leap_year(1900), "divisible by 100 but not 400");
        assert!(SimpleDate::is_leap_year(2000), "divisible by 400");
    }

    #[test]
    fn days_in_month_matches_calendar() {
        assert_eq!(SimpleDate::days_in_month(2024, 2), 29);
        assert_eq!(SimpleDate::days_in_month(2023, 2), 28);
        assert_eq!(SimpleDate::days_in_month(2024, 4), 30);
        assert_eq!(SimpleDate::days_in_month(2024, 1), 31);
    }

    #[test]
    fn day_of_week_matches_known_dates() {
        // 2024-01-01 was a Monday.
        assert_eq!(SimpleDate::day_of_week(2024, 1, 1), 1);
        // 2000-01-01 was a Saturday.
        assert_eq!(SimpleDate::day_of_week(2000, 1, 1), 6);
        // 2024-07-17 (today, this session) was a Wednesday.
        assert_eq!(SimpleDate::day_of_week(2024, 7, 17), 3);
    }

    #[test]
    fn month_navigation_wraps_year() {
        let d = SimpleDate::new(2024, 1, 15);
        assert_eq!(d.prev_month(), SimpleDate::new(2023, 12, 15));
        let d = SimpleDate::new(2024, 12, 15);
        assert_eq!(d.next_month(), SimpleDate::new(2025, 1, 15));
    }

    #[test]
    fn layout_reports_expected_height() {
        let font = rosace_render::FontCache::embedded();
        let theme = rosace_theme::built_in::dark_theme();
        let ctx = LayoutCtx::new(Constraints::loose(400.0, 400.0), &font, &theme);
        let size = DatePicker::new(SimpleDate::new(2024, 7, 1)).layout(&ctx);
        assert_eq!(size.height, HEADER_H + WEEKDAY_ROW_H + GRID_ROWS as f32 * CELL_H);
    }

    #[test]
    fn min_max_range_disables_out_of_range_dates() {
        let dp = DatePicker::new(SimpleDate::new(2024, 7, 1))
            .min_date(SimpleDate::new(2024, 7, 10))
            .max_date(SimpleDate::new(2024, 7, 20));
        assert!(dp.is_disabled(SimpleDate::new(2024, 7, 5)));
        assert!(dp.is_disabled(SimpleDate::new(2024, 7, 25)));
        assert!(!dp.is_disabled(SimpleDate::new(2024, 7, 15)));
    }
}
