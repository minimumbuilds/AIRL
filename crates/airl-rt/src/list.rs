use crate::error::rt_error;
use crate::memory::airl_value_retain;
use crate::value::{rt_bool, rt_int, rt_list, RtData, RtValue};

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
        _ => rt_error("airl_empty: not a List"),
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
        _ => rt_error("airl_length: not a List, Str, or Map"),
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
        _ => rt_error("airl_at: not a List"),
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
        _ => rt_error("airl_append: not a List"),
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
