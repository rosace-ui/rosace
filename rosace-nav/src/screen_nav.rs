use std::sync::{Arc, Mutex};

use rosace_core::Context;
use rosace_state::Atom;

use crate::transition::{ScreenTransition, SlideDirection, TransitionStyle};

/// Default screen-transition style, keyed by platform (D108/Phase 26 Step
/// 3) — mirrors `rosace-scroll::ScrollStyle`'s shape exactly. This is the
/// ONLY place platform is consulted for transition style — one pure
/// lookup, never branches scattered through navigation logic — and it is
/// always the lowest-priority source: an app's own theme `ext` value or an
/// explicit `.transition_style(...)` on `ScreenNav` both override it (see
/// `ScreenNav::new`/`transition_style`).
#[derive(Debug, Clone, Copy)]
pub struct NavTransitionStyle {
    /// The base style. For `Slide`, the direction here is ignored —
    /// `ScreenNav` always resolves the real direction itself (push enters
    /// from the right, pop enters from the left), so only whether this is
    /// `Slide`/`Fade`/`None` actually matters.
    pub style: TransitionStyleKind,
}

/// Style category, without a baked-in slide direction — see
/// `NavTransitionStyle`'s doc comment for why direction is resolved
/// separately by `ScreenNav` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStyleKind {
    None,
    Slide,
    Fade,
}

impl NavTransitionStyle {
    /// iOS/macOS/Android default to a horizontal slide (matches native
    /// drill-in navigation on all three); desktop (Windows/Linux) and web
    /// default to a plain fade — no strong native "slide" convention on
    /// either.
    pub fn default_for_platform(platform: rosace_core::Platform) -> TransitionStyleKind {
        match platform {
            rosace_core::Platform::Ios
            | rosace_core::Platform::MacOs
            | rosace_core::Platform::Android => TransitionStyleKind::Slide,
            rosace_core::Platform::Windows
            | rosace_core::Platform::Linux
            | rosace_core::Platform::Web => TransitionStyleKind::Fade,
        }
    }
}

/// A reactive navigation stack created inside a component's `build()`.
///
/// Unlike `Navigator<R>`, which is allocated externally, `ScreenNav<R>` is
/// created via `ctx.state()` so the owning component automatically subscribes
/// to route changes — push/pop triggers a rebuild without any extra wiring.
///
/// # Example
/// ```rust,ignore
/// impl Component for AppShell {
///     fn build(&self, ctx: &mut Context) -> Element {
///         let nav = ScreenNav::new(ctx, Screen::Home);
///         match nav.current() {
///             Screen::Home  => HomeView::render(nav.clone()),
///             Screen::About => AboutView::render(nav.clone()),
///         }
///     }
/// }
/// ```
#[derive(Clone)]
pub struct ScreenNav<R: Clone + Send + Sync + 'static> {
    atom: Atom<Vec<R>>,
    /// The screen that was current immediately before the most recent
    /// push/pop/replace — `None` before any navigation has happened. Lets
    /// a `ScreenTransitionView` build the OUTGOING screen's widget the
    /// same way `current()` builds the incoming one.
    previous: Atom<Option<R>>,
    /// Shared transition state (D108/Phase 26 Step 3) — `Arc<Mutex<...>>`,
    /// not an `Atom`, since it's driven every PAINT frame (`update(dt)`)
    /// by a widget, not read through the reactive-rebuild atom system;
    /// same reasoning as `rosace-scroll::ScrollController`'s non-atom
    /// bookkeeping fields.
    transition: Arc<Mutex<ScreenTransition>>,
    /// Resolved once at construction: explicit override, else the app's
    /// theme `ext` value, else the platform default. Never re-resolved
    /// per-navigation — matches `rosace-scroll`'s `resolve_physics` being
    /// called once per `ScrollView::paint`, not per-scroll-event.
    style: TransitionStyleKind,
}

impl<R: Clone + Send + Sync + 'static> ScreenNav<R> {
    /// Create a new `ScreenNav` with `initial` as the root screen.
    ///
    /// Must be called unconditionally inside `Component::build()` — it follows
    /// the hook rules (same call-site order each frame).
    pub fn new(ctx: &mut Context, initial: R) -> Self {
        let atom = ctx.state(vec![initial]);
        let previous = ctx.state(None);
        // Persisted through the SAME hook mechanism as `atom`/`previous` —
        // NOT a fresh `Arc::new(Mutex::new(...))` on every call. A fresh
        // one here would silently discard any in-flight transition on the
        // very next rebuild (which `push`/`pop` themselves always trigger,
        // since they mutate `atom`), leaving `is_active()` permanently
        // false the moment a caller even looked at the "same" `ScreenNav`
        // instance across a rebuild boundary — found via a real headless
        // `FrameEngine` integration test showing the transition never
        // activated despite `trigger()` definitely being called.
        let transition = ctx.state(Arc::new(Mutex::new(ScreenTransition::new(0.0, 0.0)))).get();
        let style = rosace_theme::provider::use_theme()
            .ext::<NavTransitionStyle>()
            .map(|s| s.style)
            .unwrap_or_else(|| NavTransitionStyle::default_for_platform(rosace_core::use_platform()));
        Self {
            atom,
            previous,
            transition,
            style,
        }
    }

    /// Override the transition style regardless of the platform default or
    /// the app's theme `ext` value — the highest-priority source, same
    /// shape as `ScrollView::physics(...)`.
    pub fn transition_style(mut self, kind: TransitionStyleKind) -> Self {
        self.style = kind;
        self
    }

    fn resolved_style(&self, direction: SlideDirection) -> TransitionStyle {
        match self.style {
            TransitionStyleKind::None => TransitionStyle::None,
            TransitionStyleKind::Slide => TransitionStyle::Slide(direction),
            TransitionStyleKind::Fade => TransitionStyle::Fade,
        }
    }

    /// Push a new screen onto the stack. Triggers a component rebuild and,
    /// unless animations are globally disabled, an enter-from-the-right
    /// slide (or the app/platform's resolved style).
    pub fn push(&self, route: R) {
        self.previous.set(self.current());
        self.atom.update(|s| {
            let mut v = s.clone();
            v.push(route);
            v
        });
        self.trigger_if_enabled(self.resolved_style(SlideDirection::Left));
    }

    /// Pop the top screen. No-ops at the root. Returns true if a pop occurred.
    /// Triggers the reverse of `push`'s transition (enter from the left).
    pub fn pop(&self) -> bool {
        if self.atom.get().len() > 1 {
            self.previous.set(self.current());
            self.atom.update(|s| {
                let mut v = s.clone();
                v.pop();
                v
            });
            self.trigger_if_enabled(self.resolved_style(SlideDirection::Right));
            true
        } else {
            false
        }
    }

    /// Replace the current screen without adding to history depth. Fades
    /// (never slides — there's no push/pop direction to derive a slide
    /// from) unless the resolved style is `None`.
    pub fn replace(&self, route: R) {
        self.previous.set(self.current());
        self.atom.update(|s| {
            let mut v = s.clone();
            if let Some(last) = v.last_mut() {
                *last = route;
            }
            v
        });
        let style = if self.style == TransitionStyleKind::None { TransitionStyle::None } else { TransitionStyle::Fade };
        self.trigger_if_enabled(style);
    }

    fn trigger_if_enabled(&self, style: TransitionStyle) {
        if !rosace_theme::provider::use_theme().animation.enabled {
            return;
        }
        self.transition.lock().unwrap_or_else(|e| e.into_inner()).trigger(style);
    }

    /// The current (top) screen, or `None` if the stack is somehow empty.
    pub fn current(&self) -> Option<R> {
        self.atom.get().last().cloned()
    }

    /// The screen that was current immediately before the last
    /// push/pop/replace — for building the OUTGOING widget during a
    /// transition. `None` before any navigation.
    pub fn previous(&self) -> Option<R> {
        self.previous.get()
    }

    /// `true` when back navigation is possible (depth > 1).
    pub fn can_pop(&self) -> bool {
        self.atom.get().len() > 1
    }

    /// Stack depth (root is 1).
    pub fn depth(&self) -> usize {
        self.atom.get().len()
    }

    /// The shared transition handle — for `ScreenTransitionView` (or any
    /// custom paint code) to drive `.update(dt)`/`.set_viewport(w, h)`
    /// each frame. Deliberately not generic over `R`, so consumers don't
    /// need to know the app's route enum type.
    pub fn transition_handle(&self) -> Arc<Mutex<ScreenTransition>> {
        Arc::clone(&self.transition)
    }
}

#[cfg(test)]
mod tests {
    // ScreenNav requires a real Context from the runtime; light logic tests
    // are covered by NavigationStack tests. Integration is exercised by
    // phase14_demo at runtime, and by rosace/src/engine.rs's headless
    // FrameEngine tests (D108/Phase 26 Step 3).

    use super::*;

    #[test]
    fn default_for_platform_is_slide_on_ios_macos_android() {
        for p in [rosace_core::Platform::Ios, rosace_core::Platform::MacOs, rosace_core::Platform::Android] {
            assert_eq!(NavTransitionStyle::default_for_platform(p), TransitionStyleKind::Slide);
        }
    }

    #[test]
    fn default_for_platform_is_fade_on_desktop_and_web() {
        for p in [rosace_core::Platform::Windows, rosace_core::Platform::Linux, rosace_core::Platform::Web] {
            assert_eq!(NavTransitionStyle::default_for_platform(p), TransitionStyleKind::Fade);
        }
    }
}
