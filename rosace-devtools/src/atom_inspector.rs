use std::collections::HashMap;

/// A snapshot of atom values at a point in time.
#[derive(Debug, Default)]
pub struct AtomSnapshot {
    /// Map from atom ID to its string representation and TypeId.
    pub entries: HashMap<u64, AtomEntry>,
}

/// A single atom entry in a snapshot.
#[derive(Debug, Clone)]
pub struct AtomEntry {
    pub atom_id: u64,
    pub type_name: &'static str,
    pub display: String,
}

/// Inspects and records Atom state snapshots with time-travel cursor.
pub struct AtomInspector {
    history: Vec<AtomSnapshot>,
    max_snapshots: usize,
    /// Current time-travel position. `history.len() - 1` = latest frame.
    cursor: usize,
}

impl AtomInspector {
    pub fn new() -> Self {
        Self { history: Vec::new(), max_snapshots: 60, cursor: 0 }
    }

    pub fn max_snapshots(mut self, n: usize) -> Self {
        self.max_snapshots = n;
        self
    }

    /// Record a new snapshot and advance cursor to the latest frame.
    pub fn record(&mut self, snapshot: AtomSnapshot) {
        if self.history.len() >= self.max_snapshots {
            self.history.remove(0);
        }
        self.history.push(snapshot);
        self.cursor = self.history.len().saturating_sub(1);
    }

    // ── Time-travel cursor API ────────────────────────────────────────────

    pub fn cursor(&self) -> usize { self.cursor }

    /// Jump to a specific snapshot by index.
    pub fn travel_to(&mut self, index: usize) {
        if index < self.history.len() { self.cursor = index; }
    }

    /// Step one frame backward (toward older snapshots).
    pub fn step_back(&mut self) { self.cursor = self.cursor.saturating_sub(1); }

    /// Step one frame forward (toward the latest snapshot).
    pub fn step_forward(&mut self) {
        if self.cursor + 1 < self.history.len() { self.cursor += 1; }
    }

    /// Snapshot at the current cursor position.
    pub fn current(&self) -> Option<&AtomSnapshot> { self.history.get(self.cursor) }

    /// Render the snapshot at the cursor as an ASCII table.
    pub fn render_at_cursor(&self) -> String {
        match self.current() {
            None => "┌─ ATOMS ─────────────────────────────────────────\n│  (no snapshot yet)\n└─────────────────────────────────────────────────\n".to_string(),
            Some(snap) => {
                let mut out = format!(
                    "┌─ ATOMS  frame {}/{} ──────────────────────────────\n",
                    self.cursor + 1, self.history.len()
                );
                let mut ids: Vec<u64> = snap.entries.keys().copied().collect();
                ids.sort();
                for id in ids {
                    if let Some(e) = snap.entries.get(&id) {
                        out.push_str(&format!("│  [{:>6}] {} = {}\n", e.atom_id, e.type_name, e.display));
                    }
                }
                out.push_str("└─────────────────────────────────────────────────\n");
                out
            }
        }
    }

    /// How many snapshots are stored.
    pub fn snapshot_count(&self) -> usize {
        self.history.len()
    }

    /// Most recent snapshot.
    pub fn latest(&self) -> Option<&AtomSnapshot> {
        self.history.last()
    }

    /// Travel back n frames (0 = latest).
    pub fn at_frame(&self, offset_from_latest: usize) -> Option<&AtomSnapshot> {
        let idx = self.history.len().checked_sub(1 + offset_from_latest)?;
        self.history.get(idx)
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.history.clear();
    }

    /// Render the latest snapshot as an ASCII table.
    pub fn render_latest(&self) -> String {
        match self.latest() {
            None => "[AtomInspector] No snapshots yet.\n".to_string(),
            Some(snap) => {
                let mut out =
                    String::from("┌─ ATOMS ─────────────────────────────────────────\n");
                let mut ids: Vec<u64> = snap.entries.keys().copied().collect();
                ids.sort();
                for id in ids {
                    if let Some(e) = snap.entries.get(&id) {
                        out.push_str(&format!(
                            "│  [{:>6}] {} = {}\n",
                            e.atom_id, e.type_name, e.display
                        ));
                    }
                }
                out.push_str("└─────────────────────────────────────────────────\n");
                out
            }
        }
    }
}

impl Default for AtomInspector {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: build an AtomEntry from any Debug value.
pub fn atom_entry<T: std::fmt::Debug + 'static>(id: u64, value: &T) -> AtomEntry {
    AtomEntry {
        atom_id: id,
        type_name: std::any::type_name::<T>(),
        display: format!("{:?}", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atom_inspector_new_empty() {
        let inspector = AtomInspector::new();
        assert_eq!(inspector.snapshot_count(), 0);
        assert!(inspector.latest().is_none());
    }

    #[test]
    fn atom_entry_helper_sets_fields() {
        let entry = atom_entry(42, &100u32);
        assert_eq!(entry.atom_id, 42);
        assert_eq!(entry.type_name, "u32");
        assert_eq!(entry.display, "100");
    }

    #[test]
    fn atom_snapshot_insert_and_retrieve() {
        let mut snap = AtomSnapshot::default();
        snap.entries.insert(1, atom_entry(1, &"hello"));
        assert!(snap.entries.contains_key(&1));
        assert_eq!(snap.entries[&1].display, "\"hello\"");
    }

    #[test]
    fn atom_inspector_record_snapshot() {
        let mut inspector = AtomInspector::new();
        let mut snap = AtomSnapshot::default();
        snap.entries.insert(7, atom_entry(7, &42i32));
        inspector.record(snap);
        assert_eq!(inspector.snapshot_count(), 1);
    }

    #[test]
    fn atom_inspector_at_frame_latest() {
        let mut inspector = AtomInspector::new();
        let mut snap = AtomSnapshot::default();
        snap.entries.insert(1, atom_entry(1, &true));
        inspector.record(snap);
        assert!(inspector.at_frame(0).is_some());
        assert_eq!(inspector.at_frame(0).unwrap().entries[&1].display, "true");
    }

    #[test]
    fn atom_inspector_at_frame_offset() {
        let mut inspector = AtomInspector::new();

        let mut snap1 = AtomSnapshot::default();
        snap1.entries.insert(1, atom_entry(1, &10u32));
        inspector.record(snap1);

        let mut snap2 = AtomSnapshot::default();
        snap2.entries.insert(1, atom_entry(1, &20u32));
        inspector.record(snap2);

        // offset 0 = latest (snap2)
        assert_eq!(inspector.at_frame(0).unwrap().entries[&1].display, "20");
        // offset 1 = one before latest (snap1)
        assert_eq!(inspector.at_frame(1).unwrap().entries[&1].display, "10");
        // offset 2 = out of range
        assert!(inspector.at_frame(2).is_none());
    }

    #[test]
    fn atom_inspector_max_snapshots_evicts_old() {
        let mut inspector = AtomInspector::new().max_snapshots(2);
        for i in 0..4u64 {
            let mut snap = AtomSnapshot::default();
            snap.entries.insert(0, atom_entry(0, &i));
            inspector.record(snap);
        }
        assert_eq!(inspector.snapshot_count(), 2);
        // Latest should hold value 3
        assert_eq!(inspector.latest().unwrap().entries[&0].display, "3");
    }
}
