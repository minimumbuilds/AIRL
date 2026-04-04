#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

use core::fmt::Write;
use crate::error::rt_error;
use crate::value::{rt_bool, rt_int, rt_list, rt_str, RtData, RtValue};

/// `char-at(s, idx)` — return the Nth Unicode character as a 1-char string.
/// Exits with rt_error if idx is out of bounds.
#[no_mangle]
pub extern "C" fn airl_char_at(s: *mut RtValue, idx: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let iv = unsafe { &*idx };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("char-at: first argument must be a Str"),
    };
    let index = match &iv.data {
        RtData::Int(n) => *n,
        _ => rt_error("char-at: second argument must be an Int"),
    };
    if index < 0 {
        rt_error(&format!(
            "char-at: index {} out of bounds for string of length {}",
            index,
            str_val.chars().count()
        ));
    }
    let i = index as usize;
    match str_val.chars().nth(i) {
        Some(c) => rt_str(c.to_string()),
        None => rt_error(&format!(
            "char-at: index {} out of bounds for string of length {}",
            index,
            str_val.chars().count()
        )),
    }
}

/// `substring(s, start, end)` — chars from start (inclusive) to end (exclusive).
/// Exits with rt_error if end < start.
#[no_mangle]
pub extern "C" fn airl_substring(
    s: *mut RtValue,
    start: *mut RtValue,
    end: *mut RtValue,
) -> *mut RtValue {
    let sv = unsafe { &*s };
    let startv = unsafe { &*start };
    let endv = unsafe { &*end };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("substring: first argument must be a Str"),
    };
    let start_idx = match &startv.data {
        RtData::Int(n) => {
            if *n < 0 { rt_error(&format!("substring: start index {} is negative", n)); }
            *n as usize
        }
        _ => rt_error("substring: second argument must be an Int"),
    };
    let end_idx = match &endv.data {
        RtData::Int(n) => {
            if *n < 0 { rt_error(&format!("substring: end index {} is negative", n)); }
            *n as usize
        }
        _ => rt_error("substring: third argument must be an Int"),
    };
    if end_idx < start_idx {
        rt_error(&format!(
            "substring: end ({}) < start ({})",
            end_idx, start_idx
        ));
    }
    let result: String = str_val
        .chars()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .collect();
    rt_str(result)
}

/// `chars(s)` — return a list of 1-char strings for each Unicode character.
#[no_mangle]
pub extern "C" fn airl_chars(s: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("chars: argument must be a Str"),
    };
    let char_count = str_val.chars().count();
    let mut items = Vec::with_capacity(char_count);
    for c in str_val.chars() {
        items.push(rt_str(c.to_string()));
    }
    rt_list(items)
}

/// `split(s, delim)` — split s by delimiter, return a list of strings.
#[no_mangle]
pub extern "C" fn airl_split(s: *mut RtValue, delim: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let dv = unsafe { &*delim };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("split: first argument must be a Str"),
    };
    let delim_val = match &dv.data {
        RtData::Str(d) => d,
        _ => rt_error("split: second argument must be a Str"),
    };
    let items: Vec<*mut RtValue> = str_val
        .split(delim_val.as_str())
        .map(|p| rt_str(p.to_string()))
        .collect();
    rt_list(items)
}

/// `join(list, sep)` — join list items with separator.
/// String items are used as-is; other items use their Display representation.
#[no_mangle]
pub extern "C" fn airl_join(list: *mut RtValue, sep: *mut RtValue) -> *mut RtValue {
    let lv = unsafe { &*list };
    let sv = unsafe { &*sep };
    let items = match &lv.data {
        RtData::List { .. } => crate::list::list_items(&lv.data),
        _ => rt_error("join: first argument must be a List"),
    };
    let sep_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("join: second argument must be a Str"),
    };
    // Estimate capacity: sum of string lengths + separators
    let mut est_cap = sep_val.len() * items.len().saturating_sub(1);
    for &item in items.iter() {
        let val = unsafe { &*item };
        match &val.data {
            RtData::Str(s) => est_cap += s.len(),
            _ => est_cap += 16, // rough estimate for non-string Display
        }
    }
    let mut result = String::with_capacity(est_cap);
    for (idx, &item) in items.iter().enumerate() {
        if idx > 0 {
            result.push_str(sep_val.as_str());
        }
        let val = unsafe { &*item };
        match &val.data {
            RtData::Str(s) => result.push_str(s),
            _ => { let _ = write!(result, "{}", val); }
        }
    }
    rt_str(result)
}

/// `contains(s, sub)` — true if s contains sub.
#[no_mangle]
pub extern "C" fn airl_contains(s: *mut RtValue, sub: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let subv = unsafe { &*sub };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("contains: first argument must be a Str"),
    };
    let sub_val = match &subv.data {
        RtData::Str(s) => s,
        _ => rt_error("contains: second argument must be a Str"),
    };
    rt_bool(str_val.contains(sub_val.as_str()))
}

/// `starts-with(s, prefix)` — true if s starts with prefix.
#[no_mangle]
pub extern "C" fn airl_starts_with(s: *mut RtValue, prefix: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let pv = unsafe { &*prefix };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("starts-with: first argument must be a Str"),
    };
    let prefix_val = match &pv.data {
        RtData::Str(s) => s,
        _ => rt_error("starts-with: second argument must be a Str"),
    };
    rt_bool(str_val.starts_with(prefix_val.as_str()))
}

/// `ends-with(s, suffix)` — true if s ends with suffix.
#[no_mangle]
pub extern "C" fn airl_ends_with(s: *mut RtValue, suffix: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let sfx = unsafe { &*suffix };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("ends-with: first argument must be a Str"),
    };
    let suffix_val = match &sfx.data {
        RtData::Str(s) => s,
        _ => rt_error("ends-with: second argument must be a Str"),
    };
    rt_bool(str_val.ends_with(suffix_val.as_str()))
}

/// `index-of(s, sub)` — character index of first occurrence of sub, or -1 if not found.
#[no_mangle]
pub extern "C" fn airl_index_of(s: *mut RtValue, sub: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let subv = unsafe { &*sub };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("index-of: first argument must be a Str"),
    };
    let sub_val = match &subv.data {
        RtData::Str(s) => s,
        _ => rt_error("index-of: second argument must be a Str"),
    };
    match str_val.find(sub_val.as_str()) {
        Some(byte_offset) => {
            let char_index = str_val[..byte_offset].chars().count() as i64;
            rt_int(char_index)
        }
        None => rt_int(-1),
    }
}

/// `trim(s)` — trim leading and trailing whitespace.
#[no_mangle]
pub extern "C" fn airl_trim(s: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("trim: argument must be a Str"),
    };
    rt_str(str_val.trim().to_string())
}

/// `to-upper(s)` — convert to uppercase.
#[no_mangle]
pub extern "C" fn airl_to_upper(s: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("to-upper: argument must be a Str"),
    };
    rt_str(str_val.to_uppercase())
}

/// `to-lower(s)` — convert to lowercase.
#[no_mangle]
pub extern "C" fn airl_to_lower(s: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("to-lower: argument must be a Str"),
    };
    rt_str(str_val.to_lowercase())
}

/// `replace(s, old, new)` — replace all occurrences of old with new.
#[no_mangle]
pub extern "C" fn airl_replace(
    s: *mut RtValue,
    old: *mut RtValue,
    new: *mut RtValue,
) -> *mut RtValue {
    let sv = unsafe { &*s };
    let ov = unsafe { &*old };
    let nv = unsafe { &*new };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("replace: first argument must be a Str"),
    };
    let old_val = match &ov.data {
        RtData::Str(s) => s,
        _ => rt_error("replace: second argument must be a Str"),
    };
    let new_val = match &nv.data {
        RtData::Str(s) => s,
        _ => rt_error("replace: third argument must be a Str"),
    };
    rt_str(str_val.replace(old_val.as_str(), new_val.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_list, rt_str};

    // Helper: free a list and all its items (for freshly-created lists where items are owned by list)
    unsafe fn free_list_and_items(list: *mut RtValue) {
        let items = (*list).as_list().to_vec();
        drop(Box::from_raw(list));
        for item in items {
            airl_value_release(item);
        }
    }

    #[test]
    fn char_at_ascii() {
        unsafe {
            let s = rt_str("hello".to_string());
            let idx = rt_int(1);
            let r = airl_char_at(s, idx);
            assert_eq!((*r).as_str(), "e");
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(idx);
        }
    }

    #[test]
    fn char_at_unicode() {
        unsafe {
            // "héllo": h=0, é=1, l=2, l=3, o=4
            let s = rt_str("héllo".to_string());
            let idx = rt_int(1);
            let r = airl_char_at(s, idx);
            assert_eq!((*r).as_str(), "é");
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(idx);
        }
    }

    #[test]
    fn substring_basic() {
        unsafe {
            let s = rt_str("hello".to_string());
            let start = rt_int(1);
            let end = rt_int(3);
            let r = airl_substring(s, start, end);
            assert_eq!((*r).as_str(), "el");
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(start);
            airl_value_release(end);
        }
    }

    #[test]
    fn substring_unicode() {
        unsafe {
            // "héllo" chars: h é l l o
            let s = rt_str("héllo".to_string());
            let start = rt_int(0);
            let end = rt_int(2);
            let r = airl_substring(s, start, end);
            assert_eq!((*r).as_str(), "hé");
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(start);
            airl_value_release(end);
        }
    }

    #[test]
    fn chars_basic() {
        unsafe {
            let s = rt_str("hi".to_string());
            let r = airl_chars(s);
            match &(*r).data {
                RtData::List { .. } => {
                    let items = crate::list::list_items(&(*r).data);
                    assert_eq!(items.len(), 2);
                    assert_eq!((*items[0]).as_str(), "h");
                    assert_eq!((*items[1]).as_str(), "i");
                }
                _ => panic!("expected list"),
            }
            free_list_and_items(r);
            airl_value_release(s);
        }
    }

    #[test]
    fn split_basic() {
        unsafe {
            let s = rt_str("a,b,c".to_string());
            let delim = rt_str(",".to_string());
            let r = airl_split(s, delim);
            match &(*r).data {
                RtData::List { .. } => {
                    let items = crate::list::list_items(&(*r).data);
                    assert_eq!(items.len(), 3);
                    assert_eq!((*items[0]).as_str(), "a");
                    assert_eq!((*items[1]).as_str(), "b");
                    assert_eq!((*items[2]).as_str(), "c");
                }
                _ => panic!("expected list"),
            }
            free_list_and_items(r);
            airl_value_release(s);
            airl_value_release(delim);
        }
    }

    #[test]
    fn join_roundtrip() {
        unsafe {
            // split then join should reproduce original
            let s = rt_str("a,b,c".to_string());
            let delim = rt_str(",".to_string());
            let split_result = airl_split(s, delim);

            let sep = rt_str(",".to_string());
            let joined = airl_join(split_result, sep);
            assert_eq!((*joined).as_str(), "a,b,c");

            airl_value_release(joined);
            free_list_and_items(split_result);
            airl_value_release(s);
            airl_value_release(delim);
            airl_value_release(sep);
        }
    }

    #[test]
    fn join_with_non_string_items() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let list = rt_list(vec![a, b]);
            let sep = rt_str("-".to_string());
            let r = airl_join(list, sep);
            assert_eq!((*r).as_str(), "1-2");
            airl_value_release(r);
            airl_value_release(list);
            airl_value_release(sep);
        }
    }

    #[test]
    fn contains_true() {
        unsafe {
            let s = rt_str("hello".to_string());
            let sub = rt_str("ell".to_string());
            let r = airl_contains(s, sub);
            assert!((*r).as_bool());
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(sub);
        }
    }

    #[test]
    fn contains_false() {
        unsafe {
            let s = rt_str("hello".to_string());
            let sub = rt_str("xyz".to_string());
            let r = airl_contains(s, sub);
            assert!(!(*r).as_bool());
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(sub);
        }
    }

    #[test]
    fn starts_with_true() {
        unsafe {
            let s = rt_str("hello".to_string());
            let prefix = rt_str("hel".to_string());
            let r = airl_starts_with(s, prefix);
            assert!((*r).as_bool());
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(prefix);
        }
    }

    #[test]
    fn starts_with_false() {
        unsafe {
            let s = rt_str("hello".to_string());
            let prefix = rt_str("ell".to_string());
            let r = airl_starts_with(s, prefix);
            assert!(!(*r).as_bool());
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(prefix);
        }
    }

    #[test]
    fn ends_with_true() {
        unsafe {
            let s = rt_str("hello".to_string());
            let suffix = rt_str("llo".to_string());
            let r = airl_ends_with(s, suffix);
            assert!((*r).as_bool());
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(suffix);
        }
    }

    #[test]
    fn ends_with_false() {
        unsafe {
            let s = rt_str("hello".to_string());
            let suffix = rt_str("hel".to_string());
            let r = airl_ends_with(s, suffix);
            assert!(!(*r).as_bool());
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(suffix);
        }
    }

    #[test]
    fn index_of_found() {
        unsafe {
            let s = rt_str("hello".to_string());
            let sub = rt_str("ll".to_string());
            let r = airl_index_of(s, sub);
            assert_eq!((*r).as_int(), 2);
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(sub);
        }
    }

    #[test]
    fn index_of_not_found() {
        unsafe {
            let s = rt_str("hello".to_string());
            let sub = rt_str("xyz".to_string());
            let r = airl_index_of(s, sub);
            assert_eq!((*r).as_int(), -1);
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(sub);
        }
    }

    #[test]
    fn index_of_unicode() {
        unsafe {
            // "héllo": h=0, é=1, l=2, l=3, o=4
            // find "ll" starts at char index 2
            let s = rt_str("héllo".to_string());
            let sub = rt_str("ll".to_string());
            let r = airl_index_of(s, sub);
            assert_eq!((*r).as_int(), 2);
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(sub);
        }
    }

    #[test]
    fn trim_basic() {
        unsafe {
            let s = rt_str("  hello  ".to_string());
            let r = airl_trim(s);
            assert_eq!((*r).as_str(), "hello");
            airl_value_release(r);
            airl_value_release(s);
        }
    }

    #[test]
    fn to_upper_basic() {
        unsafe {
            let s = rt_str("hello".to_string());
            let r = airl_to_upper(s);
            assert_eq!((*r).as_str(), "HELLO");
            airl_value_release(r);
            airl_value_release(s);
        }
    }

    #[test]
    fn to_lower_basic() {
        unsafe {
            let s = rt_str("HELLO".to_string());
            let r = airl_to_lower(s);
            assert_eq!((*r).as_str(), "hello");
            airl_value_release(r);
            airl_value_release(s);
        }
    }

    #[test]
    fn replace_basic() {
        unsafe {
            let s = rt_str("aab".to_string());
            let old = rt_str("a".to_string());
            let new = rt_str("x".to_string());
            let r = airl_replace(s, old, new);
            assert_eq!((*r).as_str(), "xxb");
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(old);
            airl_value_release(new);
        }
    }

    #[test]
    fn replace_no_match() {
        unsafe {
            let s = rt_str("hello".to_string());
            let old = rt_str("z".to_string());
            let new = rt_str("x".to_string());
            let r = airl_replace(s, old, new);
            assert_eq!((*r).as_str(), "hello");
            airl_value_release(r);
            airl_value_release(s);
            airl_value_release(old);
            airl_value_release(new);
        }
    }

    #[test]
    fn chars_unicode() {
        unsafe {
            let s = rt_str("héllo".to_string());
            let r = airl_chars(s);
            match &(*r).data {
                RtData::List { .. } => {
                    let items = crate::list::list_items(&(*r).data);
                    assert_eq!(items.len(), 5);
                    assert_eq!((*items[0]).as_str(), "h");
                    assert_eq!((*items[1]).as_str(), "é");
                    assert_eq!((*items[2]).as_str(), "l");
                }
                _ => panic!("expected list"),
            }
            free_list_and_items(r);
            airl_value_release(s);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Character code conversion
// ─────────────────────────────────────────────────────────────────────────────

/// `char-code(s)` — return the Unicode codepoint of the first character as Int.
#[no_mangle]
pub extern "C" fn airl_char_code(s: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let str_val = match &sv.data {
        RtData::Str(s) => s,
        _ => rt_error("char-code: expected string"),
    };
    match str_val.chars().next() {
        Some(ch) => rt_int(ch as i64),
        None => rt_error("char-code: empty string"),
    }
}

/// `char-from-code(n)` — return a 1-character string from a Unicode codepoint.
#[no_mangle]
pub extern "C" fn airl_char_from_code(n: *mut RtValue) -> *mut RtValue {
    let nv = unsafe { &*n };
    let code = match &nv.data {
        RtData::Int(n) => *n as u32,
        _ => rt_error("char-from-code: expected integer"),
    };
    match char::from_u32(code) {
        Some(ch) => rt_str(ch.to_string()),
        None => rt_error(&format!("char-from-code: invalid codepoint {}", code)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Character classification
// ─────────────────────────────────────────────────────────────────────────────

/// `char-alpha?(s)` — true if first char is Unicode alphabetic.
#[no_mangle]
pub extern "C" fn airl_char_alpha(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    rt_bool(input.chars().next().map_or(false, |c| c.is_alphabetic()))
}

/// `char-digit?(s)` — true if first char is ASCII digit 0-9.
#[no_mangle]
pub extern "C" fn airl_char_digit(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    rt_bool(input.chars().next().map_or(false, |c| c.is_ascii_digit()))
}

/// `char-whitespace?(s)` — true if first char is Unicode whitespace.
#[no_mangle]
pub extern "C" fn airl_char_whitespace(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    rt_bool(input.chars().next().map_or(false, |c| c.is_whitespace()))
}

/// `char-upper?(s)` — true if first char is Unicode uppercase.
#[no_mangle]
pub extern "C" fn airl_char_upper(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    rt_bool(input.chars().next().map_or(false, |c| c.is_uppercase()))
}

/// `char-lower?(s)` — true if first char is Unicode lowercase.
#[no_mangle]
pub extern "C" fn airl_char_lower(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    rt_bool(input.chars().next().map_or(false, |c| c.is_lowercase()))
}

/// `string-ci=?(a, b)` — Unicode case-folded equality comparison.
#[no_mangle]
pub extern "C" fn airl_string_ci_eq(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let sa = match unsafe { &(*a).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    let sb = match unsafe { &(*b).data } { RtData::Str(s) => s, _ => return rt_bool(false) };
    rt_bool(sa.to_lowercase() == sb.to_lowercase())
}

/// `string-to-float(s)` — parse string as f64, return Result.
#[no_mangle]
pub extern "C" fn airl_string_to_float(s: *mut RtValue) -> *mut RtValue {
    let sv = unsafe { &*s };
    let str_val = match &sv.data {
        RtData::Str(s) => s.as_str(),
        _ => rt_error("string-to-float: expected string"),
    };
    match str_val.parse::<f64>() {
        Ok(f) => {
            let tag = rt_str("Ok".to_string());
            let inner = crate::value::rt_float(f);
            let result = crate::variant::airl_make_variant(tag, inner);
            crate::memory::airl_value_release(tag);
            result
        }
        Err(_) => {
            let tag = rt_str("Err".to_string());
            let inner = rt_str("invalid float".to_string());
            let result = crate::variant::airl_make_variant(tag, inner);
            crate::memory::airl_value_release(tag);
            result
        }
    }
}
