use std::collections::HashMap;

use crate::value::{rt_bool, rt_bytes, rt_float, rt_int, rt_nil, rt_str, rt_unit, rt_variant, RtData, RtValue};

/// Increment refcount. Null-safe.
#[no_mangle]
pub extern "C" fn airl_value_retain(ptr: *mut RtValue) {
    if ptr.is_null() {
        return;
    }
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

unsafe fn free_value(ptr: *mut RtValue) {
    // Recursively release nested pointers before dropping
    match &(*ptr).data {
        RtData::List(items) => {
            for &item in items {
                airl_value_release(item);
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
    std::ptr::drop_in_place(ptr);
    crate::pool::pool_release(ptr);
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
            RtData::List(items) => {
                // Retain each item and share
                for &item in items {
                    airl_value_retain(item);
                }
                crate::value::rt_list(items.clone())
            }
            RtData::Map(m) => {
                // Retain each value and share; Arc::clone for keys (no heap alloc)
                for &val in m.values() {
                    airl_value_retain(val);
                }
                let new_map = m.iter().map(|(k, &v)| (std::sync::Arc::clone(k), v)).collect();
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
