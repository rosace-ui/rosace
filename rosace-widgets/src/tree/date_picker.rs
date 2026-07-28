//! `DatePicker` (D115/Phase 32 Step 1) — a month calendar grid with
//! year/month navigation. Pure-Rust date math (no `chrono` dependency,
//! matching the workspace's no-new-deps bias) — just enough calendar
//! arithmetic (leap years, days-in-month, day-of-week via Zeller's
//! congruence) to lay out a correct grid; not a general date library.

use std::sync::{Arc, Mutex};
use rosace_core::types::{Point, Rect, Size};
use rosace_render::{Color, DrawCommand};
use super::{LayoutCtx, PaintCtx, Widget, vcenter_text_y, intersect_rect};

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

    /// Absolute month index (year*12 + month-1) — a monotone integer used to
    /// animate month-to-month slides and to compare/step months cheaply.
    pub fn month_ordinal(self) -> i32 { self.year * 12 + (self.month as i32 - 1) }

    /// Inverse of [`Self::month_ordinal`] — day defaults to 1.
    pub fn from_month_ordinal(ord: i32) -> Self {
        Self::new(ord.div_euclid(12), (ord.rem_euclid(12) + 1) as u8, 1)
    }

    fn month_name(month: u8) -> &'static str {
        const NAMES: [&str; 12] = ["January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December"];
        NAMES[(month.clamp(1, 12) - 1) as usize]
    }
}

const WEEKDAY_LABELS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];

/// How the calendar selects — a single day or a start→end range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SelectionMode { #[default] Single, Range }

/// Which way month-to-month transitions slide: `Horizontal` (Material,
/// default) slides left/right; `Vertical` slides up/down (iOS-style).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PageAxis { #[default] Horizontal, Vertical }

/// The `SimpleDate` occupying grid `slot` (0..41) of `view`'s 6×7 month page,
/// plus whether it belongs to `view` (vs a leading/trailing neighbour month).
fn slot_date(view: SimpleDate, slot: usize) -> (SimpleDate, bool) {
    let first = SimpleDate::day_of_week(view.year, view.month, 1) as i32;
    let di = slot as i32 - first + 1; // 1-based day within `view`; may spill either side
    let days = SimpleDate::days_in_month(view.year, view.month) as i32;
    if di < 1 {
        let pm = view.prev_month();
        let pd = SimpleDate::days_in_month(pm.year, pm.month) as i32;
        (SimpleDate::new(pm.year, pm.month, (pd + di) as u8), false)
    } else if di > days {
        let nm = view.next_month();
        (SimpleDate::new(nm.year, nm.month, (di - days) as u8), false)
    } else {
        (SimpleDate::new(view.year, view.month, di as u8), true)
    }
}

/// How many week rows `view`'s month actually occupies (4–6). Rows beyond
/// this are entirely next-month and are neither drawn nor selectable.
fn rows_in_month(view: SimpleDate) -> usize {
    let first = SimpleDate::day_of_week(view.year, view.month, 1) as usize;
    let days = SimpleDate::days_in_month(view.year, view.month) as usize;
    (first + days - 1) / 7 + 1
}

/// Map a content-space point to the day under it within `view`'s grid.
/// Returns the date, whether it is in `view`, and edge flags (pointer above
/// the month / below it) that drive cross-month navigation. Rows past the
/// month's real extent count as `below` — the empty tail is not selectable.
fn day_at(px: f32, py: f32, body: Rect, view: SimpleDate) -> (SimpleDate, bool, bool, bool) {
    let rows = rows_in_month(view) as i32;
    let cw = body.size.width / 7.0;
    let col = ((px - body.origin.x) / cw).floor() as i32;
    let row = ((py - body.origin.y) / CELL_H).floor() as i32;
    let above = row < 0;
    let below = row >= rows;
    let c = col.clamp(0, 6);
    let r = row.clamp(0, rows - 1);
    let (d, in_cur) = slot_date(view, (r * 7 + c) as usize);
    (d, in_cur, above, below)
}

/// Pure range-transition rule (shared by tap dispatch and tests): no range or
/// a complete one → start fresh; an open start → complete it (ordered).
fn next_range_for(
    cur: Option<(SimpleDate, Option<SimpleDate>)>,
    d: SimpleDate,
) -> (SimpleDate, Option<SimpleDate>) {
    match cur {
        None | Some((_, Some(_))) => (d, None),
        Some((s, None)) => if d >= s { (s, Some(d)) } else { (d, Some(s)) },
    }
}

/// Per-drag scratch state, owned by the active `on_press_at` closure for the
/// life of one press→release. `view` tracks the month the drag is currently
/// over so a range can extend across month pages (auto-paged at the edges).
struct DragState {
    view: SimpleDate,
    anchor: Option<SimpleDate>,
    moved: bool,
    /// Moves spent inside the top/bottom edge zone since the last auto-page —
    /// throttles continuous cross-month paging while dragging past the edge.
    edge_ticks: u32,
}

/// Auto-page every N drag-moves spent in the edge zone (continuous paging).
const EDGE_PAGE_EVERY: u32 = 5;

/// A month calendar grid: header with prev/next month nav, weekday labels,
/// a 7-column day grid. Controlled — the app owns `viewed_month`/`selected`.
pub struct DatePicker {
    /// The month currently displayed (day component is ignored for display).
    viewed_month: SimpleDate,
    selected: Option<SimpleDate>,
    today: Option<SimpleDate>,
    min: Option<SimpleDate>,
    max: Option<SimpleDate>,
    mode: SelectionMode,
    /// (start, optional end) for `Range` mode.
    range: Option<(SimpleDate, Option<SimpleDate>)>,
    accent: Option<Color>,
    range_color: Option<Color>,
    axis: PageAxis,
    /// The single selection callback. Fires with `(start, end)`: `Single` mode
    /// always passes `end = None`; `Range` passes `(start, None)` when the start
    /// is picked and `(start, Some(end))` once the range completes.
    on_select: Option<Arc<dyn Fn(SimpleDate, Option<SimpleDate>) + Send + Sync>>,
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
            mode: SelectionMode::Single,
            range: None,
            accent: None,
            range_color: None,
            axis: PageAxis::Horizontal,
            on_select: None,
            on_month_change: None,
        }
    }

    pub fn selected(mut self, d: SimpleDate) -> Self { self.selected = Some(d); self }
    pub fn today(mut self, d: SimpleDate) -> Self { self.today = Some(d); self }
    pub fn min_date(mut self, d: SimpleDate) -> Self { self.min = Some(d); self }
    pub fn max_date(mut self, d: SimpleDate) -> Self { self.max = Some(d); self }
    pub fn accent(mut self, c: Color) -> Self { self.accent = Some(c); self }
    /// Selection mode — `Single` (default) or `Range`.
    pub fn mode(mut self, m: SelectionMode) -> Self { self.mode = m; self }
    /// The current (start, end) selection for `Range` mode.
    pub fn range(mut self, start: SimpleDate, end: Option<SimpleDate>) -> Self {
        self.mode = SelectionMode::Range; self.range = Some((start, end)); self
    }
    /// The in-between band fill color (default: a faint accent).
    pub fn range_color(mut self, c: Color) -> Self { self.range_color = Some(c); self }
    /// Direction month transitions slide — `Horizontal` (default) or `Vertical`.
    pub fn axis(mut self, a: PageAxis) -> Self { self.axis = a; self }

    /// The one selection callback, fired right after a day is chosen. It
    /// receives `(start, end)`:
    /// - `Single` mode → `(date, None)`.
    /// - `Range` mode → `(start, None)` when the start is picked, then
    ///   `(start, Some(end))` once the range completes (drag reports the live
    ///   `(start, Some(end))` as you sweep).
    pub fn on_select(mut self, f: impl Fn(SimpleDate, Option<SimpleDate>) + Send + Sync + 'static) -> Self {
        self.on_select = Some(Arc::new(f));
        self
    }

    /// Compute the next range given the current one and a tapped date.
    #[cfg_attr(not(test), allow(dead_code))]
    fn next_range(&self, d: SimpleDate) -> (SimpleDate, Option<SimpleDate>) {
        next_range_for(self.range, d)
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

/// Resolved theme colours for a paint pass (borrow of `ctx.theme` must end
/// before mutable painting — so we snapshot up front).
struct Pal {
    bg: Color,
    on_bg: Color,
    muted: Color,
    accent: Color,
    disabled_fg: Color,
    band: Color,
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

impl DatePicker {
    /// Paint one month's 6×7 grid into `area`, clipped to `clip`. Leading and
    /// trailing days from neighbour months render faded; the range shows as a
    /// solid per-row band (rounded at the true endpoints) that wraps line to
    /// line, with accent endpoint discs and a today ring on top.
    fn paint_month(&self, ctx: &mut PaintCtx, area: Rect, month: SimpleDate, clip: Rect, pal: &Pal) {
        let mut mc = ctx.child(area);
        mc.clip_rect = Some(clip);
        let cw = area.size.width / 7.0;
        let dot_r = (cw.min(CELL_H) * 0.36).min(16.0);
        let (r_start, r_end) = match self.range { Some((s, e)) => (Some(s), e), None => (None, None) };
        let is_range = self.mode == SelectionMode::Range;

        // ── Gooey range band: one full-cell-height rect per row spanning the
        //    selected columns. Full height means consecutive rows TOUCH, so a
        //    multi-week range reads as one connected shape; the true start/end
        //    get a round cap (a disc behind the accent dot), everything else
        //    is square so week-to-week wraps join seamlessly. ──
        let rows = rows_in_month(month); // 4–6 real weeks; skip all-next-month rows
        let band_h = CELL_H; // full row height → consecutive rows touch (connected)
        if let (true, Some(s), Some(e)) = (is_range, r_start, r_end) {
            let cap_r = band_h / 2.0;
            for row in 0..rows {
                let mut lo: Option<usize> = None;
                let mut hi: Option<usize> = None;
                for col in 0..7 {
                    let (d, _) = slot_date(month, row * 7 + col);
                    if d >= s && d <= e { lo = lo.or(Some(col)); hi = Some(col); }
                }
                if let (Some(lo), Some(hi)) = (lo, hi) {
                    let (lo_d, _) = slot_date(month, row * 7 + lo);
                    let (hi_d, _) = slot_date(month, row * 7 + hi);
                    let lo_is_start = lo_d == s;
                    let hi_is_end = hi_d == e;
                    let x0 = area.origin.x + lo as f32 * cw + if lo_is_start { cw / 2.0 } else { 0.0 };
                    let x1 = area.origin.x + hi as f32 * cw + if hi_is_end { cw / 2.0 } else { cw };
                    let y = area.origin.y + row as f32 * CELL_H + (CELL_H - band_h) / 2.0;
                    mc.fill_rect(Rect { origin: Point { x: x0, y }, size: Size { width: (x1 - x0).max(0.0), height: band_h } }, pal.band);
                    // Rounded caps at the genuine endpoints.
                    let cy = y + band_h / 2.0;
                    if lo_is_start { mc.fill_circle(Point { x: area.origin.x + lo as f32 * cw + cw / 2.0, y: cy }, cap_r, pal.band); }
                    if hi_is_end { mc.fill_circle(Point { x: area.origin.x + hi as f32 * cw + cw / 2.0, y: cy }, cap_r, pal.band); }
                }
            }
        }

        // ── Day cells. ──
        for slot in 0..rows * 7 {
            let (col, row) = (slot % 7, slot / 7);
            let (date, in_cur) = slot_date(month, slot);
            let x = area.origin.x + col as f32 * cw;
            let y = area.origin.y + row as f32 * CELL_H;
            let center = Point { x: x + cw / 2.0, y: y + CELL_H / 2.0 };
            let disabled = self.is_disabled(date);
            let is_endpoint = is_range && (r_start == Some(date) || r_end == Some(date));
            let selected_single = !is_range && in_cur && self.selected == Some(date);
            let show_circle = is_endpoint || selected_single;

            if show_circle {
                mc.fill_circle(center, dot_r, pal.accent);
            } else if in_cur && self.today == Some(date) {
                mc.stroke_rrect(Rect {
                    origin: Point { x: center.x - dot_r, y: center.y - dot_r },
                    size: Size { width: dot_r * 2.0, height: dot_r * 2.0 },
                }, dot_r, pal.accent, 1.5);
            }

            let day_str = date.day.to_string();
            let dw = mc.font.measure_text(&day_str, 13.0);
            let fg = if show_circle { pal.bg }
                     else if !in_cur || disabled { pal.disabled_fg }
                     else { pal.on_bg };
            mc.draw_text_at(&day_str, Point { x: x + (cw - dw) / 2.0, y: vcenter_text_y(y, CELL_H, mc.font, 13.0) }, fg, 13.0);
        }
    }

    /// Paint the year-picker grid (4 columns × 3 rows) for the window starting
    /// at `base`; selecting a year jumps the view and returns to Days mode.
    fn paint_years(&self, ctx: &mut PaintCtx, body: Rect, base: i32, pal: &Pal, ctrl: &rosace_scroll::ScrollController) {
        const COLS: usize = 4;
        const ROWS: usize = 3;
        let cw = body.size.width / COLS as f32;
        let ch = body.size.height / ROWS as f32;
        for i in 0..COLS * ROWS {
            let year = base + i as i32;
            let (col, row) = (i % COLS, i / COLS);
            let cell = Rect {
                origin: Point { x: body.origin.x + col as f32 * cw, y: body.origin.y + row as f32 * ch },
                size: Size { width: cw, height: ch },
            };
            let mut yc = ctx.child(cell);
            yc.hoverable();
            let (hov, prs) = (yc.hovered(), yc.pressed());
            let selected = year == self.viewed_month.year;
            let center = Point { x: cell.origin.x + cw / 2.0, y: cell.origin.y + ch / 2.0 };
            let pill = Rect { origin: Point { x: center.x - cw * 0.38, y: center.y - 16.0 }, size: Size { width: cw * 0.76, height: 32.0 } };
            if selected {
                yc.fill_rrect(pill, 16.0, pal.accent);
            } else if hov || prs {
                yc.fill_rrect(pill, 16.0, with_alpha(pal.on_bg, if prs { 0.14 } else { 0.08 }));
            }
            let label = year.to_string();
            let lw = yc.font.measure_text(&label, 15.0);
            let fg = if selected { pal.bg } else { pal.on_bg };
            yc.draw_text_at(&label, Point { x: center.x - lw / 2.0, y: vcenter_text_y(cell.origin.y, ch, yc.font, 15.0) }, fg, 15.0);

            let ctrl = ctrl.clone();
            let month = self.viewed_month.month;
            let day = self.viewed_month.day;
            match &self.on_month_change {
                Some(f) => {
                    let f = f.clone();
                    yc.register_hit(Arc::new(move || {
                        f(SimpleDate::new(year, month, day));
                        let o = ctrl.offset.get();
                        ctrl.offset.set([0.0, o[1]]); // back to Days mode
                    }));
                }
                None => yc.register_hit(Arc::new(|| {})),
            }
        }
    }
}

impl Widget for DatePicker {
    fn layout(&self, ctx: &LayoutCtx) -> Size {
        let width = super::avail_w(ctx.constraints).clamp(7.0 * CELL_H, 320.0);
        let height = HEADER_H + WEEKDAY_ROW_H + GRID_ROWS as f32 * CELL_H;
        Size { width, height }
    }

    fn paint(&self, ctx: &mut PaintCtx) {
        let pal = {
            let t = &ctx.theme.colors;
            let accent = self.accent.unwrap_or_else(|| ctx.tc(t.primary));
            Pal {
                bg: ctx.tc(t.surface),
                on_bg: ctx.tc(t.on_surface),
                muted: ctx.tc(t.outline),
                accent,
                disabled_fg: with_alpha(ctx.tc(t.on_surface), 0.35),
                band: self.range_color.unwrap_or_else(|| with_alpha(accent, 0.32)),
            }
        };
        let r = ctx.rect;
        let cell_w = r.size.width / 7.0;

        // View mode + year-window persist in this node's scroll controller
        // (offset[0] = 0 Days / 1 Years, offset[1] = year-grid base) — the
        // Carousel-style "spare slot" pattern, no app-owned atom required.
        let ctrl = ctx.scroll_controller();
        let years_mode = ctrl.offset.get()[0] > 0.5;
        let year_base = {
            let stored = ctrl.offset.get()[1] as i32;
            if stored == 0 { self.viewed_month.year - self.viewed_month.year.rem_euclid(12) } else { stored }
        };

        // ── Header: ‹  Month Year (tap → years)  › ──────────────────────────
        let header_rect = Rect { origin: r.origin, size: Size { width: r.size.width, height: HEADER_H } };
        let nav_w = HEADER_H;
        let month = self.viewed_month;
        let label = if years_mode {
            format!("{} \u{2013} {}", year_base, year_base + 11)
        } else {
            format!("{} {}", SimpleDate::month_name(month.month), month.year)
        };
        // Tappable label (centre) toggles Days ⇄ Years.
        let label_rect = Rect {
            origin: Point { x: header_rect.origin.x + nav_w, y: header_rect.origin.y },
            size: Size { width: (r.size.width - 2.0 * nav_w).max(0.0), height: HEADER_H },
        };
        {
            let mut hdr = ctx.child(label_rect);
            hdr.hoverable();
            let text_w = hdr.font.measure_text(&label, 15.0);
            hdr.draw_text_at(&label, Point {
                x: label_rect.origin.x + (label_rect.size.width - text_w) / 2.0,
                y: vcenter_text_y(label_rect.origin.y, HEADER_H, hdr.font, 15.0),
            }, pal.on_bg, 15.0);
            let ctrl_t = ctrl.clone();
            hdr.register_hit(Arc::new(move || {
                let o = ctrl_t.offset.get();
                ctrl_t.offset.set([if o[0] > 0.5 { 0.0 } else { 1.0 }, o[1]]);
            }));
        }

        // Chevrons — page the month (Days) or the year window (Years).
        let prev_rect = Rect { origin: header_rect.origin, size: Size { width: nav_w, height: HEADER_H } };
        let next_rect = Rect {
            origin: Point { x: header_rect.origin.x + r.size.width - nav_w, y: header_rect.origin.y },
            size: Size { width: nav_w, height: HEADER_H },
        };
        for (rect, kind, back) in [
            (prev_rect, super::IconKind::ChevronLeft, true),
            (next_rect, super::IconKind::ChevronRight, false),
        ] {
            let mut btn = ctx.child(rect);
            btn.hoverable();
            let (hov, prs) = (btn.hovered(), btn.pressed());
            let c = Point { x: rect.origin.x + nav_w / 2.0, y: rect.origin.y + HEADER_H / 2.0 };
            if hov || prs {
                btn.fill_circle(c, 15.0, with_alpha(pal.on_bg, if prs { 0.14 } else { 0.08 }));
            }
            let isz = 22.0;
            let ir = Rect { origin: Point { x: c.x - isz / 2.0, y: c.y - isz / 2.0 }, size: Size { width: isz, height: isz } };
            super::Icon::new(kind).size(isz).color(pal.on_bg).paint(&mut btn.child(ir));
            if years_mode {
                let ctrl_y = ctrl.clone();
                let target_base = year_base + if back { -12 } else { 12 };
                btn.register_hit(Arc::new(move || {
                    let o = ctrl_y.offset.get();
                    ctrl_y.offset.set([o[0], target_base as f32]);
                }));
            } else {
                match &self.on_month_change {
                    Some(f) => { let f = f.clone(); let next = if back { month.prev_month() } else { month.next_month() }; btn.register_hit(Arc::new(move || f(next))); }
                    None => btn.register_hit(Arc::new(|| {})),
                }
            }
        }

        // Year picker fills the body and returns early.
        if years_mode {
            let body = Rect {
                origin: Point { x: r.origin.x, y: r.origin.y + HEADER_H },
                size: Size { width: r.size.width, height: r.size.height - HEADER_H },
            };
            // Persist the base so chevron paging is stable across frames.
            if ctrl.offset.get()[1] as i32 == 0 { ctrl.offset.set([1.0, year_base as f32]); }
            self.paint_years(ctx, body, year_base, &pal, &ctrl);
            ctx.semantics(super::Semantics::new(rosace_core::Role::Unknown).label(format!("Year picker, {label}")));
            return;
        }

        // ── Weekday labels. ──
        let weekday_y = r.origin.y + HEADER_H;
        for (i, wl) in WEEKDAY_LABELS.iter().enumerate() {
            let w = ctx.font.measure_text(wl, 12.0);
            ctx.draw_text_at(wl, Point {
                x: r.origin.x + i as f32 * cell_w + (cell_w - w) / 2.0,
                y: vcenter_text_y(weekday_y, WEEKDAY_ROW_H, ctx.font, 12.0),
            }, pal.muted, 12.0);
        }

        // ── Animated month slide: draw the month(s) that overlap the eased
        //    ordinal, offset horizontally, clipped to the body. ──
        let grid_top = weekday_y + WEEKDAY_ROW_H;
        let body = Rect {
            origin: Point { x: r.origin.x, y: grid_top },
            size: Size { width: r.size.width, height: GRID_ROWS as f32 * CELL_H },
        };
        let target_ord = month.month_ordinal() as f32;
        let eased = ctx.animate_to(target_ord, 0.0);
        let vertical = self.axis == PageAxis::Vertical;
        // Actively sliding only when the eased position hasn't reached the
        // target (large year jumps snap: >1.5 months). At rest exactly one
        // month is in view and it already fits, so we skip the clip entirely —
        // a stray PushClip inside a GPU-composited scroll layer is applied in
        // the wrong coordinate space and would crop the calendar. The clip is
        // only needed to hide the incoming/outgoing month during a transition.
        let sliding = (eased - target_ord).abs() > 0.001 && (eased - target_ord).abs() <= 1.5;
        let slide = if sliding { eased } else { target_ord };
        let lo = slide.floor();
        let clip = ctx.clip_rect.and_then(|p| intersect_rect(p, body)).unwrap_or(body);
        if sliding { ctx.record(DrawCommand::PushClip { rect: body }); }
        for ord in [lo as i32, lo as i32 + 1] {
            let off = (ord as f32 - slide) * if vertical { body.size.height } else { body.size.width };
            let (x, y) = if vertical { (body.origin.x, body.origin.y + off) } else { (body.origin.x + off, body.origin.y) };
            // Cull the page once it is fully off the body on the slide axis.
            let (lead, span, size) = if vertical { (y, body.origin.y, body.size.height) } else { (x, body.origin.x, body.size.width) };
            if lead + size <= span || lead >= span + size { continue; }
            let area = Rect { origin: Point { x, y }, size: body.size };
            self.paint_month(ctx, area, SimpleDate::from_month_ordinal(ord), clip, &pal);
        }
        if sliding { ctx.record(DrawCommand::PopClip); }

        // ── Gesture owner: one stable body-level positional handler. Its
        //    coordinates arrive already remapped into content space (unlike
        //    `current_pointer()`), so drag-select is correct inside scroll
        //    views; the closure owns the whole press→release for one gesture. ──
        {
            let g = ctx.child(body);
            let is_range = self.mode == SelectionMode::Range;
            let range0 = self.range;
            let (min, max) = (self.min, self.max);
            let is_disabled = move |d: SimpleDate| min.is_some_and(|m| d < m) || max.is_some_and(|m| d > m);
            let on_select = self.on_select.clone();
            let on_month = self.on_month_change.clone();
            let start_view = month;
            let st = Arc::new(Mutex::new(DragState { view: start_view, anchor: None, moved: false, edge_ticks: 0 }));
            g.on_press_at(move |px, py| {
                let mut s = st.lock().unwrap();
                let view = s.view;
                let (date, in_cur, above, below) = day_at(px, py, body, view);
                if s.anchor.is_none() {
                    // Press-down. Establish the anchor and apply tap semantics
                    // (a drag overrides this on the next move).
                    s.anchor = Some(date);
                    // A tap past the month's real rows just navigates there.
                    if above || below {
                        let nm = if above { view.prev_month() } else { view.next_month() };
                        s.view = nm;
                        if let Some(m) = &on_month { m(nm); }
                        return;
                    }
                    // A leading/trailing day inside the grid moves the view to
                    // its month, then selects it there (don't select a stray
                    // neighbour-month day while showing this month).
                    if !in_cur {
                        let nm = SimpleDate::new(date.year, date.month, 1);
                        s.view = nm;
                        if let Some(m) = &on_month { m(nm); }
                    }
                    if is_disabled(date) { return; }
                    if let Some(f) = &on_select {
                        if is_range {
                            let (ns, ne) = next_range_for(range0, date);
                            f(ns, ne);
                        } else {
                            f(date, None);
                        }
                    }
                    return;
                }
                let anchor = s.anchor.unwrap();
                if is_range && date != anchor {
                    if let Some(f) = &on_select {
                        if !is_disabled(date) {
                            s.moved = true;
                            let (a, b) = if date >= anchor { (anchor, date) } else { (date, anchor) };
                            f(a, Some(b));
                        }
                    }
                }
                // Cross-month: while dragging past the top/bottom edge, page
                // repeatedly (throttled) so a range can sweep across several
                // months — not just the immediate neighbour.
                let edge = above || below;
                if is_range && edge {
                    s.edge_ticks += 1;
                    if s.edge_ticks >= EDGE_PAGE_EVERY {
                        s.edge_ticks = 0;
                        let nm = if above { view.prev_month() } else { view.next_month() };
                        s.view = nm;
                        if let Some(m) = &on_month { m(nm); }
                    }
                } else {
                    s.edge_ticks = 0;
                }
            });
        }

        ctx.semantics(super::Semantics::new(rosace_core::Role::Unknown)
            .label(format!("Date picker, {label}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosace_layout::Constraints;

    #[test]
    #[ignore] // DATE_PNG=/path cargo test -p rosace-widgets date_range_showcase -- --ignored --nocapture
    fn date_range_showcase() {
        use super::super::app::WidgetApp;
        let out = std::env::var("DATE_PNG").unwrap_or_else(|_| "date.png".to_string());
        let mut theme = rosace_theme::built_in::dark_theme();
        theme.animation.enabled = false;
        let (w, h) = (320u32, 300u32);
        // A multi-row range → solid wrapping band + faded leading/trailing days.
        let range = DatePicker::new(SimpleDate::new(2026, 7, 1))
            .today(SimpleDate::new(2026, 7, 24))
            .range(SimpleDate::new(2026, 7, 8), Some(SimpleDate::new(2026, 7, 19)));
        std::fs::write(&out, WidgetApp::new(w, h).theme(theme.clone()).render_png(&range)).unwrap();
        println!("wrote {out}");
    }

    #[test]
    fn slot_date_fills_leading_and_trailing_from_neighbour_months() {
        // July 2026 starts on a Wednesday (day_of_week == 3), so slots 0..2
        // are the tail of June, slot 3 is Jul 1, and the grid overruns into
        // August after Jul 31.
        let view = SimpleDate::new(2026, 7, 1);
        assert_eq!(SimpleDate::day_of_week(2026, 7, 1), 3);
        assert_eq!(slot_date(view, 0), (SimpleDate::new(2026, 6, 28), false));
        assert_eq!(slot_date(view, 2), (SimpleDate::new(2026, 6, 30), false));
        assert_eq!(slot_date(view, 3), (SimpleDate::new(2026, 7, 1), true));
        assert_eq!(slot_date(view, 33), (SimpleDate::new(2026, 7, 31), true));
        assert_eq!(slot_date(view, 34), (SimpleDate::new(2026, 8, 1), false));
    }

    #[test]
    fn day_at_maps_point_to_cell_and_flags_edges() {
        let body = Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: 280.0, height: 216.0 } };
        let view = SimpleDate::new(2026, 7, 1);
        let cw = 280.0 / 7.0;
        // Row 0, col 3 (centre) → Jul 1.
        let (d, in_cur, above, below) = day_at(cw * 3.5, CELL_H * 0.5, body, view);
        assert_eq!((d, in_cur, above, below), (SimpleDate::new(2026, 7, 1), true, false, false));
        // Above the grid → prev-month edge flag.
        let (_, _, above, _) = day_at(cw * 3.5, -5.0, body, view);
        assert!(above);
        // Below the last row → next-month edge flag.
        let (_, _, _, below) = day_at(cw * 3.5, CELL_H * (GRID_ROWS as f32) + 5.0, body, view);
        assert!(below);
    }

    #[test]
    #[ignore] // STACK_PNG=/path cargo test -p rosace-widgets stacked_pickers -- --ignored --nocapture
    fn stacked_pickers() {
        use super::super::app::WidgetApp;
        use super::super::column::Column;
        let out = std::env::var("STACK_PNG").unwrap_or_else(|_| "stack.png".to_string());
        let mut theme = rosace_theme::built_in::dark_theme();
        theme.animation.enabled = false;
        let col = Column::new()
            .spacing(10.0)
            .child(DatePicker::new(SimpleDate::new(2026, 7, 1)).axis(PageAxis::Vertical)
                .mode(SelectionMode::Range).range(SimpleDate::new(2026, 7, 8), Some(SimpleDate::new(2026, 7, 16))))
            .child(DatePicker::new(SimpleDate::new(2026, 7, 1))
                .today(SimpleDate::new(2026, 7, 28))
                .min_date(SimpleDate::new(2026, 7, 6)).max_date(SimpleDate::new(2026, 7, 24)));
        std::fs::write(&out, WidgetApp::new(340, 620).theme(theme).render_png(&col)).unwrap();
        println!("wrote {out}");
    }

    #[test]
    fn rows_in_month_omits_all_next_month_weeks() {
        // July 2026 starts Wed and has 31 days → 5 rows (the 6th would be all
        // August, so it must not render).
        assert_eq!(rows_in_month(SimpleDate::new(2026, 7, 1)), 5);
        // Feb 2026 starts Sunday, 28 days → exactly 4 rows.
        assert_eq!(SimpleDate::day_of_week(2026, 2, 1), 0);
        assert_eq!(rows_in_month(SimpleDate::new(2026, 2, 1)), 4);
        // A month that genuinely needs 6 rows: Aug 2026 starts Saturday, 31 days.
        assert_eq!(SimpleDate::day_of_week(2026, 8, 1), 6);
        assert_eq!(rows_in_month(SimpleDate::new(2026, 8, 1)), 6);
    }

    #[test]
    fn month_ordinal_roundtrips() {
        let d = SimpleDate::new(2026, 7, 15);
        assert_eq!(SimpleDate::from_month_ordinal(d.month_ordinal()), SimpleDate::new(2026, 7, 1));
        assert_eq!(SimpleDate::new(2026, 12, 1).month_ordinal() + 1,
                   SimpleDate::new(2027, 1, 1).month_ordinal());
    }

    #[test]
    fn next_range_starts_completes_and_restarts() {
        let d = |day| SimpleDate::new(2026, 7, day);
        let base = DatePicker::new(d(1));
        assert_eq!(base.next_range(d(5)), (d(5), None), "empty → start");
        let started = DatePicker::new(d(1)).range(d(5), None);
        assert_eq!(started.next_range(d(9)), (d(5), Some(d(9))), "start+later → complete");
        assert_eq!(started.next_range(d(2)), (d(2), Some(d(5))), "start+earlier → ordered");
        let complete = DatePicker::new(d(1)).range(d(5), Some(d(9)));
        assert_eq!(complete.next_range(d(12)), (d(12), None), "complete → restart");
    }

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
