//! Phase 32 Step 1 exit-bar demo (D115): `BottomNavigationBar` +
//! `FloatingActionButton` live in a `Scaffold` — tab switching drives
//! real state, the FAB increments a counter.
//!
//! Run: `cargo run -p rosace-examples --bin bottom_nav_demo`

use rosace::prelude::*;

struct BottomNavDemo;

impl Component for BottomNavDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let tab = ctx.state(0usize);
        let count = ctx.state(0i32);

        let body_text = match tab.get() {
            0 => format!("Home — FAB pressed {} times", count.get()),
            1 => "Search screen".to_string(),
            _ => "Profile screen".to_string(),
        };

        let body = Column::new()
            .padding(EdgeInsets::all(24.0))
            .child(Spacer::gap(0.0, 60.0))
            .child(Text::display(body_text).align(TextAlign::Center));

        let mk_item = |label: &str, kind: IconKind, index: usize, tab: &Atom<usize>| {
            let t = tab.clone();
            let mut item = BottomNavItem::new(label)
                .icon(Icon::new(kind).size(20.0))
                .on_press(move || t.set(index));
            if tab.get() == index {
                item = item.active();
            }
            item
        };

        let bar = BottomNavigationBar::new()
            .item(mk_item("Home", IconKind::Home, 0, &tab).badge(3))
            .item(mk_item("Search", IconKind::Search, 1, &tab))
            .item(mk_item("Profile", IconKind::User, 2, &tab));

        let c = count.clone();
        Scaffold::new(body)
            .app_bar(AppBar::new("Bottom Nav + FAB"))
            .bottom_bar(bar)
            .fab(FloatingActionButton::new().on_press(move || c.set(c.get() + 1)))
            .into_element()
    }
}

fn main() {
    App::new().title("Bottom Nav Demo").size(420, 720).launch(BottomNavDemo);
}
