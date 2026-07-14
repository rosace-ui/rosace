//! General HTTP client (D113/Phase 30 Step 1) — `ureq`-backed (sync,
//! `rustls` TLS), replacing the Phase-6-era hand-rolled HTTP/1.0 GET that
//! rejected `https://` outright.
//!
//! Blocking by design: calls run inside the same `std::thread` + `mpsc`
//! background-thread pattern the crate already uses for image loads
//! (`ImageLoader`), so nothing on the UI thread ever blocks — use
//! [`HttpClient::fetch`] for the non-blocking wrapper, or the blocking
//! methods directly from your own worker thread.
//!
//! # Web (wasm32)
//! NOT implemented on `wasm32-unknown-unknown` — `ureq`/`rustls` are
//! `std::net`-based and don't compile there. Every request on wasm
//! returns `Err` with a clear message instead of silently panicking (the
//! named, documented gap `PHASE_30.md`'s wasm constraint requires; a real
//! browser-`fetch()` backend is future work, tracked there).

use std::sync::mpsc::{self, Receiver};
#[cfg(not(target_arch = "wasm32"))]
use std::thread;

/// The HTTP methods the client supports (D113: REST-shaped coverage;
/// GraphQL/gRPC are explicitly out of Phase 30's scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl HttpMethod {
    // Only the native (ureq) path formats the method; the wasm stub never
    // reaches a wire format.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
        }
    }
}

/// A completed HTTP response. Non-2xx statuses are NOT errors at this
/// layer — the request reached the server and got an answer; `status`
/// carries it. `Err` is reserved for transport failures (DNS, TLS,
/// timeout, connection refused).
#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// The body as UTF-8 text (lossy — bytes stay available in `body`).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// True for 2xx statuses.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// One request, built up before sending. Construct via [`HttpClient::get`]
/// etc. or [`HttpRequest::new`] directly.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self { method, url: url.into(), headers: Vec::new(), body: None }
    }

    /// Add a request header (repeatable).
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Set the request body (POST/PUT).
    pub fn body(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.body = Some(bytes.into());
        self
    }
}

/// A pending non-blocking request — poll once per frame, same convention
/// as `ImageLoader::poll`. The background thread owns the socket; dropping
/// the handle detaches it (the thread finishes and its result is
/// discarded), which is what unmount-cleanup wants.
pub struct HttpHandle {
    rx: Receiver<Result<HttpResponse, String>>,
    done: bool,
}

impl HttpHandle {
    /// Returns `Some` exactly once, when the request completes; `None`
    /// while still in flight (or after the result was already taken).
    pub fn poll(&mut self) -> Option<Result<HttpResponse, String>> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(result) => {
                self.done = true;
                Some(result)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.done = true;
                Some(Err("request thread terminated without a result".to_string()))
            }
        }
    }
}

/// The client. Cheap to clone (shares one connection-pooling agent on
/// native). All blocking methods are meant for background threads — use
/// [`HttpClient::fetch`] from UI-adjacent code.
#[derive(Clone)]
pub struct HttpClient {
    #[cfg(not(target_arch = "wasm32"))]
    agent: ureq::Agent,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Self {
        Self {}
    }

    /// Blocking GET — call from a background thread.
    pub fn get(&self, url: impl Into<String>) -> Result<HttpResponse, String> {
        self.send(HttpRequest::new(HttpMethod::Get, url))
    }

    /// Blocking POST — call from a background thread.
    pub fn post(&self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Result<HttpResponse, String> {
        self.send(HttpRequest::new(HttpMethod::Post, url).body(body))
    }

    /// Blocking PUT — call from a background thread.
    pub fn put(&self, url: impl Into<String>, body: impl Into<Vec<u8>>) -> Result<HttpResponse, String> {
        self.send(HttpRequest::new(HttpMethod::Put, url).body(body))
    }

    /// Blocking DELETE — call from a background thread.
    pub fn delete(&self, url: impl Into<String>) -> Result<HttpResponse, String> {
        self.send(HttpRequest::new(HttpMethod::Delete, url))
    }

    /// Blocking send of an arbitrary request — call from a background
    /// thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn send(&self, req: HttpRequest) -> Result<HttpResponse, String> {
        let mut request = self.agent.request(req.method.as_str(), &req.url);
        for (name, value) in &req.headers {
            request = request.set(name, value);
        }
        let result = match &req.body {
            Some(bytes) => request.send_bytes(bytes),
            None => request.call(),
        };
        let response = match result {
            Ok(r) => r,
            // Non-2xx IS a response — surface it with its status, don't
            // flatten it into the transport-error channel.
            Err(ureq::Error::Status(_code, r)) => r,
            Err(ureq::Error::Transport(t)) => return Err(t.to_string()),
        };

        let status = response.status();
        let headers = response
            .headers_names()
            .into_iter()
            .filter_map(|name| {
                response.header(&name).map(|v| (name.clone(), v.to_string()))
            })
            .collect();
        let mut body = Vec::new();
        use std::io::Read;
        response
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|e| format!("read body: {}", e))?;

        Ok(HttpResponse { status, headers, body })
    }

    /// wasm32: the named, documented gap (see the module doc) — every
    /// request fails with a clear message instead of panicking.
    #[cfg(target_arch = "wasm32")]
    pub fn send(&self, _req: HttpRequest) -> Result<HttpResponse, String> {
        Err("rosace-net: HTTP is not yet implemented on web (wasm32) — see PHASE_30.md's wasm constraint".to_string())
    }

    /// Non-blocking send: runs the request on a background thread, returns
    /// a handle to poll once per frame — the same shape `ImageLoader`
    /// already uses.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn fetch(&self, req: HttpRequest) -> HttpHandle {
        let (tx, rx) = mpsc::channel();
        let client = self.clone();
        thread::spawn(move || {
            let _ = tx.send(client.send(req));
        });
        HttpHandle { rx, done: false }
    }

    /// wasm32: `std::thread::spawn` PANICS at runtime on
    /// `wasm32-unknown-unknown`, so the stub delivers the documented
    /// error through the channel directly — the handle's first poll
    /// returns it, no thread involved.
    #[cfg(target_arch = "wasm32")]
    pub fn fetch(&self, req: HttpRequest) -> HttpHandle {
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(self.send(req));
        HttpHandle { rx, done: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_text_and_success_helpers() {
        let ok = HttpResponse { status: 204, headers: vec![], body: b"hi".to_vec() };
        assert!(ok.is_success());
        assert_eq!(ok.text(), "hi");
        let nope = HttpResponse { status: 404, headers: vec![], body: vec![] };
        assert!(!nope.is_success());
    }

    #[test]
    fn request_builder_accumulates_headers_and_body() {
        let req = HttpRequest::new(HttpMethod::Post, "https://example.com/x")
            .header("Content-Type", "application/json")
            .header("X-Two", "2")
            .body(b"{}".to_vec());
        assert_eq!(req.headers.len(), 2);
        assert_eq!(req.body.as_deref(), Some(b"{}".as_slice()));
        assert_eq!(req.method.as_str(), "POST");
    }

    #[test]
    fn transport_failure_is_an_err_not_a_panic() {
        // A port that nothing listens on — must come back as Err quickly.
        let client = HttpClient::new();
        let result = client.get("http://127.0.0.1:9"); // discard port, unroutable service
        assert!(result.is_err(), "connection refused must be Err, got {result:?}");
    }

    /// The Step 1 exit-bar's HTTPS half, testable headlessly. Hits the
    /// real network, so it's ignored in normal runs — run explicitly with
    /// `cargo test -p rosace-net -- --ignored`.
    #[test]
    #[ignore = "hits the real network (HTTPS exit-bar verification)"]
    fn https_get_fetches_real_json() {
        let client = HttpClient::new();
        let resp = client.get("https://httpbin.org/json").expect("https must work now");
        assert!(resp.is_success());
        assert!(resp.text().contains("slideshow"), "expected the httpbin sample json");
    }
}
