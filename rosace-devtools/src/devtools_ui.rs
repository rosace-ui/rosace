//! The in-app DevTools overlay, built from REAL ROSACE widgets (a
//! `FloatingActionButton` + a `TabBar`/`ScrollView` panel) and injected as a
//! normal `OverlayEntry` — so it renders through the widget pipeline with
//! damage-tracking, hit-testing, press animation, and theming for free. This
//! replaces the earlier hand-drawn engine chrome.
//!
//! State lives in two `GlobalAtom`s so the (engine-injected) overlay is
//! stateless to construct; tapping the FAB / a tab flips them and requests a
//! frame, and the next build re-injects the overlay reflecting the new state.

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;
use rosace_widgets::tree::{
    Column, Container, FloatingActionButton, LayerPosition, OverlayEntry, Positioned,
    ScrollView, Stack, Tab, TabBar, Text, Widget,
};
use rosace_render::Color;

/// Whether the DevTools panel is open.
pub static DEVTOOLS_OPEN: GlobalAtom<bool> = GlobalAtom::new(AtomId(9101), || false);
/// The selected DevTools tab (see [`crate::trace_panel::DEVTOOLS_TABS`]).
pub static DEVTOOLS_TAB: GlobalAtom<usize> = GlobalAtom::new(AtomId(9102), || 0);

/// Accent (ROSACE default #7C4DFF).
fn accent() -> Color { Color::rgb(124, 77, 255) }

/// Toggle open / switch tab, then force a repaint so the engine re-injects the
/// overlay with the new state (nothing "subscribes" to these from a Component).
fn poke() {
    rosace_state::reset_to_global_dirty();
    rosace_state::request_frame();
}

/// Build the DevTools overlay for this frame. `rows` are the pre-formatted
/// activity lines for the current tab (the engine reads the flight recorder and
/// filters via [`TracePanel::rows_for`]). Returns a full-screen `OverlayEntry`.
pub fn devtools_overlay(rows: Vec<String>) -> OverlayEntry {
    let open = DEVTOOLS_OPEN.get();

    let mut stack = Stack::new();

    // The panel (only when open), docked top-right.
    if open {
        stack = stack.child(
            Positioned::new(panel(rows))
                .top(12.0)
                .right(12.0)
                .width(440.0)
                .height(460.0),
        );
    }

    // The FAB, bottom-right — a real widget with its own elevation + press anim.
    let label = if open { "\u{00d7}" } else { "</>" }; // × when open
    let fab = FloatingActionButton::new()
        .label(label)
        .background(accent())
        .on_press(|| {
            DEVTOOLS_OPEN.set(!DEVTOOLS_OPEN.get());
            poke();
        });
    stack = stack.child(
        Positioned::new(fab).right(20.0).bottom(20.0).width(56.0).height(56.0),
    );

    OverlayEntry::new(LayerPosition::Fill, stack)
}

/// The panel body: a `TabBar` over a scrolling list of the current tab's rows.
fn panel(rows: Vec<String>) -> impl Widget {
    let tab = DEVTOOLS_TAB.get();

    let bar = TabBar::new()
        .selected(tab)
        .height(38.0)
        .indicator_color(accent())
        .on_change(|i| {
            DEVTOOLS_TAB.set(i);
            poke();
        });
    let bar = crate::trace_panel::DEVTOOLS_TABS
        .iter()
        .fold(bar, |b, label| b.tab(Tab::new(*label)));

    // Rows → Text lines, newest first, inside a scroll view.
    let mut list = Column::new().spacing(4.0);
    for line in rows.iter().rev() {
        list = list.child(Text::new(line).size(12.5).color(Color::rgb(206, 214, 224)));
    }

    Container::new()
        .background(Color::rgba(20, 22, 28, 240))
        .radius(12.0)
        .child(
            Column::new()
                .child(bar)
                .child(ScrollView::new(
                    Container::new()
                        .padding(rosace_widgets::tree::EdgeInsets::all(10.0))
                        .child(list),
                )),
        )
}
