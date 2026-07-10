use crate::route::Route;

/// A single entry in the navigation history.
#[derive(Debug, Clone)]
pub struct HistoryEntry<R: Route> {
    pub route: R,
    /// Monotonic counter value at the time this entry was pushed (not wall time).
    pub sequence: u64,
}

/// Registry of keep-alive screen states.
///
/// Screens stay alive in memory after navigation away, until the stack is
/// cleared (D030). The registry records which routes have been visited so the
/// UI layer can decide to keep their state mounted.
pub struct KeepAliveRegistry<R: Route> {
    entries: Vec<HistoryEntry<R>>,
    sequence: u64,
}

impl<R: Route> KeepAliveRegistry<R> {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            sequence: 0,
        }
    }

    /// Inserts `route` into the registry and returns its sequence number.
    pub fn push(&mut self, route: R) -> u64 {
        self.sequence += 1;
        let seq = self.sequence;
        self.entries.push(HistoryEntry { route, sequence: seq });
        seq
    }

    /// Returns `true` if `route` is present in the registry.
    pub fn contains(&self, route: &R) -> bool {
        self.entries.iter().any(|e| &e.route == route)
    }

    /// Removes all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of entries currently in the registry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the registry has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<R: Route> Default for KeepAliveRegistry<R> {
    fn default() -> Self {
        Self::new()
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
    }
    impl Route for Screen {}

    #[test]
    fn keep_alive_push_increments_sequence() {
        let mut reg: KeepAliveRegistry<Screen> = KeepAliveRegistry::new();
        let s1 = reg.push(Screen::Home);
        let s2 = reg.push(Screen::Detail);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert!(s2 > s1);
    }

    #[test]
    fn keep_alive_contains_pushed_route() {
        let mut reg: KeepAliveRegistry<Screen> = KeepAliveRegistry::new();
        reg.push(Screen::Home);
        assert!(reg.contains(&Screen::Home));
        assert!(!reg.contains(&Screen::Settings));
    }

    #[test]
    fn keep_alive_clear_removes_all() {
        let mut reg: KeepAliveRegistry<Screen> = KeepAliveRegistry::new();
        reg.push(Screen::Home);
        reg.push(Screen::Detail);
        reg.clear();
        assert_eq!(reg.len(), 0);
        assert!(!reg.contains(&Screen::Home));
    }

    #[test]
    fn keep_alive_len_tracks_pushes() {
        let mut reg: KeepAliveRegistry<Screen> = KeepAliveRegistry::new();
        assert_eq!(reg.len(), 0);
        reg.push(Screen::Home);
        assert_eq!(reg.len(), 1);
        reg.push(Screen::Detail);
        assert_eq!(reg.len(), 2);
    }
}
