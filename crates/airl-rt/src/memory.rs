use std::collections::HashMap;
use core::sync::atomic::Ordering;

use crate::value::{rt_bool, rt_bytes, rt_float, rt_int, rt_nil, rt_str, rt_unit, rt_variant, RtData, RtValue};

/// Increment refcount atomically. Null-safe.
///
/// SEC-1: Uses AtomicU32::fetch_add to prevent data races on the refcount.
/// SEC-2: When rc reaches u32::MAX, the value becomes immortal (never freed).
#[no_mangle]
pub extern "C" fn airl_value_retain(ptr: *mut RtValue) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let old = (*ptr).rc.fetch_add(1, Ordering::Relaxed);
        // SEC-2: If we just incremented past the immortal threshold,
        // clamp back to u32::MAX to make the value immortal.
        if old >= u32::MAX - 1 {
            (*ptr).rc.store(u32::MAX, Ordering::Relaxed);
        }
    }
}

/// Decrement refcount atomically, free at zero. Null-safe.
///
/// SEC-1: Uses AtomicU32::fetch_sub to prevent data races on the refcount.
/// SEC-2: Immortal values (rc == u32::MAX) are never freed.
#[no_mangle]
pub extern "C" fn airl_value_release(ptr: *mut RtValue) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        // SEC-2: Immortal values are never freed
        let current = (*ptr).rc.load(Ordering::Relaxed);
        if current == u32::MAX {
            return;
        }
        // fetch_sub returns the previous value. If prev was 1, the new value
        // is 0 and we should free. If prev was 0, something is very wrong
        // (double-free) — restore to 0 and bail.
        let prev = (*ptr).rc.fetch_sub(1, Ordering::AcqRel);
        if prev == 1 {
            // Refcount is now 0 — we have exclusive access. Free.
            free_value(ptr);
        } else if prev == 0 {
            // Double-release detected — restore to 0, do nothing.
            (*ptr).rc.store(0, Ordering::Relaxed);
        }
    }
}

unsafe fn free_value(ptr: *mut RtValue) {
    // Recursively release nested pointers before dropping
    match &(*ptr).data {
        RtData::List { items, offset, parent } => {
            if let Some(p) = parent {
                // This is a tail view -- release the parent, don't free items
                airl_value_release(*p);
            } else {
                // We own the items -- release elements from offset
                for &item in &items[*offset..] {
                    airl_value_release(item);
                }
            }
        }
        RtData::Map(m) => {
            for &val in m.values() {
                airl_value_release(val);
            }
        }
        RtData::Variant { inner, .. } => {
            airl_value_release(*inner);
        }
        RtData::Closure { captures, .. } => {
            for &cap in captures {
                airl_value_release(cap);
            }
        }
        _ => {}
    }
    drop(Box::from_raw(ptr));
}

/// Clone a value. For primitives: allocate new. For containers: shallow — retain shared items.
#[no_mangle]
pub extern "C" fn airl_value_clone(ptr: *mut RtValue) -> *mut RtValue {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    unsafe {
        match &(*ptr).data {
            RtData::Nil => rt_nil(),
            RtData::Unit => rt_unit(),
            RtData::Int(v) => rt_int(*v),
            RtData::Float(v) => rt_float(*v),
            RtData::Bool(v) => rt_bool(*v),
            RtData::Str(s) => rt_str(s.clone()),
            RtData::Bytes(v) => rt_bytes(v.clone()),
            RtData::List { .. } => {
                // Resolve views through the parent pointer before cloning
                let slice = crate::list::list_items(&(*ptr).data);
                for &item in slice {
                    airl_value_retain(item);
                }
                crate::value::rt_list(slice.to_vec())
            }
            RtData::Map(m) => {
                // Retain each value and share
                for &val in m.values() {
                    airl_value_retain(val);
                }
                let mut new_map: HashMap<String, *mut RtValue> = HashMap::new();
                for (k, &v) in m {
                    new_map.insert(k.clone(), v);
                }
                crate::value::rt_map(new_map)
            }
            RtData::Variant { tag_name, inner } => {
                airl_value_retain(*inner);
                rt_variant(tag_name.clone(), *inner)
            }
            RtData::Closure { func_ptr, captures } => {
                for &cap in captures {
                    airl_value_retain(cap);
                }
                crate::value::RtValue::alloc(
                    crate::value::TAG_CLOSURE,
                    RtData::Closure {
                        func_ptr: *func_ptr,
                        captures: captures.clone(),
                    },
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{rt_int, rt_list, rt_nil};

    #[test]
    fn test_retain_release_basic() {
        unsafe {
            let v = rt_int(10);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            airl_value_retain(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 2);
            airl_value_release(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            // Final release frees — don't access after this
            airl_value_release(v);
        }
    }

    #[test]
    fn test_release_null_safe() {
        // Should not panic
        airl_value_release(std::ptr::null_mut());
        airl_value_retain(std::ptr::null_mut());
    }

    #[test]
    fn test_clone_int() {
        unsafe {
            let v = rt_int(99);
            let c = airl_value_clone(v);
            assert!(!c.is_null());
            assert_eq!((*c).as_int(), 99);
            assert_eq!((*c).rc.load(Ordering::Relaxed), 1);
            airl_value_release(v);
            airl_value_release(c);
        }
    }

    #[test]
    fn test_clone_list_retains_items() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            assert_eq!((*a).rc.load(Ordering::Relaxed), 1);
            assert_eq!((*b).rc.load(Ordering::Relaxed), 1);

            let list = rt_list(vec![a, b]);

            // Clone the list — items should be retained (rc=2)
            let cloned = airl_value_clone(list);
            assert!(!cloned.is_null());
            assert_eq!((*a).rc.load(Ordering::Relaxed), 2);
            assert_eq!((*b).rc.load(Ordering::Relaxed), 2);

            // Release cloned list — items back to rc=1
            airl_value_release(cloned);
            assert_eq!((*a).rc.load(Ordering::Relaxed), 1);
            assert_eq!((*b).rc.load(Ordering::Relaxed), 1);

            // Release original list — items freed
            airl_value_release(list);
        }
    }

    // ── SEC-2: Immortal sentinel tests ───────────────────────────────

    #[test]
    fn test_saturated_rc_becomes_immortal() {
        unsafe {
            let v = rt_int(42);
            // Set rc to MAX - 1 to test saturation behavior
            (*v).rc.store(u32::MAX - 1, Ordering::Relaxed);
            airl_value_retain(v); // should clamp to u32::MAX
            assert_eq!((*v).rc.load(Ordering::Relaxed), u32::MAX);
            // Further retains should keep it at MAX
            airl_value_retain(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), u32::MAX);
            // Release should be a no-op on immortal values
            airl_value_release(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), u32::MAX);
            // Clean up: force-set rc to 1 so we can free
            (*v).rc.store(1, Ordering::Relaxed);
            airl_value_release(v);
        }
    }

    #[test]
    fn test_immortal_release_is_noop() {
        unsafe {
            let v = rt_int(99);
            (*v).rc.store(u32::MAX, Ordering::Relaxed);
            // Multiple releases should all be no-ops
            airl_value_release(v);
            airl_value_release(v);
            airl_value_release(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), u32::MAX);
            // Clean up
            (*v).rc.store(1, Ordering::Relaxed);
            airl_value_release(v);
        }
    }
}
