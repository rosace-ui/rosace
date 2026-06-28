use std::fs;
use std::path::PathBuf;

/// Simple PNG snapshot comparison for visual regression tests.
///
/// Snapshots are stored in `test_snapshots/<name>.png` relative to the
/// current working directory (usually the crate root when running `cargo test`).
pub struct SnapshotAssert {
    dir: PathBuf,
    /// Maximum allowed differing pixels before `assert_snapshot` panics.
    pub threshold: usize,
}

impl SnapshotAssert {
    pub fn new() -> Self {
        Self { dir: PathBuf::from("test_snapshots"), threshold: 0 }
    }

    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = dir.into();
        self
    }

    pub fn with_threshold(mut self, pixels: usize) -> Self {
        self.threshold = pixels;
        self
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.png", name))
    }

    /// Save `png` bytes as the baseline snapshot for `name`.
    pub fn save_snapshot(&self, name: &str, png: &[u8]) {
        fs::create_dir_all(&self.dir)
            .unwrap_or_else(|e| panic!("snapshot dir error: {}", e));
        fs::write(self.path(name), png)
            .unwrap_or_else(|e| panic!("snapshot write error: {}", e));
    }

    /// Compare `png` against the saved baseline. Panics when the diff exceeds
    /// `self.threshold`. Saves the snapshot automatically if no baseline exists.
    pub fn assert_snapshot(&self, name: &str, png: &[u8]) {
        let path = self.path(name);
        if !path.exists() {
            self.save_snapshot(name, png);
            return;
        }
        let baseline = fs::read(&path)
            .unwrap_or_else(|e| panic!("snapshot read error: {}", e));
        let diff = Self::pixel_diff_count(&baseline, png);
        assert!(
            diff <= self.threshold,
            "snapshot '{}' differs by {} pixels (threshold {})",
            name, diff, self.threshold
        );
    }

    /// Count the number of bytes that differ between two PNG-encoded buffers.
    /// When the buffers have different lengths the excess bytes of the longer
    /// one are all counted as differing.
    pub fn pixel_diff_count(a: &[u8], b: &[u8]) -> usize {
        let common = a.len().min(b.len());
        let diff: usize = a[..common].iter().zip(b[..common].iter())
            .filter(|(x, y)| x != y)
            .count();
        diff + a.len().abs_diff(b.len())
    }
}

impl Default for SnapshotAssert {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_diff_identical() {
        let a = vec![1u8, 2, 3, 4];
        assert_eq!(SnapshotAssert::pixel_diff_count(&a, &a), 0);
    }

    #[test]
    fn pixel_diff_one_byte() {
        let a = vec![0u8, 0, 0];
        let b = vec![0u8, 1, 0];
        assert_eq!(SnapshotAssert::pixel_diff_count(&a, &b), 1);
    }

    #[test]
    fn pixel_diff_different_length() {
        let a = vec![0u8, 0];
        let b = vec![0u8, 0, 0, 0];
        assert_eq!(SnapshotAssert::pixel_diff_count(&a, &b), 2);
    }

    #[test]
    fn pixel_diff_all_different() {
        let a = vec![0u8, 0, 0];
        let b = vec![1u8, 1, 1];
        assert_eq!(SnapshotAssert::pixel_diff_count(&a, &b), 3);
    }

    #[test]
    fn snapshot_assert_saves_when_no_baseline() {
        let dir = std::env::temp_dir().join("tezzera_snapshot_test");
        let _ = fs::remove_dir_all(&dir);
        let sa = SnapshotAssert::new().with_dir(&dir);
        let png = vec![0x89u8, 0x50, 0x4E, 0x47]; // fake PNG header
        sa.assert_snapshot("test_no_baseline", &png);
        assert!(dir.join("test_no_baseline.png").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_assert_passes_identical() {
        let dir = std::env::temp_dir().join("tezzera_snapshot_test2");
        let _ = fs::remove_dir_all(&dir);
        let sa = SnapshotAssert::new().with_dir(&dir);
        let png = vec![0x89u8, 0x50, 0x4E, 0x47];
        sa.save_snapshot("identical", &png);
        sa.assert_snapshot("identical", &png); // must not panic
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_assert_with_threshold() {
        let dir = std::env::temp_dir().join("tezzera_snapshot_test3");
        let _ = fs::remove_dir_all(&dir);
        let sa = SnapshotAssert::new().with_dir(&dir).with_threshold(5);
        let baseline = vec![0u8; 10];
        let current  = vec![1u8; 10]; // 10 bytes differ
        sa.save_snapshot("thresh", &baseline);
        // 10 > threshold 5, should panic
        let result = std::panic::catch_unwind(|| {
            let sa2 = SnapshotAssert::new().with_dir(&dir).with_threshold(5);
            sa2.assert_snapshot("thresh", &current);
        });
        assert!(result.is_err(), "expected panic for diff exceeding threshold");
        let _ = fs::remove_dir_all(&dir);
    }
}
