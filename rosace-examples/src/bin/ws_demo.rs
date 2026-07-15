//! Phase 30 Step 3 exit-bar demo (D113): a live WebSocket connection to a
//! real server over `wss://` (rustls) via the `use_websocket` hook —
//! sends a message a second, displays the echoes as they arrive.
//!
//! Run: `cargo run -p rosace-examples --bin ws_demo`

use rosace::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

static SENDER_STARTED: AtomicBool = AtomicBool::new(false);

const URL: &str = "wss://ws.postman-echo.com/raw";

struct WsDemo;

impl Component for WsDemo {
    fn build(&self, ctx: &mut Context) -> Element {
        let ws = rosace::ws::use_websocket(ctx, URL);

        // Fire a numbered message every second so the echo stream is
        // visibly LIVE, not a one-shot.
        if !SENDER_STARTED.swap(true, Ordering::SeqCst) {
            let handle = ws.clone();
            std::thread::spawn(move || {
                for n in 1.. {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if !handle.send(rosace::ws::WsMessage::Text(format!(
                        "rosace ws message #{n}"
                    ))) {
                        return; // connection gone
                    }
                }
            });
        }

        let (status, lines) = match &ws.state {
            rosace::ws::WsState::Connecting => (format!("connecting to {URL}\u{2026}"), vec![]),
            rosace::ws::WsState::Open { messages } => (
                format!("OPEN \u{2014} {URL} ({} echoes)", messages.len()),
                messages
                    .iter()
                    .rev()
                    .take(10)
                    .filter_map(|m| m.as_text().map(|s| s.to_string()))
                    .collect(),
            ),
            rosace::ws::WsState::Closed(e) => (format!("closed: {e}"), vec![]),
        };

        let mut col = Column::new()
            .padding(EdgeInsets::all(24.0))
            .spacing(8.0)
            .child(Spacer::gap(0.0, 24.0))
            .child(Text::title("use_websocket over wss (tungstenite, D113)"))
            .child(Text::new(status));
        for line in lines {
            col = col.child(Text::new(format!("\u{2190} {line}")).size(13.0));
        }

        Scaffold::new(col)
            .app_bar(AppBar::new("WebSocket Demo"))
            .into_element()
    }
}

fn main() {
    App::new().title("WebSocket Demo").size(760, 560).launch(WsDemo);
}
