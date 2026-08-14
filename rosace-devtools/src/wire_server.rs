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
//! # Why HTTP and Server-Sent Events
//!
//! A browser cannot open a raw TCP socket, so a web client needs something
//! it speaks natively. The options were WebSocket (a handshake, a framing
//! layer, and either a dependency or ~150 lines of protocol) or SSE (plain
//! HTTP, `data: …\n\n`, and `EventSource` built into every browser). The
//! traffic here is one-directional — the app talks, the client listens — so
//! the WebSocket's duplex channel would be paid for and unused.
//!
//! Endpoints:
//!
//! * `GET /events` — SSE stream of [`WireEvent`]s
//! * `GET /tree`   — a [`TreeSnapshot`] as JSON, on demand
//! * `GET /`       — a plain-text index, so opening the port in a browser
//!                   explains itself instead of showing nothing
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

/// Produces the current tree when a client asks for one.
///
/// A callback rather than a stored snapshot: the render tree lives on the UI
/// thread behind an `Rc<RefCell<..>>` and cannot be shared, and a snapshot
/// taken in advance would be stale by the time anyone reads it. The app
/// installs this from somewhere that CAN see the tree.
pub type TreeProvider = Arc<dyn Fn() -> TreeSnapshot + Send + Sync>;

/// Connected clients. Writing to a dead socket simply drops it.
#[derive(Default)]
struct Clients {
    streams: Vec<TcpStream>,
}

/// The tap. Hold it to keep the server alive; drop it and the port closes.
pub struct WireServer {
    clients: Arc<Mutex<Clients>>,
    port: u16,
}

impl WireServer {
    /// Bind to `127.0.0.1:port` and start serving. `port: 0` picks a free
    /// one, which [`Self::port`] then reports.
    ///
    /// Loopback only, deliberately — see the module docs.
    pub fn start(port: u16, tree: Option<TreeProvider>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        let clients: Arc<Mutex<Clients>> = Arc::default();

        let accept_clients = clients.clone();
        std::thread::Builder::new()
            .name("rosace-devtools-wire".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    handle(stream, &accept_clients, tree.as_ref());
                }
            })?;

        Ok(Self { clients, port })
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

fn handle(mut stream: TcpStream, clients: &Arc<Mutex<Clients>>, tree: Option<&TreeProvider>) {
    use std::io::{BufRead, BufReader};

    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();

    match path.as_str() {
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
            let body = tree.map(|t| t().to_json())
                // Honest about the difference between "no tree" and "no
                // provider installed" — a client seeing an empty array would
                // otherwise conclude the app has no widgets.
                .unwrap_or_else(|| "{\"error\":\"no tree provider installed\"}".into());
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
    fn a_tree_request_returns_the_providers_snapshot() {
        let provider: TreeProvider = Arc::new(|| TreeSnapshot { nodes: Vec::new() });
        let server = WireServer::start(0, Some(provider)).expect("bind");
        let body = get(server.port(), "/tree");
        assert!(body.contains("application/json"), "must be JSON: {body}");
        assert!(body.contains("\"nodes\""), "must carry the snapshot: {body}");
    }

    /// "No provider installed" and "an app with no widgets" must not look
    /// the same to a client.
    #[test]
    fn a_tree_request_with_no_provider_says_so_rather_than_returning_empty() {
        let server = WireServer::start(0, None).expect("bind");
        let body = get(server.port(), "/tree");
        assert!(body.contains("no tree provider"), "got: {body}");
    }

    /// The point of the whole module: an event emitted while a client is
    /// connected reaches it, live.
    #[test]
    fn an_event_reaches_a_connected_client() {
        use rosace_trace::event::{ComponentId, RebuildCause};

        let server = WireServer::start(0, None).expect("bind");
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
        let server = WireServer::start(0, None).expect("bind");
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
}
