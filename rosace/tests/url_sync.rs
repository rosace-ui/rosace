//! Binding a navigator to the address bar (D031).
//!
//! Driven through a FAKE backend, because the real one is
//! `history.pushState` and no headless test has a browser. What that does and
//! does not prove is worth being exact about:
//!
//!   * PROVEN here — the reconciliation logic: when a push writes the URL,
//!     when it must not, that a browser Back reaches the navigator, and that
//!     applying one does not echo a duplicate entry back.
//!   * NOT proven here — that `web_history` talks to `history.pushState`
//!     correctly, that `popstate` fires, or that the browser's Back button
//!     behaves. Those need a real browser and are stated as unverified.
//!
//! The echo loop is the thing worth testing hardest. Every hand-rolled URL
//! binding has the same bug: a browser Back applies to the navigator, which
//! writes the URL back, which pushes a duplicate history entry — and the Back
//! button then needs two presses, or never leaves the page.

use rosace::nav::url::{self, UrlBackend};
use rosace::nav::{RoutePath, ScreenNav};
use rosace::routes;
use std::sync::{Arc, Mutex};

#[routes]
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    #[route("/")]
    Home,
    #[route("/widgets")]
    Widgets,
    #[route("/widget/:id")]
    Widget { id: u32 },
}

/// Records what the navigator asked the address bar to do.
#[derive(Default)]
struct Bar {
    path: Mutex<Option<String>>,
    log: Mutex<Vec<String>>,
}

/// A local newtype, because `impl Trait for Arc<T>` is not ours to write.
#[derive(Clone)]
struct FakeBar(Arc<Bar>);

impl UrlBackend for FakeBar {
    fn push(&self, path: &str) {
        self.0.log.lock().unwrap().push(format!("push {path}"));
        *self.0.path.lock().unwrap() = Some(path.to_string());
    }
    fn replace(&self, path: &str) {
        self.0.log.lock().unwrap().push(format!("replace {path}"));
        *self.0.path.lock().unwrap() = Some(path.to_string());
    }
    fn current(&self) -> Option<String> {
        self.0.path.lock().unwrap().clone()
    }
}

fn ctx() -> rosace::Context {
    rosace::Context::new(rosace_trace::event::ComponentId(0))
}

/// One process, one `OnceLock` backend — so every assertion runs inside a
/// single test against one installed bar, in a deliberate order.
#[test]
fn url_binding_end_to_end() {
    let bar = Arc::new(Bar::default());
    url::install_backend(FakeBar(Arc::clone(&bar)));

    let mut c = ctx();
    let nav: ScreenNav<Screen> = ScreenNav::new(&mut c, Screen::Home);

    // ── first sync: correct the entry, do not add one ───────────────────
    nav.sync_url();
    assert_eq!(
        bar.log.lock().unwrap().as_slice(),
        &["replace /".to_string()],
        "the first sync must REPLACE — pushing would leave an unreachable \
         entry behind the page on reload"
    );

    // ── steady state: agreeing costs nothing ────────────────────────────
    nav.sync_url();
    nav.sync_url();
    assert_eq!(
        bar.log.lock().unwrap().len(),
        1,
        "syncing when the URL already agrees must not touch history — this \
         runs every build, so a write here is a history entry per frame"
    );

    // ── the navigator moves: the URL follows, as a new entry ────────────
    nav.push(Screen::Widget { id: 3 });
    nav.sync_url();
    assert_eq!(
        bar.log.lock().unwrap().last().map(String::as_str),
        Some("push /widget/3"),
        "a navigation the app made is a new history entry"
    );
    assert_eq!(bar.path.lock().unwrap().as_deref(), Some("/widget/3"));

    // ── the BROWSER moves: the navigator follows, and does NOT echo ─────
    let before = bar.log.lock().unwrap().len();
    *bar.path.lock().unwrap() = Some("/widgets".to_string()); // browser is already there
    url::deliver_browser_navigation("/widgets");

    assert_eq!(
        nav.current(),
        Some(Screen::Widgets),
        "a browser Back must reach the navigator"
    );
    nav.sync_url();
    assert_eq!(
        bar.log.lock().unwrap().len(),
        before,
        "applying a browser navigation must write NOTHING back. Echoing it \
         pushes a duplicate entry, and the Back button then needs two presses."
    );

    // ── a path naming no route is ignored, not crashed on ───────────────
    let depth_before = nav.depth();
    url::deliver_browser_navigation("/not-a-route");
    assert_eq!(
        nav.current(),
        Some(Screen::Widgets),
        "an unroutable URL leaves the app where it was"
    );
    assert_eq!(nav.depth(), depth_before);
}
