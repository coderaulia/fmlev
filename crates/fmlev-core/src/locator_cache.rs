//! Process-wide cache of where native records were found, so a full-memory scan runs once per record
//! instead of on every call (several callers hold the global state lock while locating a record).
//!
//! A cached record is only reused after the caller's own identity check (`still_valid`, the same
//! validation an edit performs) passes again; otherwise the scan runs and replaces it. The cache never
//! decides anything by address alone, and it is keyed by process id, so a restarted game starts empty.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::error::Result;

/// Entries kept before the cache is dropped as a whole (it refills on demand).
const MAX_ENTRIES: usize = 8192;

type Key = (&'static str, i32, i32);
type Store = Mutex<HashMap<Key, Box<dyn Any + Send>>>;

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The cached record for `(kind, pid, id)` if it still validates, else the result of `scan` (cached on
/// success). `scan` runs without the cache lock held.
pub fn cached<T: Clone + Send + 'static>(
    kind: &'static str,
    pid: i32,
    id: i32,
    still_valid: impl FnOnce(&T) -> bool,
    scan: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let key = (kind, pid, id);
    let hit = store().lock().ok().and_then(|map| map.get(&key).and_then(|value| value.downcast_ref::<T>().cloned()));
    if let Some(record) = hit {
        if still_valid(&record) {
            return Ok(record);
        }
        if let Ok(mut map) = store().lock() {
            map.remove(&key);
        }
    }
    let record = scan()?;
    if let Ok(mut map) = store().lock() {
        if map.len() >= MAX_ENTRIES {
            map.clear();
        }
        map.insert(key, Box::new(record.clone()));
    }
    Ok(record)
}

/// Drops every cached record of a process (attach to a new game, rebuild, or explicit reset).
pub fn clear_pid(pid: i32) {
    if let Ok(mut map) = store().lock() {
        map.retain(|(_, cached_pid, _), _| *cached_pid != pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use std::cell::Cell;

    #[test]
    fn scan_runs_once_while_the_record_still_validates() {
        let scans = Cell::new(0);
        let scan = || {
            scans.set(scans.get() + 1);
            Ok(41u64 + scans.get())
        };
        assert_eq!(cached("test_once", 7001, 1, |_| true, scan).unwrap(), 42);
        assert_eq!(cached("test_once", 7001, 1, |_| true, scan).unwrap(), 42);
        assert_eq!(scans.get(), 1, "second call must come from the cache");
    }

    #[test]
    fn a_record_that_no_longer_validates_is_rescanned_and_replaced() {
        let scans = Cell::new(0);
        let scan = || {
            scans.set(scans.get() + 1);
            Ok(scans.get() * 10)
        };
        assert_eq!(cached("test_stale", 7002, 1, |_: &i32| true, scan).unwrap(), 10);
        assert_eq!(cached("test_stale", 7002, 1, |_| false, scan).unwrap(), 20, "stale record is replaced");
        assert_eq!(cached("test_stale", 7002, 1, |_| true, scan).unwrap(), 20);
        assert_eq!(scans.get(), 2);
    }

    #[test]
    fn failures_are_not_cached_and_keys_do_not_collide() {
        let fail = || -> Result<u8> { Err(CoreError::InvalidValue("nope".into())) };
        assert!(cached("test_fail", 7003, 1, |_| true, fail).is_err());
        assert_eq!(cached("test_fail", 7003, 1, |_| true, || Ok(5u8)).unwrap(), 5, "an earlier failure left nothing behind");
        // Same id, different kind / pid / type are separate entries.
        assert_eq!(cached("test_fail_other", 7003, 1, |_| true, || Ok(6u8)).unwrap(), 6);
        assert_eq!(cached("test_fail", 7004, 1, |_| true, || Ok(7u8)).unwrap(), 7);
        assert_eq!(cached("test_fail", 7003, 1, |_| true, || Ok(9u8)).unwrap(), 5);
    }

    #[test]
    fn clear_pid_drops_only_that_process() {
        assert_eq!(cached("test_clear", 7005, 1, |_| true, || Ok(1u8)).unwrap(), 1);
        assert_eq!(cached("test_clear", 7006, 1, |_| true, || Ok(2u8)).unwrap(), 2);
        clear_pid(7005);
        assert_eq!(cached("test_clear", 7005, 1, |_| true, || Ok(3u8)).unwrap(), 3, "rescanned after clear");
        assert_eq!(cached("test_clear", 7006, 1, |_| true, || Ok(4u8)).unwrap(), 2, "other pid untouched");
    }
}
