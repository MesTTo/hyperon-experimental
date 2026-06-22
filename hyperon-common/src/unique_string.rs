use std::sync::Arc;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, LazyLock};
use crate::immutable_string::ImmutableString;

/// The global string interner is sharded by string-hash so concurrent interning
/// (every `Atom::sym` create and drop) does not serialize on one mutex. A single
/// mutex here was the dominant contention in parallel querying (profiled at ~13%
/// `lock_contended` across 8 threads in the metta-on-mork backend). The stored
/// `u64` is the string's hash, so a string maps to the same shard on insert and
/// on drop. `INTERN_SHARDS` is a power of two so the index is a mask.
const INTERN_SHARDS: usize = 64;

static UNIQUE_STRINGS: LazyLock<[Mutex<UniqueStringStorage>; INTERN_SHARDS]> =
    LazyLock::new(|| std::array::from_fn(|_| Mutex::new(UniqueStringStorage::new())));

#[inline]
fn intern_shard(h: u64) -> usize {
    (h as usize) & (INTERN_SHARDS - 1)
}

struct UniqueStringStorage {
    strings: HashMap<Arc<ImmutableString>, u64>,
}

impl UniqueStringStorage {
    fn new() -> Self {
        Self {
            strings: HashMap::new(),
        }
    }

    fn insert(&mut self, key: ImmutableString) -> (Arc<ImmutableString>, u64) {
        let key = Arc::new(key.into());
        match self.strings.entry(key) {
            Entry::Occupied(o) => (o.key().clone(), *o.get()),
            Entry::Vacant(v) => {
                let key = v.key().clone();
                let h = hash(key.as_str());
                v.insert(h);
                (key, h)
            }
        }
    }

    fn remove(&mut self, key: &Arc<ImmutableString>) {
        self.strings.remove(key);
    }
}

#[allow(private_interfaces)]
#[derive(Debug, Clone, Eq)]
pub enum UniqueString {
    Const(&'static str),
    Store(Arc<ImmutableString>, u64),
}

impl UniqueString {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Store(rc, _hash) => rc.as_str(),
            Self::Const(s) => s,
        }
    }

    #[inline]
    fn new_store(value: ImmutableString) -> Self {
        let shard = intern_shard(hash(value.as_str()));
        let (s, h) = UNIQUE_STRINGS[shard].lock().unwrap().insert(value);
        Self::Store(s, h)
    }
}

impl std::fmt::Display for UniqueString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl PartialEq for UniqueString {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Store(a, _), Self::Store(b, _)) => Arc::as_ptr(a) == Arc::as_ptr(b),
            (Self::Const(a), Self::Const(b)) => *a == *b,
            _ => self.as_str() == other.as_str(),
        }
    }
}

#[inline]
fn hash(s: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

impl Hash for UniqueString {
    fn hash<H>(&self, state: &mut H) where H: Hasher {
        match self {
            Self::Const(s) => state.write_u64(hash(s)),
            Self::Store(_rc, hash) => state.write_u64(*hash),
        }
    }
}

impl From<ImmutableString> for UniqueString {
    fn from(value: ImmutableString) -> Self {
        Self::new_store(value)
    }
}

impl<I: Into<String>> From<I> for UniqueString {
    fn from(value: I) -> Self {
        Self::new_store(Into::<String>::into(value).into())
    }
}

impl Drop for UniqueString {
    fn drop(&mut self) {
        match self {
            Self::Store(rc, hash) => {
                let lock = UNIQUE_STRINGS[intern_shard(*hash)].lock();
                // one instance is inside the storage and second one is inside self
                if Arc::strong_count(rc) == 2 {
                    lock.unwrap().remove(rc);
                }
            },
            Self::Const(_) => {},
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::hint::black_box;
    use std::collections::HashMap;

    // unique_string_drop test requires exclusive access to the UNIQUE_STRINGS
    static SEQUENTIAL_RUN: Mutex<()> = Mutex::new(());

    #[test]
    fn unique_string_drop() {
        let _lock = SEQUENTIAL_RUN.lock();

        {
            let a = UniqueString::from("unique_string_drop");
            let b = UniqueString::from("unique_string_drop");
            let count: usize = UNIQUE_STRINGS.iter().map(|s| s.lock().unwrap().strings.len()).sum();
            assert_eq!(count, 1);
            black_box(a);
            black_box(b);
        }
        let count: usize = UNIQUE_STRINGS.iter().map(|s| s.lock().unwrap().strings.len()).sum();
        assert_eq!(count, 0);
    }

    #[test]
    fn unique_string_equal() {
        let _lock = SEQUENTIAL_RUN.lock();

        let store_a = UniqueString::from("unique_string_equal");
        let store_b = UniqueString::from("unique_string_equal");
        let const_c = UniqueString::Const("unique_string_equal");
        let const_d = UniqueString::Const("unique_string_equal");
        assert_eq!(store_a, store_b);
        assert_eq!(store_a, const_c);
        assert_eq!(const_c, store_a);
        assert_eq!(const_c, const_d);
    }

    #[test]
    fn unique_string_from() {
        let _lock = SEQUENTIAL_RUN.lock();

        let store_a = UniqueString::from("unique_string_from");
        assert!(matches!(store_a, UniqueString::Store(_, _)));
        let store_b = UniqueString::from(String::from("unique_string_from"));
        assert!(matches!(store_b, UniqueString::Store(_, _)));
    }

    #[test]
    fn unique_string_as_a_key() {
        let _lock = SEQUENTIAL_RUN.lock();

        assert_eq!(UniqueString::Const("text"), UniqueString::new_store("text".into()));

        let mut map = HashMap::new();
        map.insert(UniqueString::Const("key"), 42);
        assert_eq!(map.get(&UniqueString::Const("key")), Some(&42));
        assert_eq!(map.get(&UniqueString::new_store("key".into())), Some(&42));
    }
}
