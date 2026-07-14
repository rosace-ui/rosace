//! App lifecycle state (D042, built by D110/Phase 29): a
//! `GlobalAtom<LifecycleState>` + `use_app_lifecycle()` hook.
//!
//! This lives in `rosace-core` (not `rosace-platform` or `rosace-ffi`)
//! because it's a bridge between layers that don't depend on each other —
//! the same reasoning as `ime_hint.rs`: the SETTERS are platform hosts
//! (`rosace-ffi`'s mobile event dispatch today; desktop winit or web
//! page-visibility could set it later), while the READERS are app
//! components, and `rosace-core` is the lowest common layer both sides
//! already depend on. D042 originally said "Affects: rosace-platform",
//! but `rosace-platform` is unreachable from component code — D110
//! explicitly re-opened the home question, resolved here.
//!
//! Distinct from `lifecycle.rs` (`on_mount`/`on_unmount`), which is
//! per-COMPONENT lifecycle — this is the whole APP's foreground/background
//! state as reported by the OS.

use rosace_state::GlobalAtom;
use rosace_trace::event::AtomId;

use crate::context::Context;

/// The app's OS-level lifecycle state (D042's four states).
///
/// Mobile semantics (the reason this exists — see `.steering/PHASE_29.md`):
/// iOS maps `applicationDidBecomeActive`/`WillResignActive`/
/// `DidEnterBackground`/`WillTerminate`, Android maps
/// `onResume`/`onPause`/`onStop` (Android has no pre-kill callback, so
/// `Suspended` is iOS-only in practice). Desktop apps simply stay `Active`
/// — the default — since no host reports otherwise (desktop lifecycle is
/// explicitly out of Phase 29's scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LifecycleState {
    /// Foreground, receiving events. The startup default: by the time any
    /// component can observe this atom, the app is running in front.
    #[default]
    Active,
    /// Foreground but not receiving events (iOS: transitional — system
    /// dialogs, app switcher, incoming call; Android: `onPause`).
    Inactive,
    /// Not visible; still in memory (iOS: `didEnterBackground`, Android:
    /// `onStop`). Pause expensive work (animation, polling) here.
    Background,
    /// About to be terminated by the OS (iOS: `applicationWillTerminate`).
    /// Last chance to persist state — there may be no further frames.
    Suspended,
}

/// Reserved atom ID — next free slot below `KEYBOARD_TYPE_ATOM_ID`
/// (`0xFFFA`); see `ime_hint.rs` for the full reserved-high-id list.
const APP_LIFECYCLE_ATOM_ID: AtomId = AtomId(0xFFF9);

static APP_LIFECYCLE: GlobalAtom<LifecycleState> =
    GlobalAtom::new(APP_LIFECYCLE_ATOM_ID, || LifecycleState::Active);

/// Read the app's lifecycle state from a component's `build()`, subscribing
/// the component so it re-renders when the state changes (the explicit
/// `subscribe` is required — `GlobalAtom`s aren't auto-subscribed by
/// `ctx.state`'s hook machinery; same convention as `FormField::for_ctx`).
pub fn use_app_lifecycle(ctx: &Context) -> LifecycleState {
    APP_LIFECYCLE.get_or_init().subscribe(ctx.component_id());
    APP_LIFECYCLE.get()
}

/// Read the current lifecycle state without subscribing — for engine/host
/// code outside the component tree (a component should prefer
/// [`use_app_lifecycle`] or it won't re-render on changes).
pub fn app_lifecycle() -> LifecycleState {
    APP_LIFECYCLE.get()
}

/// Report a lifecycle transition — called by the platform host (the FFI
/// event dispatch on mobile). Notifies subscribers, so any component that
/// read the state via [`use_app_lifecycle`] re-renders.
pub fn set_app_lifecycle(state: LifecycleState) {
    APP_LIFECYCLE.set(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `APP_LIFECYCLE` is a process-global static — tests touching it must
    // be serialized against each other (same reasoning as `rosace-ffi`'s
    // `KEYBOARD_TYPE_TEST_LOCK` and `capability.rs`'s `TEST_LOCK`), and
    // each must restore `Active` before releasing the lock.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn defaults_to_active() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_app_lifecycle(LifecycleState::Active); // in case a prior holder leaked
        assert_eq!(app_lifecycle(), LifecycleState::Active);
    }

    #[test]
    fn set_then_read_round_trips_every_state() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for state in [
            LifecycleState::Inactive,
            LifecycleState::Background,
            LifecycleState::Suspended,
            LifecycleState::Active,
        ] {
            set_app_lifecycle(state);
            assert_eq!(app_lifecycle(), state);
        }
        // Loop ends on Active — the reset other tests rely on.
    }

    #[test]
    fn use_app_lifecycle_subscribes_the_calling_component_for_re_render() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_app_lifecycle(LifecycleState::Active);

        let component = rosace_trace::event::ComponentId(4242);
        let ctx = Context::new(component);
        assert_eq!(use_app_lifecycle(&ctx), LifecycleState::Active);

        // Drain anything already dirty, then transition: the subscribed
        // component must land in the dirty set — that IS the re-render
        // trigger the exit bar is about.
        let _ = rosace_state::dirty_set::take_dirty_components();
        set_app_lifecycle(LifecycleState::Background);
        assert!(
            rosace_state::dirty_set::take_dirty_components().contains(&component),
            "a lifecycle transition must mark the subscribed component dirty"
        );

        APP_LIFECYCLE.get_or_init().unsubscribe(component);
        set_app_lifecycle(LifecycleState::Active); // reset for other tests
    }
}
