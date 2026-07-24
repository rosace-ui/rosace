# Navigation

Routes in ROSACE are typed Rust enums, never strings. This chapter covers `ScreenNav` — the API you'll actually use in app code — the navigation stack model underneath it, and back buttons, guards, and transitions.

## Routes are an enum

Define every screen your app has as variants of one enum:

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Home,
    Counter,
    Detail { id: u64 },
}
```

There's no separate route trait to implement by hand for `ScreenNav` — any `Clone + Send + Sync + 'static` type works. (The lower-level `Navigator`/`Route` API described below does ask for an explicit `impl Route for Screen {}`; `Route` just requires `Debug + Clone + PartialEq + Send + Sync + 'static`.)

## ScreenNav: the app-facing navigator

`ScreenNav<R>` is created with `ctx.state`-style hooks, so the owning component automatically rebuilds on navigation — no manual subscription:

```rust
use rosace::prelude::*;

impl Component for AppRoot {
    fn build(&self, ctx: &mut Context) -> Element {
        let nav = ScreenNav::new(ctx, Screen::Home);

        let screen = match nav.current().unwrap_or(Screen::Home) {
            Screen::Home => home_screen(&nav),
            Screen::Counter => counter_screen(),
            Screen::Detail { id } => detail_screen(id),
        };

        Scaffold::new(screen).into_element()
    }
}
```

Like `ctx.state`, `ScreenNav::new` must be called unconditionally at a stable point in `build` — it follows the same hook rules.

Navigate from anywhere you have a `nav.clone()` (it's cheap — clones share the same stack):

```rust
pub fn home_screen(nav: &ScreenNav<Screen>) -> impl Widget {
    let nav = nav.clone();
    ListTile::new("Counter")
        .subtitle("A simple counter")
        .on_press(move || nav.push(Screen::Counter))
}
```

`ScreenNav` methods:

- `nav.push(route)` — push a new screen.
- `nav.pop()` — go back one screen; no-ops (returns `false`) at the root.
- `nav.replace(route)` — swap the current screen without adding history depth.
- `nav.current()` — the current route, or `None` if the stack is somehow empty.
- `nav.can_pop()` — whether there's anywhere to pop back to.
- `nav.depth()` — stack depth (root counts as 1).

## Back button

`AppBar` gets a one-call back button via the `AppBarNavExt` extension (in `rosace::prelude`) — it adds a `← Back` leading button that pops `nav`, and only appears when `nav.can_pop()` is true:

```rust
let bar = AppBar::new(screen.title()).back_button(&nav);
Scaffold::new(screen).app_bar(bar).into_element()
```

This replaces the manual `if nav.can_pop() { bar.leading(Button::new("← Back").on_press(...)) }` boilerplate every app used to write by hand.

## Screen transitions

Every `push`/`pop`/`replace` on `ScreenNav` triggers an animated transition automatically (as long as the theme's `animation.enabled` is true) — no extra call needed. The default is platform-appropriate: a horizontal slide on iOS/macOS/Android (matching native drill-in navigation), a fade on desktop/web. Override it explicitly with `.transition_style(...)`:

```rust
let nav = ScreenNav::new(ctx, Screen::Home)
    .transition_style(TransitionStyleKind::Fade); // None | Slide | Fade
```

To actually render the animation, pass the outgoing screen and the transition handle to `ScreenTransitionView` instead of handing the current screen's widget straight to `Scaffold::new`:

```rust
let build_screen = {
    let nav = nav.clone();
    move |s: Screen| -> BoxedWidget {
        match s {
            Screen::Home => Box::new(home_screen(&nav)),
            Screen::Counter => Box::new(counter_screen()),
            Screen::Detail { id } => Box::new(detail_screen(id)),
        }
    }
};
let screen = nav.current().unwrap_or(Screen::Home);
let body = build_screen(screen);
let outgoing = nav.previous().map(build_screen);
let view = ScreenTransitionView::new(body, outgoing, nav.transition_handle());

Scaffold::new(view).app_bar(bar).into_element()
```

`ScreenTransitionView` paints the outgoing screen (if mid-transition) and the incoming screen, each offset by the shared spring-eased transition state, and cleans itself up once the spring settles. `rsc new`'s generated `app.rs` uses exactly this shape — it's the reference pattern for wiring navigation into your root component.

## The stack model underneath

`ScreenNav` is a thin, reactive wrapper over a plain **navigation stack**: `push` appends, `pop` removes the top (never the root), `replace` swaps the top in place without changing depth, and the stack is backed by an [`Atom`](../GLOSSARY.md#atom)`<Vec<R>>` — which is exactly why navigating triggers a rebuild for free, the same mechanism as any other state change (see [Components & State](components-and-state.md)).

Screens that are popped off the stack aren't dropped immediately — a keep-alive registry retains them (by route) until the stack is reset to a new root, so a tab's scroll position or in-progress form state isn't lost just because the user navigated away and back. This is opt-in bookkeeping the stack does for you; there's no widget-level API to opt out of it per screen currently.

## Guards

Navigation guards run before every push and can allow, block, or (in a future phase) redirect a navigation. Guards are attached to the lower-level `Navigator<R>` type, not `ScreenNav`:

```rust
use rosace_nav::{Navigator, NavigationGuard, NavigationDecision, BlockWhenGuard};

let nav = Navigator::new(Screen::Home)
    .with_guard(BlockWhenGuard::new(|| has_unsaved_changes()));

nav.push(Screen::Settings); // returns false, stays on Home, if the guard blocks
```

`BlockWhenGuard::new(condition)` is the ready-made guard for the common "block while X is true" case. For anything more custom, implement `NavigationGuard<R>` yourself:

```rust
struct ConfirmLeave;
impl NavigationGuard<Screen> for ConfirmLeave {
    fn before_navigate(&self, from: Option<&Screen>, to: &Screen) -> NavigationDecision {
        NavigationDecision::Allow // or ::Block, or ::RedirectTo(path)
    }
}
```

`Navigator<R>` has the same push/pop/replace/reset_to/current/can_go_back/depth shape as `ScreenNav`, but is a plain (non-reactive) handle — it doesn't auto-rebuild a component on navigation the way `ScreenNav` does, which is why `ScreenNav` is the one to reach for inside `build()`. Guards on `ScreenNav` itself aren't wired up yet; if you need guarded navigation today, drive the `Navigator` directly.

## Nested navigation

Each `Navigator`/`ScreenNav` instance owns an entirely independent stack — giving each tab of a tab bar (or each pane of a split view) its own `ScreenNav` gives you fully independent nested navigation, each remembering its own history.

---

**Under the hood:** how `ScreenNav`'s push/pop drives a component rebuild through the same `Atom` machinery as any other state is in [State & Reactivity](../architecture/state-and-reactivity.md).

Next: [Theming](theming.md).
