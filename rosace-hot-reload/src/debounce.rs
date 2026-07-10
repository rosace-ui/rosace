use std::time::{Duration, Instant};

/// Collapses rapid successive events into one.
pub struct Debouncer {
    window: Duration,
    last_trigger: Option<Instant>,
}

impl Debouncer {
    pub fn new(window: Duration) -> Self {
        Self { window, last_trigger: None }
    }

    /// Returns true if enough time has passed since the last accepted event.
    pub fn should_emit(&mut self) -> bool {
        let now = Instant::now();
        match self.last_trigger {
            Some(last) if now.duration_since(last) < self.window => false,
            _ => {
                self.last_trigger = Some(now);
                true
            }
        }
    }

    /// Reset the debounce timer.
    pub fn reset(&mut self) {
        self.last_trigger = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn debouncer_emits_first_event() {
        let mut d = Debouncer::new(Duration::from_millis(100));
        assert!(d.should_emit(), "first event should always emit");
    }

    #[test]
    fn debouncer_suppresses_rapid_second() {
        let mut d = Debouncer::new(Duration::from_millis(100));
        assert!(d.should_emit());
        // immediately call again — should be suppressed
        assert!(!d.should_emit(), "rapid second event must be suppressed");
    }

    #[test]
    fn debouncer_emits_after_window_expires() {
        let window = Duration::from_millis(30);
        let mut d = Debouncer::new(window);
        assert!(d.should_emit());
        // wait longer than window
        thread::sleep(Duration::from_millis(50));
        assert!(d.should_emit(), "should emit after window expires");
    }

    #[test]
    fn debouncer_reset_allows_next_emit() {
        let mut d = Debouncer::new(Duration::from_millis(500));
        assert!(d.should_emit());
        assert!(!d.should_emit(), "suppressed before reset");
        d.reset();
        assert!(d.should_emit(), "should emit after reset");
    }

    #[test]
    fn debouncer_zero_window_always_emits() {
        let mut d = Debouncer::new(Duration::from_millis(0));
        assert!(d.should_emit());
        assert!(d.should_emit(), "zero window — every call should emit");
        assert!(d.should_emit());
    }
}
