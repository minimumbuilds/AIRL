#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

#[cfg(not(target_os = "airlos"))]
use std::collections::HashMap;
#[cfg(target_os = "airlos")]
use alloc::collections::BTreeMap as HashMap;

#[cfg(not(target_os = "airlos"))]
use std::fmt::Write;
#[cfg(target_os = "airlos")]
use core::fmt::Write;
#[cfg(not(target_os = "airlos"))]
use std::sync::OnceLock;
use crate::error::rt_error;
use crate::memory::airl_value_retain;
use crate::value::{rt_bool, rt_bytes, rt_float, rt_int, rt_list, rt_list_at, rt_map, rt_nil, rt_str, rt_variant, RtData, RtValue};

#[cfg(not(target_os = "airlos"))]
static SITE_REVERSE_LIST_CLONE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
#[cfg(not(target_os = "airlos"))]
static SITE_TAKE_CLONE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
#[cfg(not(target_os = "airlos"))]
static SITE_DROP_CLONE: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

#[cfg(not(target_os = "airlos"))]
#[inline]
fn site(slot: &'static std::sync::OnceLock<u16>, name: &'static str) -> u16 {
    *slot.get_or_init(|| crate::diag::register_site(name))
}

fn ok_variant(inner: *mut RtValue) -> *mut RtValue {
    rt_variant("Ok".into(), inner)
}

fn err_variant(msg: &str) -> *mut RtValue {
    rt_variant("Err".into(), rt_str(msg.into()))
}

// ─────────────────────────────────────────────────────────────────────────────
// SEC-6: Process execution restriction via AIRL_ALLOW_EXEC env var
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
fn check_exec(command: &str) -> Result<(), *mut RtValue> {
    let bin_name = std::path::Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command);
    // Use env_snapshot() instead of std::env::var — mimalloc corrupts the libc
    // environ block after startup, so std::env::var is unreliable post-init.
    let val = env_snapshot().get("AIRL_ALLOW_EXEC").map(|s| s.as_str()).unwrap_or("");
    match val.trim() {
        "*" => Ok(()),
        v if !v.is_empty() => {
            let allowed: Vec<&str> = v.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if allowed.iter().any(|a| *a == bin_name) {
                Ok(())
            } else {
                Err(err_variant(&format!("shell-exec: '{}' not in AIRL_ALLOW_EXEC allowlist", bin_name)))
            }
        }
        _ => Err(err_variant("shell-exec: disabled (set AIRL_ALLOW_EXEC to enable)")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────


// ─────────────────────────────────────────────────────────────────────────────
// SEC-9: TCP recv size limit (256 MB)
// ─────────────────────────────────────────────────────────────────────────────

const TCP_RECV_MAX_BYTES: usize = 256 * 1024 * 1024;

// ── char-count ──

#[no_mangle]
pub extern "C" fn airl_char_count(s: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*s };
    match &val.data {
        RtData::Str(s) => rt_int(s.chars().count() as i64),
        _ => rt_int(0),
    }
}

// ── str (variadic concat) ──

#[no_mangle]
pub extern "C" fn airl_str_variadic(args: *const *mut RtValue, count: i64) -> *mut RtValue {
    let count = count as usize;
    // Estimate capacity from string arguments
    let mut est_cap = 0usize;
    for i in 0..count {
        let v = unsafe { &**args.add(i) };
        match &v.data {
            RtData::Str(s) => est_cap += s.len(),
            _ => est_cap += 16,
        }
    }
    let mut result = String::with_capacity(est_cap);
    for i in 0..count {
        let v = unsafe { &**args.add(i) };
        match &v.data {
            RtData::Str(s) => result.push_str(s),
            _ => { let _ = write!(result, "{}", v); }
        }
    }
    rt_str(result)
}

// ── format (template with {} placeholders) ──

#[no_mangle]
pub extern "C" fn airl_format_variadic(args: *const *mut RtValue, count: i64) -> *mut RtValue {
    let count = count as usize;
    if count == 0 { return rt_str(String::new()); }
    let tmpl_val = unsafe { &**args.add(0) };
    let tmpl = match &tmpl_val.data {
        RtData::Str(s) => s.as_str(),
        _ => return rt_str(String::new()),
    };
    let mut result = String::with_capacity(tmpl.len() + count * 16);
    let mut arg_idx = 1usize;
    let mut chars = tmpl.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'}') && arg_idx < count {
            chars.next(); // consume '}'
            let v = unsafe { &**args.add(arg_idx) };
            arg_idx += 1;
            match &v.data {
                RtData::Str(s) => result.push_str(s),
                _ => { let _ = write!(result, "{}", v); }
            }
        } else {
            result.push(c);
        }
    }
    rt_str(result)
}

// ── assert ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_assert(cond: *mut RtValue, msg: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*cond };
    let truthy = match &val.data {
        RtData::Bool(b) => *b,
        RtData::Int(n) => *n != 0,
        RtData::Nil => false,
        _ => true,
    };
    if !truthy {
        let msg_val = unsafe { &*msg };
        match &msg_val.data {
            RtData::Str(s) => eprintln!("Assertion failed: {}", s),
            _ => eprintln!("Assertion failed"),
        }
        // Intentional process::exit: AIRL `assert` semantics require program termination on failure.
        // This is not a library error — the AIRL program explicitly requested abort.
        std::process::exit(1);
    }
    rt_bool(true)
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_assert(cond: *mut RtValue, msg: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*cond };
    let truthy = match &val.data {
        RtData::Bool(b) => *b,
        RtData::Int(n) => *n != 0,
        RtData::Nil => false,
        _ => true,
    };
    if !truthy {
        let msg_val = unsafe { &*msg };
        let text = match &msg_val.data {
            RtData::Str(s) => format!("Assertion failed: {}\n", s),
            _ => "Assertion failed\n".to_string(),
        };
        crate::airlos::vga_print(&text);
        crate::airlos::exit(1);
    }
    rt_bool(true)
}

// ── panic ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_panic(msg: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*msg };
    match &val.data {
        RtData::Str(s) => eprintln!("panic: {}", s),
        _ => eprintln!("panic"),
    }
    // Intentional process::exit: AIRL `panic` semantics require program termination.
    // This is not a library error — the AIRL program explicitly requested abort.
    std::process::exit(1);
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_panic(msg: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*msg };
    let text = match &val.data {
        RtData::Str(s) => format!("panic: {}\n", s),
        _ => "panic\n".to_string(),
    };
    crate::airlos::vga_print(&text);
    crate::airlos::exit(1);
}

// ── exit ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_exit(code: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*code };
    let c = match &val.data {
        RtData::Int(n) => *n as i32,
        _ => 1,
    };
    // Intentional process::exit: AIRL `exit` semantics require process termination.
    // This is not a library error — the AIRL program explicitly requested exit with a code.
    std::process::exit(c);
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_exit(code: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*code };
    let c = match &val.data {
        RtData::Int(n) => *n as i32,
        _ => 1,
    };
    crate::airlos::exit(c);
}

// ── sleep ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_sleep(ms: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*ms };
    if let RtData::Int(millis) = &val.data {
        if *millis > 0 {
            std::thread::sleep(std::time::Duration::from_millis(*millis as u64));
        }
    }
    rt_nil()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_sleep(ms: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*ms };
    if let RtData::Int(millis) = &val.data {
        if *millis > 0 {
            // Yield loop with SYS_GET_TICKS
            let target = crate::airlos::get_ticks() + *millis as u64;
            while crate::airlos::get_ticks() < target {}
        }
    }
    rt_nil()
}

// ── format-time ──

#[no_mangle]
pub extern "C" fn airl_format_time(ms_val: *mut RtValue, fmt_val: *mut RtValue) -> *mut RtValue {
    let ms = match unsafe { &(*ms_val).data } {
        RtData::Int(n) => *n,
        _ => return rt_str(String::new()),
    };
    let fmt = match unsafe { &(*fmt_val).data } {
        RtData::Str(s) => s.as_str(),
        _ => return rt_str(String::new()),
    };
    let secs = ms / 1000;
    // Simple UTC formatting using manual computation (same as Rust runtime)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Civil date from days since epoch (Howard Hinnant algorithm)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let result = fmt
        .replace("%Y", &format!("{:04}", y))
        .replace("%m", &format!("{:02}", m))
        .replace("%d", &format!("{:02}", d))
        .replace("%H", &format!("{:02}", hours))
        .replace("%M", &format!("{:02}", minutes))
        .replace("%S", &format!("{:02}", seconds));
    rt_str(result)
}

// ── read-lines ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_read_lines(path: *mut RtValue) -> *mut RtValue {
    let p = match unsafe { &(*path).data } {
        RtData::Str(s) => s.as_str(),
        _ => return rt_list(vec![]),
    };
    let checked = match crate::io::sandbox_check(p) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("read-lines: {}", msg)),
    };
    match std::fs::read_to_string(&checked) {
        Ok(content) => {
            let items: Vec<*mut RtValue> = content.lines().map(|l| rt_str(l.to_string())).collect();
            rt_list(items)
        }
        Err(_) => rt_list(vec![]),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_read_lines(path: *mut RtValue) -> *mut RtValue {
    let p = match unsafe { &(*path).data } {
        RtData::Str(s) => s.as_str(),
        _ => return rt_list(vec![]),
    };
    match crate::airlos::read_file(p) {
        Ok(bytes) => {
            let content = String::from_utf8_lossy(&bytes);
            let items: Vec<*mut RtValue> = content.lines().map(|l| rt_str(l.to_string())).collect();
            rt_list(items)
        }
        Err(_) => rt_list(vec![]),
    }
}

// ── List operations ──

#[no_mangle]
pub extern "C" fn airl_concat_lists(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let a_val = unsafe { &*a };
    let b_val = unsafe { &*b };
    let a_slice = match &a_val.data {
        RtData::List { .. } => crate::list::list_items(&a_val.data),
        _ => &[],
    };
    let b_slice = match &b_val.data {
        RtData::List { .. } => crate::list::list_items(&b_val.data),
        _ => &[],
    };
    let mut items = Vec::with_capacity(a_slice.len() + b_slice.len());
    for &item in a_slice { crate::memory::airl_value_retain(item); items.push(item); }
    for &item in b_slice { crate::memory::airl_value_retain(item); items.push(item); }
    rt_list(items)
}

#[no_mangle]
pub extern "C" fn airl_range(start: *mut RtValue, end: *mut RtValue) -> *mut RtValue {
    let s = match unsafe { &(*start).data } { RtData::Int(n) => *n, _ => return rt_list(vec![]) };
    let e = match unsafe { &(*end).data } { RtData::Int(n) => *n, _ => return rt_list(vec![]) };
    let len = match e.checked_sub(s) {
        Some(n) if n > 0 => n,
        _ => return rt_list(vec![]),
    };
    if len > 10_000_000 {
        rt_error(&format!("range: size {} exceeds 10,000,000 element limit", len));
    }
    let mut items = Vec::with_capacity(len as usize);
    for i in s..e { items.push(rt_int(i)); }
    rt_list(items)
}

#[no_mangle]
pub extern "C" fn airl_reverse_list(list: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &mut *list };
    // COW fast path: sole owner, not a view
    if val.rc.load(core::sync::atomic::Ordering::Relaxed) == 1 {
        if let RtData::List { items, offset, parent } = &mut val.data {
            if parent.is_none() && *offset == 0 {
                items.reverse();
                crate::memory::airl_value_retain(list);
                return list;
            }
        }
    }
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        let reversed: Vec<*mut RtValue> = slice.iter().rev().map(|&i| { crate::memory::airl_value_retain(i); i }).collect();
        rt_list_at(reversed, site(&SITE_REVERSE_LIST_CLONE, "misc.rs:airl_reverse_list.clone-path"))
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_take(n_val: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let n = match unsafe { &(*n_val).data } { RtData::Int(n) => *n as usize, _ => return rt_list(vec![]) };
    let val = unsafe { &*list };
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        let take_n = n.min(slice.len());
        let taken: Vec<*mut RtValue> = slice[..take_n].iter().map(|&i| { crate::memory::airl_value_retain(i); i }).collect();
        rt_list_at(taken, site(&SITE_TAKE_CLONE, "misc.rs:airl_take"))
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_drop(n_val: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let n = match unsafe { &(*n_val).data } { RtData::Int(n) => *n as usize, _ => return rt_list(vec![]) };
    let val = unsafe { &*list };
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        if n >= slice.len() { return rt_list(vec![]); }
        let dropped: Vec<*mut RtValue> = slice[n..].iter().map(|&i| { crate::memory::airl_value_retain(i); i }).collect();
        rt_list_at(dropped, site(&SITE_DROP_CLONE, "misc.rs:airl_drop"))
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_zip(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let a_val = unsafe { &*a };
    let b_val = unsafe { &*b };
    if let (RtData::List { .. }, RtData::List { .. }) = (&a_val.data, &b_val.data) {
        let a_slice = crate::list::list_items(&a_val.data);
        let b_slice = crate::list::list_items(&b_val.data);
        let len = a_slice.len().min(b_slice.len());
        let items: Vec<*mut RtValue> = (0..len).map(|i| {
            crate::memory::airl_value_retain(a_slice[i]);
            crate::memory::airl_value_retain(b_slice[i]);
            rt_list(vec![a_slice[i], b_slice[i]])
        }).collect();
        rt_list(items)
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_flatten(list: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*list };
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        // Pre-estimate capacity to avoid repeated reallocation
        let capacity: usize = slice.iter().map(|&item| {
            let sub = unsafe { &*item };
            if let RtData::List { .. } = &sub.data {
                crate::list::list_len(&sub.data)
            } else {
                1
            }
        }).sum();
        let mut result = Vec::with_capacity(capacity);
        for &item in slice {
            let sub = unsafe { &*item };
            if let RtData::List { .. } = &sub.data {
                for &si in crate::list::list_items(&sub.data) { crate::memory::airl_value_retain(si); result.push(si); }
            } else {
                crate::memory::airl_value_retain(item); result.push(item);
            }
        }
        rt_list(result)
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_enumerate(list: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*list };
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        let result: Vec<*mut RtValue> = slice.iter().enumerate().map(|(i, &item)| {
            crate::memory::airl_value_retain(item);
            rt_list(vec![rt_int(i as i64), item])
        }).collect();
        rt_list(result)
    } else {
        rt_list(vec![])
    }
}

// ── Path operations ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_path_join(parts: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*parts };
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        let mut path = std::path::PathBuf::new();
        for &item in slice {
            let s = unsafe { &*item };
            if let RtData::Str(p) = &s.data { path.push(p); }
        }
        rt_str(path.to_string_lossy().into_owned())
    } else {
        rt_str(String::new())
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_path_join(parts: *mut RtValue) -> *mut RtValue {
    // Simple string-based path join for AIRLOS (no std::path)
    let val = unsafe { &*parts };
    if let RtData::List { .. } = &val.data {
        let slice = crate::list::list_items(&val.data);
        let mut result = alloc::string::String::new();
        for &item in slice {
            let s = unsafe { &*item };
            if let RtData::Str(p) = &s.data {
                if !result.is_empty() && !result.ends_with('/') {
                    result.push('/');
                }
                result.push_str(p);
            }
        }
        rt_str(result)
    } else {
        rt_str(alloc::string::String::new())
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_path_parent(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        let p = std::path::Path::new(s);
        rt_str(p.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
    } else {
        rt_str(String::new())
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_path_parent(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        match s.rfind('/') {
            Some(0) => rt_str("/".into()),
            Some(i) => rt_str(s[..i].into()),
            None => rt_str(alloc::string::String::new()),
        }
    } else {
        rt_str(alloc::string::String::new())
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_path_filename(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        let p = std::path::Path::new(s);
        rt_str(p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
    } else {
        rt_str(String::new())
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_path_filename(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        match s.rfind('/') {
            Some(i) => rt_str(s[i+1..].into()),
            None => rt_str(s.clone()),
        }
    } else {
        rt_str(alloc::string::String::new())
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_path_extension(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        let p = std::path::Path::new(s);
        rt_str(p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default())
    } else {
        rt_str(String::new())
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_path_extension(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        // Get filename, then find last '.'
        let fname = match s.rfind('/') {
            Some(i) => &s[i+1..],
            None => s.as_str(),
        };
        match fname.rfind('.') {
            Some(i) if i > 0 => rt_str(fname[i+1..].into()),
            _ => rt_str(alloc::string::String::new()),
        }
    } else {
        rt_str(alloc::string::String::new())
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_is_absolute(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        rt_bool(std::path::Path::new(s).is_absolute())
    } else {
        rt_bool(false)
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_is_absolute(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        rt_bool(s.starts_with('/'))
    } else {
        rt_bool(false)
    }
}

// ── Regex ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_regex_match(pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.as_str(), _ => return rt_nil() };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => return rt_nil() };
    match regex::Regex::new(pattern) {
        Ok(re) => match re.find(input) {
            Some(m) => rt_str(m.as_str().to_string()),
            None => rt_nil(),
        },
        Err(_) => rt_nil(),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_regex_match(_pat: *mut RtValue, _s: *mut RtValue) -> *mut RtValue {
    rt_nil() // regex not available on AIRLOS
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_regex_find_all(pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.as_str(), _ => return rt_list(vec![]) };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => return rt_list(vec![]) };
    match regex::Regex::new(pattern) {
        Ok(re) => {
            let items: Vec<*mut RtValue> = re.find_iter(input).map(|m| rt_str(m.as_str().to_string())).collect();
            rt_list(items)
        }
        Err(_) => rt_list(vec![]),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_regex_find_all(_pat: *mut RtValue, _s: *mut RtValue) -> *mut RtValue {
    rt_list(vec![])
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_regex_replace(pat: *mut RtValue, s: *mut RtValue, replacement: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.as_str(), _ => return crate::memory::airl_value_clone(s) };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => return crate::memory::airl_value_clone(s) };
    let repl = match unsafe { &(*replacement).data } { RtData::Str(s) => s.as_str(), _ => return crate::memory::airl_value_clone(s) };
    match regex::Regex::new(pattern) {
        Ok(re) => rt_str(re.replace_all(input, repl).into_owned()),
        Err(_) => rt_str(input.to_string()),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_regex_replace(_pat: *mut RtValue, s: *mut RtValue, _replacement: *mut RtValue) -> *mut RtValue {
    crate::memory::airl_value_clone(s) // return input unchanged
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_regex_split(pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.as_str(), _ => return rt_list(vec![]) };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => return rt_list(vec![]) };
    match regex::Regex::new(pattern) {
        Ok(re) => {
            let items: Vec<*mut RtValue> = re.split(input).map(|s| rt_str(s.to_string())).collect();
            rt_list(items)
        }
        Err(_) => rt_list(vec![rt_str(input.to_string())]),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_regex_split(_pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    rt_list(vec![crate::memory::airl_value_clone(s)])
}

// ── Crypto ──
// All crypto functions require external crates (sha2, hmac, base64, hex, etc.)
// which are not available on AIRLOS. Gate everything with cfg.

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_sha256(s: *mut RtValue) -> *mut RtValue {
    use sha2::{Digest, Sha256};
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let hash = Sha256::digest(&input);
    rt_str(hex::encode(hash))
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_hmac_sha256(key: *mut RtValue, msg: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let k = match unsafe { &(*key).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let m = match unsafe { &(*msg).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let mut mac = Hmac::<Sha256>::new_from_slice(&k)
        .unwrap_or_else(|e| rt_error(&format!("hmac-sha256: {e}")));
    mac.update(&m);
    rt_str(hex::encode(mac.finalize().into_bytes()))
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_base64_encode(s: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => "" };
    rt_str(base64::engine::general_purpose::STANDARD.encode(input.as_bytes()))
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_base64_decode(s: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => "" };
    match base64::engine::general_purpose::STANDARD.decode(input.as_bytes()) {
        Ok(bytes) => rt_str(String::from_utf8_lossy(&bytes).into_owned()),
        Err(_) => rt_str(String::new()),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_random_bytes(n: *mut RtValue) -> *mut RtValue {
    let raw = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    if raw < 0 { rt_error("random-bytes: count must be non-negative"); }
    let count = raw as usize;
    if count > 256 * 1024 * 1024 {
        rt_error(&format!("random-bytes: count {} exceeds 256 MiB limit", count));
    }
    use rand::RngCore;
    let mut buf = vec![0u8; count];
    rand::thread_rng().fill_bytes(&mut buf);
    let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
    rt_str(hex)
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_random_bytes(_n: *mut RtValue) -> *mut RtValue {
    err_variant("random-bytes: not available on AIRLOS")
}

// ── Crypto (byte-oriented) ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_sha512(s: *mut RtValue) -> *mut RtValue {
    use sha2::{Digest, Sha512};
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let hash = Sha512::digest(&input);
    rt_str(hex::encode(hash))
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_hmac_sha512(key: *mut RtValue, msg: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    let k = match unsafe { &(*key).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let m = match unsafe { &(*msg).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let mut mac = Hmac::<Sha512>::new_from_slice(&k)
        .unwrap_or_else(|e| rt_error(&format!("hmac-sha512: {e}")));
    mac.update(&m);
    rt_str(hex::encode(mac.finalize().into_bytes()))
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_sha256_bytes(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&bytes);
    rt_bytes(hash.to_vec())
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_sha512_bytes(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    use sha2::Digest;
    let hash = sha2::Sha512::digest(&bytes);
    rt_bytes(hash.to_vec())
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_hmac_sha256_bytes(key: *mut RtValue, data: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    let k = unsafe { borrow_or_extract(key) };
    let d = unsafe { borrow_or_extract(data) };
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&k)
        .unwrap_or_else(|e| rt_error(&format!("hmac-sha256-bytes: {e}")));
    mac.update(&d);
    rt_bytes(mac.finalize().into_bytes().to_vec())
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_bytes_xor(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let a_bytes = extract_bytes(a);
    let b_bytes = extract_bytes(b);
    if a_bytes.len() != b_bytes.len() {
        rt_error("bytes-xor: length mismatch");
    }
    let result: Vec<u8> = a_bytes.iter().zip(b_bytes.iter()).map(|(x, y)| x ^ y).collect();
    rt_bytes(result)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_bytes_xor_scalar(buf: *mut RtValue, scalar: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(buf);
    let s = match unsafe { &(*scalar).data } {
        RtData::Int(n) => (*n & 0xFF) as u8,
        _ => rt_error("bytes-xor-scalar: scalar must be Int"),
    };
    let result: Vec<u8> = bytes.iter().map(|b| b ^ s).collect();
    rt_bytes(result)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_hmac_sha512_bytes(key: *mut RtValue, data: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    let k = unsafe { borrow_or_extract(key) };
    let d = unsafe { borrow_or_extract(data) };
    let mut mac = Hmac::<sha2::Sha512>::new_from_slice(&k)
        .unwrap_or_else(|e| rt_error(&format!("hmac-sha512-bytes: {e}")));
    mac.update(&d);
    rt_bytes(mac.finalize().into_bytes().to_vec())
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_pbkdf2_sha256(password: *mut RtValue, salt: *mut RtValue, iterations: *mut RtValue, key_len: *mut RtValue) -> *mut RtValue {
    let pw = match unsafe { &(*password).data } { RtData::Str(s) => s.as_str(), _ => return rt_bytes(vec![]) };
    let salt_bytes = unsafe { borrow_or_extract(salt) };
    let iters = match unsafe { &(*iterations).data } { RtData::Int(n) => *n as u32, _ => 4096 };
    let klen = match unsafe { &(*key_len).data } { RtData::Int(n) => *n as usize, _ => 32 };
    let mut derived = vec![0u8; klen];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(pw.as_bytes(), &salt_bytes, iters, &mut derived);
    rt_bytes(derived)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_pbkdf2_sha512(password: *mut RtValue, salt: *mut RtValue, iterations: *mut RtValue, key_len: *mut RtValue) -> *mut RtValue {
    let pw = match unsafe { &(*password).data } { RtData::Str(s) => s.as_str(), _ => return rt_bytes(vec![]) };
    let salt_bytes = unsafe { borrow_or_extract(salt) };
    let iters = match unsafe { &(*iterations).data } { RtData::Int(n) => *n as u32, _ => 4096 };
    let klen = match unsafe { &(*key_len).data } { RtData::Int(n) => *n as usize, _ => 64 };
    let mut derived = vec![0u8; klen];
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(pw.as_bytes(), &salt_bytes, iters, &mut derived);
    rt_bytes(derived)
}

// ── AES-256-GCM ──────────────────────────────────────────────────────────────
//
// aes-256-gcm-encrypt key nonce plaintext → (Ok ciphertext-bytes) | (Err msg)
// aes-256-gcm-decrypt key nonce ciphertext → (Ok plaintext-bytes) | (Err msg)
//
// key   : Bytes — must be exactly 32 bytes
// nonce : Bytes — must be exactly 12 bytes (96-bit GCM standard)
// The ciphertext returned by encrypt has the 16-byte authentication tag appended.
// decrypt expects ciphertext produced by encrypt (tag-appended form).

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_aes_256_gcm_encrypt(
    key: *mut RtValue,
    nonce: *mut RtValue,
    plaintext: *mut RtValue,
) -> *mut RtValue {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Key, Nonce,
    };
    let key_bytes = unsafe { borrow_or_extract(key) };
    let nonce_bytes = unsafe { borrow_or_extract(nonce) };
    let pt_bytes = unsafe { borrow_or_extract(plaintext) };

    if key_bytes.len() != 32 {
        return err_variant(&format!("aes-256-gcm-encrypt: key must be 32 bytes, got {}", key_bytes.len()));
    }
    if nonce_bytes.len() != 12 {
        return err_variant(&format!("aes-256-gcm-encrypt: nonce must be 12 bytes, got {}", nonce_bytes.len()));
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.encrypt(nonce, pt_bytes.as_ref()) {
        Ok(ct) => ok_variant(rt_bytes(ct)),
        Err(e) => err_variant(&format!("aes-256-gcm-encrypt: {e}")),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_aes_256_gcm_encrypt(
    _key: *mut RtValue,
    _nonce: *mut RtValue,
    _plaintext: *mut RtValue,
) -> *mut RtValue {
    err_variant("aes-256-gcm-encrypt: not available on AIRLOS")
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_aes_256_gcm_decrypt(
    key: *mut RtValue,
    nonce: *mut RtValue,
    ciphertext: *mut RtValue,
) -> *mut RtValue {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Key, Nonce,
    };
    let key_bytes = unsafe { borrow_or_extract(key) };
    let nonce_bytes = unsafe { borrow_or_extract(nonce) };
    let ct_bytes = unsafe { borrow_or_extract(ciphertext) };

    if key_bytes.len() != 32 {
        return err_variant(&format!("aes-256-gcm-decrypt: key must be 32 bytes, got {}", key_bytes.len()));
    }
    if nonce_bytes.len() != 12 {
        return err_variant(&format!("aes-256-gcm-decrypt: nonce must be 12 bytes, got {}", nonce_bytes.len()));
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&nonce_bytes);
    match cipher.decrypt(nonce, ct_bytes.as_ref()) {
        Ok(pt) => ok_variant(rt_bytes(pt)),
        Err(_) => err_variant("aes-256-gcm-decrypt: authentication failed (bad key, nonce, or corrupted ciphertext)"),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_aes_256_gcm_decrypt(
    _key: *mut RtValue,
    _nonce: *mut RtValue,
    _ciphertext: *mut RtValue,
) -> *mut RtValue {
    err_variant("aes-256-gcm-decrypt: not available on AIRLOS")
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_base64_decode_bytes(data: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let bytes = unsafe { borrow_or_extract(data) };
    match base64::engine::general_purpose::STANDARD.decode(&*bytes) {
        Ok(decoded) => rt_bytes(decoded),
        Err(_) => rt_bytes(vec![]),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_base64_encode_bytes(data: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let bytes = unsafe { borrow_or_extract(data) };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&*bytes);
    rt_bytes(encoded.into_bytes())
}

#[no_mangle]
pub extern "C" fn airl_bitwise_xor(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = match unsafe { &(*a).data } { RtData::Int(n) => *n, _ => 0 };
    let vb = match unsafe { &(*b).data } { RtData::Int(n) => *n, _ => 0 };
    rt_int(va ^ vb)
}

#[no_mangle]
pub extern "C" fn airl_bitwise_and(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = match unsafe { &(*a).data } { RtData::Int(n) => *n, _ => 0 };
    let vb = match unsafe { &(*b).data } { RtData::Int(n) => *n, _ => 0 };
    rt_int(va & vb)
}

#[no_mangle]
pub extern "C" fn airl_bitwise_or(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = match unsafe { &(*a).data } { RtData::Int(n) => *n, _ => 0 };
    let vb = match unsafe { &(*b).data } { RtData::Int(n) => *n, _ => 0 };
    rt_int(va | vb)
}

#[no_mangle]
pub extern "C" fn airl_bitwise_shr(a: *mut RtValue, n: *mut RtValue) -> *mut RtValue {
    let va = match unsafe { &(*a).data } { RtData::Int(n) => *n, _ => 0 };
    let vn = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    rt_int(((va as u64) >> ((vn as u64) & 63)) as i64)
}

#[no_mangle]
pub extern "C" fn airl_bitwise_shl(a: *mut RtValue, n: *mut RtValue) -> *mut RtValue {
    let va = match unsafe { &(*a).data } { RtData::Int(n) => *n, _ => 0 };
    let vn = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    rt_int(((va as u64) << ((vn as u64) & 63)) as i64)
}

// ── Type conversions ──

#[no_mangle]
pub extern "C" fn airl_int_to_string(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    rt_str(val.to_string())
}

#[no_mangle]
pub extern "C" fn airl_float_to_string(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Float(f) => *f, _ => 0.0 };
    let s = if val == (val as i64 as f64) && val.is_finite() { format!("{:.1}", val) } else { format!("{}", val) };
    rt_str(s)
}

#[no_mangle]
pub extern "C" fn airl_string_to_int(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("not a string") };
    match input.parse::<i64>() {
        Ok(v) => ok_variant(rt_int(v)),
        Err(e) => err_variant(&format!("invalid int: {}", e)),
    }
}

// ── System ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_cpu_count() -> *mut RtValue {
    rt_int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_cpu_count() -> *mut RtValue {
    rt_int(1) // AIRLOS is single-core
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_time_now() -> *mut RtValue {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    rt_int(ms)
}

/// `rt-stats(label)` — print a one-line runtime allocator snapshot to stderr,
/// tagged with the given label. Returns Nil. Used for investigating leak
/// sources at phase and file boundaries in the bootstrap compiler.
///
/// When `AIRL_RT_TRACE` is unset, this is effectively no-op: the diag
/// counters are only bumped once tracing has been initialized, so reading
/// them without tracing will show zeros. We still print, so scripts can
/// detect the call ran.
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_rt_stats(label: *mut RtValue) -> *mut RtValue {
    let s = match unsafe { &(*label).data } {
        RtData::Str(s) => s.as_str(),
        _ => "?",
    };
    crate::diag::print_stats(s);
    crate::value::rt_nil()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_rt_stats(_label: *mut RtValue) -> *mut RtValue {
    crate::value::rt_nil()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_time_now() -> *mut RtValue {
    rt_int(crate::airlos::get_ticks() as i64)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_getenv(name: *mut RtValue) -> *mut RtValue {
    let key = match unsafe { &(*name).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("not a string") };
    // Read from cached environment snapshot taken at process start (before
    // mimalloc can corrupt the libc environ block).
    let env_cache = env_snapshot();
    match env_cache.get(key) {
        Some(val) => ok_variant(rt_str(val.clone())),
        None => err_variant(&format!("env var not found: {}", key)),
    }
}

#[cfg(not(target_os = "airlos"))]
fn env_snapshot() -> &'static std::collections::HashMap<String, String> {
    static CACHE: OnceLock<std::collections::HashMap<String, String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        std::env::vars().collect()
    })
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_getenv(_name: *mut RtValue) -> *mut RtValue {
    err_variant("getenv: not available on AIRLOS")
}

#[cfg(not(target_os = "airlos"))]
fn extract_cmd_args(cmd: *mut RtValue, args_list: *mut RtValue) -> Result<(String, Vec<String>), *mut RtValue> {
    let command = match unsafe { &(*cmd).data } { RtData::Str(s) => s.clone(), _ => return Err(err_variant("shell-exec: not a string")) };
    let mut cmd_args = Vec::new();
    if let RtData::List { .. } = unsafe { &(*args_list).data } {
        for &item in crate::list::list_items(unsafe { &(*args_list).data }) {
            if let RtData::Str(s) = unsafe { &(*item).data } { cmd_args.push(s.clone()); }
        }
    }
    Ok((command, cmd_args))
}

#[cfg(not(target_os = "airlos"))]
fn output_to_map(output: std::process::Output) -> *mut RtValue {
    let mut m = HashMap::new();
    m.insert("stdout".to_string(), rt_str(String::from_utf8_lossy(&output.stdout).into_owned()));
    m.insert("stderr".to_string(), rt_str(String::from_utf8_lossy(&output.stderr).into_owned()));
    m.insert("exit-code".to_string(), rt_int(output.status.code().unwrap_or(-1) as i64));
    ok_variant(rt_map(m))
}

// SEC-6: Gated by AIRL_ALLOW_EXEC env var. Uses Command::new().args() (safe, no shell).
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_shell_exec(cmd: *mut RtValue, args_list: *mut RtValue) -> *mut RtValue {
    let (command, cmd_args) = match extract_cmd_args(cmd, args_list) { Ok(v) => v, Err(e) => return e };
    if let Err(e) = check_exec(&command) { return e; }
    match std::process::Command::new(&command).args(&cmd_args).output() {
        Ok(output) => output_to_map(output),
        Err(e) => err_variant(&format!("exec: {}", e)),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_shell_exec(_cmd: *mut RtValue, _args_list: *mut RtValue) -> *mut RtValue {
    err_variant("exec: not supported on AIRLOS")
}

// SEC-6: Gated by AIRL_ALLOW_EXEC env var. Uses Command::new().args() (safe, no shell).
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_shell_exec_with_stdin(cmd: *mut RtValue, args_list: *mut RtValue, stdin_data: *mut RtValue) -> *mut RtValue {
    let (command, cmd_args) = match extract_cmd_args(cmd, args_list) { Ok(v) => v, Err(e) => return e };
    if let Err(e) = check_exec(&command) { return e; }
    let stdin_str = match unsafe { &(*stdin_data).data } {
        RtData::Str(s) => s.clone(),
        _ => return err_variant("exec-with-stdin: stdin not a string"),
    };
    let mut child = match std::process::Command::new(&command)
        .args(&cmd_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return err_variant(&format!("exec-with-stdin: {}", e)),
    };
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(stdin_str.as_bytes());
    }
    match child.wait_with_output() {
        Ok(output) => output_to_map(output),
        Err(e) => err_variant(&format!("exec-with-stdin: {}", e)),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_shell_exec_with_stdin(_cmd: *mut RtValue, _args_list: *mut RtValue, _stdin_data: *mut RtValue) -> *mut RtValue {
    err_variant("exec-with-stdin: not supported on AIRLOS")
}

// ── Radix Parsing ──

#[no_mangle]
pub extern "C" fn airl_parse_int_radix(s: *mut RtValue, base: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("parse-int-radix: not a string") };
    let radix = match unsafe { &(*base).data } { RtData::Int(n) => *n as u32, _ => return err_variant("parse-int-radix: base not an int") };
    if !(2..=36).contains(&radix) { return err_variant("parse-int-radix: base must be 2-36"); }
    match i64::from_str_radix(input, radix) {
        Ok(v) => ok_variant(rt_int(v)),
        Err(e) => err_variant(&format!("parse-int-radix: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_int_to_string_radix(n: *mut RtValue, base: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => return rt_str("NaN".into()) };
    let radix = match unsafe { &(*base).data } { RtData::Int(n) => *n as u32, _ => return rt_str("?".into()) };
    if !(2..=36).contains(&radix) { return rt_str("?base".into()); }
    if val == 0 { return rt_str("0".into()); }
    let negative = val < 0;
    let mut v = if negative { (val as i128).unsigned_abs() } else { val as u128 };
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while v > 0 {
        buf.push(digits[(v % radix as u128) as usize]);
        v /= radix as u128;
    }
    if negative { buf.push(b'-'); }
    buf.reverse();
    rt_str(String::from_utf8(buf).unwrap())
}

// ── System Utilities ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_get_cwd() -> *mut RtValue {
    match std::env::current_dir() {
        Ok(p) => rt_str(p.to_string_lossy().into_owned()),
        Err(e) => rt_str(format!("<cwd-error: {}>", e)),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_get_cwd() -> *mut RtValue {
    rt_str("/".to_string())
}

// ── JSON ──

/// Maximum JSON parsing recursion depth to prevent stack overflow.
const JSON_MAX_DEPTH: usize = 128;

#[no_mangle]
pub extern "C" fn airl_json_parse(text: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*text).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("json-parse: not a string") };
    match parse_json_value(input.trim(), 0) {
        Some((val, _)) => ok_variant(val),
        None => err_variant(&format!("json-parse: invalid JSON: {}", input)),
    }
}

/// Minimal recursive-descent JSON parser returning (*mut RtValue, remaining_input).
fn parse_json_value(input: &str, depth: usize) -> Option<(*mut RtValue, &str)> {
    if depth > JSON_MAX_DEPTH { return None; }
    let s = input.trim_start();
    if s.is_empty() { return None; }
    match s.as_bytes()[0] {
        b'"' => parse_json_string(s),
        b'{' => parse_json_object(s, depth),
        b'[' => parse_json_array(s, depth),
        b't' if s.starts_with("true") => Some((rt_bool(true), &s[4..])),
        b'f' if s.starts_with("false") => Some((rt_bool(false), &s[5..])),
        b'n' if s.starts_with("null") => Some((rt_nil(), &s[4..])),
        _ => parse_json_number(s),
    }
}

fn parse_json_string(s: &str) -> Option<(*mut RtValue, &str)> {
    if !s.starts_with('"') { return None; }
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'"' => { result.push('"'); i += 2; }
                b'\\' => { result.push('\\'); i += 2; }
                b'/' => { result.push('/'); i += 2; }
                b'n' => { result.push('\n'); i += 2; }
                b'r' => { result.push('\r'); i += 2; }
                b't' => { result.push('\t'); i += 2; }
                b'b' => { result.push('\u{0008}'); i += 2; }
                b'f' => { result.push('\u{000C}'); i += 2; }
                b'u' if i + 5 < bytes.len() => {
                    if let Ok(cp) = u32::from_str_radix(&s[i+2..i+6], 16) {
                        if let Some(ch) = char::from_u32(cp) { result.push(ch); }
                    }
                    i += 6;
                }
                _ => { result.push(bytes[i] as char); i += 1; }
            }
        } else if bytes[i] == b'"' {
            return Some((rt_str(result), &s[i+1..]));
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    None
}

fn parse_json_number(s: &str) -> Option<(*mut RtValue, &str)> {
    let mut end = 0;
    let bytes = s.as_bytes();
    if end < bytes.len() && bytes[end] == b'-' { end += 1; }
    while end < bytes.len() && bytes[end].is_ascii_digit() { end += 1; }
    let mut is_float = false;
    if end < bytes.len() && bytes[end] == b'.' {
        is_float = true; end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() { end += 1; }
    }
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        is_float = true; end += 1;
        if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') { end += 1; }
        while end < bytes.len() && bytes[end].is_ascii_digit() { end += 1; }
    }
    if end == 0 { return None; }
    let num_str = &s[..end];
    if is_float {
        num_str.parse::<f64>().ok().map(|f| (rt_float(f), &s[end..]))
    } else {
        num_str.parse::<i64>().ok().map(|n| (rt_int(n), &s[end..]))
    }
}

fn parse_json_array(s: &str, depth: usize) -> Option<(*mut RtValue, &str)> {
    let mut rest = s[1..].trim_start(); // skip '['
    let mut items: Vec<*mut RtValue> = Vec::new();
    if rest.starts_with(']') { return Some((rt_list(items), &rest[1..])); }
    loop {
        let (val, r) = parse_json_value(rest, depth + 1)?;
        items.push(val);
        rest = r.trim_start();
        if rest.starts_with(',') { rest = rest[1..].trim_start(); }
        else if rest.starts_with(']') { return Some((rt_list(items), &rest[1..])); }
        else { return None; }
    }
}

fn parse_json_object(s: &str, depth: usize) -> Option<(*mut RtValue, &str)> {
    let mut rest = s[1..].trim_start(); // skip '{'
    let mut map: HashMap<String, *mut RtValue> = HashMap::new();
    if rest.starts_with('}') { return Some((rt_map(map), &rest[1..])); }
    loop {
        // Parse key (must be string)
        let (key_val, r) = parse_json_string(rest.trim_start())?;
        let key = unsafe { match &(*key_val).data { RtData::Str(s) => s.clone(), _ => return None } };
        crate::memory::airl_value_release(key_val);
        rest = r.trim_start();
        if !rest.starts_with(':') { return None; }
        rest = rest[1..].trim_start();
        let (val, r) = parse_json_value(rest, depth + 1)?;
        map.insert(key, val);
        rest = r.trim_start();
        if rest.starts_with(',') { rest = rest[1..].trim_start(); }
        else if rest.starts_with('}') { return Some((rt_map(map), &rest[1..])); }
        else { return None; }
    }
}

fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[no_mangle]
pub extern "C" fn airl_json_stringify(val: *mut RtValue) -> *mut RtValue {
    fn to_json(v: &RtValue) -> String {
        match &v.data {
            RtData::Str(s) => escape_json_string(s),
            RtData::Int(n) => n.to_string(),
            RtData::Float(f) => f.to_string(),
            RtData::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            RtData::Nil | RtData::Unit => "null".to_string(),
            RtData::List { .. } => {
                let slice = crate::list::list_items(&v.data);
                let parts: Vec<String> = slice.iter().map(|&p| {
                    let inner = unsafe { &*p };
                    to_json(inner)
                }).collect();
                format!("[{}]", parts.join(","))
            }
            RtData::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let parts: Vec<String> = keys.iter().map(|k| {
                    let val = unsafe { &*m[*k] };
                    format!("{}:{}", escape_json_string(k), to_json(val))
                }).collect();
                format!("{{{}}}", parts.join(","))
            }
            RtData::Variant { tag_name, inner } => {
                let inner_v = unsafe { &**inner };
                match &inner_v.data {
                    RtData::Unit => format!("\"({})\"", tag_name),
                    _ => format!("\"({} {})\"", tag_name, to_json(inner_v)),
                }
            }
            RtData::Closure { .. } => "\"<closure>\"".to_string(),
            RtData::Bytes(v) => format!("\"<Bytes len={}>\"", v.len()),
            RtData::PartialApp { func_name, remaining_arity, .. } => {
                format!("\"<partial {} remaining={}>\"", func_name, remaining_arity)
            }
            RtData::BCFuncNative(bcf) => {
                format!("\"<bcfunc {} arity={}>\"", bcf.name, bcf.arity)
            }
        }
    }
    let v = unsafe { &*val };
    rt_str(to_json(v))
}

// ── TCP sockets, TLS, compression ──
// These features require OS networking and external crates not available on AIRLOS.
// The entire block below is gated with cfg(not(target_os = "airlos")).
#[cfg(not(target_os = "airlos"))]
use std::net::TcpStream;
#[cfg(not(target_os = "airlos"))]
use std::sync::atomic::{AtomicI64, Ordering};
#[cfg(not(target_os = "airlos"))]
use std::io::{Read, Write as IoWrite};

#[cfg(not(target_os = "airlos"))]
enum RtTcpHandle {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
    TlsServer(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

#[cfg(not(target_os = "airlos"))]
impl Read for RtTcpHandle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self { RtTcpHandle::Plain(s) => s.read(buf), RtTcpHandle::Tls(s) => s.read(buf), RtTcpHandle::TlsServer(s) => s.read(buf) }
    }
}

#[cfg(not(target_os = "airlos"))]
impl IoWrite for RtTcpHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self { RtTcpHandle::Plain(s) => s.write(buf), RtTcpHandle::Tls(s) => s.write(buf), RtTcpHandle::TlsServer(s) => s.write(buf) }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self { RtTcpHandle::Plain(s) => s.flush(), RtTcpHandle::Tls(s) => s.flush(), RtTcpHandle::TlsServer(s) => s.flush() }
    }
}

#[cfg(not(target_os = "airlos"))]
impl RtTcpHandle {
    fn set_timeout(&self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        let stream = match self { RtTcpHandle::Plain(s) => s, RtTcpHandle::Tls(s) => s.get_ref(), RtTcpHandle::TlsServer(s) => s.get_ref() };
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)?;
        Ok(())
    }
}

#[cfg(not(target_os = "airlos"))]
static NEXT_TCP_HANDLE: AtomicI64 = AtomicI64::new(1);

#[cfg(not(target_os = "airlos"))]
fn tcp_handles() -> &'static std::sync::Mutex<std::collections::HashMap<i64, RtTcpHandle>> {
    use std::sync::{Mutex, OnceLock};
    static HANDLES: OnceLock<Mutex<std::collections::HashMap<i64, RtTcpHandle>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

// ── TCP server (listen/accept) ──────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
static NEXT_LISTENER_HANDLE: AtomicI64 = AtomicI64::new(1);

#[cfg(not(target_os = "airlos"))]
fn tcp_listeners() -> &'static std::sync::Mutex<std::collections::HashMap<i64, std::net::TcpListener>> {
    use std::sync::{Mutex, OnceLock};
    static LISTENERS: OnceLock<Mutex<std::collections::HashMap<i64, std::net::TcpListener>>> = OnceLock::new();
    LISTENERS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Bind a TCP server socket. Returns Result[handle, error].
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_listen(port: *mut RtValue, backlog: *mut RtValue) -> *mut RtValue {
    let p = match unsafe { &(*port).data } { RtData::Int(n) => *n as u16, _ => return err_variant("tcp-listen: port must be int") };
    let _ = match unsafe { &(*backlog).data } { RtData::Int(n) => *n, _ => 128 }; // backlog hint (OS may ignore)
    match std::net::TcpListener::bind(format!("0.0.0.0:{}", p)) {
        Ok(listener) => {
            let handle = NEXT_LISTENER_HANDLE.fetch_add(1, Ordering::SeqCst);
            tcp_listeners().lock().unwrap().insert(handle, listener);
            ok_variant(rt_int(handle))
        }
        Err(e) => err_variant(&format!("tcp-listen: {}", e)),
    }
}

/// Accept a connection on a listening socket. Blocking. Returns Result[conn-handle, error].
/// The returned handle is a regular TCP connection handle (same as tcp-connect returns).
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_accept(listener_handle: *mut RtValue) -> *mut RtValue {
    let lh = match unsafe { &(*listener_handle).data } { RtData::Int(n) => *n, _ => return err_variant("tcp-accept: handle must be int") };
    // Remove listener from map so we can block on accept without holding the lock
    let listener = tcp_listeners().lock().unwrap().remove(&lh);
    match listener {
        Some(listener) => {
            let result = match listener.accept() {
                Ok((stream, _addr)) => {
                    // SEC-17: Set default 30s read/write timeouts on accepted connections.
                    // These can be overridden by the AIRL program via tcp-set-timeout.
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
                    stream.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();
                    let conn_handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
                    tcp_handles().lock().unwrap().insert(conn_handle, RtTcpHandle::Plain(stream));
                    ok_variant(rt_int(conn_handle))
                }
                Err(e) => err_variant(&format!("tcp-accept: {}", e)),
            };
            // Put listener back
            tcp_listeners().lock().unwrap().insert(lh, listener);
            result
        }
        None => err_variant(&format!("tcp-accept: invalid listener handle {}", lh)),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_connect(host: *mut RtValue, port: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*host).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("host must be string") };
    let p = match unsafe { &(*port).data } { RtData::Int(n) => *n as u16, _ => return err_variant("port must be int") };
    match TcpStream::connect(format!("{}:{}", h, p)) {
        Ok(stream) => {
            // SEC-17: Set default 30s read/write timeouts to prevent indefinite hangs.
            // These can be overridden by the AIRL program via tcp-set-timeout.
            stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
            stream.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();
            let handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
            tcp_handles().lock().unwrap().insert(handle, RtTcpHandle::Plain(stream));
            ok_variant(rt_int(handle))
        }
        Err(e) => err_variant(&format!("tcp-connect: {}", e)),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_close(handle: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    match tcp_handles().lock().unwrap().remove(&h) { Some(_) => ok_variant(rt_nil()), None => err_variant("invalid handle") }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_send(handle: *mut RtValue, data: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let bytes: Vec<u8> = match unsafe { &(*data).data } {
        RtData::Bytes(v) => v.clone(),
        RtData::List { .. } => crate::list::list_items(unsafe { &(*data).data }).iter().map(|i| match unsafe { &(**i).data } { RtData::Int(n) => *n as u8, _ => 0 }).collect(),
        _ => return err_variant("data must be bytes or list"),
    };
    // Remove handle from map during I/O to avoid holding the mutex across blocking write.
    // This prevents deadlock when multiple threads do concurrent TCP operations.
    let stream = tcp_handles().lock().unwrap().remove(&h);
    match stream {
        Some(mut s) => {
            let result = match s.write_all(&bytes) {
                Ok(()) => ok_variant(rt_int(bytes.len() as i64)),
                Err(e) => err_variant(&format!("tcp-send: {}", e)),
            };
            tcp_handles().lock().unwrap().insert(h, s);
            result
        }
        None => err_variant("invalid handle"),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_recv(handle: *mut RtValue, max_bytes: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let max = match unsafe { &(*max_bytes).data } { RtData::Int(n) => *n as usize, _ => return err_variant("max must be int") };
    // SEC-9: Cap recv buffer at 256 MB to prevent OOM from untrusted input
    if max > TCP_RECV_MAX_BYTES {
        return err_variant("tcp-recv: requested size exceeds 256MB limit");
    }
    let stream = tcp_handles().lock().unwrap().remove(&h);
    match stream {
        Some(mut s) => {
            let mut buf = vec![0u8; max];
            let result = match s.read(&mut buf) {
                Ok(n) => { buf.truncate(n); ok_variant(rt_bytes(buf)) }
                Err(e) => err_variant(&format!("tcp-recv: {}", e)),
            };
            tcp_handles().lock().unwrap().insert(h, s);
            result
        }
        None => err_variant("invalid handle"),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_recv_exact(handle: *mut RtValue, count: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let n = match unsafe { &(*count).data } { RtData::Int(n) => *n as usize, _ => return err_variant("count must be int") };
    // SEC-9: Cap recv buffer at 256 MB to prevent OOM from untrusted input
    if n > TCP_RECV_MAX_BYTES {
        return err_variant("tcp-recv: requested size exceeds 256MB limit");
    }
    let stream = tcp_handles().lock().unwrap().remove(&h);
    match stream {
        Some(mut s) => {
            let mut buf = vec![0u8; n];
            let result = match s.read_exact(&mut buf) {
                Ok(()) => ok_variant(rt_bytes(buf)),
                Err(e) => err_variant(&format!("tcp-recv-exact: {}", e)),
            };
            tcp_handles().lock().unwrap().insert(h, s);
            result
        }
        None => err_variant("invalid handle"),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_set_timeout(handle: *mut RtValue, ms: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let millis = match unsafe { &(*ms).data } { RtData::Int(n) => *n, _ => return err_variant("ms must be int") };
    let timeout = if millis > 0 { Some(std::time::Duration::from_millis(millis as u64)) } else { None };
    let handles = tcp_handles().lock().unwrap();
    match handles.get(&h) {
        Some(tcp_handle) => match tcp_handle.set_timeout(timeout) { Ok(()) => ok_variant(rt_nil()), Err(e) => err_variant(&format!("tcp-set-timeout: {}", e)) },
        None => err_variant("invalid handle"),
    }
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_connect_tls(host: *mut RtValue, port: *mut RtValue, ca_path: *mut RtValue, cert_path: *mut RtValue, key_path: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*host).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("host must be string") };
    let p = match unsafe { &(*port).data } { RtData::Int(n) => *n as u16, _ => return err_variant("port must be int") };
    let ca = match unsafe { &(*ca_path).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("ca-path must be string") };
    let cert = match unsafe { &(*cert_path).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("cert-path must be string") };
    let key = match unsafe { &(*key_path).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("key-path must be string") };

    let mut root_store = rustls::RootCertStore::empty();
    if ca.is_empty() {
        // SEC-16: Warn when no CA path is specified so operators notice the fallback.
        // Security implication: any certificate signed by a publicly-trusted CA will be
        // accepted. If the caller intended CA pinning, passing "" bypasses that intent.
        eprintln!("warning: tcp-connect-tls: no CA path specified, using system root certificates");
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    } else {
        let ca_data = match std::fs::read(&ca) { Ok(d) => d, Err(e) => return err_variant(&format!("tcp-connect-tls: read CA: {}", e)) };
        let certs: Vec<_> = match rustls_pemfile::certs(&mut &ca_data[..]).collect::<Result<Vec<_>, _>>() { Ok(c) => c, Err(e) => return err_variant(&format!("tcp-connect-tls: parse CA: {}", e)) };
        for c in certs { let _ = root_store.add(c); }
    }

    let config_builder = rustls::ClientConfig::builder().with_root_certificates(root_store);
    let config = if !cert.is_empty() && !key.is_empty() {
        let cert_data = match std::fs::read(&cert) { Ok(d) => d, Err(e) => return err_variant(&format!("read cert: {}", e)) };
        let key_data = match std::fs::read(&key) { Ok(d) => d, Err(e) => return err_variant(&format!("read key: {}", e)) };
        let certs: Vec<_> = match rustls_pemfile::certs(&mut &cert_data[..]).collect::<Result<Vec<_>, _>>() { Ok(c) => c, Err(e) => return err_variant(&format!("parse cert: {}", e)) };
        let pkey = match rustls_pemfile::private_key(&mut &key_data[..]) { Ok(Some(k)) => k, _ => return err_variant("no private key found") };
        match config_builder.with_client_auth_cert(certs, pkey) { Ok(c) => c, Err(e) => return err_variant(&format!("client auth: {}", e)) }
    } else {
        config_builder.with_no_client_auth()
    };

    let tcp = match TcpStream::connect(format!("{}:{}", h, p)) { Ok(s) => s, Err(e) => return err_variant(&format!("tcp-connect-tls: {}", e)) };
    // SEC-17: Set default 30s read/write timeouts on the underlying TCP stream.
    // These can be overridden by the AIRL program via tcp-set-timeout.
    tcp.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
    tcp.set_write_timeout(Some(std::time::Duration::from_secs(30))).ok();
    let server_name = match rustls::pki_types::ServerName::try_from(h.to_string()) { Ok(n) => n, Err(e) => return err_variant(&format!("invalid hostname: {}", e)) };
    let conn = match rustls::ClientConnection::new(std::sync::Arc::new(config), server_name) { Ok(c) => c, Err(e) => return err_variant(&format!("tls init: {}", e)) };
    let tls_stream = rustls::StreamOwned::new(conn, tcp);

    let handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
    tcp_handles().lock().unwrap().insert(handle, RtTcpHandle::Tls(Box::new(tls_stream)));
    ok_variant(rt_int(handle))
}

/// Upgrade an already-accepted plain TCP handle to server-side TLS.
/// Signature: (conn-handle : i64, cert-path : String, key-path : String) -> Result[i64, String]
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_tcp_accept_tls(
    conn_handle: *mut RtValue,
    cert_path: *mut RtValue,
    key_path: *mut RtValue,
) -> *mut RtValue {
    let ch = match unsafe { &(*conn_handle).data } { RtData::Int(n) => *n, _ => return err_variant("tcp-accept-tls: handle must be Int") };
    let cert_file = match unsafe { &(*cert_path).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("tcp-accept-tls: cert-path must be String") };
    let key_file = match unsafe { &(*key_path).data } { RtData::Str(s) => s.as_str(), _ => return err_variant("tcp-accept-tls: key-path must be String") };

    let cert_data = match std::fs::read(&cert_file) { Ok(d) => d, Err(e) => return err_variant(&format!("tcp-accept-tls: read cert: {}", e)) };
    let key_data = match std::fs::read(&key_file) { Ok(d) => d, Err(e) => return err_variant(&format!("tcp-accept-tls: read key: {}", e)) };

    let certs: Vec<_> = match rustls_pemfile::certs(&mut &cert_data[..]).collect::<Result<Vec<_>, _>>() {
        Ok(c) => c,
        Err(e) => return err_variant(&format!("tcp-accept-tls: parse cert: {}", e)),
    };
    let pkey = match rustls_pemfile::private_key(&mut &key_data[..]) {
        Ok(Some(k)) => k,
        _ => return err_variant("tcp-accept-tls: no private key found"),
    };

    let config = match rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, pkey)
    {
        Ok(c) => c,
        Err(e) => return err_variant(&format!("tcp-accept-tls: server config: {}", e)),
    };

    let removed = tcp_handles().lock().unwrap().remove(&ch);
    let plain_stream = match removed {
        Some(RtTcpHandle::Plain(s)) => s,
        Some(other) => {
            // Re-insert the existing handle so it isn't lost
            tcp_handles().lock().unwrap().insert(ch, other);
            return err_variant("tcp-accept-tls: handle is already TLS");
        }
        None => return err_variant(&format!("tcp-accept-tls: invalid handle {}", ch)),
    };

    let server_conn = match rustls::ServerConnection::new(std::sync::Arc::new(config)) {
        Ok(c) => c,
        Err(e) => {
            tcp_handles().lock().unwrap().insert(ch, RtTcpHandle::Plain(plain_stream));
            return err_variant(&format!("tcp-accept-tls: server connection: {}", e));
        }
    };

    let tls_stream = rustls::StreamOwned::new(server_conn, plain_stream);
    tcp_handles().lock().unwrap().insert(ch, RtTcpHandle::TlsServer(Box::new(tls_stream)));
    ok_variant(rt_int(ch))
}

// ── Byte encoding ──

#[no_mangle]
pub extern "C" fn airl_bytes_new_empty() -> *mut RtValue {
    rt_bytes(Vec::new())
}

/// Allocate a zero-filled byte array of the given size.
#[no_mangle]
pub extern "C" fn airl_bytes_alloc(n: *mut RtValue) -> *mut RtValue {
    const MAX_ALLOC: usize = 256 * 1024 * 1024;
    let size = match unsafe { &(*n).data } {
        RtData::Int(n) => {
            if *n < 0 { rt_error("bytes-alloc: size must be non-negative"); }
            *n as usize
        }
        _ => rt_error("bytes-alloc: argument must be an Int"),
    };
    if size > MAX_ALLOC {
        rt_error(&format!("bytes-alloc: size {} exceeds 256 MiB limit", size));
    }
    rt_bytes(vec![0u8; size])
}

/// Get a single byte at the given index (bounds-checked).
/// Returns the byte value as an Int.
#[no_mangle]
pub extern "C" fn airl_bytes_get(buf: *mut RtValue, index: *mut RtValue) -> *mut RtValue {
    let bytes = match unsafe { &(*buf).data } {
        RtData::Bytes(v) => v,
        _ => rt_error("bytes-get: first argument must be Bytes"),
    };
    let i = match unsafe { &(*index).data } {
        RtData::Int(n) => *n,
        _ => rt_error("bytes-get: index must be an Int"),
    };
    if i < 0 || i as usize >= bytes.len() {
        rt_error("bytes-get: index out of bounds");
    }
    rt_int(bytes[i as usize] as i64)
}

/// Set a single byte at the given index (bounds-checked, COW semantics).
/// Returns a new Bytes value (or mutates in-place if sole owner).
#[no_mangle]
pub extern "C" fn airl_bytes_set(buf: *mut RtValue, index: *mut RtValue, val: *mut RtValue) -> *mut RtValue {
    let i = match unsafe { &(*index).data } {
        RtData::Int(n) => *n,
        _ => rt_error("bytes-set!: index must be an Int"),
    };
    let byte_val = match unsafe { &(*val).data } {
        RtData::Int(n) => *n as u8,
        _ => rt_error("bytes-set!: value must be an Int"),
    };
    let v = unsafe { &mut *buf };
    // COW: if sole owner, mutate in-place
    if v.rc.load(core::sync::atomic::Ordering::Acquire) == 1 {
        if let RtData::Bytes(ref mut bytes) = v.data {
            if i < 0 || i as usize >= bytes.len() {
                rt_error("bytes-set!: index out of bounds");
            }
            bytes[i as usize] = byte_val;
            airl_value_retain(buf);
            return buf;
        }
    }
    // Otherwise, clone and mutate
    let mut bytes = match &v.data {
        RtData::Bytes(b) => b.clone(),
        _ => rt_error("bytes-set!: first argument must be Bytes"),
    };
    if i < 0 || i as usize >= bytes.len() {
        rt_error("bytes-set!: index out of bounds");
    }
    bytes[i as usize] = byte_val;
    rt_bytes(bytes)
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int8(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n as u8, _ => 0 };
    rt_bytes(vec![val])
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int16(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n as i16, _ => 0 };
    rt_bytes(val.to_be_bytes().to_vec())
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int32(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n as i32, _ => 0 };
    rt_bytes(val.to_be_bytes().to_vec())
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int64(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    rt_bytes(val.to_be_bytes().to_vec())
}

fn extract_bytes(val: *mut RtValue) -> Vec<u8> {
    match unsafe { &(*val).data } {
        RtData::Bytes(v) => v.clone(),
        RtData::List { .. } => crate::list::list_items(unsafe { &(*val).data }).iter().map(|i| match unsafe { &(**i).data } { RtData::Int(n) => *n as u8, _ => 0 }).collect(),
        _ => vec![],
    }
}

/// Borrow the raw bytes from a Bytes value without cloning.
/// Returns empty slice for non-Bytes types (callers that need List[Int]
/// compat should use borrow_or_extract instead).
///
/// # Safety
/// The caller must ensure `val` is a valid, non-null pointer to an RtValue
/// that remains alive and unmutated for the lifetime `'a`.
#[allow(dead_code)]  // kept for callers that only handle Bytes (not List[Int])
unsafe fn borrow_bytes<'a>(val: *mut RtValue) -> &'a [u8] {
    match &(*val).data {
        RtData::Bytes(v) => v.as_slice(),
        _ => &[],
    }
}

/// Zero-copy borrow for Bytes, with fallback to owned extraction for List[Int].
/// Uses Cow to avoid allocation in the common Bytes case while preserving
/// backward compatibility with List[Int] representations.
///
/// # Safety
/// The caller must ensure `val` is a valid, non-null pointer to an RtValue
/// that remains alive and unmutated for the lifetime `'a`.
#[cfg(not(target_os = "airlos"))]
unsafe fn borrow_or_extract<'a>(val: *mut RtValue) -> std::borrow::Cow<'a, [u8]> {
    match &(*val).data {
        RtData::Bytes(v) => std::borrow::Cow::Borrowed(v.as_slice()),
        RtData::List { .. } => std::borrow::Cow::Owned(extract_bytes(val)),
        _ => std::borrow::Cow::Borrowed(&[]),
    }
}

#[cfg(target_os = "airlos")]
unsafe fn borrow_or_extract<'a>(val: *mut RtValue) -> alloc::borrow::Cow<'a, [u8]> {
    match &(*val).data {
        RtData::Bytes(v) => alloc::borrow::Cow::Borrowed(v.as_slice()),
        RtData::List { .. } => alloc::borrow::Cow::Owned(extract_bytes(val)),
        _ => alloc::borrow::Cow::Borrowed(&[]),
    }
}

fn byte_at(val: *mut RtValue, off: usize) -> u8 {
    match unsafe { &(*val).data } {
        RtData::Bytes(v) => if off < v.len() { v[off] } else { 0 },
        RtData::List { .. } => {
            let slice = crate::list::list_items(unsafe { &(*val).data });
            if off < slice.len() { match unsafe { &(*slice[off]).data } { RtData::Int(n) => *n as u8, _ => 0 } } else { 0 }
        }
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn airl_bytes_to_int16(buf: *mut RtValue, offset: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let val = ((byte_at(buf, off) as i16) << 8) | (byte_at(buf, off + 1) as i16);
    rt_int(val as i64)
}

#[no_mangle]
pub extern "C" fn airl_bytes_to_int32(buf: *mut RtValue, offset: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let val = ((byte_at(buf, off) as i32) << 24) | ((byte_at(buf, off+1) as i32) << 16) | ((byte_at(buf, off+2) as i32) << 8) | (byte_at(buf, off+3) as i32);
    rt_int(val as i64)
}

#[no_mangle]
pub extern "C" fn airl_bytes_to_int64(buf: *mut RtValue, offset: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let mut val: i64 = 0;
    for i in 0..8 { val = (val << 8) | (byte_at(buf, off + i) as i64); }
    rt_int(val)
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_string(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s, _ => return rt_bytes(Vec::new()) };
    rt_bytes(input.clone().into_bytes())
}

#[no_mangle]
pub extern "C" fn airl_bytes_to_string(buf: *mut RtValue, offset: *mut RtValue, len: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let slen = match unsafe { &(*len).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let bytes = unsafe { borrow_or_extract(buf) };
    if off + slen > bytes.len() { return rt_str(String::new()); }
    rt_str(String::from_utf8_lossy(&bytes[off..off+slen]).into_owned())
}

#[no_mangle]
pub extern "C" fn airl_bytes_concat(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let a_ref = unsafe { borrow_or_extract(a) };
    let b_ref = unsafe { borrow_or_extract(b) };
    let mut result = Vec::with_capacity(a_ref.len() + b_ref.len());
    result.extend_from_slice(&a_ref);
    result.extend_from_slice(&b_ref);
    rt_bytes(result)
}

/// Concatenate a list of byte lists in one O(n) pass.
/// Input: List[List[u8]]. Output: List[u8].
/// Replaces the O(n²) `(fold (fn [acc part] (bytes-concat acc part)) [] parts)` pattern.
#[no_mangle]
pub extern "C" fn airl_bytes_concat_all(parts: *mut RtValue) -> *mut RtValue {
    let part_lists = match unsafe { &(*parts).data } {
        RtData::List { .. } => crate::list::list_items(unsafe { &(*parts).data }),
        _ => return rt_bytes(vec![]),
    };
    // Single extraction pass: borrow or extract each part once, measure total
    let mut total = 0usize;
    #[cfg(not(target_os = "airlos"))]
    type ByteCow<'a> = std::borrow::Cow<'a, [u8]>;
    #[cfg(target_os = "airlos")]
    type ByteCow<'a> = alloc::borrow::Cow<'a, [u8]>;
    let borrowed: Vec<ByteCow<'_>> = part_lists.iter().map(|&p| {
        let cow = unsafe { borrow_or_extract(p) };
        total += cow.len();
        cow
    }).collect();
    // Copy pass: write into pre-allocated result from the already-extracted cows
    let mut result = Vec::with_capacity(total);
    for cow in &borrowed {
        result.extend_from_slice(cow);
    }
    rt_bytes(result)
}

#[no_mangle]
pub extern "C" fn airl_bytes_slice(buf: *mut RtValue, offset: *mut RtValue, len: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let slen = match unsafe { &(*len).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let bytes = unsafe { borrow_or_extract(buf) };
    if off + slen > bytes.len() { return rt_bytes(vec![]); }
    rt_bytes(bytes[off..off+slen].to_vec())
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_crc32c(buf: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(buf) };
    rt_int(crc32c::crc32c(&bytes) as i64)
}

// ── Compression builtins ─────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_gzip_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, &bytes)
        .unwrap_or_else(|e| rt_error(&format!("gzip-compress: {e}")));
    let compressed = encoder.finish()
        .unwrap_or_else(|e| rt_error(&format!("gzip-compress: {e}")));
    rt_bytes(compressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_gzip_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    use flate2::read::GzDecoder;
    const MAX_DECOMPRESS_SIZE: usize = 256 * 1024 * 1024;
    let mut decoder = GzDecoder::new(&*bytes);
    let mut decompressed = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut decoder, &mut chunk)
            .unwrap_or_else(|e| rt_error(&format!("gzip-decompress: {e}")));
        if n == 0 { break; }
        decompressed.extend_from_slice(&chunk[..n]);
        if decompressed.len() > MAX_DECOMPRESS_SIZE {
            rt_error("gzip-decompress: output exceeds 256 MiB limit");
        }
    }
    rt_bytes(decompressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_snappy_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    let compressed = snap::raw::Encoder::new().compress_vec(&bytes)
        .unwrap_or_else(|e| rt_error(&format!("snappy-compress: {e}")));
    rt_bytes(compressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_snappy_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    let decompressed = snap::raw::Decoder::new().decompress_vec(&bytes)
        .unwrap_or_else(|e| rt_error(&format!("snappy-decompress: {e}")));
    if decompressed.len() > 256 * 1024 * 1024 {
        rt_error("snappy-decompress: output exceeds 256 MiB limit");
    }
    rt_bytes(decompressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_lz4_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    let compressed = lz4_flex::compress_prepend_size(&bytes);
    rt_bytes(compressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_lz4_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    let decompressed = lz4_flex::decompress_size_prepended(&bytes)
        .unwrap_or_else(|e| rt_error(&format!("lz4-decompress: {e}")));
    if decompressed.len() > 256 * 1024 * 1024 {
        rt_error("lz4-decompress: output exceeds 256 MiB limit");
    }
    rt_bytes(decompressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_zstd_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    let compressed = zstd::encode_all(&*bytes, 3)
        .unwrap_or_else(|e| rt_error(&format!("zstd-compress: {e}")));
    rt_bytes(compressed)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_zstd_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = unsafe { borrow_or_extract(data) };
    let decompressed = zstd::decode_all(&*bytes)
        .unwrap_or_else(|e| rt_error(&format!("zstd-decompress: {e}")));
    if decompressed.len() > 256 * 1024 * 1024 {
        rt_error("zstd-decompress: output exceeds 256 MiB limit");
    }
    rt_bytes(decompressed)
}

// airl_run_bytecode and airl_compile_to_executable are defined elsewhere in airl-runtime

// ── DNS resolve ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_dns_resolve(hostname: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*hostname).data } {
        RtData::Str(s) => s.clone(),
        _ => return err_variant("dns-resolve: hostname must be string"),
    };
    use std::net::ToSocketAddrs;
    match format!("{}:0", h).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => ok_variant(rt_str(addr.ip().to_string())),
            None => err_variant("dns-resolve: no addresses found"),
        },
        Err(e) => err_variant(&format!("dns-resolve: {}", e)),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_dns_resolve(hostname: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*hostname).data } {
        RtData::Str(s) => s.as_str(),
        _ => return err_variant("dns-resolve: hostname must be string"),
    };
    let net_svc = get_net_port();
    if net_svc <= 0 {
        return err_variant("dns-resolve: net service not available");
    }
    // Build NET_DNS_RESOLVE request: header(16) + hostname[224]
    let mut msg = [0u8; 256];
    // type = 0x520 (NET_DNS_RESOLVE)
    msg[0..4].copy_from_slice(&0x520u32.to_le_bytes());
    // seq, payload_len, flags = 0
    let hostname_bytes = h.as_bytes();
    let copy_len = hostname_bytes.len().min(223); // leave room for null terminator
    msg[16..16 + copy_len].copy_from_slice(&hostname_bytes[..copy_len]);
    // msg[16 + copy_len] is already 0 (null terminator)

    let mut resp = [0u8; 256];
    let n = crate::airlos::ipc_sendrecv(net_svc, &msg[..240], &mut resp);
    if n < 24 { // header(16) + status(4) + addr(4) minimum
        return err_variant("dns-resolve: IPC error");
    }
    let status = i32::from_le_bytes([resp[16], resp[17], resp[18], resp[19]]);
    match status {
        0 => {
            // Parse addr_str from offset 24, up to 16 bytes, null-terminated
            let str_start = 24;
            let str_end = (str_start + 16).min(n as usize);
            let addr_bytes = &resp[str_start..str_end];
            let len = addr_bytes.iter().position(|&b| b == 0).unwrap_or(addr_bytes.len());
            match core::str::from_utf8(&addr_bytes[..len]) {
                Ok(s) => ok_variant(rt_str(s.into())),
                Err(_) => err_variant("dns-resolve: invalid response encoding"),
            }
        }
        -1 => err_variant("dns-resolve: NXDOMAIN"),
        -2 => err_variant("dns-resolve: timeout"),
        _ => err_variant("dns-resolve: unknown error"),
    }
}

// ── ICMP ping ──

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_icmp_ping(addr: *mut RtValue, timeout_ms: *mut RtValue) -> *mut RtValue {
    let _ = match unsafe { &(*addr).data } {
        RtData::Str(s) => s.clone(),
        _ => return err_variant("icmp-ping: addr must be string"),
    };
    let _ = match unsafe { &(*timeout_ms).data } {
        RtData::Int(n) => *n,
        _ => return err_variant("icmp-ping: timeout must be int"),
    };
    err_variant("icmp-ping: not available on Linux (use system ping)")
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_icmp_ping(addr: *mut RtValue, timeout_ms: *mut RtValue) -> *mut RtValue {
    let addr_str = match unsafe { &(*addr).data } {
        RtData::Str(s) => s.as_str(),
        _ => return err_variant("icmp-ping: addr must be string"),
    };
    let timeout = match unsafe { &(*timeout_ms).data } {
        RtData::Int(n) => *n as u32,
        _ => return err_variant("icmp-ping: timeout must be int"),
    };
    // Parse dotted-decimal IP to u32 (network byte order)
    let ip_u32 = match parse_ipv4(addr_str) {
        Some(ip) => ip,
        None => return err_variant("icmp-ping: invalid IPv4 address"),
    };
    let net_svc = get_net_port();
    if net_svc <= 0 {
        return err_variant("icmp-ping: net service not available");
    }
    // Build NET_ICMP_PING request: header(16) + dest_addr(4) + seq(2) + payload_len(2) + timeout_ms(4) = 28
    let mut msg = [0u8; 28];
    // type = 0x522 (NET_ICMP_PING)
    msg[0..4].copy_from_slice(&0x522u32.to_le_bytes());
    // seq, payload_len, flags in header = 0
    // dest_addr at offset 16
    msg[16..20].copy_from_slice(&ip_u32.to_be_bytes());
    // seq = 1 at offset 20
    msg[20..22].copy_from_slice(&1u16.to_le_bytes());
    // payload_len = 56 at offset 22
    msg[22..24].copy_from_slice(&56u16.to_le_bytes());
    // timeout_ms at offset 24
    msg[24..28].copy_from_slice(&timeout.to_le_bytes());

    let mut resp = [0u8; 256];
    let n = crate::airlos::ipc_sendrecv(net_svc, &msg, &mut resp);
    if n < 28 { // header(16) + status(4) + rtt_us(4) + ttl(1) + pad(1) + seq(2)
        return err_variant("icmp-ping: IPC error");
    }
    let status = i32::from_le_bytes([resp[16], resp[17], resp[18], resp[19]]);
    match status {
        0 => {
            let rtt_us = u32::from_le_bytes([resp[20], resp[21], resp[22], resp[23]]);
            let ttl = resp[24];
            let seq = u16::from_le_bytes([resp[26], resp[27]]);
            let mut m = HashMap::new();
            // rtt_ms = rtt_us / 1000 (integer division, microseconds to milliseconds)
            m.insert("rtt_ms".into(), rt_int((rtt_us / 1000) as i64));
            m.insert("ttl".into(), rt_int(ttl as i64));
            m.insert("seq".into(), rt_int(seq as i64));
            ok_variant(rt_map(m))
        }
        -1 => err_variant("icmp-ping: timeout"),
        -2 => err_variant("icmp-ping: unreachable"),
        _ => err_variant("icmp-ping: unknown error"),
    }
}

// ── AIRLOS net service helper ──

#[cfg(target_os = "airlos")]
fn get_net_port() -> i32 {
    static NET_SVC: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
    let mut svc = NET_SVC.load(core::sync::atomic::Ordering::Relaxed);
    if svc <= 0 {
        for _ in 0..5000 {
            svc = crate::airlos::lookup_service("net");
            if svc > 0 {
                NET_SVC.store(svc, core::sync::atomic::Ordering::Relaxed);
                return svc;
            }
            unsafe { crate::airlos::syscall0(2); } // SYS_YIELD
        }
        return 0;
    }
    svc
}

#[cfg(target_os = "airlos")]
fn parse_ipv4(s: &str) -> Option<u32> {
    let mut octets = [0u8; 4];
    let mut parts = s.split('.');
    for octet in &mut octets {
        let part = parts.next()?;
        *octet = part.parse::<u8>().ok()?;
    }
    if parts.next().is_some() {
        return None; // too many parts
    }
    Some(u32::from_be_bytes(octets))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DNS resolve tests ──

    #[test]
    fn test_dns_resolve_localhost() {
        let hostname = rt_str("localhost".to_string());
        let result = airl_dns_resolve(hostname);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name: tag, inner } => {
                assert_eq!(tag.as_str(), "Ok");
                let inner_v = unsafe { &**inner };
                match &inner_v.data {
                    RtData::Str(s) => assert!(s == "127.0.0.1" || s == "::1", "unexpected addr: {}", s),
                    _ => panic!("expected string inside Ok"),
                }
            }
            _ => panic!("expected variant"),
        }
    }

    #[test]
    fn test_dns_resolve_bad_type() {
        let not_a_string = rt_int(42);
        let result = airl_dns_resolve(not_a_string);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, .. } => assert_eq!(tag_name.as_str(), "Err"),
            _ => panic!("expected Err variant"),
        }
    }

    // ── ICMP ping tests ──

    #[test]
    fn test_icmp_ping_returns_err_on_linux() {
        let addr = rt_str("127.0.0.1".to_string());
        let timeout = rt_int(1000);
        let result = airl_icmp_ping(addr, timeout);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name: tag, inner } => {
                assert_eq!(tag.as_str(), "Err");
                let inner_v = unsafe { &**inner };
                match &inner_v.data {
                    RtData::Str(s) => assert!(s.contains("not available on Linux")),
                    _ => panic!("expected string inside Err"),
                }
            }
            _ => panic!("expected Err variant"),
        }
    }

    #[test]
    fn test_icmp_ping_bad_addr_type() {
        let not_a_string = rt_int(42);
        let timeout = rt_int(1000);
        let result = airl_icmp_ping(not_a_string, timeout);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, .. } => assert_eq!(tag_name.as_str(), "Err"),
            _ => panic!("expected Err variant"),
        }
    }

    #[test]
    fn test_icmp_ping_bad_timeout_type() {
        let addr = rt_str("127.0.0.1".to_string());
        let not_an_int = rt_str("bad".to_string());
        let result = airl_icmp_ping(addr, not_an_int);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, .. } => assert_eq!(tag_name.as_str(), "Err"),
            _ => panic!("expected Err variant"),
        }
    }

    #[test]
    fn test_tcp_accept_tls_invalid_handle() {
        let handle = rt_int(-999);
        let cert = rt_str("nonexistent.pem".to_string());
        let key = rt_str("nonexistent.key".to_string());
        let result = airl_tcp_accept_tls(handle, cert, key);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, .. } => assert_eq!(tag_name, "Err", "Expected Err variant for invalid handle"),
            _ => panic!("Expected Variant result"),
        }
    }

    #[test]
    fn test_tcp_accept_tls_bad_cert_path() {
        // Insert a real Plain handle so we get past handle lookup to the cert-read path
        let h = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = TcpStream::connect(addr).unwrap();
        let (server_stream, _) = listener.accept().unwrap();
        tcp_handles().lock().unwrap().insert(h, RtTcpHandle::Plain(server_stream));

        let handle = rt_int(h);
        let cert = rt_str("nonexistent.pem".to_string());
        let key = rt_str("nonexistent.key".to_string());
        let result = airl_tcp_accept_tls(handle, cert, key);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Err");
                let msg = unsafe { &(**inner).data };
                match msg {
                    RtData::Str(s) => assert!(s.contains("read cert"), "Error should mention cert read failure, got: {}", s),
                    _ => panic!("Expected string error message"),
                }
            }
            _ => panic!("Expected Variant result"),
        }
        // Handle should be restored after cert-read failure (it fails before removing handle)
        // Actually, cert read happens before handle removal, so handle is still in the map
        assert!(tcp_handles().lock().unwrap().remove(&h).is_some(), "Handle should still exist after cert-read failure");
    }

    // Self-signed test cert generated with: openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 36500 -subj '/CN=localhost'
    const TEST_CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\nMIIBfjCCASWgAwIBAgIUVUs3Wd34fOdaD+6BF19k2DREyCswCgYIKoZIzj0EAwIw\nFDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTI2MDMzMTIzNDUzOFoYDzIxMjYwMzA3\nMjM0NTM4WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO\nPQMBBwNCAARq4GkaYS1M0NabCS8Zt02WyfgAOucJGySwVn7j1I6Np4DT0/KJDNSf\nUCmU9iMGZBLDbMlqhSs3DbVjO7uJoXH2o1MwUTAdBgNVHQ4EFgQUkuMOkLwIH6Vq\nX1UvvuVqt6CoC0gwHwYDVR0jBBgwFoAUkuMOkLwIH6VqX1UvvuVqt6CoC0gwDwYD\nVR0TAQH/BAUwAwEB/zAKBggqhkjOPQQDAgNHADBEAiAnIvjPxc1TeaFtDz0ylnIF\nK9SkuMKbd1TymhYpz5K7oAIgYcH/9ur6MYhEq6dgQxa00PiGLhsXdnlC7Ls9RDgf\nWtk=\n-----END CERTIFICATE-----\n";
    const TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgSFCSJXetH85rkIkY\nq5q6SeKcRLOKX1Rz3KCP+b/EET+hRANCAARq4GkaYS1M0NabCS8Zt02WyfgAOucJ\nGySwVn7j1I6Np4DT0/KJDNSfUCmU9iMGZBLDbMlqhSs3DbVjO7uJoXH2\n-----END PRIVATE KEY-----\n";

    #[test]
    fn test_tcp_accept_tls_already_tls() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        // Write test cert/key to temp files
        let dir = std::env::temp_dir().join(format!("airl_tls_test_{}", NEXT_TCP_HANDLE.load(Ordering::SeqCst)));
        std::fs::create_dir_all(&dir).unwrap();
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, TEST_CERT_PEM).unwrap();
        std::fs::write(&key_path, TEST_KEY_PEM).unwrap();

        // Create a Tls (client) handle — this should be rejected by tcp-accept-tls
        let h = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = TcpStream::connect(addr).unwrap();
        let (server_stream, _) = listener.accept().unwrap();

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), server_name).unwrap();
        let tls_stream = rustls::StreamOwned::new(conn, server_stream);
        tcp_handles().lock().unwrap().insert(h, RtTcpHandle::Tls(Box::new(tls_stream)));

        let handle = rt_int(h);
        let cert_arg = rt_str(cert_path.to_str().unwrap().to_string());
        let key_arg = rt_str(key_path.to_str().unwrap().to_string());
        let result = airl_tcp_accept_tls(handle, cert_arg, key_arg);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Err");
                let msg = unsafe { &(**inner).data };
                match msg {
                    RtData::Str(s) => assert!(s.contains("already TLS"), "Error should mention already TLS, got: {}", s),
                    _ => panic!("Expected string error message"),
                }
            }
            _ => panic!("Expected Variant result"),
        }
        // Handle should be preserved after "already TLS" error
        assert!(tcp_handles().lock().unwrap().get(&h).is_some(), "Handle should be preserved after already-TLS error");
        // Clean up
        tcp_handles().lock().unwrap().remove(&h);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Zero-copy borrow_bytes / borrow_or_extract tests ──────────────────

    #[test]
    fn test_borrow_bytes_from_bytes_value() {
        let data = vec![1u8, 2, 3, 4, 5];
        let val = rt_bytes(data.clone());
        let borrowed = unsafe { borrow_bytes(val) };
        assert_eq!(borrowed, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_borrow_bytes_from_non_bytes_returns_empty() {
        let val = rt_int(42);
        let borrowed = unsafe { borrow_bytes(val) };
        assert_eq!(borrowed, &[] as &[u8]);

        let val2 = rt_str("hello".to_string());
        let borrowed2 = unsafe { borrow_bytes(val2) };
        assert_eq!(borrowed2, &[] as &[u8]);

        let val3 = rt_nil();
        let borrowed3 = unsafe { borrow_bytes(val3) };
        assert_eq!(borrowed3, &[] as &[u8]);
    }

    #[test]
    fn test_borrow_bytes_empty_bytes() {
        let val = rt_bytes(vec![]);
        let borrowed = unsafe { borrow_bytes(val) };
        assert_eq!(borrowed, &[] as &[u8]);
        assert_eq!(borrowed.len(), 0);
    }

    #[test]
    fn test_borrow_or_extract_bytes_value_is_borrowed() {
        let data = vec![10u8, 20, 30];
        let val = rt_bytes(data.clone());
        let cow = unsafe { borrow_or_extract(val) };
        assert!(matches!(cow, std::borrow::Cow::Borrowed(_)), "Bytes should produce Borrowed variant");
        assert_eq!(&*cow, &[10, 20, 30]);
    }

    #[test]
    fn test_borrow_or_extract_list_of_ints_is_owned() {
        let items = vec![rt_int(65), rt_int(66), rt_int(67)];
        let val = rt_list(items);
        let cow = unsafe { borrow_or_extract(val) };
        assert!(matches!(cow, std::borrow::Cow::Owned(_)), "List[Int] should produce Owned variant");
        assert_eq!(&*cow, &[65u8, 66, 67]);
    }

    #[test]
    fn test_borrow_or_extract_non_bytes_non_list() {
        let val = rt_int(99);
        let cow = unsafe { borrow_or_extract(val) };
        assert!(matches!(cow, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*cow, &[] as &[u8]);
    }

    #[test]
    fn test_crc32c_zero_copy() {
        let data = rt_bytes(b"hello world".to_vec());
        let result = airl_crc32c(data);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Int(n) => {
                let expected = crc32c::crc32c(b"hello world") as i64;
                assert_eq!(*n, expected);
            }
            _ => panic!("Expected Int result from crc32c"),
        }
    }

    #[test]
    fn test_crc32c_empty_bytes() {
        let data = rt_bytes(vec![]);
        let result = airl_crc32c(data);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Int(n) => assert_eq!(*n, crc32c::crc32c(&[]) as i64),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_crc32c_non_bytes_returns_zero_crc() {
        let data = rt_int(42);
        let result = airl_crc32c(data);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Int(n) => assert_eq!(*n, crc32c::crc32c(&[]) as i64),
            _ => panic!("Expected Int result"),
        }
    }

    #[test]
    fn test_bytes_concat_zero_copy() {
        let a = rt_bytes(vec![1, 2, 3]);
        let b = rt_bytes(vec![4, 5, 6]);
        let result = airl_bytes_concat(a, b);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![1, 2, 3, 4, 5, 6]),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_concat_empty_sides() {
        let a = rt_bytes(vec![]);
        let b = rt_bytes(vec![1, 2]);
        let result = airl_bytes_concat(a, b);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![1, 2]),
            _ => panic!("Expected Bytes result"),
        }

        let a2 = rt_bytes(vec![3, 4]);
        let b2 = rt_bytes(vec![]);
        let result2 = airl_bytes_concat(a2, b2);
        let rv2 = unsafe { &*result2 };
        match &rv2.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![3, 4]),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_concat_all_zero_copy() {
        let parts = rt_list(vec![
            rt_bytes(vec![1, 2]),
            rt_bytes(vec![3, 4]),
            rt_bytes(vec![5, 6]),
        ]);
        let result = airl_bytes_concat_all(parts);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![1, 2, 3, 4, 5, 6]),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_concat_all_empty_list() {
        let parts = rt_list(vec![]);
        let result = airl_bytes_concat_all(parts);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert!(v.is_empty()),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_concat_all_non_list() {
        let val = rt_int(42);
        let result = airl_bytes_concat_all(val);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert!(v.is_empty()),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_slice_zero_copy() {
        let data = rt_bytes(vec![10, 20, 30, 40, 50]);
        let offset = rt_int(1);
        let len = rt_int(3);
        let result = airl_bytes_slice(data, offset, len);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![20, 30, 40]),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_slice_out_of_bounds() {
        let data = rt_bytes(vec![1, 2, 3]);
        let offset = rt_int(2);
        let len = rt_int(5);
        let result = airl_bytes_slice(data, offset, len);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert!(v.is_empty()),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_to_string_zero_copy() {
        let data = rt_bytes(b"hello".to_vec());
        let offset = rt_int(0);
        let len = rt_int(5);
        let result = airl_bytes_to_string(data, offset, len);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Str(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Str result"),
        }
    }

    #[test]
    fn test_bytes_to_string_out_of_bounds() {
        let data = rt_bytes(b"hi".to_vec());
        let offset = rt_int(0);
        let len = rt_int(10);
        let result = airl_bytes_to_string(data, offset, len);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Str(s) => assert_eq!(s, ""),
            _ => panic!("Expected Str result"),
        }
    }

    #[test]
    fn test_sha256_bytes_zero_copy() {
        use sha2::Digest;
        let input = b"test data";
        let expected = sha2::Sha256::digest(input).to_vec();
        let data = rt_bytes(input.to_vec());
        let result = airl_sha256_bytes(data);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &expected),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_base64_roundtrip_zero_copy() {
        let original = vec![0u8, 1, 2, 255, 254, 253];
        let data = rt_bytes(original.clone());
        let encoded = airl_base64_encode_bytes(data);
        let decoded = airl_base64_decode_bytes(encoded);
        let rv = unsafe { &*decoded };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &original),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_compression_roundtrip_gzip() {
        let original = b"hello world hello world hello world".to_vec();
        let data = rt_bytes(original.clone());
        let compressed = airl_gzip_compress(data);
        let decompressed = airl_gzip_decompress(compressed);
        let rv = unsafe { &*decompressed };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &original),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_compression_roundtrip_snappy() {
        let original = b"snappy test data snappy test data".to_vec();
        let data = rt_bytes(original.clone());
        let compressed = airl_snappy_compress(data);
        let decompressed = airl_snappy_decompress(compressed);
        let rv = unsafe { &*decompressed };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &original),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_compression_roundtrip_lz4() {
        let original = b"lz4 test data lz4 test data".to_vec();
        let data = rt_bytes(original.clone());
        let compressed = airl_lz4_compress(data);
        let decompressed = airl_lz4_decompress(compressed);
        let rv = unsafe { &*decompressed };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &original),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_compression_roundtrip_zstd() {
        let original = b"zstd test data zstd test data".to_vec();
        let data = rt_bytes(original.clone());
        let compressed = airl_zstd_compress(data);
        let decompressed = airl_zstd_decompress(compressed);
        let rv = unsafe { &*decompressed };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &original),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_concat_with_list_int_backward_compat() {
        // Verify List[Int] still works via borrow_or_extract fallback
        let list_a = rt_list(vec![rt_int(1), rt_int(2)]);
        let bytes_b = rt_bytes(vec![3, 4]);
        let result = airl_bytes_concat(list_a, bytes_b);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![1, 2, 3, 4]),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_bytes_concat_all_mixed_list_and_bytes() {
        // Mix of Bytes and List[Int] parts
        let parts = rt_list(vec![
            rt_bytes(vec![1, 2]),
            rt_list(vec![rt_int(3), rt_int(4)]),
            rt_bytes(vec![5, 6]),
        ]);
        let result = airl_bytes_concat_all(parts);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &vec![1, 2, 3, 4, 5, 6]),
            _ => panic!("Expected Bytes result"),
        }
    }

    #[test]
    fn test_borrow_bytes_large_buffer() {
        // Verify zero-copy works with large buffers (the whole point of this optimization)
        let large = vec![42u8; 1_000_000];
        let val = rt_bytes(large.clone());
        let borrowed = unsafe { borrow_bytes(val) };
        assert_eq!(borrowed.len(), 1_000_000);
        assert_eq!(borrowed[0], 42);
        assert_eq!(borrowed[999_999], 42);
        // Verify it's actually the same memory (pointer comparison)
        let inner = match unsafe { &(*val).data } {
            RtData::Bytes(v) => v.as_ptr(),
            _ => panic!("Expected Bytes"),
        };
        assert_eq!(borrowed.as_ptr(), inner, "borrow_bytes should return a reference to the same memory, not a copy");
    }

    #[test]
    fn test_hmac_sha256_bytes_zero_copy() {
        use hmac::{Hmac, Mac};
        let key_data = b"secret_key".to_vec();
        let msg_data = b"message".to_vec();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&key_data).unwrap();
        mac.update(&msg_data);
        let expected = mac.finalize().into_bytes().to_vec();

        let key = rt_bytes(key_data);
        let msg = rt_bytes(msg_data);
        let result = airl_hmac_sha256_bytes(key, msg);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Bytes(v) => assert_eq!(v, &expected),
            _ => panic!("Expected Bytes result"),
        }
    }

    // ── SEC-9: TCP recv size cap tests ──

    #[test]
    fn test_tcp_recv_rejects_oversized_request() {
        let handle = rt_int(99999); // nonexistent handle, but size check happens first
        let too_big = rt_int((TCP_RECV_MAX_BYTES as i64) + 1);
        let result = airl_tcp_recv(handle, too_big);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Err");
                let msg = unsafe { &(**inner).data };
                match msg {
                    RtData::Str(s) => assert!(s.contains("256MB"), "Error should mention 256MB limit, got: {}", s),
                    _ => panic!("Expected string error message"),
                }
            }
            _ => panic!("Expected Variant result"),
        }
    }

    #[test]
    fn test_tcp_recv_exact_rejects_oversized_request() {
        let handle = rt_int(99999);
        let too_big = rt_int((TCP_RECV_MAX_BYTES as i64) + 1);
        let result = airl_tcp_recv_exact(handle, too_big);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Err");
                let msg = unsafe { &(**inner).data };
                match msg {
                    RtData::Str(s) => assert!(s.contains("256MB"), "Error should mention 256MB limit, got: {}", s),
                    _ => panic!("Expected string error message"),
                }
            }
            _ => panic!("Expected Variant result"),
        }
    }

    #[test]
    fn test_tcp_recv_allows_valid_size() {
        // A valid size should pass the size check and fail on invalid handle instead
        let handle = rt_int(99999);
        let valid_size = rt_int(1024);
        let result = airl_tcp_recv(handle, valid_size);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Err");
                let msg = unsafe { &(**inner).data };
                match msg {
                    RtData::Str(s) => assert!(s.contains("invalid handle"), "Should fail on handle, not size, got: {}", s),
                    _ => panic!("Expected string error message"),
                }
            }
            _ => panic!("Expected Variant result"),
        }
    }

    // ── SEC-17: Default TCP timeouts test ──

    #[test]
    fn test_tcp_connect_sets_default_timeouts() {
        // Start a local listener, connect, and verify timeouts are set
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let host = rt_str(addr.ip().to_string());
        let port = rt_int(addr.port() as i64);
        let result = airl_tcp_connect(host, port);
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::Variant { tag_name, inner } => {
                assert_eq!(tag_name, "Ok");
                let h = match unsafe { &(**inner).data } { RtData::Int(n) => *n, _ => panic!("Expected int handle") };
                let handles = tcp_handles().lock().unwrap();
                let tcp_handle = handles.get(&h).expect("handle should exist");
                match tcp_handle {
                    RtTcpHandle::Plain(s) => {
                        let read_timeout = s.read_timeout().unwrap();
                        let write_timeout = s.write_timeout().unwrap();
                        assert_eq!(read_timeout, Some(std::time::Duration::from_secs(30)));
                        assert_eq!(write_timeout, Some(std::time::Duration::from_secs(30)));
                    }
                    _ => panic!("Expected Plain handle"),
                }
                drop(handles);
                tcp_handles().lock().unwrap().remove(&h);
            }
            _ => panic!("Expected Ok variant"),
        }
    }

    // ── COW reverse tests ──

    #[test]
    fn test_reverse_cow_returns_same_pointer() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            assert_eq!((*list).rc.load(core::sync::atomic::Ordering::Relaxed), 1);
            let result = airl_reverse_list(list);
            assert_eq!(result, list, "COW reverse should return same pointer when rc == 1");
            let slice = crate::list::list_items(&(*result).data);
            assert_eq!(slice.len(), 3);
            assert_eq!((*slice[0]).as_int(), 3);
            assert_eq!((*slice[1]).as_int(), 2);
            assert_eq!((*slice[2]).as_int(), 1);
            crate::memory::airl_value_release(result);
        }
    }

    #[test]
    fn test_reverse_clones_when_shared() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let list = rt_list(vec![a, b]);
            crate::memory::airl_value_retain(list); // rc == 2
            let result = airl_reverse_list(list);
            assert_ne!(result, list, "reverse should clone when rc > 1");
            crate::memory::airl_value_release(list);
            crate::memory::airl_value_release(list);
            crate::memory::airl_value_release(result);
        }
    }

    // ── airl_range arithmetic safety tests ──

    fn range_len(result: *mut RtValue) -> usize {
        let rv = unsafe { &*result };
        match &rv.data {
            RtData::List { items, .. } => items.len(),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_range_happy_path() {
        let s = rt_int(0);
        let e = rt_int(5);
        let result = airl_range(s, e);
        assert_eq!(range_len(result), 5);
    }

    #[test]
    fn test_range_empty_when_start_equals_end() {
        let s = rt_int(3);
        let e = rt_int(3);
        let result = airl_range(s, e);
        assert_eq!(range_len(result), 0, "range(n,n) should be empty");
    }

    #[test]
    fn test_range_empty_when_start_greater_than_end() {
        let s = rt_int(10);
        let e = rt_int(3);
        let result = airl_range(s, e);
        assert_eq!(range_len(result), 0, "range(10,3) should be empty — no underflow");
    }

    #[test]
    fn test_range_empty_on_negative_end_less_than_start() {
        // e - s would underflow as usize without the checked_sub guard
        let s = rt_int(i64::MAX);
        let e = rt_int(0);
        let result = airl_range(s, e);
        assert_eq!(range_len(result), 0, "range(i64::MAX, 0) should be empty");
    }

    #[test]
    fn test_range_wrong_type_returns_empty() {
        let s = rt_str("not-an-int".to_string());
        let e = rt_int(5);
        let result = airl_range(s, e);
        assert_eq!(range_len(result), 0, "non-Int start should return empty list");
    }

    // ── AES-256-GCM ──────────────────────────────────────────────────────────

    fn make_bytes_val(data: &[u8]) -> *mut RtValue {
        rt_bytes(data.to_vec())
    }

    fn extract_variant_tag(val: *mut RtValue) -> String {
        match unsafe { &(*val).data } {
            RtData::Variant { tag_name, .. } => tag_name.clone(),
            _ => "not-a-variant".into(),
        }
    }

    fn extract_variant_inner_bytes(val: *mut RtValue) -> Vec<u8> {
        match unsafe { &(*val).data } {
            RtData::Variant { inner, .. } => match unsafe { &(**inner).data } {
                RtData::Bytes(b) => b.clone(),
                _ => vec![],
            },
            _ => vec![],
        }
    }

    fn extract_variant_inner_str(val: *mut RtValue) -> String {
        match unsafe { &(*val).data } {
            RtData::Variant { inner, .. } => match unsafe { &(**inner).data } {
                RtData::Str(s) => s.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    #[test]
    fn test_aes_256_gcm_encrypt_decrypt_roundtrip() {
        let key = make_bytes_val(&[0u8; 32]);
        let nonce = make_bytes_val(&[0u8; 12]);
        let plaintext = make_bytes_val(b"hello, world!");

        let enc_result = airl_aes_256_gcm_encrypt(key, nonce, plaintext);
        assert_eq!(extract_variant_tag(enc_result), "Ok", "encrypt should return Ok");
        let ciphertext_bytes = extract_variant_inner_bytes(enc_result);
        // Ciphertext = plaintext (13 bytes) + 16-byte GCM tag = 29 bytes
        assert_eq!(ciphertext_bytes.len(), 13 + 16, "ciphertext should be plaintext len + 16 (tag)");

        let ciphertext = make_bytes_val(&ciphertext_bytes);
        let key2 = make_bytes_val(&[0u8; 32]);
        let nonce2 = make_bytes_val(&[0u8; 12]);
        let dec_result = airl_aes_256_gcm_decrypt(key2, nonce2, ciphertext);
        assert_eq!(extract_variant_tag(dec_result), "Ok", "decrypt should return Ok");
        let recovered = extract_variant_inner_bytes(dec_result);
        assert_eq!(recovered, b"hello, world!", "decrypted plaintext must match original");
    }

    #[test]
    fn test_aes_256_gcm_wrong_key_fails_auth() {
        let key = make_bytes_val(&[0u8; 32]);
        let nonce = make_bytes_val(&[0u8; 12]);
        let plaintext = make_bytes_val(b"secret");

        let enc_result = airl_aes_256_gcm_encrypt(key, nonce, plaintext);
        let ciphertext_bytes = extract_variant_inner_bytes(enc_result);

        let wrong_key = make_bytes_val(&[1u8; 32]);
        let nonce2 = make_bytes_val(&[0u8; 12]);
        let ciphertext = make_bytes_val(&ciphertext_bytes);
        let dec_result = airl_aes_256_gcm_decrypt(wrong_key, nonce2, ciphertext);
        assert_eq!(extract_variant_tag(dec_result), "Err", "decrypt with wrong key must return Err");
        let msg = extract_variant_inner_str(dec_result);
        assert!(msg.contains("authentication failed"), "error message should mention authentication failure");
    }

    #[test]
    fn test_aes_256_gcm_bad_key_length() {
        let short_key = make_bytes_val(&[0u8; 16]); // 16 bytes — wrong for AES-256
        let nonce = make_bytes_val(&[0u8; 12]);
        let plaintext = make_bytes_val(b"test");
        let result = airl_aes_256_gcm_encrypt(short_key, nonce, plaintext);
        assert_eq!(extract_variant_tag(result), "Err");
        assert!(extract_variant_inner_str(result).contains("key must be 32 bytes"));
    }

    #[test]
    fn test_aes_256_gcm_bad_nonce_length() {
        let key = make_bytes_val(&[0u8; 32]);
        let short_nonce = make_bytes_val(&[0u8; 8]); // 8 bytes — wrong
        let plaintext = make_bytes_val(b"test");
        let result = airl_aes_256_gcm_encrypt(key, short_nonce, plaintext);
        assert_eq!(extract_variant_tag(result), "Err");
        assert!(extract_variant_inner_str(result).contains("nonce must be 12 bytes"));
    }

    #[test]
    fn test_aes_256_gcm_empty_plaintext() {
        let key = make_bytes_val(&[0xABu8; 32]);
        let nonce = make_bytes_val(&[0x01u8; 12]);
        let plaintext = make_bytes_val(b"");

        let enc_result = airl_aes_256_gcm_encrypt(key, nonce, plaintext);
        assert_eq!(extract_variant_tag(enc_result), "Ok");
        let ct = extract_variant_inner_bytes(enc_result);
        // Empty plaintext → only 16-byte tag
        assert_eq!(ct.len(), 16);

        let key2 = make_bytes_val(&[0xABu8; 32]);
        let nonce2 = make_bytes_val(&[0x01u8; 12]);
        let ciphertext = make_bytes_val(&ct);
        let dec_result = airl_aes_256_gcm_decrypt(key2, nonce2, ciphertext);
        assert_eq!(extract_variant_tag(dec_result), "Ok");
        let recovered = extract_variant_inner_bytes(dec_result);
        assert_eq!(recovered, b"" as &[u8]);
    }

    #[test]
    fn test_aes_256_gcm_tampered_ciphertext_fails() {
        let key = make_bytes_val(&[0u8; 32]);
        let nonce = make_bytes_val(&[0u8; 12]);
        let plaintext = make_bytes_val(b"hello");

        let enc_result = airl_aes_256_gcm_encrypt(key, nonce, plaintext);
        let mut ct = extract_variant_inner_bytes(enc_result);
        // Flip a bit in the ciphertext
        ct[0] ^= 0xFF;

        let key2 = make_bytes_val(&[0u8; 32]);
        let nonce2 = make_bytes_val(&[0u8; 12]);
        let tampered = make_bytes_val(&ct);
        let dec_result = airl_aes_256_gcm_decrypt(key2, nonce2, tampered);
        assert_eq!(extract_variant_tag(dec_result), "Err", "tampered ciphertext must fail authentication");
    }
}
