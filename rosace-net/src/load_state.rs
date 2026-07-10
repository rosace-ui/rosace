/// The loading state of an async resource.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LoadState<T: Clone> {
    /// Not started.
    #[default]
    Idle,
    /// Request is in flight.
    Loading,
    /// Successfully loaded.
    Loaded(T),
    /// Failed with an error message.
    Failed(String),
}

impl<T: Clone> LoadState<T> {
    pub fn is_idle(&self) -> bool { matches!(self, LoadState::Idle) }
    pub fn is_loading(&self) -> bool { matches!(self, LoadState::Loading) }
    pub fn is_loaded(&self) -> bool { matches!(self, LoadState::Loaded(_)) }
    pub fn is_failed(&self) -> bool { matches!(self, LoadState::Failed(_)) }

    pub fn value(&self) -> Option<&T> {
        if let LoadState::Loaded(v) = self { Some(v) } else { None }
    }

    pub fn error(&self) -> Option<&str> {
        if let LoadState::Failed(e) = self { Some(e) } else { None }
    }

    /// Map the loaded value.
    pub fn map<U: Clone, F: FnOnce(T) -> U>(self, f: F) -> LoadState<U> {
        match self {
            LoadState::Idle => LoadState::Idle,
            LoadState::Loading => LoadState::Loading,
            LoadState::Loaded(v) => LoadState::Loaded(f(v)),
            LoadState::Failed(e) => LoadState::Failed(e),
        }
    }
}


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_state_idle_default() {
        let s: LoadState<Vec<u8>> = LoadState::default();
        assert_eq!(s, LoadState::Idle);
    }

    #[test]
    fn load_state_is_idle() {
        let s: LoadState<u32> = LoadState::Idle;
        assert!(s.is_idle());
        assert!(!s.is_loading());
        assert!(!s.is_loaded());
        assert!(!s.is_failed());
    }

    #[test]
    fn load_state_is_loading() {
        let s: LoadState<u32> = LoadState::Loading;
        assert!(s.is_loading());
        assert!(!s.is_idle());
        assert!(!s.is_loaded());
        assert!(!s.is_failed());
    }

    #[test]
    fn load_state_is_loaded() {
        let s: LoadState<u32> = LoadState::Loaded(42);
        assert!(s.is_loaded());
        assert!(!s.is_idle());
        assert!(!s.is_loading());
        assert!(!s.is_failed());
    }

    #[test]
    fn load_state_is_failed() {
        let s: LoadState<u32> = LoadState::Failed("oops".into());
        assert!(s.is_failed());
        assert!(!s.is_idle());
        assert!(!s.is_loading());
        assert!(!s.is_loaded());
    }

    #[test]
    fn load_state_value_when_loaded() {
        let s: LoadState<u32> = LoadState::Loaded(99);
        assert_eq!(s.value(), Some(&99));
    }

    #[test]
    fn load_state_value_when_not_loaded() {
        let s: LoadState<u32> = LoadState::Loading;
        assert_eq!(s.value(), None);
        let s2: LoadState<u32> = LoadState::Idle;
        assert_eq!(s2.value(), None);
        let s3: LoadState<u32> = LoadState::Failed("err".into());
        assert_eq!(s3.value(), None);
    }

    #[test]
    fn load_state_error_when_failed() {
        let s: LoadState<u32> = LoadState::Failed("connection refused".into());
        assert_eq!(s.error(), Some("connection refused"));
        let ok: LoadState<u32> = LoadState::Loaded(1);
        assert_eq!(ok.error(), None);
    }

    #[test]
    fn load_state_map_loaded() {
        let s: LoadState<u32> = LoadState::Loaded(10);
        let mapped = s.map(|v| v * 2);
        assert_eq!(mapped, LoadState::Loaded(20u32));
    }

    #[test]
    fn load_state_map_idle_stays_idle() {
        let s: LoadState<u32> = LoadState::Idle;
        let mapped = s.map(|v| v * 2);
        assert_eq!(mapped, LoadState::Idle);
    }
}
