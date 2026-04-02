use std::collections::HashMap;
use core::sync::atomic::Ordering;

use crate::error::rt_error;
use crate::memory::{airl_value_retain, airl_value_release};
use crate::value::{rt_bool, rt_int, rt_list, rt_map, rt_nil, rt_str, RtData, RtValue};

/// Return an empty map.
#[no_mangle]
pub extern "C" fn airl_map_new() -> *mut RtValue {
    rt_map(HashMap::new())
}

/// Build a map from a flat alternating list: ["k1" v1 "k2" v2 ...].
/// Keys must be Str values. Values are retained.
#[no_mangle]
pub extern "C" fn airl_map_from(pairs: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*pairs };
    match &v.data {
        RtData::List { .. } => {
            let items = crate::list::list_items(&v.data);
            if items.len() % 2 != 0 {
                rt_error("airl_map_from: list must have even length (alternating key-value pairs)");
            }
            let mut map: HashMap<String, *mut RtValue> = HashMap::new();
            let mut i = 0;
            while i < items.len() {
                let key_ptr = items[i];
                let val_ptr = items[i + 1];
                let key = unsafe { &*key_ptr };
                let k = match &key.data {
                    RtData::Str(s) => s.clone(),
                    _ => rt_error("airl_map_from: key must be a Str"),
                };
                airl_value_retain(val_ptr);
                map.insert(k, val_ptr);
                i += 2;
            }
            rt_map(map)
        }
        _ => rt_error("airl_map_from: argument must be a List"),
    }
}

/// Look up a key (must be Str). Returns the value (retained) or nil if missing.
#[no_mangle]
pub extern "C" fn airl_map_get(m: *mut RtValue, key: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    let key_v = unsafe { &*key };
    let k = match &key_v.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("airl_map_get: key must be a Str"),
    };
    match &map_v.data {
        RtData::Map(map) => match map.get(k) {
            Some(&val) => {
                airl_value_retain(val);
                val
            }
            None => rt_nil(),
        },
        _ => rt_error("airl_map_get: first argument must be a Map"),
    }
}

/// Look up a key. Returns the value (retained) or default (retained) if missing.
#[no_mangle]
pub extern "C" fn airl_map_get_or(
    m: *mut RtValue,
    key: *mut RtValue,
    default: *mut RtValue,
) -> *mut RtValue {
    let map_v = unsafe { &*m };
    let key_v = unsafe { &*key };
    let k = match &key_v.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("airl_map_get_or: key must be a Str"),
    };
    match &map_v.data {
        RtData::Map(map) => match map.get(k) {
            Some(&val) => {
                airl_value_retain(val);
                val
            }
            None => {
                airl_value_retain(default);
                default
            }
        },
        _ => rt_error("airl_map_get_or: first argument must be a Map"),
    }
}

/// Return a new map with key→val inserted. The original map is unchanged
/// (unless rc == 1, in which case the map is mutated in place as a COW
/// optimisation — O(1) instead of O(N)).
/// All existing values are retained in the new map; val is retained.
#[no_mangle]
pub extern "C" fn airl_map_set(
    m: *mut RtValue,
    key: *mut RtValue,
    val: *mut RtValue,
) -> *mut RtValue {
    let key_v = unsafe { &*key };
    let k_str = match &key_v.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("airl_map_set: key must be a Str"),
    };

    let v = unsafe { &mut *m };
    // COW fast path: sole owner → mutate in place (O(1) instead of O(N))
    // SEC-18: Use atomic load with Acquire ordering to prevent race conditions
    if v.rc.load(Ordering::Acquire) == 1 {
        match &mut v.data {
            RtData::Map(map) => {
                // Avoid key allocation when key already exists — update value in place
                airl_value_retain(val);
                if let Some(slot) = map.get_mut(k_str) {
                    airl_value_release(*slot);
                    *slot = val;
                } else {
                    map.insert(k_str.to_string(), val);
                }
                airl_value_retain(m); // caller expects +1 ref on returned map
                return m;
            }
            _ => rt_error("airl_map_set: first argument must be a Map"),
        }
    }

    // rc > 1: clone as before (existing logic, unchanged)
    match &v.data {
        RtData::Map(map) => {
            let mut new_map: HashMap<String, *mut RtValue> = HashMap::with_capacity(map.len() + 1);
            for (existing_key, &existing_val) in map {
                airl_value_retain(existing_val);
                new_map.insert(existing_key.clone(), existing_val);
            }
            if let Some(&old_val) = new_map.get(k_str) {
                airl_value_release(old_val);
            }
            airl_value_retain(val);
            new_map.insert(k_str.to_string(), val);
            rt_map(new_map)
        }
        _ => rt_error("airl_map_set: first argument must be a Map"),
    }
}

/// Return true if the map contains the given key.
#[no_mangle]
pub extern "C" fn airl_map_has(m: *mut RtValue, key: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    let key_v = unsafe { &*key };
    let k = match &key_v.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("airl_map_has: key must be a Str"),
    };
    match &map_v.data {
        RtData::Map(map) => rt_bool(map.contains_key(k)),
        _ => rt_error("airl_map_has: first argument must be a Map"),
    }
}

/// Remove a key from the map. When rc == 1 (sole owner), the key is removed
/// in place and the removed value is released. When rc > 1, returns a new map
/// without the key (the removed value is not retained in the copy).
#[no_mangle]
pub extern "C" fn airl_map_remove(m: *mut RtValue, key: *mut RtValue) -> *mut RtValue {
    let key_v = unsafe { &*key };
    // Borrow as &str — HashMap::remove accepts &str via Borrow<str>.
    // (airl_map_set clones to an owned String because HashMap::insert requires it.)
    let k = match &key_v.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("airl_map_remove: key must be a Str"),
    };

    let v = unsafe { &mut *m };
    // COW fast path: sole owner → mutate in place
    // SEC-18: Use atomic load with Acquire ordering to prevent race conditions
    if v.rc.load(Ordering::Acquire) == 1 {
        match &mut v.data {
            RtData::Map(map) => {
                if let Some(old_val) = map.remove(k) {
                    airl_value_release(old_val);
                }
                airl_value_retain(m); // caller expects +1 ref
                return m;
            }
            _ => rt_error("airl_map_remove: first argument must be a Map"),
        }
    }

    // rc > 1: clone without the removed key (existing logic, unchanged)
    match &v.data {
        RtData::Map(map) => {
            let mut new_map: HashMap<String, *mut RtValue> = HashMap::with_capacity(map.len());
            for (existing_key, &existing_val) in map {
                if existing_key.as_str() == k {
                    continue;
                }
                airl_value_retain(existing_val);
                new_map.insert(existing_key.clone(), existing_val);
            }
            rt_map(new_map)
        }
        _ => rt_error("airl_map_remove: first argument must be a Map"),
    }
}

/// Return a list of key strings (unordered — AIRL maps are semantically unordered).
#[no_mangle]
pub extern "C" fn airl_map_keys(m: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    match &map_v.data {
        RtData::Map(map) => {
            let items: Vec<*mut RtValue> = map.keys().map(|k| rt_str(k.clone())).collect();
            rt_list(items)
        }
        _ => rt_error("airl_map_keys: argument must be a Map"),
    }
}

/// Return a list of values (unordered — AIRL maps are semantically unordered).
/// Each value is retained.
#[no_mangle]
pub extern "C" fn airl_map_values(m: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    match &map_v.data {
        RtData::Map(map) => {
            let items: Vec<*mut RtValue> = map
                .values()
                .map(|&val| {
                    airl_value_retain(val);
                    val
                })
                .collect();
            rt_list(items)
        }
        _ => rt_error("airl_map_values: argument must be a Map"),
    }
}

/// Return the number of entries as an Int.
#[no_mangle]
pub extern "C" fn airl_map_size(m: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    match &map_v.data {
        RtData::Map(map) => rt_int(map.len() as i64),
        _ => rt_error("airl_map_size: argument must be a Map"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_list, rt_str};

    // Helper: create a key RtValue (Str). Caller must release.
    fn mk_key(s: &str) -> *mut RtValue {
        rt_str(s.to_string())
    }

    #[test]
    fn map_new_is_empty() {
        unsafe {
            let m = airl_map_new();
            let sz = airl_map_size(m);
            assert_eq!((*sz).as_int(), 0);
            airl_value_release(sz);
            airl_value_release(m);
        }
    }

    #[test]
    fn map_set_and_get_roundtrip() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("x");
            let v = rt_int(42);
            let m2 = airl_map_set(m, k, v);

            let k2 = mk_key("x");
            let got = airl_map_get(m2, k2);
            assert_eq!((*got).as_int(), 42);

            airl_value_release(got);
            airl_value_release(k2);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m);
            airl_value_release(m2);
        }
    }

    #[test]
    fn map_get_missing_returns_nil() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("missing");
            let got = airl_map_get(m, k);
            assert_eq!((*got).tag, crate::value::TAG_NIL);
            airl_value_release(got);
            airl_value_release(k);
            airl_value_release(m);
        }
    }

    #[test]
    fn map_get_or_with_default() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("absent");
            let default = rt_int(99);
            let got = airl_map_get_or(m, k, default);
            assert_eq!((*got).as_int(), 99);
            airl_value_release(got);
            airl_value_release(default);
            airl_value_release(k);
            airl_value_release(m);
        }
    }

    #[test]
    fn map_get_or_present_key() {
        unsafe {
            let m = airl_map_new();
            let k1 = mk_key("a");
            let v = rt_int(7);
            let m2 = airl_map_set(m, k1, v);

            let k2 = mk_key("a");
            let default = rt_int(0);
            let got = airl_map_get_or(m2, k2, default);
            assert_eq!((*got).as_int(), 7);

            airl_value_release(got);
            airl_value_release(default);
            airl_value_release(k2);
            airl_value_release(v);
            airl_value_release(k1);
            airl_value_release(m);
            airl_value_release(m2);
        }
    }

    #[test]
    fn map_has_true_and_false() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("key");
            let v = rt_int(1);
            let m2 = airl_map_set(m, k, v);

            let k_present = mk_key("key");
            let k_absent = mk_key("nope");
            let has_present = airl_map_has(m2, k_present);
            let has_absent = airl_map_has(m2, k_absent);
            assert!((*has_present).as_bool());
            assert!(!(*has_absent).as_bool());

            airl_value_release(has_present);
            airl_value_release(has_absent);
            airl_value_release(k_present);
            airl_value_release(k_absent);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m);
            airl_value_release(m2);
        }
    }

    #[test]
    fn map_remove() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("a");
            let v = rt_int(5);
            let m2 = airl_map_set(m, k, v);

            let k_rm = mk_key("a");
            let m3 = airl_map_remove(m2, k_rm);

            let k_check = mk_key("a");
            let got = airl_map_get(m3, k_check);
            assert_eq!((*got).tag, crate::value::TAG_NIL);

            let sz = airl_map_size(m3);
            assert_eq!((*sz).as_int(), 0);

            airl_value_release(got);
            airl_value_release(sz);
            airl_value_release(k_check);
            airl_value_release(k_rm);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m);
            airl_value_release(m2);
            airl_value_release(m3);
        }
    }

    #[test]
    fn map_keys_contains_all() {
        unsafe {
            let m = airl_map_new();
            let kb = mk_key("b");
            let vb = rt_int(2);
            let m2 = airl_map_set(m, kb, vb);
            let ka = mk_key("a");
            let va = rt_int(1);
            let m3 = airl_map_set(m2, ka, va);

            let keys = airl_map_keys(m3);
            match &(*keys).data {
                RtData::List { .. } => {
                    let items = crate::list::list_items(&(*keys).data);
                    assert_eq!(items.len(), 2);
                    // Order is unspecified; collect and sort for assertion
                    let mut key_strs: Vec<&str> = items.iter().map(|&p| (*p).as_str()).collect();
                    key_strs.sort();
                    assert_eq!(key_strs, vec!["a", "b"]);
                }
                _ => panic!("expected list"),
            }

            airl_value_release(keys);
            airl_value_release(va);
            airl_value_release(ka);
            airl_value_release(vb);
            airl_value_release(kb);
            airl_value_release(m);
            airl_value_release(m2);
            airl_value_release(m3);
        }
    }

    #[test]
    fn map_values_contains_all() {
        unsafe {
            let m = airl_map_new();
            let kb = mk_key("b");
            let vb = rt_int(20);
            let m2 = airl_map_set(m, kb, vb);
            let ka = mk_key("a");
            let va = rt_int(10);
            let m3 = airl_map_set(m2, ka, va);

            let vals = airl_map_values(m3);
            match &(*vals).data {
                RtData::List { .. } => {
                    let items = crate::list::list_items(&(*vals).data);
                    assert_eq!(items.len(), 2);
                    // Order is unspecified; collect and sort for assertion
                    let mut val_ints: Vec<i64> = items.iter().map(|&p| (*p).as_int()).collect();
                    val_ints.sort();
                    assert_eq!(val_ints, vec![10, 20]);
                }
                _ => panic!("expected list"),
            }

            airl_value_release(vals);
            airl_value_release(va);
            airl_value_release(ka);
            airl_value_release(vb);
            airl_value_release(kb);
            airl_value_release(m);
            airl_value_release(m2);
            airl_value_release(m3);
        }
    }

    #[test]
    fn map_from_pairs() {
        unsafe {
            // Build flat alternating list: ["x" 1 "y" 2]
            let k1 = rt_str("x".to_string());
            let v1 = rt_int(1);
            let k2 = rt_str("y".to_string());
            let v2 = rt_int(2);
            // rt_list takes ownership (rc stays 1 each)
            let pairs = rt_list(vec![k1, v1, k2, v2]);

            let map = airl_map_from(pairs);

            let gk1 = mk_key("x");
            let got1 = airl_map_get(map, gk1);
            assert_eq!((*got1).as_int(), 1);

            let gk2 = mk_key("y");
            let got2 = airl_map_get(map, gk2);
            assert_eq!((*got2).as_int(), 2);

            let sz = airl_map_size(map);
            assert_eq!((*sz).as_int(), 2);

            airl_value_release(sz);
            airl_value_release(got1);
            airl_value_release(got2);
            airl_value_release(gk1);
            airl_value_release(gk2);
            airl_value_release(pairs);
            airl_value_release(map);
        }
    }

    #[test]
    fn map_size() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("z");
            let v = rt_int(9);
            let m2 = airl_map_set(m, k, v);

            let sz = airl_map_size(m2);
            assert_eq!((*sz).as_int(), 1);

            airl_value_release(sz);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m);
            airl_value_release(m2);
        }
    }

    #[test]
    fn map_set_overwrites_key() {
        unsafe {
            let m = airl_map_new();
            let k1 = mk_key("k");
            let v1 = rt_int(1);
            let m2 = airl_map_set(m, k1, v1);

            let k2 = mk_key("k");
            let v2 = rt_int(2);
            let m3 = airl_map_set(m2, k2, v2);

            let k3 = mk_key("k");
            let got = airl_map_get(m3, k3);
            assert_eq!((*got).as_int(), 2);

            let sz = airl_map_size(m3);
            assert_eq!((*sz).as_int(), 1);

            airl_value_release(got);
            airl_value_release(sz);
            airl_value_release(k3);
            airl_value_release(v2);
            airl_value_release(k2);
            airl_value_release(v1);
            airl_value_release(k1);
            airl_value_release(m);
            airl_value_release(m2);
            airl_value_release(m3);
        }
    }

    #[test]
    fn map_set_cow_returns_same_pointer() {
        unsafe {
            let m = airl_map_new(); // rc=1
            let k = mk_key("a");
            let v = rt_int(10);
            let m2 = airl_map_set(m, k, v);
            // COW: sole owner, should mutate in place
            assert_eq!(m2, m, "map_set with rc=1 should return the same pointer");
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m2);
            airl_value_release(m);
        }
    }

    #[test]
    fn map_set_clone_when_shared() {
        unsafe {
            let m = airl_map_new();
            airl_value_retain(m); // rc=2
            let k = mk_key("b");
            let v = rt_int(20);
            let m2 = airl_map_set(m, k, v);
            // Shared: should clone
            assert_ne!(m2, m, "map_set with rc>1 should return a different pointer");
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m2);
            airl_value_release(m); // release the extra retain
            airl_value_release(m); // release original
        }
    }

    #[test]
    fn map_remove_cow_returns_same_pointer() {
        unsafe {
            let m = airl_map_new(); // rc=1
            let k = mk_key("a");
            let v = rt_int(5);
            let m2 = airl_map_set(m, k, v);
            // m2 == m (COW path), map_set retained m so rc=2 now.
            // Release once to get rc back to 1 for the remove COW test.
            airl_value_release(m2);
            let k_rm = mk_key("a");
            let m3 = airl_map_remove(m, k_rm);
            assert_eq!(m3, m, "map_remove with rc=1 should return the same pointer");
            airl_value_release(k_rm);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m3);
            airl_value_release(m);
        }
    }

    #[test]
    fn map_remove_clone_when_shared() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("c");
            let v = rt_int(30);
            let m2 = airl_map_set(m, k, v);
            // m2 == m (COW path, rc was 1). Now retain to make rc>1.
            airl_value_retain(m2);
            let k_rm = mk_key("c");
            let m3 = airl_map_remove(m2, k_rm);
            assert_ne!(m3, m2, "map_remove with rc>1 should return a different pointer");
            airl_value_release(k_rm);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m3);
            airl_value_release(m2); // release extra retain
            airl_value_release(m2); // release map_set retain
            airl_value_release(m);  // release original
        }
    }
}
