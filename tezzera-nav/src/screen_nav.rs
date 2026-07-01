use tezzera_core::Context;
use tezzera_state::Atom;

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
}

impl<R: Clone + Send + Sync + 'static> ScreenNav<R> {
    /// Create a new `ScreenNav` with `initial` as the root screen.
    ///
    /// Must be called unconditionally inside `Component::build()` — it follows
    /// the hook rules (same call-site order each frame).
    pub fn new(ctx: &mut Context, initial: R) -> Self {
        let atom = ctx.state(vec![initial]);
        Self { atom }
    }

    /// Push a new screen onto the stack. Triggers a component rebuild.
    pub fn push(&self, route: R) {
        self.atom.update(|s| {
            let mut v = s.clone();
            v.push(route);
            v
        });
    }

    /// Pop the top screen. No-ops at the root. Returns true if a pop occurred.
    pub fn pop(&self) -> bool {
        if self.atom.get().len() > 1 {
            self.atom.update(|s| {
                let mut v = s.clone();
                v.pop();
                v
            });
            true
        } else {
            false
        }
    }

    /// Replace the current screen without adding to history depth.
    pub fn replace(&self, route: R) {
        self.atom.update(|s| {
            let mut v = s.clone();
            if let Some(last) = v.last_mut() {
                *last = route;
            }
            v
        });
    }

    /// The current (top) screen, or `None` if the stack is somehow empty.
    pub fn current(&self) -> Option<R> {
        self.atom.get().last().cloned()
    }

    /// `true` when back navigation is possible (depth > 1).
    pub fn can_pop(&self) -> bool {
        self.atom.get().len() > 1
    }

    /// Stack depth (root is 1).
    pub fn depth(&self) -> usize {
        self.atom.get().len()
    }
}

#[cfg(test)]
mod tests {
    // ScreenNav requires a real Context from the runtime; light logic tests
    // are covered by NavigationStack tests. Integration is exercised by
    // phase14_demo at runtime.
}
