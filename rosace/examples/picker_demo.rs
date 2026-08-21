//! Gallery of the Material clock-dial `TimePicker` and the range/single
//! `DatePicker`, exercising the full public API. Run:
//! `cargo run -p rosace --example picker_demo`
//!
//! Sections:
//!  1. TimePicker — 12h / 24h dial (tap header to switch hour⇄minute, drag).
//!  2. DatePicker — Single select.
//!  3. DatePicker — Range (drag to sweep a gooey band; tap restarts).
//!  4. DatePicker — Range with custom accent + band colors.
//!  5. DatePicker — Vertical page axis (months slide up/down).
//!  6. DatePicker — Bounded by min/max, with a "today" ring.

use rosace::prelude::*;
use rosace::widgets::{
    DatePicker, PageAxis, SelectionMode, SimpleDate, SimpleTime, TimePicker, TimeUnit,
};

type Range = Option<(SimpleDate, Option<SimpleDate>)>;

struct Demo;

impl Component for Demo {
    fn build(&self, ctx: &mut Context) -> BoxedWidget {
        // ── State (the app owns everything; pickers are controlled) ─────────
        let time = ctx.state(SimpleTime::new(9, 30));
        let unit = ctx.state(TimeUnit::Hour);
        let use24 = ctx.state(false);

        // One (viewed_month, selection) pair per calendar section.
        let single_month = ctx.state(SimpleDate::new(2026, 7, 1));
        let single_sel = ctx.state(Some(SimpleDate::new(2026, 7, 12)));

        let range_month = ctx.state(SimpleDate::new(2026, 7, 1));
        let range_sel = ctx.state(None::<(SimpleDate, Option<SimpleDate>)>);

        let custom_month = ctx.state(SimpleDate::new(2026, 7, 1));
        let custom_sel = ctx.state(Some((SimpleDate::new(2026, 7, 8), Some(SimpleDate::new(2026, 7, 16)))) as Range);

        let vert_month = ctx.state(SimpleDate::new(2026, 7, 1));
        let vert_sel = ctx.state(None::<(SimpleDate, Option<SimpleDate>)>);

        let bounded_month = ctx.state(SimpleDate::new(2026, 7, 1));
        let bounded_sel = ctx.state(None::<SimpleDate>);

        // ── 1. TimePicker ───────────────────────────────────────────────────
        let clock = {
            let (tc, uc) = (time.clone(), unit.clone());
            let mut c = TimePicker::new(time.get())
                .editing(unit.get())
                .on_change(move |v| tc.set(v))
                .on_unit_change(move |u| uc.set(u));
            if use24.get() { c = c.use_24h(); }
            c
        };
        let h24_toggle = {
            let u = use24.clone();
            Switch::new(use24.get()).on_change(move |v| u.set(v))
        };
        let (h12, pm) = time.get().hour_12();
        let time_label = format!("{h12:02}:{:02} {}", time.get().minute, if pm { "PM" } else { "AM" });

        // ── 2. DatePicker · Single ──────────────────────────────────────────
        let single = {
            let (mc, sc) = (single_month.clone(), single_sel.clone());
            let mut dp = DatePicker::new(single_month.get()).today(SimpleDate::new(2026, 7, 28));
            if let Some(d) = single_sel.get() { dp = dp.selected(d); }
            dp.on_select(move |d, _| sc.set(Some(d)))
                .on_month_change(move |m| mc.set(m))
        };

        // ── 3. DatePicker · Range ───────────────────────────────────────────
        let range = range_picker(&range_month, &range_sel, None, None, PageAxis::Horizontal);

        // ── 4. DatePicker · Range with custom colors ────────────────────────
        let custom = range_picker(
            &custom_month,
            &custom_sel,
            Some(Color::rgb(0, 191, 165)),          // teal accent
            Some(Color::rgba(0, 191, 165, 60)),     // matching soft band
            PageAxis::Horizontal,
        );

        // ── 5. DatePicker · Range, vertical page axis ───────────────────────
        let vertical = range_picker(&vert_month, &vert_sel, None, None, PageAxis::Vertical);

        // ── 6. DatePicker · Bounded (min/max) ───────────────────────────────
        let bounded = {
            let (mc, sc) = (bounded_month.clone(), bounded_sel.clone());
            let mut dp = DatePicker::new(bounded_month.get())
                .today(SimpleDate::new(2026, 7, 28))
                .min_date(SimpleDate::new(2026, 7, 6))
                .max_date(SimpleDate::new(2026, 7, 24));
            if let Some(d) = bounded_sel.get() { dp = dp.selected(d); }
            dp.on_select(move |d, _| sc.set(Some(d)))
                .on_month_change(move |m| mc.set(m))
        };

        // ── Layout ──────────────────────────────────────────────────────────
        Scaffold::new(ScrollView::new(
            Column::new()
                .padding(EdgeInsets::all(20.0))
                .spacing(10.0)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .child(heading("1 · TimePicker"))
                .child(caption(&time_label))
                .child(Row::new().spacing(8.0).child(caption("24-hour dial")).child(h24_toggle))
                .child(clock)
                .child(heading("2 · DatePicker — Single"))
                .child(caption(&opt_date_label(single_sel.get())))
                .child(single)
                .child(heading("3 · DatePicker — Range (drag)"))
                .child(caption(&range_label(range_sel.get())))
                .child(range)
                .child(heading("4 · Range — custom colors"))
                .child(caption(&range_label(custom_sel.get())))
                .child(custom)
                .child(heading("5 · Range — vertical page axis"))
                .child(caption("chevrons slide months up/down"))
                .child(vertical)
                .child(heading("6 · Bounded (min 6 · max 24)"))
                .child(caption(&opt_date_label(bounded_sel.get())))
                .child(bounded),
        ))
        .boxed()
    }
}

/// Build a controlled Range DatePicker with optional custom colors + axis.
fn range_picker(
    month: &Atom<SimpleDate>,
    sel: &Atom<Range>,
    accent: Option<Color>,
    band: Option<Color>,
    axis: PageAxis,
) -> DatePicker {
    let mut dp = DatePicker::new(month.get())
        .mode(SelectionMode::Range)
        .axis(axis)
        .today(SimpleDate::new(2026, 7, 28));
    if let Some(a) = accent { dp = dp.accent(a); }
    if let Some(b) = band { dp = dp.range_color(b); }
    if let Some((s, e)) = sel.get() { dp = dp.range(s, e); }
    let (sc, mc) = (sel.clone(), month.clone());
    dp.on_select(move |s, e| sc.set(Some((s, e))))
        .on_month_change(move |m| mc.set(m))
}

fn heading(t: &str) -> Text { Text::new(t).size(15.0) }
fn caption(t: &str) -> Text { Text::new(t).size(12.0) }

fn opt_date_label(d: Option<SimpleDate>) -> String {
    match d {
        Some(d) => format!("selected: {}/{}/{}", d.year, d.month, d.day),
        None => "nothing selected".to_string(),
    }
}

fn range_label(r: Range) -> String {
    match r {
        Some((s, Some(e))) => format!("{}/{} \u{2192} {}/{}", s.month, s.day, e.month, e.day),
        Some((s, None)) => format!("{}/{} \u{2192} \u{2026}", s.month, s.day),
        None => "drag to select a range".to_string(),
    }
}

fn main() {
    App::new().title("ROSACE — Picker Gallery").size(360, 780).launch(Demo);
}
