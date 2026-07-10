use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct ChangeEvent {
    /// The file that changed.
    pub path: PathBuf,
    /// When the change was detected.
    pub at: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn change_event_has_path_and_time() {
        let now = SystemTime::now();
        let event = ChangeEvent {
            path: PathBuf::from("src/main.rs"),
            at: now,
        };
        assert_eq!(event.path, PathBuf::from("src/main.rs"));
        assert_eq!(event.at, now);
    }

    #[test]
    fn change_event_clone() {
        let now = SystemTime::now();
        let event = ChangeEvent {
            path: PathBuf::from("src/lib.rs"),
            at: now,
        };
        let cloned = event.clone();
        assert_eq!(cloned.path, event.path);
        assert_eq!(cloned.at, event.at);
    }
}
