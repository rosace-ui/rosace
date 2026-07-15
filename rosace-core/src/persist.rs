//! Atom persistence seams (D008 tiers, implemented by D114/D121 —
//! Phase 31 Step 2).
//!
//! `rosace-core` owns only the SEAMS: the [`PersistBackend`] trait and
//! its process-global install slot, plus [`PersistValue`] (bytes
//! round-trip) — so core/state stay SQLite-free. `rosace-storage`
//! implements the backend; `App::launch` installs it pointed at the
//! platform app-data directory. See `Context::state_permanent` for the
//! hook itself.
//!
//! Tier status (honest, per D121): `permanent` is real (this module).
//! `reload`/`session` are no-ops BY CONSTRUCTION today — atoms already
//! live for the whole process, and hot reload (D102) doesn't exist yet —
//! documented here rather than silently claimed implemented.

use std::sync::OnceLock;

/// The key-value store `permanent` atoms write through — implemented by
/// `rosace-storage` (SQLite) and installed once at app startup.
pub trait PersistBackend: Send + Sync {
    /// `None` = key absent. `Err` = real storage failure.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String>;
    fn set(&self, key: &str, value: &[u8]) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

static BACKEND: OnceLock<Box<dyn PersistBackend>> = OnceLock::new();

/// Install the process-wide persist backend. First install wins (an
/// `App::launch` convention, not a hot-swappable slot); later calls are
/// ignored and return `false`.
pub fn set_persist_backend(backend: Box<dyn PersistBackend>) -> bool {
    BACKEND.set(backend).is_ok()
}

/// The installed backend, if any. `None` = nothing persisted this run
/// (e.g. headless tests that never called `App::launch`) — persistent
/// atoms then behave exactly like plain `ctx.state`.
pub fn persist_backend() -> Option<&'static dyn PersistBackend> {
    BACKEND.get().map(|b| b.as_ref())
}

/// Byte round-trip for persistable values. Deliberately small (D121):
/// primitives + `String` + `Vec<u8>`; a serde-based blanket impl is a
/// named deferral, since a new serialization dependency is its own
/// decision.
pub trait PersistValue: Sized {
    fn to_persist_bytes(&self) -> Vec<u8>;
    /// `None` = bytes don't decode (e.g. the stored type changed) — the
    /// caller falls back to the default rather than panicking on stale
    /// data.
    fn from_persist_bytes(bytes: &[u8]) -> Option<Self>;
}

impl PersistValue for String {
    fn to_persist_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
    fn from_persist_bytes(bytes: &[u8]) -> Option<Self> {
        String::from_utf8(bytes.to_vec()).ok()
    }
}

impl PersistValue for Vec<u8> {
    fn to_persist_bytes(&self) -> Vec<u8> {
        self.clone()
    }
    fn from_persist_bytes(bytes: &[u8]) -> Option<Self> {
        Some(bytes.to_vec())
    }
}

impl PersistValue for bool {
    fn to_persist_bytes(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
    fn from_persist_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0] => Some(false),
            [1] => Some(true),
            _ => None,
        }
    }
}

/// Little-endian fixed-width encodings for the numeric primitives.
macro_rules! persist_le_number {
    ($($t:ty),*) => {$(
        impl PersistValue for $t {
            fn to_persist_bytes(&self) -> Vec<u8> {
                self.to_le_bytes().to_vec()
            }
            fn from_persist_bytes(bytes: &[u8]) -> Option<Self> {
                Some(<$t>::from_le_bytes(bytes.try_into().ok()?))
            }
        }
    )*};
}
persist_le_number!(i32, i64, u32, u64, f32, f64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_round_trip() {
        assert_eq!(String::from_persist_bytes(&"héllo".to_string().to_persist_bytes()), Some("héllo".to_string()));
        assert_eq!(bool::from_persist_bytes(&true.to_persist_bytes()), Some(true));
        assert_eq!(i64::from_persist_bytes(&(-42i64).to_persist_bytes()), Some(-42));
        assert_eq!(f64::from_persist_bytes(&(1.5f64).to_persist_bytes()), Some(1.5));
        assert_eq!(u32::from_persist_bytes(&7u32.to_persist_bytes()), Some(7));
    }

    #[test]
    fn stale_bytes_decode_to_none_not_panic() {
        assert_eq!(i32::from_persist_bytes(b"way-too-long-for-i32"), None);
        assert_eq!(bool::from_persist_bytes(&[9]), None);
        assert_eq!(String::from_persist_bytes(&[0xFF, 0xFE]), None);
    }
}
