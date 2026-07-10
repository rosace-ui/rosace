/// Re-exports of shared geometric and identity types from `rosace-trace`.
pub use rosace_trace::event::{AtomId, ComponentId, Location, Point, Rect, Size};

/// A stable identity key used to reconcile elements across rebuilds.
///
/// Keys are local to a sibling list — they do not need to be globally unique.
/// Strings are FNV-1a hashed to `u64`; integers are used directly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key(pub u64);

const FNV_OFFSET: u64 = 14695981039346656037;
const FNV_PRIME:  u64 = 1099511628211;

fn fnv1a(s: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

impl From<&str>  for Key { fn from(s: &str)  -> Key { Key(fnv1a(s)) } }
impl From<String> for Key { fn from(s: String) -> Key { Key(fnv1a(&s)) } }
impl From<u64>   for Key { fn from(n: u64)   -> Key { Key(n) } }
impl From<u32>   for Key { fn from(n: u32)   -> Key { Key(n as u64) } }
impl From<i32>   for Key { fn from(n: i32)   -> Key { Key(n as u64) } }
impl From<usize> for Key { fn from(n: usize) -> Key { Key(n as u64) } }
