//! Phase 30 Step 4 exit-bar demo (D113): `use_network_status` reacting
//! live to the OS network being disabled/re-enabled.
//!
//! Run: `cargo run -p rosace-examples --bin net_status_demo`, then
//! toggle Wi-Fi off and on.

use rosace::prelude::*;

struct NetStatusDemo;

impl Component for NetStatusDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let status = rosace::net::use_network_status(ctx);

        let (label, detail) = match status {
            rosace::net::NetworkStatus::Unknown => ("UNKNOWN", "first probe pending\u{2026}"),
            rosace::net::NetworkStatus::Online => ("ONLINE", "probe reaches 1.1.1.1:443 / 8.8.8.8:53"),
            rosace::net::NetworkStatus::Offline => ("OFFLINE", "no probe target reachable"),
        };

        Scaffold::new(
            Column::new()
                .padding(EdgeInsets::all(24.0))
                .spacing(12.0)
                .child(Spacer::gap(0.0, 48.0))
                .child(Text::display(label).align(TextAlign::Center))
                .child(Text::new(detail).align(TextAlign::Center))
                .child(Spacer::gap(0.0, 16.0))
                .child(Text::new("Toggle Wi-Fi to watch it react.").align(TextAlign::Center)),
        )
        .app_bar(AppBar::new("Network Status"))
        .into_element()
    }
}

fn main() {
    App::new().title("Network Status").size(640, 480).launch(NetStatusDemo);
}
