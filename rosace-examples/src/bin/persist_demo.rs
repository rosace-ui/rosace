//! Phase 31 Step 2 exit-bar demo (D114/D121): a `state_permanent` value
//! observably survives fully quitting and relaunching the app.
//!
//! Run repeatedly: `cargo run -p rosace-examples --bin persist_demo` —
//! the launch counter increments across runs (stored in the platform
//! app-data dir's rosace.sqlite), and the note field shows a string
//! restored from disk.

use rosace::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

static BUMPED_THIS_RUN: AtomicBool = AtomicBool::new(false);

struct PersistDemo;

impl Component for PersistDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let launches = ctx.state_permanent("launch_count", 0i64);
        let note = ctx.state_permanent("note", String::from("(first run — no note yet)"));

        // Count THIS process exactly once (a rebuild must not re-count).
        if !BUMPED_THIS_RUN.swap(true, Ordering::SeqCst) {
            launches.set(launches.get() + 1);
            note.set(format!("written during launch #{}", launches.get()));
        }

        Scaffold::new(
            Column::new()
                .padding(EdgeInsets::all(24.0))
                .spacing(12.0)
                .child(Spacer::gap(0.0, 48.0))
                .child(Text::display(format!("Launch #{}", launches.get())).align(TextAlign::Center))
                .child(Text::new("state_permanent(\"launch_count\") — quit and relaunch me").align(TextAlign::Center))
                .child(Spacer::gap(0.0, 16.0))
                .child(Text::title("Restored note"))
                .child(Text::new(note.get())),
        )
        .app_bar(AppBar::new("Persist Demo"))
        .into_element()
    }
}

fn main() {
    App::new().title("Persist Demo").size(640, 480).launch(PersistDemo);
}
