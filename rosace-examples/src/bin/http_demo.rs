//! Phase 30 Steps 1+2 exit-bar demo (D113): fetch real JSON from a real
//! HTTPS endpoint through the `use_query` hook and render each state.
//!
//! Run: `cargo run -p rosace-examples --bin http_demo`

use rosace::prelude::*;

const URL: &str = "https://httpbin.org/json";

struct HttpDemo;

impl Component for HttpDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let query = rosace::net::use_query(ctx, URL);

        let (status, body) = match &query {
            rosace::net::QueryState::Idle => ("idle".to_string(), String::new()),
            rosace::net::QueryState::Loading => (format!("loading {URL}\u{2026}"), String::new()),
            rosace::net::QueryState::Loaded(resp) => {
                (format!("HTTP {} from {}", resp.status, URL), resp.text())
            }
            rosace::net::QueryState::Failed(e) => (format!("transport error: {e}"), String::new()),
        };

        let mut col = Column::new()
            .padding(EdgeInsets::all(24.0))
            .spacing(10.0)
            .child(Spacer::gap(0.0, 24.0))
            .child(Text::title("use_query over HTTPS (rustls via ureq, D113)"))
            .child(Text::new(status));
        for line in body.lines().take(16) {
            col = col.child(Text::new(line.to_string()).size(13.0));
        }

        Scaffold::new(col)
            .app_bar(AppBar::new("HTTP Demo"))
            .into_element()
    }
}

fn main() {
    App::new().title("HTTP Demo").size(760, 560).launch(HttpDemo);
}
