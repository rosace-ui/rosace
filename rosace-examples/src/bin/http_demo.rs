//! Phase 30 Step 1 exit-bar demo (D113): fetch real JSON from a real
//! HTTPS endpoint with the `ureq`-backed `HttpClient` and render it.
//!
//! Run: `cargo run -p rosace-examples --bin http_demo`

use rosace::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

static FETCH_STARTED: AtomicBool = AtomicBool::new(false);

const URL: &str = "https://httpbin.org/json";

struct HttpDemo;

impl Component for HttpDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        // (status line, body text) — written by the fetch thread, read here.
        let result = ctx.state((String::from("fetching\u{2026}"), String::new()));

        if !FETCH_STARTED.swap(true, Ordering::SeqCst) {
            let result = result.clone();
            std::thread::spawn(move || {
                let client = rosace::net::HttpClient::new();
                match client.get(URL) {
                    Ok(resp) => {
                        let status = format!("HTTP {} from {}", resp.status, URL);
                        // Show a readable slice of the JSON (title + first lines).
                        let body = resp.text();
                        result.set((status, body));
                    }
                    Err(e) => result.set((format!("transport error: {e}"), String::new())),
                }
            });
        }

        let (status, body) = result.get();
        let mut col = Column::new()
            .padding(EdgeInsets::all(24.0))
            .spacing(10.0)
            .child(Spacer::gap(0.0, 24.0))
            .child(Text::title("GET over HTTPS (rustls via ureq, D113)"))
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
