#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;
use core::sync::atomic::Ordering;

use crate::error::rt_error;
use crate::memory::airl_value_retain;
use crate::value::{rt_bool, rt_bytes, rt_int, rt_list, RtData, RtValue, TAG_LIST};

/// Return a slice of the list's elements, resolving views through the parent pointer.
pub fn list_items(data: &RtData) -> &[*mut RtValue] {
    match data {
        RtData::List { items, offset, parent } => {
            if let Some(p) = parent {
                let root = unsafe { &**p };
                match &root.data {
                    RtData::List { items: root_items, .. } => &root_items[*offset..],
                    _ => rt_error("list view: parent is not a List"),
                }
            } else {
                &items[*offset..]
            }
        }
        _ => rt_error("not a List"),
    }
}

/// Return the logical length of the list (respecting offset and views).
pub fn list_len(data: &RtData) -> usize {
    list_items(data).len()
}

#[no_mangle]
pub extern "C" fn airl_head(list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    let slice = list_items(&v.data);
    if slice.is_empty() {
        rt_error("airl_head: empty list");
    }
    let item = slice[0];
    airl_value_retain(item);
    item
}

#[no_mangle]
pub extern "C" fn airl_tail(list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    let slice = list_items(&v.data);
    if slice.is_empty() {
        rt_error("airl_tail: empty list");
    }

    // Find the root owner
    let root = match &v.data {
        RtData::List { parent: Some(p), .. } => *p,
        _ => list,
    };
    // Get current offset
    let current_offset = match &v.data {
        RtData::List { offset, .. } => *offset,
        _ => 0,
    };

    airl_value_retain(root);
    // Create view: empty items, offset+1, parent=root
    RtValue::alloc(TAG_LIST, RtData::List {
        items: Vec::new(),
        offset: current_offset + 1,
        parent: Some(root),
    })
}

#[no_mangle]
pub extern "C" fn airl_cons(elem: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &mut *list };
    // COW fast path: sole owner, not a view, at start of array
    if v.rc.load(Ordering::Relaxed) == 1 {
        if let RtData::List { items, offset, parent } = &mut v.data {
            if parent.is_none() && *offset == 0 {
                airl_value_retain(elem);
                items.insert(0, elem);
                airl_value_retain(list); // caller expects +1 ref
                return list;
            }
        }
    }
    // Clone path
    let slice = list_items(&v.data);
    airl_value_retain(elem);
    let mut new_items = Vec::with_capacity(slice.len() + 1);
    new_items.push(elem);
    for &item in slice {
        airl_value_retain(item);
        new_items.push(item);
    }
    rt_list(new_items)
}

#[no_mangle]
pub extern "C" fn airl_empty(list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List { .. } => rt_bool(list_items(&v.data).is_empty()),
        RtData::Bytes(v) => rt_bool(v.is_empty()),
        _ => rt_error("airl_empty: not a List or Bytes"),
    }
}

#[no_mangle]
pub extern "C" fn airl_list_new(items: *const *mut RtValue, count: usize) -> *mut RtValue {
    if count == 0 {
        return rt_list(Vec::new());
    }
    let slice = unsafe { core::slice::from_raw_parts(items, count) };
    let mut vec = Vec::with_capacity(count);
    for &item in slice {
        airl_value_retain(item);
        vec.push(item);
    }
    rt_list(vec)
}

#[no_mangle]
pub extern "C" fn airl_length(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::List { .. } => rt_int(list_len(&val.data) as i64),
        RtData::Str(s) => rt_int(s.chars().count() as i64),
        RtData::Map(m) => rt_int(m.len() as i64),
        RtData::Bytes(v) => rt_int(v.len() as i64),
        _ => rt_error("airl_length: not a List, Str, Map, or Bytes"),
    }
}

#[no_mangle]
pub extern "C" fn airl_at(list: *mut RtValue, index: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    let idx = unsafe { &*index };
    let i = match &idx.data {
        RtData::Int(n) => *n,
        _ => rt_error("airl_at: index must be an Int"),
    };
    match &v.data {
        RtData::List { .. } => {
            let slice = list_items(&v.data);
            if i < 0 || i as usize >= slice.len() {
                rt_error("airl_at: index out of bounds");
            }
            let item = slice[i as usize];
            airl_value_retain(item);
            item
        }
        RtData::Bytes(bytes) => {
            if i < 0 || i as usize >= bytes.len() {
                rt_error("airl_at: index out of bounds");
            }
            rt_int(bytes[i as usize] as i64)
        }
        _ => rt_error("airl_at: not a List or Bytes"),
    }
}

#[no_mangle]
pub extern "C" fn airl_append(list: *mut RtValue, elem: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &mut *list };
    // COW fast path: sole owner, not a view, at start of array
    // Relaxed is safe: SendableRtValue's retain (Relaxed fetch_add) and
    // release (AcqRel fetch_sub) ensure visibility across threads.
    // Acquire here only adds 5-10 cycles/op with no correctness benefit.
    if v.rc.load(Ordering::Relaxed) == 1 {
        match &mut v.data {
            RtData::List { items, offset, parent } if parent.is_none() && *offset == 0 => {
                airl_value_retain(elem);
                items.push(elem);
                airl_value_retain(list); // caller expects +1 ref
                return list;
            }
            RtData::Bytes(bytes) => {
                let b = match unsafe { &(*elem).data } {
                    RtData::Int(n) => *n as u8,
                    _ => rt_error("airl_append: Bytes can only append Int"),
                };
                bytes.push(b);
                airl_value_retain(list); // caller expects +1 ref
                return list;
            }
            _ => {} // fall through to clone path
        }
    }
    // Clone path
    match &v.data {
        RtData::List { .. } => {
            let slice = list_items(&v.data);
            let mut new_items = Vec::with_capacity(slice.len() + 1);
            for &item in slice {
                airl_value_retain(item);
                new_items.push(item);
            }
            airl_value_retain(elem);
            new_items.push(elem);
            rt_list(new_items)
        }
        RtData::Bytes(bytes) => {
            let b = match unsafe { &(*elem).data } {
                RtData::Int(n) => *n as u8,
                _ => rt_error("airl_append: Bytes can only append Int"),
            };
            let mut new_bytes = bytes.clone();
            new_bytes.push(b);
            rt_bytes(new_bytes)
        }
        _ => rt_error("airl_append: not a List or Bytes"),
    }
}

/// `at-or(list, idx, default)` -- safe indexing, returns default on out-of-bounds.
#[no_mangle]
pub extern "C" fn airl_at_or(list: *mut RtValue, idx: *mut RtValue, default: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    let iv = unsafe { &*idx };
    let i = match &iv.data {
        RtData::Int(n) => {
            // SEC-4: Negative indices should return default, not wrap to huge usize
            if *n < 0 {
                airl_value_retain(default);
                return default;
            }
            *n as usize
        }
        _ => rt_error("at-or: index must be Int"),
    };
    match &v.data {
        RtData::List { .. } => {
            let slice = list_items(&v.data);
            if i >= slice.len() {
                airl_value_retain(default);
                default
            } else {
                airl_value_retain(slice[i]);
                slice[i]
            }
        }
        _ => rt_error("at-or: first argument must be List"),
    }
}

/// `set-at(list, idx, val)` -- return new list with element at idx replaced.
#[no_mangle]
pub extern "C" fn airl_set_at(list: *mut RtValue, idx: *mut RtValue, val: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &mut *list };
    let iv = unsafe { &*idx };
    let i = match &iv.data {
        RtData::Int(n) => {
            if *n < 0 { rt_error(&format!("set-at: index {} is negative", n)); }
            *n as usize
        }
        _ => rt_error("set-at: index must be Int"),
    };
    match &v.data {
        RtData::List { .. } => {
            let slice = list_items(&v.data);
            if i >= slice.len() {
                rt_error(&format!("set-at: index {} out of bounds (len {})", i, slice.len()));
            }
        }
        _ => rt_error("set-at: first argument must be List"),
    }
    // COW fast path: sole owner, not a view
    if v.rc.load(Ordering::Relaxed) == 1 {
        if let RtData::List { items, offset, parent } = &mut v.data {
            if parent.is_none() && *offset == 0 {
                let old = items[i];
                airl_value_retain(val);
                items[i] = val;
                crate::memory::airl_value_release(old);
                airl_value_retain(list); // caller expects +1 ref
                return list;
            }
        }
    }
    // Clone path
    let slice = list_items(&v.data);
    let mut new_items = Vec::with_capacity(slice.len());
    for (j, &item) in slice.iter().enumerate() {
        if j == i {
            airl_value_retain(val);
            new_items.push(val);
        } else {
            airl_value_retain(item);
            new_items.push(item);
        }
    }
    rt_list(new_items)
}

/// `list-contains?(list, val)` -- check if element is in list.
#[no_mangle]
pub extern "C" fn airl_list_contains(list: *mut RtValue, val: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List { .. } => {
            let slice = list_items(&v.data);
            for &item in slice {
                // Use the runtime equality check
                let eq_result = crate::comparison::airl_eq(item, val);
                let is_true = unsafe {
                    match &(*eq_result).data {
                        RtData::Bool(b) => *b,
                        _ => false,
                    }
                };
                crate::memory::airl_value_release(eq_result);
                if is_true {
                    return crate::value::rt_bool(true);
                }
            }
            crate::value::rt_bool(false)
        }
        _ => rt_error("list-contains?: first argument must be List"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_list, rt_str};

    #[test]
    fn head_tail() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);

            let h = airl_head(list);
            assert_eq!((*h).as_int(), 1);
            airl_value_release(h);

            let t = airl_tail(list);
            let slice = list_items(&(*t).data);
            assert_eq!(slice.len(), 2);
            assert_eq!((*slice[0]).as_int(), 2);
            assert_eq!((*slice[1]).as_int(), 3);
            airl_value_release(t);
            airl_value_release(list);
        }
    }

    #[test]
    fn cons_prepends() {
        unsafe {
            let a = rt_int(2);
            let b = rt_int(3);
            let list = rt_list(vec![a, b]);
            let elem = rt_int(1);
            let result = airl_cons(elem, list);
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 3);
            assert_eq!((*slice[0]).as_int(), 1);
            assert_eq!((*slice[1]).as_int(), 2);
            assert_eq!((*slice[2]).as_int(), 3);
            airl_value_release(elem);
            airl_value_release(list);
            airl_value_release(result);
        }
    }

    #[test]
    fn empty_check() {
        unsafe {
            let empty = rt_list(vec![]);
            let r = airl_empty(empty);
            assert!((*r).as_bool());
            airl_value_release(r);

            let a = rt_int(1);
            let nonempty = rt_list(vec![a]);
            let r2 = airl_empty(nonempty);
            assert!(!(*r2).as_bool());
            airl_value_release(r2);
            airl_value_release(empty);
            airl_value_release(nonempty);
        }
    }

    #[test]
    fn length_list() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let list = rt_list(vec![a, b]);
            let r = airl_length(list);
            assert_eq!((*r).as_int(), 2);
            airl_value_release(r);
            airl_value_release(list);
        }
    }

    #[test]
    fn length_str() {
        unsafe {
            let s = rt_str("hello".to_string());
            let r = airl_length(s);
            assert_eq!((*r).as_int(), 5);
            airl_value_release(r);
            airl_value_release(s);
        }
    }

    #[test]
    fn at_index() {
        unsafe {
            let a = rt_int(10);
            let b = rt_int(20);
            let list = rt_list(vec![a, b]);
            let idx = rt_int(1);
            let r = airl_at(list, idx);
            assert_eq!((*r).as_int(), 20);
            airl_value_release(r);
            airl_value_release(idx);
            airl_value_release(list);
        }
    }

    #[test]
    fn append_elem() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let list = rt_list(vec![a, b]);
            let elem = rt_int(3);
            let result = airl_append(list, elem);
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 3);
            assert_eq!((*slice[2]).as_int(), 3);
            airl_value_release(elem);
            airl_value_release(list);
            airl_value_release(result);
        }
    }

    // -- COW (Copy-on-Write) tests --

    #[test]
    fn append_cow_returns_same_pointer() {
        unsafe {
            let a = rt_int(1);
            let list = rt_list(vec![a]);
            assert_eq!((*list).rc.load(Ordering::Relaxed), 1);
            let elem = rt_int(2);
            let result = airl_append(list, elem);
            assert_eq!(result, list, "COW append should return same pointer when rc == 1");
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 2);
            assert_eq!((*slice[0]).as_int(), 1);
            assert_eq!((*slice[1]).as_int(), 2);
            airl_value_release(elem);
            airl_value_release(result);
        }
    }

    #[test]
    fn append_clone_when_shared() {
        unsafe {
            let a = rt_int(1);
            let list = rt_list(vec![a]);
            airl_value_retain(list); // rc == 2 -> shared
            assert_eq!((*list).rc.load(Ordering::Relaxed), 2);
            let elem = rt_int(2);
            let result = airl_append(list, elem);
            assert_ne!(result, list, "append should return new pointer when rc > 1");
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 2);
            airl_value_release(elem);
            airl_value_release(list);
            airl_value_release(list); // second release for the extra retain
            airl_value_release(result);
        }
    }

    // -- COW tail view tests --

    #[test]
    fn tail_view_returns_correct_elements() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let t = airl_tail(list);
            let slice = list_items(&(*t).data);
            assert_eq!(slice.len(), 2);
            assert_eq!((*slice[0]).as_int(), 2);
            assert_eq!((*slice[1]).as_int(), 3);
            airl_value_release(t);
            airl_value_release(list);
        }
    }

    #[test]
    fn tail_of_tail_view() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let t1 = airl_tail(list);
            let t2 = airl_tail(t1);
            let slice = list_items(&(*t2).data);
            assert_eq!(slice.len(), 1);
            assert_eq!((*slice[0]).as_int(), 3);
            airl_value_release(t2);
            airl_value_release(t1);
            airl_value_release(list);
        }
    }

    #[test]
    fn tail_view_keeps_root_alive() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let t = airl_tail(list);
            // Release original list -- view retains root, so it should still work
            airl_value_release(list);
            let slice = list_items(&(*t).data);
            assert_eq!(slice.len(), 2);
            assert_eq!((*slice[0]).as_int(), 2);
            assert_eq!((*slice[1]).as_int(), 3);
            airl_value_release(t);
        }
    }

    #[test]
    fn append_inplace_sole_owner() {
        unsafe {
            let a = rt_int(1);
            let list = rt_list(vec![a]);
            assert_eq!((*list).rc.load(Ordering::Relaxed), 1);
            let elem = rt_int(2);
            let result = airl_append(list, elem);
            assert_eq!(result, list, "rc==1, no parent, offset==0 should be in-place");
            airl_value_release(elem);
            airl_value_release(result);
        }
    }

    #[test]
    fn append_clones_when_shared() {
        unsafe {
            let a = rt_int(1);
            let list = rt_list(vec![a]);
            airl_value_retain(list); // rc=2
            let elem = rt_int(2);
            let result = airl_append(list, elem);
            assert_ne!(result, list, "rc>1 should clone");
            airl_value_release(elem);
            airl_value_release(list);
            airl_value_release(list);
            airl_value_release(result);
        }
    }

    #[test]
    fn append_clones_when_view() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let view = airl_tail(list);
            // view has rc==1 but parent is Some -- must clone
            assert_eq!((*view).rc.load(Ordering::Relaxed), 1);
            let elem = rt_int(4);
            let result = airl_append(view, elem);
            assert_ne!(result, view, "view (parent is Some) should clone");
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 3);
            assert_eq!((*slice[0]).as_int(), 2);
            assert_eq!((*slice[1]).as_int(), 3);
            assert_eq!((*slice[2]).as_int(), 4);
            airl_value_release(elem);
            airl_value_release(result);
            airl_value_release(view);
            airl_value_release(list);
        }
    }

    #[test]
    fn cons_on_tail_view() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let view = airl_tail(list);
            let elem = rt_int(0);
            let result = airl_cons(elem, view);
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 3);
            assert_eq!((*slice[0]).as_int(), 0);
            assert_eq!((*slice[1]).as_int(), 2);
            assert_eq!((*slice[2]).as_int(), 3);
            airl_value_release(elem);
            airl_value_release(result);
            airl_value_release(view);
            airl_value_release(list);
        }
    }

    #[test]
    fn length_on_tail_view() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let view = airl_tail(list);
            let len = airl_length(view);
            assert_eq!((*len).as_int(), 2);
            airl_value_release(len);
            airl_value_release(view);
            airl_value_release(list);
        }
    }

    #[test]
    fn empty_on_tail_view() {
        unsafe {
            // Non-empty view
            let a = rt_int(1);
            let b = rt_int(2);
            let list = rt_list(vec![a, b]);
            let view = airl_tail(list);
            let r = airl_empty(view);
            assert!(!(*r).as_bool());
            airl_value_release(r);
            airl_value_release(view);
            airl_value_release(list);

            // Tail of single-element list -> empty view
            let x = rt_int(42);
            let list2 = rt_list(vec![x]);
            let view2 = airl_tail(list2);
            let r2 = airl_empty(view2);
            assert!((*r2).as_bool());
            airl_value_release(r2);
            airl_value_release(view2);
            airl_value_release(list2);
        }
    }

    #[test]
    fn fold_pattern_with_tail() {
        // Build [1..100], sum using head/tail pattern, verify == 5050
        unsafe {
            let items: Vec<*mut RtValue> = (1..=100).map(|i| rt_int(i)).collect();
            let mut list = rt_list(items);
            let mut sum: i64 = 0;
            loop {
                let slice = list_items(&(*list).data);
                if slice.is_empty() {
                    break;
                }
                let h = airl_head(list);
                sum += (*h).as_int();
                airl_value_release(h);
                let t = airl_tail(list);
                airl_value_release(list);
                list = t;
            }
            airl_value_release(list);
            assert_eq!(sum, 5050);
        }
    }

    #[test]
    fn clone_tail_view() {
        // Clone a tail view and verify the clone contains the correct elements
        // and is a fresh non-view list (no parent pointer).
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            let view = airl_tail(list);

            // view should show [2, 3]
            let view_slice = list_items(&(*view).data);
            assert_eq!(view_slice.len(), 2);

            // Clone the view
            let cloned = crate::memory::airl_value_clone(view);
            assert!(!cloned.is_null());

            // Cloned list should contain [2, 3]
            let cloned_slice = list_items(&(*cloned).data);
            assert_eq!(cloned_slice.len(), 2);
            assert_eq!((*cloned_slice[0]).as_int(), 2);
            assert_eq!((*cloned_slice[1]).as_int(), 3);

            // Cloned list should be a fresh non-view (no parent, offset 0)
            match &(*cloned).data {
                RtData::List { parent, offset, items } => {
                    assert!(parent.is_none(), "clone should produce a non-view list");
                    assert_eq!(*offset, 0, "clone should have offset 0");
                    assert_eq!(items.len(), 2, "clone should own its items directly");
                }
                _ => panic!("cloned value is not a List"),
            }

            airl_value_release(cloned);
            airl_value_release(view);
            airl_value_release(list);
        }
    }

    #[test]
    fn discriminant_unchanged() {
        // Verify that the discriminant of a manually constructed RtData::List
        // matches the one produced by rt_list, ensuring the AOT tag is stable.
        let manual = RtData::List {
            items: vec![],
            offset: 0,
            parent: None,
        };
        unsafe {
            let runtime = rt_list(vec![]);
            let runtime_disc = core::mem::discriminant(&(*runtime).data);
            let manual_disc = core::mem::discriminant(&manual);
            assert_eq!(
                runtime_disc, manual_disc,
                "RtData::List discriminant must be stable for AOT tag consistency"
            );
            airl_value_release(runtime);
        }
    }

    // ── SEC-4: Negative index in at-or ───────────────────────────────

    #[test]
    fn at_or_negative_index_returns_default() {
        unsafe {
            let a = rt_int(10);
            let b = rt_int(20);
            let list = rt_list(vec![a, b]);
            let idx = rt_int(-1);
            let default = rt_int(99);
            let result = airl_at_or(list, idx, default);
            assert_eq!((*result).as_int(), 99);
            airl_value_release(result);
            airl_value_release(default);
            airl_value_release(idx);
            airl_value_release(list);
        }
    }

    #[test]
    fn at_or_negative_large_index_returns_default() {
        unsafe {
            let a = rt_int(10);
            let list = rt_list(vec![a]);
            let idx = rt_int(i64::MIN);
            let default = rt_int(42);
            let result = airl_at_or(list, idx, default);
            assert_eq!((*result).as_int(), 42);
            airl_value_release(result);
            airl_value_release(default);
            airl_value_release(idx);
            airl_value_release(list);
        }
    }

    #[test]
    fn at_or_valid_index_returns_element() {
        unsafe {
            let a = rt_int(10);
            let b = rt_int(20);
            let list = rt_list(vec![a, b]);
            let idx = rt_int(1);
            let default = rt_int(99);
            let result = airl_at_or(list, idx, default);
            assert_eq!((*result).as_int(), 20);
            airl_value_release(result);
            airl_value_release(default);
            airl_value_release(idx);
            airl_value_release(list);
        }
    }

    #[test]
    fn at_or_out_of_bounds_returns_default() {
        unsafe {
            let a = rt_int(10);
            let list = rt_list(vec![a]);
            let idx = rt_int(5);
            let default = rt_int(77);
            let result = airl_at_or(list, idx, default);
            assert_eq!((*result).as_int(), 77);
            airl_value_release(result);
            airl_value_release(default);
            airl_value_release(idx);
            airl_value_release(list);
        }
    }

    // ── COW cons tests ──

    #[test]
    fn cons_cow_returns_same_pointer() {
        unsafe {
            let a = rt_int(2);
            let b = rt_int(3);
            let list = rt_list(vec![a, b]);
            assert_eq!((*list).rc.load(Ordering::Relaxed), 1);
            let elem = rt_int(1);
            let result = airl_cons(elem, list);
            assert_eq!(result, list, "COW cons should return same pointer when rc == 1");
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 3);
            assert_eq!((*slice[0]).as_int(), 1);
            assert_eq!((*slice[1]).as_int(), 2);
            assert_eq!((*slice[2]).as_int(), 3);
            airl_value_release(elem);
            airl_value_release(result);
        }
    }

    #[test]
    fn cons_clones_when_shared() {
        unsafe {
            let a = rt_int(2);
            let list = rt_list(vec![a]);
            airl_value_retain(list); // rc == 2
            let elem = rt_int(1);
            let result = airl_cons(elem, list);
            assert_ne!(result, list, "cons should clone when rc > 1");
            airl_value_release(elem);
            airl_value_release(list);
            airl_value_release(list);
            airl_value_release(result);
        }
    }

    #[test]
    fn cons_clones_when_view() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let list = rt_list(vec![a, b]);
            let view = airl_tail(list);
            assert_eq!((*view).rc.load(Ordering::Relaxed), 1);
            let elem = rt_int(0);
            let result = airl_cons(elem, view);
            assert_ne!(result, view, "cons should clone when list is a view");
            let slice = list_items(&(*result).data);
            assert_eq!(slice.len(), 2);
            assert_eq!((*slice[0]).as_int(), 0);
            assert_eq!((*slice[1]).as_int(), 2);
            airl_value_release(elem);
            airl_value_release(result);
            airl_value_release(view);
            airl_value_release(list);
        }
    }

    // ── COW set-at tests ──

    #[test]
    fn set_at_cow_returns_same_pointer() {
        unsafe {
            let a = rt_int(10);
            let b = rt_int(20);
            let list = rt_list(vec![a, b]);
            assert_eq!((*list).rc.load(Ordering::Relaxed), 1);
            let idx = rt_int(1);
            let val = rt_int(99);
            let result = airl_set_at(list, idx, val);
            assert_eq!(result, list, "COW set-at should return same pointer when rc == 1");
            let slice = list_items(&(*result).data);
            assert_eq!((*slice[0]).as_int(), 10);
            assert_eq!((*slice[1]).as_int(), 99);
            airl_value_release(val);
            airl_value_release(idx);
            airl_value_release(result);
        }
    }

    #[test]
    fn set_at_clones_when_shared() {
        unsafe {
            let a = rt_int(10);
            let b = rt_int(20);
            let list = rt_list(vec![a, b]);
            airl_value_retain(list); // rc == 2
            let idx = rt_int(0);
            let val = rt_int(99);
            let result = airl_set_at(list, idx, val);
            assert_ne!(result, list, "set-at should clone when rc > 1");
            airl_value_release(val);
            airl_value_release(idx);
            airl_value_release(list);
            airl_value_release(list);
            airl_value_release(result);
        }
    }
}
