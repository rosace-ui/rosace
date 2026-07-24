//! Interactive demo of the Material clock-dial TimePicker and the range
//! DatePicker. Run: `cargo run -p rosace --example picker_demo`
//!
//! - Clock: tap the hour/minute in the header to switch which the dial edits,
//!   then tap/drag the dial. Tap AM/PM to toggle. Hand animates on change.
//! - Calendar: tap a day to start a range, tap another to complete it; tap
//!   again to start over. Use the nav arrows to change month.

use rosace::prelude::*;
use rosace::widgets::{
    DatePicker, SelectionMode, SimpleDate, SimpleTime, TimePicker, TimeUnit,
};

struct Demo;

impl Component for Demo {
    fn build(&self, ctx: &mut Context) -> Element {
        let time = ctx.state(SimpleTime::new(9, 30));
        let unit = ctx.state(TimeUnit::Hour);
        let month = ctx.state(SimpleDate::new(2026, 7, 1));
        let range = ctx.state(None::<(SimpleDate, Option<SimpleDate>)>);

        // ── Time picker ────────────────────────────────────────────────────
        let clock = {
            let (tc, uc) = (time.clone(), unit.clone());
            TimePicker::new(time.get())
                .editing(unit.get())
                .on_change(move |v| tc.set(v))
                .on_unit_change(move |u| uc.set(u))
        };

        // ── Date picker (range mode) ───────────────────────────────────────
        let calendar = {
            let mut dp = DatePicker::new(month.get())
                .mode(SelectionMode::Range)
                .today(SimpleDate::new(2026, 7, 24));
            if let Some((s, e)) = range.get() {
                dp = dp.range(s, e);
            }
            let rc = range.clone();
            let mc = month.clone();
            dp.on_range_change(move |s, e| rc.set(Some((s, e))))
                .on_month_change(move |nm| mc.set(nm))
        };

        let (h12, pm) = time.get().hour_12();
        let range_label = match range.get() {
            Some((s, Some(e))) => format!("{}/{} \u{2192} {}/{}", s.month, s.day, e.month, e.day),
            Some((s, None)) => format!("{}/{} \u{2192} \u{2026}", s.month, s.day),
            None => "tap two days".to_string(),
        };

        Scaffold::new(
            ScrollView::new(
                Column::new()
                    .padding(EdgeInsets::all(20.0))
                    .spacing(14.0)
                    .child(Text::new("TimePicker").size(15.0))
                    .child(Text::new(format!("{h12:02}:{:02} {}", time.get().minute, if pm { "PM" } else { "AM" })).size(12.0))
                    .child(clock)
                    .child(Text::new("DatePicker — Range").size(15.0))
                    .child(Text::new(range_label).size(12.0))
                    .child(calendar),
            ),
        )
        .into_element()
    }
}

fn main() {
    App::new().title("ROSACE — Picker Demo").size(360, 760).launch(Demo);
}
