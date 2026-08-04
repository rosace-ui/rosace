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

/// Default row text color — routine activity (network, nav, ffi, mount/unmount).
const ROW_DEFAULT: Color = Color::rgb(206, 214, 224);

/// Pick a row's text color from its own formatted line (see
/// `trace_panel::row`'s `NET `/`LOG `/`NAV `/`FFI `/`UI  ` prefixes and, for
/// logs, the embedded level label). Reading the already-formatted string
/// instead of the raw `RosaceTrace` keeps this purely a rendering concern —
/// `trace_panel` stays a plain string formatter with no color/theme
/// knowledge of its own.
fn row_color(line: &str) -> Color {
    if line.starts_with("LOG") {
        if line.contains("ERROR") {
            return Color::rgb(255, 99, 99);
        }
        if line.contains("WARN") {
            return Color::rgb(255, 186, 84);
        }
        return ROW_DEFAULT;
    }
    if line.starts_with("FFI") && line.contains('\u{2717}') {
        return Color::rgb(255, 99, 99); // FFI ✗ error
    }
    if line.starts_with("NET") {
        return Color::rgb(110, 200, 255);
    }
    if line.starts_with("NAV") {
        return Color::rgb(197, 158, 255);
    }
    ROW_DEFAULT
}

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

    // This overlay is `LayerPosition::Fill` — the FULL viewport, including
    // any platform-reserved edge (iOS home indicator, Android nav bar). A
    // fixed pixel offset from the raw edge can land the FAB inside that
    // reserved strip, where the OS's own gesture recognizer eats the tap
    // before it ever reaches the app (read as "the FAB doesn't respond" —
    // 2026-07-31 user report on a real iOS device).
    let sa = rosace_core::use_safe_area();

    // `Stack::new()` defaults to `StackFit::Loose` (shrink to the largest
    // child) — under that fit it ignores the TIGHT win_w×win_h constraints
    // the engine deliberately passes a `Fill` overlay (see the comment at
    // its `entry.widget.layout` call site) and collapses to the FAB's own
    // 40×40 size instead. `Positioned`'s `.bottom()/.left()` then resolve
    // against that tiny collapsed rect pinned at the window's top-left,
    // not the real window — the FAB rendered top-left over the traffic
    // lights instead of bottom-left. `Expand` makes this Stack actually
    // fill the tight constraints it's given.
    let mut stack = Stack::new().fit(rosace_widgets::tree::stack::StackFit::Expand);

    // The panel (only when open), docked top-right.
    if open {
        stack = stack.child(
            Positioned::new(panel(rows))
                .top(12.0 + sa.top)
                .right(12.0 + sa.right)
                .width(440.0)
                .height(460.0),
        );
    }

    // The FAB — bottom-left (D123, moved off the bottom-right corner some
    // apps' own primary FAB occupies), small enough to stay out of the way
    // of real app content.
    const FAB_SIZE: f32 = 40.0;
    let label = if open { "\u{00d7}" } else { "</>" }; // × when open
    let fab = FloatingActionButton::new()
        .size(FAB_SIZE)
        .label(label)
        .background(accent())
        .on_press(|| {
            DEVTOOLS_OPEN.set(!DEVTOOLS_OPEN.get());
            poke();
        });
    stack = stack.child(
        Positioned::new(fab)
            .left(14.0 + sa.left)
            .bottom(14.0 + sa.bottom)
            .width(FAB_SIZE)
            .height(FAB_SIZE),
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

    // Rows → Text lines, newest first, inside a scroll view. Colored by
    // severity/category (from the row's own prefix — see `row_color`) so an
    // ERROR is visually distinct from routine NET/NAV noise at a glance,
    // instead of every line rendering in the same flat gray.
    let mut list = Column::new().spacing(4.0);
    for line in rows.iter().rev() {
        list = list.child(Text::new(line).size(12.5).color(row_color(line)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_log_is_red() {
        assert_eq!(row_color("LOG  ERROR home boom"), Color::rgb(255, 99, 99));
    }

    #[test]
    fn warn_log_is_amber() {
        assert_eq!(row_color("LOG  WARN  home careful"), Color::rgb(255, 186, 84));
    }

    #[test]
    fn info_log_is_default() {
        assert_eq!(row_color("LOG  INFO  home loaded"), ROW_DEFAULT);
    }

    #[test]
    fn network_is_blue() {
        assert_eq!(row_color("NET  → Get https://x"), Color::rgb(110, 200, 255));
    }

    #[test]
    fn ffi_error_is_red() {
        assert_eq!(row_color("FFI  \u{2717} camera denied"), Color::rgb(255, 99, 99));
    }

    #[test]
    fn ffi_success_is_default() {
        assert_eq!(row_color("FFI  capture 12.0ms"), ROW_DEFAULT);
    }

    #[test]
    fn nav_is_purple() {
        assert_eq!(row_color("NAV  → route /home"), Color::rgb(197, 158, 255));
    }

    #[test]
    fn placeholder_row_is_default() {
        assert_eq!(row_color("(no activity yet)"), ROW_DEFAULT);
    }
}
