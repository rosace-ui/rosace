//! A local socket an external DevTools client connects to.
//!
//! # What this is, and is not
//!
//! It is a tap. Events are serialised as they happen and written to whoever
//! is connected; the tree is serialised when asked for. Nothing is retained
//! — disconnect and the data is gone, because it was never ROSACE's to keep.
//!
//! Concretely, that means a client which connects late has missed what came
//! before, and that is correct rather than a limitation to engineer around.
//! Buffering "just a little" for late joiners is how a tap becomes a
//! database: it needs a size, then an eviction policy, then a way to ask for
//! history, and every app pays for it whether or not anyone connects.
//!
//! # Why HTTP, and how it is still bidirectional
//!
//! A browser cannot open a raw TCP socket, so a web client needs something
//! it speaks natively. That left WebSocket (a handshake, a framing layer,
//! and either a dependency or ~150 lines of protocol) or plain HTTP.
//!
//! Traffic is NOT one-directional. A client that only watches is half a
//! tool: picking a widget on screen, highlighting a node, toggling select
//! mode — those are the client DRIVING the app. So there are two channels
//! rather than one duplex socket:
//!
//! * **down** — SSE (`EventSource`, built into every browser), for the
//!   continuous stream;
//! * **up** — `POST /command`, for the occasional instruction.
//!
//! The asymmetry matches the traffic: events fire constantly and must not
//! cost a round trip, while commands are user-initiated and rare, where a
//! POST's overhead is irrelevant. WebSocket would collapse both into one
//! connection and buy a duplex channel whose downstream half SSE already
//! covers — worth it only if commands ever become high-frequency (live
//! hover-tracking from the client, say). The command shape below would not
//! change if that swap ever happens.
//!
//! Endpoints:
//!
//! * `GET  /events`  — SSE stream of [`WireEvent`]s
//! * `GET  /tree`    — a [`TreeSnapshot`] as JSON, on demand
//! * `POST /command` — a [`WireCommand`] from the client
//! * `GET  /`        — a plain-text index, so opening the port in a browser
//!                     explains itself instead of showing nothing
//!
//! # Reaching it from a device
//!
//! The server binds to the DEVICE's loopback, which is exactly what the
//! platform tunnels already forward — so no app is ever exposed to the
//! network it happens to be on.
//!
//! | Target | How the client reaches it |
//! |---|---|
//! | Desktop | direct — `http://127.0.0.1:<port>` |
//! | Android device or emulator | `adb forward tcp:<host> tcp:<device>`, then connect on `<host>` |
//! | iOS simulator | direct — the simulator shares the host's loopback |
//! | iOS device | needs a tunnel: `iproxy <host> <device>` (libimobiledevice) |
//!
//! Binding `0.0.0.0` would remove the tunnel step and is the reason not to:
//! it would put a live view of an app's internals on whatever café Wi-Fi the
//! phone is on. The tunnel is one command and is the security boundary.
//!
//! iOS on a physical device is the weak spot — `iproxy` is a third-party
//! tool, not something Apple ships. Stated rather than discovered later.
//!
//! # Debug builds only
//!
//! Opening a port is not something a shipped app should do because a
//! developer once wanted a timeline. The whole module is `debug_assertions`
//! only, and binds to loopback — never `0.0.0.0`, which would expose an
//! app's internals to the network it is on.

#![cfg(debug_assertions)]

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use rosace_trace::event::RosaceTrace;
use rosace_trace::TraceSubscriber;

use crate::wire::{TreeSnapshot, WireEvent};

/// An instruction from the client.
///
/// Deliberately a small closed set. A general "eval this" escape hatch would
/// be easier and is the wrong shape: this port exists in debug builds on a
/// developer's machine, and an open-ended command channel is an open-ended
/// hole. Each variant maps to something `ElementInspector` already does.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum WireCommand {
    /// Turn widget-select mode on or off.
    SelectMode { on: bool },
    /// Highlight a node without selecting it (client hovering the tree).
    Hover { node: Option<usize> },
    /// Select a node (client clicking the tree).
    Select { node: Option<usize> },
}

/// The most recently published snapshots, readable from the socket thread.
///
/// PUBLISHED by the UI thread, not pulled from it. The first version of this
/// took a `Fn() -> TreeSnapshot` callback, which was unsound: the callback
/// runs on the socket thread, while the render tree is an `Rc<RefCell<..>>`
/// owned by the UI thread and cannot be touched from anywhere else. It
/// compiled only because the closure was `Send + Sync` while what it would
/// have captured is not.
///
/// So the direction is inverted. The UI thread calls
/// [`WireServer::publish_tree`] on a frame boundary, where it legitimately
/// holds the tree, and the socket thread serves whatever was last published.
///
/// This does hold ONE snapshot, which bends "ROSACE stores nothing" — but a
/// single latest value is a cache, not a history: it has a fixed size, no
/// retention policy and nothing to evict. A client asking "what is on
/// screen" has to be answered from somewhere.
#[derive(Default)]
struct Published {
    tree: Option<TreeSnapshot>,
}

/// Connected clients. Writing to a dead socket simply drops it.
#[derive(Default)]
struct Clients {
    streams: Vec<TcpStream>,
}

/// Commands waiting for the UI thread.
///
/// A POST arrives on the server thread, and everything a command touches —
/// the inspector, the render tree — belongs to the UI thread. So commands
/// are QUEUED and drained on a frame boundary, the same shape accessibility
/// actions and the back intent already use. Acting on them inline would mean
/// mutating UI state from a socket thread.
///
/// Bounded: a client that spams commands while no frame runs must not grow
/// this without limit.
#[derive(Default)]
struct Commands {
    pending: Vec<WireCommand>,
}

const MAX_PENDING_COMMANDS: usize = 64;

/// The tap. Hold it to keep the server alive; drop it and the port closes.
pub struct WireServer {
    clients: Arc<Mutex<Clients>>,
    commands: Arc<Mutex<Commands>>,
    published: Arc<Mutex<Published>>,
    port: u16,
}

impl WireServer {
    /// Bind to `127.0.0.1:port` and start serving. `port: 0` picks a free
    /// one, which [`Self::port`] then reports.
    ///
    /// Loopback only, deliberately — see the module docs.
    pub fn start(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        let clients: Arc<Mutex<Clients>> = Arc::default();
        let commands: Arc<Mutex<Commands>> = Arc::default();
        let published: Arc<Mutex<Published>> = Arc::default();

        let (ac, acm, ap) = (clients.clone(), commands.clone(), published.clone());
        std::thread::Builder::new()
            .name("rosace-devtools-wire".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    handle(stream, &ac, &acm, &ap);
                }
            })?;

        Ok(Self { clients, commands, published, port })
    }

    /// The bound port — useful when `start(0, ..)` chose one.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Number of connected clients. Zero is the common case, and the reason
    /// serialisation is skipped entirely when nobody is listening.
    pub fn client_count(&self) -> usize {
        self.clients.lock().map(|c| c.streams.len()).unwrap_or(0)
    }

    /// Publish the current tree. Call from the UI thread, where the render
    /// tree is legitimately reachable.
    ///
    /// Cheap when nobody is connected — check [`Self::client_count`] first
    /// if building the snapshot itself is expensive.
    pub fn publish_tree(&self, snapshot: TreeSnapshot) {
        let mut g = match self.published.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        g.tree = Some(snapshot);
    }

    /// Drain the commands a client has sent since the last call.
    ///
    /// Call once per frame from the UI thread. Returns them in arrival
    /// order; the queue is emptied.
    pub fn take_commands(&self) -> Vec<WireCommand> {
        let mut g = match self.commands.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        std::mem::take(&mut g.pending)
    }

    /// A [`TraceSubscriber`] that forwards events to connected clients.
    pub fn subscriber(&self) -> Arc<dyn TraceSubscriber + Send + Sync> {
        Arc::new(WireSubscriber { clients: self.clients.clone() })
    }
}

struct WireSubscriber {
    clients: Arc<Mutex<Clients>>,
}

impl TraceSubscriber for WireSubscriber {
    fn on_trace(&self, event: &RosaceTrace) {
        let mut guard = match self.clients.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        // Nobody listening: do not serialise. This is the whole reason the
        // check is here rather than inside the write — `to_string` on every
        // event with no client would be pure waste, and trace events fire
        // on the UI thread.
        if guard.streams.is_empty() {
            return;
        }
        let json = match serde_json::to_string(&WireEvent::from_trace(event)) {
            Ok(j) => j,
            Err(_) => return,
        };
        let frame = format!("data: {json}\n\n");
        // A client that has gone away fails its write; drop it rather than
        // accumulating dead sockets.
        guard.streams.retain_mut(|s| s.write_all(frame.as_bytes()).is_ok());
    }
}

fn handle(
    mut stream: TcpStream,
    clients: &Arc<Mutex<Clients>>,
    commands: &Arc<Mutex<Commands>>,
    published: &Arc<Mutex<Published>>,
) {
    use std::io::{BufRead, BufReader};

    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    match path.as_str() {
        "/command" if method == "POST" => {
            // Read headers for Content-Length, then exactly that many bytes.
            // Reading to EOF would block: the client keeps the connection
            // open waiting for our reply.
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 { break; }
                let trimmed = line.trim_end();
                if trimmed.is_empty() { break; }
                if let Some(v) = trimmed.strip_prefix("Content-Length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; len];
            let ok = len > 0 && std::io::Read::read_exact(&mut reader, &mut body).is_ok();
            let reply = if !ok {
                "{\"error\":\"empty body\"}".to_string()
            } else {
                match serde_json::from_slice::<WireCommand>(&body) {
                    Ok(cmd) => {
                        let mut g = match commands.lock() {
                            Ok(g) => g,
                            Err(e) => e.into_inner(),
                        };
                        if g.pending.len() < MAX_PENDING_COMMANDS {
                            g.pending.push(cmd);
                            "{\"ok\":true}".to_string()
                        } else {
                            // Say so rather than silently dropping: a client
                            // spamming a stalled app should learn that.
                            "{\"error\":\"command queue full\"}".to_string()
                        }
                    }
                    Err(e) => format!("{{\"error\":\"{}\"}}", e).replace('"', "'"),
                }
            };
            let _ = write_json(&mut stream, &reply);
        }
        "/events" => {
            // SSE: headers, then the socket stays open and becomes a sink.
            let head = "HTTP/1.1 200 OK\r\n\
                        Content-Type: text/event-stream\r\n\
                        Cache-Control: no-cache\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Connection: keep-alive\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_ok() {
                if let Ok(mut g) = clients.lock() {
                    g.streams.push(stream);
                }
            }
        }
        "/tree" => {
            let body = {
                let g = match published.lock() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                match &g.tree {
                    Some(t) => t.to_json(),
                    // Honest about the difference between "an app with no
                    // widgets" and "the app has not published yet" — an
                    // empty array would say the first when it means the
                    // second.
                    None => "{\"error\":\"no tree published yet\"}".to_string(),
                }
            };
            let _ = write_json(&mut stream, &body);
        }
        _ => {
            let body = format!(
                "rosace devtools wire\n\n\
                 GET /events  server-sent events (live trace stream)\n\
                 GET /tree    render tree snapshot (JSON)\n\n\
                 Nothing is stored here. Connect late and you have missed it.\n"
            );
            let _ = write_text(&mut stream, &body);
        }
    }
}

fn write_json(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
         Access-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())
}

fn write_text(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
         Content-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::time::Duration;

    fn get(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    }

    #[test]
    fn a_tree_request_returns_what_the_ui_thread_published() {
        let server = WireServer::start(0).expect("bind");
        server.publish_tree(TreeSnapshot { nodes: Vec::new() });
        let body = get(server.port(), "/tree");
        assert!(body.contains("application/json"), "must be JSON: {body}");
        assert!(body.contains("\"nodes\""), "must carry the snapshot: {body}");
    }

    /// "Nothing published yet" and "an app with no widgets" must not look
    /// the same to a client — an empty array would say the second.
    #[test]
    fn a_tree_request_before_anything_is_published_says_so() {
        let server = WireServer::start(0).expect("bind");
        let body = get(server.port(), "/tree");
        assert!(body.contains("no tree published"), "got: {body}");
    }

    /// Publishing replaces rather than accumulates: one latest value, which
    /// is a cache, not a history.
    #[test]
    fn publishing_again_replaces_the_previous_snapshot() {
        use crate::wire::WireNode;
        let server = WireServer::start(0).expect("bind");
        server.publish_tree(TreeSnapshot { nodes: Vec::new() });
        server.publish_tree(TreeSnapshot { nodes: vec![WireNode {
            id: 9, parent: None, children: vec![], tag: "Marker", rect: None,
            semantics: vec![], hit_count: 0, scroll_count: 0, overlay_count: 0,
            has_editable: false, hovered: false, pressed: false,
        }] });
        let body = get(server.port(), "/tree");
        assert!(body.contains("Marker"), "must serve the LATEST: {body}");
        assert_eq!(body.matches("\"id\"").count(), 1, "one snapshot, not both");
    }

    /// The point of the whole module: an event emitted while a client is
    /// connected reaches it, live.
    #[test]
    fn an_event_reaches_a_connected_client() {
        use rosace_trace::event::{ComponentId, RebuildCause};

        let server = WireServer::start(0).expect("bind");
        let sub = server.subscriber();

        let mut client = TcpStream::connect(("127.0.0.1", server.port())).expect("connect");
        write!(client, "GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();

        // Wait for the accept thread to register us, or the emit below races
        // the connection and lands with zero clients.
        for _ in 0..200 {
            if server.client_count() > 0 { break; }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(server.client_count(), 1, "the client should be registered");

        sub.on_trace(&RosaceTrace::ComponentRebuild {
            id: ComponentId(7),
            cause: RebuildCause::Manual,
            duration: Duration::from_millis(3),
        });

        let mut reader = BufReader::new(client);
        let mut saw_event = false;
        for _ in 0..40 {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 { break; }
            if line.starts_with("data: ") && line.contains("component_rebuild") {
                saw_event = true;
                break;
            }
        }
        assert!(saw_event, "the emitted event never reached the connected client");
    }

    /// With nobody connected, an event must cost nothing — no serialisation,
    /// no panic, no growth.
    #[test]
    fn emitting_with_no_client_is_a_no_op() {
        use rosace_trace::event::{ComponentId, RebuildCause};
        let server = WireServer::start(0).expect("bind");
        let sub = server.subscriber();
        for _ in 0..1000 {
            sub.on_trace(&RosaceTrace::ComponentRebuild {
                id: ComponentId(1),
                cause: RebuildCause::Manual,
                duration: Duration::from_millis(1),
            });
        }
        assert_eq!(server.client_count(), 0);
    }

    fn post(port: u16, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        write!(s, "POST /command HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
               body.len(), body).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    }

    /// The client must be able to DRIVE the app, not just watch it. Widget
    /// select mode is the case that made a one-way channel insufficient.
    #[test]
    fn a_client_command_reaches_the_ui_thread_queue() {
        let server = WireServer::start(0).expect("bind");
        assert!(server.take_commands().is_empty(), "nothing queued yet");

        let reply = post(server.port(), r#"{"cmd":"select_mode","on":true}"#);
        assert!(reply.contains("\"ok\":true"), "command rejected: {reply}");

        let cmds = server.take_commands();
        assert_eq!(cmds, vec![WireCommand::SelectMode { on: true }]);
        assert!(server.take_commands().is_empty(), "draining must empty the queue");
    }

    #[test]
    fn hover_and_select_carry_a_node_and_can_clear_it() {
        let server = WireServer::start(0).expect("bind");
        post(server.port(), r#"{"cmd":"hover","node":12}"#);
        post(server.port(), r#"{"cmd":"select","node":null}"#);
        assert_eq!(server.take_commands(), vec![
            WireCommand::Hover { node: Some(12) },
            WireCommand::Select { node: None },
        ]);
    }

    /// Malformed input must be refused, not queued — a socket on a
    /// developer's machine still deserves a closed command set.
    #[test]
    fn an_unknown_command_is_rejected_rather_than_queued() {
        let server = WireServer::start(0).expect("bind");
        let reply = post(server.port(), r#"{"cmd":"rm_rf","path":"/"}"#);
        assert!(reply.contains("error"), "should have been refused: {reply}");
        assert!(server.take_commands().is_empty(), "nothing unknown may reach the app");
    }

    /// A client spamming a stalled app must not grow the queue without
    /// limit, and should be TOLD rather than silently dropped.
    #[test]
    fn the_command_queue_is_bounded_and_says_so() {
        let server = WireServer::start(0).expect("bind");
        let mut saw_full = false;
        for _ in 0..(MAX_PENDING_COMMANDS + 10) {
            if post(server.port(), r#"{"cmd":"select_mode","on":true}"#).contains("queue full") {
                saw_full = true;
            }
        }
        assert!(saw_full, "an over-full queue must report it");
        assert_eq!(server.take_commands().len(), MAX_PENDING_COMMANDS);
    }

}
