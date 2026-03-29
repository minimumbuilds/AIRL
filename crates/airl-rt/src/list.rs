use crate::error::rt_error;
use crate::memory::airl_value_retain;
use crate::value::{rt_bool, rt_bytes, rt_int, rt_list, RtData, RtValue};

#[no_mangle]
pub extern "C" fn airl_head(list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List(items) => {
            if items.is_empty() {
                rt_error("airl_head: empty list");
            }
            let item = items[0];
            airl_value_retain(item);
            item
        }
        _ => rt_error("airl_head: not a List"),
    }
}

#[no_mangle]
pub extern "C" fn airl_tail(list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List(items) => {
            if items.is_empty() {
                rt_error("airl_tail: empty list");
            }
            let tail: Vec<*mut RtValue> = items[1..].to_vec();
            for &item in &tail {
                airl_value_retain(item);
            }
            rt_list(tail)
        }
        _ => rt_error("airl_tail: not a List"),
    }
}

#[no_mangle]
pub extern "C" fn airl_cons(elem: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List(items) => {
            airl_value_retain(elem);
            let mut new_items = Vec::with_capacity(items.len() + 1);
            new_items.push(elem);
            for &item in items {
                airl_value_retain(item);
                new_items.push(item);
            }
            rt_list(new_items)
        }
        _ => rt_error("airl_cons: not a List"),
    }
}

#[no_mangle]
pub extern "C" fn airl_empty(list: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List(items) => rt_bool(items.is_empty()),
        RtData::Bytes(v) => rt_bool(v.is_empty()),
        _ => rt_error("airl_empty: not a List or Bytes"),
    }
}

#[no_mangle]
pub extern "C" fn airl_list_new(items: *const *mut RtValue, count: usize) -> *mut RtValue {
    if count == 0 {
        return rt_list(Vec::new());
    }
    let slice = unsafe { std::slice::from_raw_parts(items, count) };
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
        RtData::List(items) => rt_int(items.len() as i64),
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
        RtData::List(items) => {
            if i < 0 || i as usize >= items.len() {
                rt_error("airl_at: index out of bounds");
            }
            let item = items[i as usize];
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
    let v = unsafe { &*list };
    match &v.data {
        RtData::List(items) => {
            let mut new_items = Vec::with_capacity(items.len() + 1);
            for &item in items {
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

/// `at-or(list, idx, default)` — safe indexing, returns default on out-of-bounds.
#[no_mangle]
pub extern "C" fn airl_at_or(list: *mut RtValue, idx: *mut RtValue, default: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    let iv = unsafe { &*idx };
    let i = match &iv.data {
        RtData::Int(n) => *n as usize,
        _ => rt_error("at-or: index must be Int"),
    };
    match &v.data {
        RtData::List(items) => {
            if i >= items.len() {
                airl_value_retain(default);
                default
            } else {
                airl_value_retain(items[i]);
                items[i]
            }
        }
        _ => rt_error("at-or: first argument must be List"),
    }
}

/// `set-at(list, idx, val)` — return new list with element at idx replaced.
#[no_mangle]
pub extern "C" fn airl_set_at(list: *mut RtValue, idx: *mut RtValue, val: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    let iv = unsafe { &*idx };
    let i = match &iv.data {
        RtData::Int(n) => *n as usize,
        _ => rt_error("set-at: index must be Int"),
    };
    match &v.data {
        RtData::List(items) => {
            if i >= items.len() {
                rt_error(&format!("set-at: index {} out of bounds (len {})", i, items.len()));
            }
            let mut new_items = Vec::with_capacity(items.len());
            for (j, &item) in items.iter().enumerate() {
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
        _ => rt_error("set-at: first argument must be List"),
    }
}

/// `list-contains?(list, val)` — check if element is in list.
#[no_mangle]
pub extern "C" fn airl_list_contains(list: *mut RtValue, val: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*list };
    match &v.data {
        RtData::List(items) => {
            for &item in items {
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

// ── Higher-order list operations (for AOT-compiled code) ─────────

use crate::closure::airl_call_closure;
use crate::value::rt_nil;

/// map: apply closure to each element, return new list
#[no_mangle]
pub extern "C" fn airl_map(closure: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_map: second arg must be a List"),
        }
    };
    let mut result = Vec::with_capacity(items.len());
    for &item in &items {
        airl_value_retain(item);
        let args: [*mut RtValue; 1] = [item];
        let val = airl_call_closure(closure, args.as_ptr(), 1);
        result.push(val);
    }
    rt_list(result)
}

/// filter: keep elements where closure returns true
#[no_mangle]
pub extern "C" fn airl_filter(closure: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_filter: second arg must be a List"),
        }
    };
    let mut result = Vec::new();
    for &item in &items {
        airl_value_retain(item);
        let args: [*mut RtValue; 1] = [item];
        let test = airl_call_closure(closure, args.as_ptr(), 1);
        let keep = unsafe { matches!(&(*test).data, RtData::Bool(true)) };
        if keep {
            airl_value_retain(item);
            result.push(item);
        }
    }
    rt_list(result)
}

/// fold: left fold with accumulator
#[no_mangle]
pub extern "C" fn airl_fold(
    closure: *mut RtValue, init: *mut RtValue, list: *mut RtValue,
) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_fold: third arg must be a List"),
        }
    };
    airl_value_retain(init);
    let mut acc = init;
    for &item in &items {
        airl_value_retain(item);
        let args: [*mut RtValue; 2] = [acc, item];
        acc = airl_call_closure(closure, args.as_ptr(), 2);
    }
    acc
}

/// sort: insertion sort with comparison closure
#[no_mangle]
pub extern "C" fn airl_sort(closure: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_sort: second arg must be a List"),
        }
    };
    if items.len() <= 1 {
        airl_value_retain(list);
        return list;
    }
    let mut vec = items;
    for i in 1..vec.len() {
        let mut j = i;
        while j > 0 {
            airl_value_retain(vec[j - 1]);
            airl_value_retain(vec[j]);
            let args: [*mut RtValue; 2] = [vec[j - 1], vec[j]];
            let cmp = airl_call_closure(closure, args.as_ptr(), 2);
            let is_less = unsafe { matches!(&(*cmp).data, RtData::Bool(true)) };
            if !is_less {
                vec.swap(j - 1, j);
                j -= 1;
            } else {
                break;
            }
        }
    }
    for &item in &vec { airl_value_retain(item); }
    rt_list(vec)
}

/// any: true if closure returns true for any element
#[no_mangle]
pub extern "C" fn airl_any(closure: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_any: second arg must be a List"),
        }
    };
    for &item in &items {
        airl_value_retain(item);
        let args: [*mut RtValue; 1] = [item];
        let test = airl_call_closure(closure, args.as_ptr(), 1);
        if unsafe { matches!(&(*test).data, RtData::Bool(true)) } {
            return rt_bool(true);
        }
    }
    rt_bool(false)
}

/// all: true if closure returns true for all elements
#[no_mangle]
pub extern "C" fn airl_all(closure: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_all: second arg must be a List"),
        }
    };
    for &item in &items {
        airl_value_retain(item);
        let args: [*mut RtValue; 1] = [item];
        let test = airl_call_closure(closure, args.as_ptr(), 1);
        if !unsafe { matches!(&(*test).data, RtData::Bool(true)) } {
            return rt_bool(false);
        }
    }
    rt_bool(true)
}

/// find: first element where closure returns true, or nil
#[no_mangle]
pub extern "C" fn airl_find(closure: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let items = unsafe {
        match &(*list).data {
            RtData::List(items) => items.clone(),
            _ => rt_error("airl_find: second arg must be a List"),
        }
    };
    for &item in &items {
        airl_value_retain(item);
        let args: [*mut RtValue; 1] = [item];
        let test = airl_call_closure(closure, args.as_ptr(), 1);
        if unsafe { matches!(&(*test).data, RtData::Bool(true)) } {
            airl_value_retain(item);
            return item;
        }
    }
    rt_nil()
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
            // retain to keep them alive after list creation (list holds one ref)
            let list = rt_list(vec![a, b, c]);

            let h = airl_head(list);
            assert_eq!((*h).as_int(), 1);
            airl_value_release(h);

            let t = airl_tail(list);
            match &(*t).data {
                RtData::List(items) => {
                    assert_eq!(items.len(), 2);
                    assert_eq!((*items[0]).as_int(), 2);
                    assert_eq!((*items[1]).as_int(), 3);
                }
                _ => panic!("expected list"),
            }
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
            match &(*result).data {
                RtData::List(items) => {
                    assert_eq!(items.len(), 3);
                    assert_eq!((*items[0]).as_int(), 1);
                    assert_eq!((*items[1]).as_int(), 2);
                    assert_eq!((*items[2]).as_int(), 3);
                }
                _ => panic!("expected list"),
            }
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
            match &(*result).data {
                RtData::List(items) => {
                    assert_eq!(items.len(), 3);
                    assert_eq!((*items[2]).as_int(), 3);
                }
                _ => panic!("expected list"),
            }
            airl_value_release(elem);
            airl_value_release(list);
            airl_value_release(result);
        }
    }
}
