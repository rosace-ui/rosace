use std::sync::{Arc, Mutex};

use rosace_state::Atom;
#[cfg(debug_assertions)]
use rosace_trace::event::{Route as TraceRoute, Transition};

use crate::history::KeepAliveRegistry;
use crate::route::Route;

/// The navigation stack for a single navigator.
///
/// Backed by an [`Atom`] so the UI rebuilds whenever navigation occurs (D026).
/// Each navigator has its own independent history stack (D027).
///
/// The keep-alive registry (D030) retains routes that have been navigated away
/// from so the UI layer can preserve their mounted state until the stack is
/// cleared via [`reset_to`].
///
/// [`reset_to`]: NavigationStack::reset_to
pub struct NavigationStack<R: Route> {
    /// Current stack of routes. The last element is the current screen.
    stack: Atom<Vec<R>>,
    /// Keep-alive registry — routes removed from the stack stay here until
    /// the stack is reset.
    keep_alive: Arc<Mutex<KeepAliveRegistry<R>>>,
}

impl<R: Route> NavigationStack<R> {
    /// Creates a new stack with `root` as the only entry.
    pub fn new(root: R) -> Self {
        let keep_alive = Arc::new(Mutex::new(KeepAliveRegistry::new()));
        Self {
            stack: rosace_state::use_atom(vec![root]),
            keep_alive,
        }
    }

    /// Pushes `route` onto the stack, making it the current screen.
    pub fn push(&self, route: R) {
        #[cfg(debug_assertions)]
        {
            let from = self.current().map(|r| TraceRoute(format!("{:?}", r)));
            let to = TraceRoute(format!("{:?}", route));
            rosace_trace::TRACING_BUS.emit(rosace_trace::event::RosaceTrace::RouteChange {
                from,
                to,
                transition: Transition("push".to_string()),
            });
        }
        self.stack.update(|s| {
            let mut s = s.clone();
            s.push(route);
            s
        });
    }

    /// Pops the current route, returning `true` on success or `false` when
    /// already at the root (the root entry is never removed).
    pub fn pop(&self) -> bool {
        let can_pop = self.stack.get().len() > 1;
        if can_pop {
            let popped = self.stack.get().last().cloned();
            #[cfg(debug_assertions)]
            let below = {
                let s = self.stack.get();
                s.get(s.len().saturating_sub(2)).cloned()
            };
            #[cfg(debug_assertions)]
            {
                let from = popped.as_ref().map(|r| TraceRoute(format!("{:?}", r)));
                let to_route = below
                    .as_ref()
                    .map(|r| format!("{:?}", r))
                    .unwrap_or_default();
                rosace_trace::TRACING_BUS.emit(rosace_trace::event::RosaceTrace::RouteChange {
                    from,
                    to: TraceRoute(to_route),
                    transition: Transition("pop".to_string()),
                });
            }
            if let Some(route) = popped {
                self.keep_alive.lock().unwrap().push(route);
            }
            self.stack.update(|s| {
                let mut s = s.clone();
                s.pop();
                s
            });
        }
        can_pop
    }

    /// Replaces the current route without adding to the history depth.
    pub fn replace(&self, route: R) {
        #[cfg(debug_assertions)]
        {
            let from = self.current().map(|r| TraceRoute(format!("{:?}", r)));
            let to = TraceRoute(format!("{:?}", route));
            rosace_trace::TRACING_BUS.emit(rosace_trace::event::RosaceTrace::RouteChange {
                from,
                to,
                transition: Transition("replace".to_string()),
            });
        }
        self.stack.update(|s| {
            let mut s = s.clone();
            if let Some(last) = s.last_mut() {
                *last = route;
            }
            s
        });
    }

    /// Clears the entire stack and resets to a single `route` as the new root.
    /// The keep-alive registry is also cleared (D030).
    pub fn reset_to(&self, route: R) {
        #[cfg(debug_assertions)]
        {
            let from = self.current().map(|r| TraceRoute(format!("{:?}", r)));
            let to = TraceRoute(format!("{:?}", route));
            rosace_trace::TRACING_BUS.emit(rosace_trace::event::RosaceTrace::RouteChange {
                from,
                to,
                transition: Transition("reset".to_string()),
            });
        }
        self.keep_alive.lock().unwrap().clear();
        self.stack.update(|_| vec![route]);
    }

    /// Returns the current route (top of stack), or `None` if the stack is
    /// somehow empty.
    pub fn current(&self) -> Option<R> {
        self.stack.get().last().cloned()
    }

    /// Number of routes on the stack.
    pub fn depth(&self) -> usize {
        self.stack.get().len()
    }

    /// Returns `true` when back navigation is possible (stack depth > 1).
    pub fn can_go_back(&self) -> bool {
        self.stack.get().len() > 1
    }

    /// Returns a snapshot of the full stack, bottom-to-top.
    pub fn stack(&self) -> Vec<R> {
        self.stack.get()
    }

    /// Returns a clone of the backing atom so Navigator can subscribe to it
    /// via `ctx.state(initial)` on first call.
    pub fn atom(&self) -> rosace_state::Atom<Vec<R>> {
        self.stack.clone()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum Screen {
        Home,
        Detail,
        Settings,
        Profile,
    }
    impl Route for Screen {}

    #[test]
    fn stack_new_has_root() {
        let s = NavigationStack::new(Screen::Home);
        assert_eq!(s.current(), Some(Screen::Home));
        assert_eq!(s.depth(), 1);
    }

    #[test]
    fn stack_push_increases_depth() {
        let s = NavigationStack::new(Screen::Home);
        s.push(Screen::Detail);
        assert_eq!(s.depth(), 2);
        assert_eq!(s.current(), Some(Screen::Detail));
    }

    #[test]
    fn stack_pop_returns_to_previous() {
        let s = NavigationStack::new(Screen::Home);
        s.push(Screen::Detail);
        let result = s.pop();
        assert!(result);
        assert_eq!(s.current(), Some(Screen::Home));
        assert_eq!(s.depth(), 1);
    }

    #[test]
    fn stack_pop_at_root_returns_false() {
        let s = NavigationStack::new(Screen::Home);
        let result = s.pop();
        assert!(!result);
        assert_eq!(s.current(), Some(Screen::Home));
    }

    #[test]
    fn stack_replace_changes_current() {
        let s = NavigationStack::new(Screen::Home);
        s.push(Screen::Detail);
        s.replace(Screen::Settings);
        assert_eq!(s.current(), Some(Screen::Settings));
        // Depth must stay the same — replace does not add an entry.
        assert_eq!(s.depth(), 2);
    }

    #[test]
    fn stack_reset_clears_to_new_root() {
        let s = NavigationStack::new(Screen::Home);
        s.push(Screen::Detail);
        s.push(Screen::Settings);
        s.reset_to(Screen::Profile);
        assert_eq!(s.depth(), 1);
        assert_eq!(s.current(), Some(Screen::Profile));
    }

    #[test]
    fn stack_can_go_back_false_at_root() {
        let s = NavigationStack::new(Screen::Home);
        assert!(!s.can_go_back());
    }

    #[test]
    fn stack_can_go_back_true_after_push() {
        let s = NavigationStack::new(Screen::Home);
        s.push(Screen::Detail);
        assert!(s.can_go_back());
    }
}
