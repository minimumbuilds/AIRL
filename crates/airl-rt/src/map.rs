#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

#[cfg(not(target_os = "airlos"))]
use std::collections::HashMap;
#[cfg(target_os = "airlos")]
use alloc::collections::BTreeMap as HashMap;
use crate::error::rt_error;
use crate::memory::{airl_value_retain, airl_value_release};
use crate::value::{rt_bool, rt_int, rt_list, rt_map, rt_map_at, rt_nil, rt_str, RtData, RtValue};

#[cfg(not(target_os = "airlos"))]
static SITE_MAP_NEW: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
#[cfg(not(target_os = "airlos"))]
static SITE_MAP_FROM: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
#[cfg(not(target_os = "airlos"))]
static SITE_MAP_SET_CLONE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
#[cfg(not(target_os = "airlos"))]
static SITE_MAP_REMOVE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

#[cfg(not(target_os = "airlos"))]
#[inline]
fn site(slot: &'static std::sync::OnceLock<u16>, name: &'static str) -> u16 {
    *slot.get_or_init(|| crate::diag::register_site(name))
}

/// Return an empty map.
#[no_mangle]
pub extern "C" fn airl_map_new() -> *mut RtValue {
    #[cfg(not(target_os = "airlos"))]
    let sid = site(&SITE_MAP_NEW, "map.rs:airl_map_new");
    #[cfg(target_os = "airlos")]
    let sid = 0u16;
    rt_map_at(HashMap::new(), sid)
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
            #[cfg(not(target_os = "airlos"))]
            let sid = site(&SITE_MAP_FROM, "map.rs:airl_map_from");
            #[cfg(target_os = "airlos")]
            let sid = 0u16;
            rt_map_at(map, sid)
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

/// Return a new map with key→val inserted. The original map is always
/// unchanged — always clones (functional semantics). All existing values are
/// retained in the new map; val is retained.
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

    // Always clone — the AOT/JIT calling convention does not retain arguments
    // before calling builtins, so rc==1 does not imply sole AIRL-level
    // ownership (the caller's binding still lives). Mutating in place would
    // violate functional semantics (the original binding would be altered).
    let v = unsafe { &*m };
    match &v.data {
        RtData::Map(map) => {
            let mut new_map: HashMap<String, *mut RtValue> = HashMap::new();
            for (existing_key, &existing_val) in map {
                airl_value_retain(existing_val);
                new_map.insert(existing_key.clone(), existing_val);
            }
            if let Some(&old_val) = new_map.get(k_str) {
                airl_value_release(old_val);
            }
            airl_value_retain(val);
            new_map.insert(k_str.to_string(), val);
            #[cfg(not(target_os = "airlos"))]
            let sid = site(&SITE_MAP_SET_CLONE, "map.rs:airl_map_set.clone-path");
            #[cfg(target_os = "airlos")]
            let sid = 0u16;
            rt_map_at(new_map, sid)
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

/// Remove a key from the map. Always returns a new map without the key
/// (functional semantics — the original map is never mutated).
#[no_mangle]
pub extern "C" fn airl_map_remove(m: *mut RtValue, key: *mut RtValue) -> *mut RtValue {
    let key_v = unsafe { &*key };
    let k = match &key_v.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("airl_map_remove: key must be a Str"),
    };

    // Always clone — same reasoning as airl_map_set.
    let v = unsafe { &*m };
    match &v.data {
        RtData::Map(map) => {
            let mut new_map: HashMap<String, *mut RtValue> = HashMap::new();
            for (existing_key, &existing_val) in map {
                if existing_key.as_str() == k {
                    continue;
                }
                airl_value_retain(existing_val);
                new_map.insert(existing_key.clone(), existing_val);
            }
            #[cfg(not(target_os = "airlos"))]
            let sid = site(&SITE_MAP_REMOVE, "map.rs:airl_map_remove.clone-path");
            #[cfg(target_os = "airlos")]
            let sid = 0u16;
            rt_map_at(new_map, sid)
        }
        _ => rt_error("airl_map_remove: first argument must be a Map"),
    }
}

/// Return a list of key strings sorted lexicographically.
/// Sorted order is stable and deterministic across runs.
#[no_mangle]
pub extern "C" fn airl_map_keys(m: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    match &map_v.data {
        RtData::Map(map) => {
            let mut keys: Vec<&str> = map.keys().map(|s| s.as_str()).collect();
            keys.sort_unstable();
            let items: Vec<*mut RtValue> = keys.iter().map(|&k| rt_str(k.to_string())).collect();
            rt_list(items)
        }
        _ => rt_error("airl_map_keys: argument must be a Map"),
    }
}

/// Return a list of values in lexicographic key order.
/// Each value is retained.
#[no_mangle]
pub extern "C" fn airl_map_values(m: *mut RtValue) -> *mut RtValue {
    let map_v = unsafe { &*m };
    match &map_v.data {
        RtData::Map(map) => {
            let mut pairs: Vec<(&str, *mut RtValue)> =
                map.iter().map(|(k, &v)| (k.as_str(), v)).collect();
            pairs.sort_unstable_by_key(|(k, _)| *k);
            let items: Vec<*mut RtValue> = pairs.into_iter().map(|(_, v)| {
                airl_value_retain(v);
                v
            }).collect();
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
    fn map_set_always_clones() {
        unsafe {
            let m = airl_map_new(); // rc=1
            let k = mk_key("a");
            let v = rt_int(10);
            let m2 = airl_map_set(m, k, v);
            // Always clones — original map is never mutated.
            assert_ne!(m2, m, "map_set must always return a new map");
            // m is still the original empty map (unmodified)
            let sz_orig = airl_map_size(m);
            assert_eq!((*sz_orig).as_int(), 0, "original map must be unchanged");
            airl_value_release(sz_orig);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m2);
            airl_value_release(m);
        }
    }

    #[test]
    fn map_remove_always_clones() {
        unsafe {
            let m = airl_map_new();
            let k = mk_key("a");
            let v = rt_int(5);
            let m2 = airl_map_set(m, k, v);
            let k_rm = mk_key("a");
            let m3 = airl_map_remove(m2, k_rm);
            // Always clones — original map is never mutated.
            assert_ne!(m3, m2, "map_remove must always return a new map");
            // m2 still has the key
            let k_check = mk_key("a");
            let got = airl_map_get(m2, k_check);
            assert_eq!((*got).as_int(), 5, "original map must be unchanged after remove");
            airl_value_release(got);
            airl_value_release(k_check);
            airl_value_release(k_rm);
            airl_value_release(v);
            airl_value_release(k);
            airl_value_release(m3);
            airl_value_release(m2);
            airl_value_release(m);
        }
    }
}
