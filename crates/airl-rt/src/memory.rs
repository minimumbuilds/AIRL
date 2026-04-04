use std::collections::HashMap;

use crate::value::{rt_bool, rt_bytes, rt_float, rt_int, rt_nil, rt_str, rt_unit, rt_variant, RtData, RtValue};

/// Increment refcount. Null-safe.
///
/// # Safety (internal)
///
/// The `rc` field is a non-atomic `u32`. This is safe under the AIRL threading
/// model: a value may only be mutated (retain/release/COW) by a single thread
/// at a time. Cross-thread transfers go through `SendableRtValue` which retains
/// before sending and ensures the sender no longer mutates.
///
/// **Known limitation:** If two threads retain/release the *same* `*mut RtValue`
/// concurrently without external synchronisation, this is a data race (UB).
/// The current AIRL compiler never generates such code, but a future concurrent
/// GC or work-stealing runtime would need atomic refcounting here.
#[no_mangle]
pub extern "C" fn airl_value_retain(ptr: *mut RtValue) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: Caller guarantees `ptr` is a valid, live RtValue. The null check
    // above handles the only expected invalid input. Non-atomic increment is
    // safe because the AIRL threading model forbids concurrent mutation.
    unsafe {
        (*ptr).rc = (*ptr).rc.saturating_add(1);
    }
}

/// Decrement refcount, free at zero. Null-safe.
#[no_mangle]
pub extern "C" fn airl_value_release(ptr: *mut RtValue) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: Same preconditions as `airl_value_retain`. Additionally, when
    // rc reaches zero we call `free_value` which takes ownership of the Box
    // and drops it — the pointer must not be used after this point.
    unsafe {
        let rc = (*ptr).rc;
        if rc == 0 {
            // Already freed or invalid — do nothing
            return;
        }
        (*ptr).rc = rc - 1;
        if (*ptr).rc == 0 {
            free_value(ptr);
        }
    }
}

/// Recursively release nested pointers and deallocate the RtValue.
///
/// # Safety
///
/// `ptr` must be a valid, exclusively-owned `*mut RtValue` with `rc == 0`.
/// After this call, `ptr` is dangling — the caller must not use it again.
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
            assert_eq!((*v).rc, 1);
            airl_value_retain(v);
            assert_eq!((*v).rc, 2);
            airl_value_release(v);
            assert_eq!((*v).rc, 1);
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
            assert_eq!((*c).rc, 1);
            airl_value_release(v);
            airl_value_release(c);
        }
    }

    #[test]
    fn test_clone_list_retains_items() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            assert_eq!((*a).rc, 1);
            assert_eq!((*b).rc, 1);

            let list = rt_list(vec![a, b]);

            // Clone the list — items should be retained (rc=2)
            let cloned = airl_value_clone(list);
            assert!(!cloned.is_null());
            assert_eq!((*a).rc, 2);
            assert_eq!((*b).rc, 2);

            // Release cloned list — items back to rc=1
            airl_value_release(cloned);
            assert_eq!((*a).rc, 1);
            assert_eq!((*b).rc, 1);

            // Release original list — items freed
            airl_value_release(list);
        }
    }
}
