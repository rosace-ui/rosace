//! On-disk key-value persistence over embedded SQLite (D114/Phase 31
//! Step 1) — the store `#[persist(permanent)]` atoms write through.
//!
//! Deliberately minimal (see `PHASE_31.md`'s Out of Scope): string key →
//! bytes value, `get`/`set`/`delete`. NOT a general query/ORM surface —
//! an app wanting relational local data is a different, future feature.
//!
//! A thin new crate rather than a dependency inside `rosace-state`, so
//! state's footprint (`trace` only) stays unchanged for apps that never
//! persist anything.
//!
//! # Why SQLite (and `bundled`)
//! SQLite IS the platform-native store — iOS and Android both ship
//! `libsqlite3` and every app links it directly; there is no
//! Swift/Kotlin layer to cross (same principle as D113's sockets-direct
//! networking). `bundled` compiles our own copy so every platform runs
//! the identical version instead of whatever the OS shipped.
//!
//! # Web (wasm32)
//! `rusqlite` links C — no wasm build. The whole API exists on wasm but
//! every call returns `Err` with a clear message (the documented
//! named-gap convention D113 established for networking); a
//! localStorage/IndexedDB backend is future work, tracked in
//! `PHASE_31.md` Step 1.

#[cfg(not(target_arch = "wasm32"))]
use rusqlite::Connection;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

/// A key-value store backed by one SQLite file. Cheap to keep open for
/// the app's lifetime; all methods take `&self` (the connection is
/// internally serialized — writes from any thread are safe).
pub struct Storage {
    #[cfg(not(target_arch = "wasm32"))]
    conn: Mutex<Connection>,
}

impl Storage {
    /// Open (creating if absent) the store at `path`. The schema is one
    /// `kv(key TEXT PRIMARY KEY, value BLOB)` table, created on first
    /// open.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let conn = Connection::open(path.as_ref())
            .map_err(|e| format!("open {}: {}", path.as_ref().display(), e))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value BLOB NOT NULL)",
            [],
        )
        .map_err(|e| format!("create kv table: {}", e))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// wasm32: the documented named gap — see the module doc.
    #[cfg(target_arch = "wasm32")]
    pub fn open(_path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        Err("rosace-storage: persistence is not yet implemented on web (wasm32) — see PHASE_31.md Step 1".to_string())
    }

    /// Read a value. `Ok(None)` = key absent (not an error).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare_cached("SELECT value FROM kv WHERE key = ?1")
            .map_err(|e| format!("prepare get: {}", e))?;
        let mut rows = stmt.query([key]).map_err(|e| format!("get {key}: {}", e))?;
        match rows.next().map_err(|e| format!("get {key}: {}", e))? {
            Some(row) => Ok(Some(row.get(0).map_err(|e| format!("get {key}: {}", e))?)),
            None => Ok(None),
        }
    }

    /// Write (insert or overwrite) a value.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set(&self, key: &str, value: &[u8]) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(|e| format!("set {key}: {}", e))?;
        Ok(())
    }

    /// Remove a key (no-op if absent).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn delete(&self, key: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM kv WHERE key = ?1", [key])
            .map_err(|e| format!("delete {key}: {}", e))?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, String> {
        Err("rosace-storage: not implemented on wasm32".to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn set(&self, _key: &str, _value: &[u8]) -> Result<(), String> {
        Err("rosace-storage: not implemented on wasm32".to_string())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn delete(&self, _key: &str) -> Result<(), String> {
        Err("rosace-storage: not implemented on wasm32".to_string())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("rosace_storage_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}_{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn set_get_delete_round_trip() {
        let path = temp_db("round_trip");
        let store = Storage::open(&path).unwrap();
        assert_eq!(store.get("missing").unwrap(), None);
        store.set("k", b"hello").unwrap();
        assert_eq!(store.get("k").unwrap().as_deref(), Some(b"hello".as_slice()));
        store.set("k", b"overwritten").unwrap();
        assert_eq!(store.get("k").unwrap().as_deref(), Some(b"overwritten".as_slice()));
        store.delete("k").unwrap();
        assert_eq!(store.get("k").unwrap(), None);
        let _ = std::fs::remove_file(path);
    }

    /// The Step 1 exit bar: the value survives CLOSING and REOPENING the
    /// connection — real on-disk persistence, not connection-lifetime
    /// memory.
    #[test]
    fn value_survives_close_and_reopen() {
        let path = temp_db("survives_reopen");
        {
            let store = Storage::open(&path).unwrap();
            store.set("session_token", b"abc123").unwrap();
        } // Storage dropped — connection fully closed.

        let reopened = Storage::open(&path).unwrap();
        assert_eq!(
            reopened.get("session_token").unwrap().as_deref(),
            Some(b"abc123".as_slice()),
            "value must survive a full close/reopen cycle"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn binary_values_round_trip_unmangled() {
        let path = temp_db("binary");
        let store = Storage::open(&path).unwrap();
        let blob: Vec<u8> = (0..=255u8).collect();
        store.set("blob", &blob).unwrap();
        assert_eq!(store.get("blob").unwrap().as_deref(), Some(blob.as_slice()));
        let _ = std::fs::remove_file(path);
    }
}

// ── D121: the persist-backend bridge ────────────────────────────────────
// `rosace-core` owns the trait (so core/state stay SQLite-free); this is
// the real implementation `App::launch` installs.
impl rosace_core::PersistBackend for Storage {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        Storage::get(self, key)
    }
    fn set(&self, key: &str, value: &[u8]) -> Result<(), String> {
        Storage::set(self, key, value)
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        Storage::delete(self, key)
    }
}
