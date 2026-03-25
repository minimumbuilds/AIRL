use crate::value::{rt_bool, rt_float, rt_int, rt_list, rt_nil, rt_str, rt_variant, RtData, RtValue};

fn ok_variant(inner: *mut RtValue) -> *mut RtValue {
    rt_variant("Ok".into(), inner)
}

fn err_variant(msg: &str) -> *mut RtValue {
    rt_variant("Err".into(), rt_str(msg.into()))
}

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
    let mut result = String::new();
    for i in 0..count {
        let v = unsafe { &**args.add(i) };
        match &v.data {
            RtData::Str(s) => result.push_str(s),
            _ => result.push_str(&format!("{}", v)),
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
                _ => result.push_str(&format!("{}", v)),
            }
        } else {
            result.push(c);
        }
    }
    rt_str(result)
}

// ── assert ──

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
        std::process::exit(1);
    }
    rt_bool(true)
}

// ── panic ──

#[no_mangle]
pub extern "C" fn airl_panic(msg: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*msg };
    match &val.data {
        RtData::Str(s) => eprintln!("panic: {}", s),
        _ => eprintln!("panic"),
    }
    std::process::exit(1);
}

// ── exit ──

#[no_mangle]
pub extern "C" fn airl_exit(code: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*code };
    let c = match &val.data {
        RtData::Int(n) => *n as i32,
        _ => 1,
    };
    std::process::exit(c);
}

// ── sleep ──

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

// ── format-time ──

#[no_mangle]
pub extern "C" fn airl_format_time(ms_val: *mut RtValue, fmt_val: *mut RtValue) -> *mut RtValue {
    let ms = match unsafe { &(*ms_val).data } {
        RtData::Int(n) => *n,
        _ => return rt_str(String::new()),
    };
    let fmt = match unsafe { &(*fmt_val).data } {
        RtData::Str(s) => s.clone(),
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

#[no_mangle]
pub extern "C" fn airl_read_lines(path: *mut RtValue) -> *mut RtValue {
    let p = match unsafe { &(*path).data } {
        RtData::Str(s) => s.clone(),
        _ => return rt_list(vec![]),
    };
    match std::fs::read_to_string(&p) {
        Ok(content) => {
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
    let mut items = Vec::new();
    if let RtData::List(a_items) = &a_val.data {
        for item in a_items { items.push(crate::memory::airl_value_clone(*item)); }
    }
    if let RtData::List(b_items) = &b_val.data {
        for item in b_items { items.push(crate::memory::airl_value_clone(*item)); }
    }
    rt_list(items)
}

#[no_mangle]
pub extern "C" fn airl_range(start: *mut RtValue, end: *mut RtValue) -> *mut RtValue {
    let s = match unsafe { &(*start).data } { RtData::Int(n) => *n, _ => return rt_list(vec![]) };
    let e = match unsafe { &(*end).data } { RtData::Int(n) => *n, _ => return rt_list(vec![]) };
    if s >= e { return rt_list(vec![]); }
    let items: Vec<*mut RtValue> = (s..e).map(|i| rt_int(i)).collect();
    rt_list(items)
}

#[no_mangle]
pub extern "C" fn airl_reverse_list(list: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*list };
    if let RtData::List(items) = &val.data {
        let reversed: Vec<*mut RtValue> = items.iter().rev().map(|i| crate::memory::airl_value_clone(*i)).collect();
        rt_list(reversed)
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_take(n_val: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let n = match unsafe { &(*n_val).data } { RtData::Int(n) => *n as usize, _ => return rt_list(vec![]) };
    let val = unsafe { &*list };
    if let RtData::List(items) = &val.data {
        let take_n = n.min(items.len());
        let taken: Vec<*mut RtValue> = items[..take_n].iter().map(|i| crate::memory::airl_value_clone(*i)).collect();
        rt_list(taken)
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_drop(n_val: *mut RtValue, list: *mut RtValue) -> *mut RtValue {
    let n = match unsafe { &(*n_val).data } { RtData::Int(n) => *n as usize, _ => return rt_list(vec![]) };
    let val = unsafe { &*list };
    if let RtData::List(items) = &val.data {
        if n >= items.len() { return rt_list(vec![]); }
        let dropped: Vec<*mut RtValue> = items[n..].iter().map(|i| crate::memory::airl_value_clone(*i)).collect();
        rt_list(dropped)
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_zip(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let a_val = unsafe { &*a };
    let b_val = unsafe { &*b };
    if let (RtData::List(a_items), RtData::List(b_items)) = (&a_val.data, &b_val.data) {
        let len = a_items.len().min(b_items.len());
        let items: Vec<*mut RtValue> = (0..len).map(|i| {
            rt_list(vec![
                crate::memory::airl_value_clone(a_items[i]),
                crate::memory::airl_value_clone(b_items[i]),
            ])
        }).collect();
        rt_list(items)
    } else {
        rt_list(vec![])
    }
}

#[no_mangle]
pub extern "C" fn airl_flatten(list: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*list };
    if let RtData::List(items) = &val.data {
        let mut result = Vec::new();
        for item in items {
            let sub = unsafe { &**item };
            if let RtData::List(sub_items) = &sub.data {
                for si in sub_items { result.push(crate::memory::airl_value_clone(*si)); }
            } else {
                result.push(crate::memory::airl_value_clone(*item));
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
    if let RtData::List(items) = &val.data {
        let result: Vec<*mut RtValue> = items.iter().enumerate().map(|(i, item)| {
            rt_list(vec![rt_int(i as i64), crate::memory::airl_value_clone(*item)])
        }).collect();
        rt_list(result)
    } else {
        rt_list(vec![])
    }
}

// ── Path operations ──

#[no_mangle]
pub extern "C" fn airl_path_join(parts: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*parts };
    if let RtData::List(items) = &val.data {
        let mut path = std::path::PathBuf::new();
        for item in items {
            let s = unsafe { &**item };
            if let RtData::Str(p) = &s.data { path.push(p); }
        }
        rt_str(path.to_string_lossy().into_owned())
    } else {
        rt_str(String::new())
    }
}

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

#[no_mangle]
pub extern "C" fn airl_is_absolute(path: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*path };
    if let RtData::Str(s) = &val.data {
        rt_bool(std::path::Path::new(s).is_absolute())
    } else {
        rt_bool(false)
    }
}

// ── Regex ──

#[no_mangle]
pub extern "C" fn airl_regex_match(pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.clone(), _ => return rt_nil() };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => return rt_nil() };
    match regex::Regex::new(&pattern) {
        Ok(re) => match re.find(&input) {
            Some(m) => rt_str(m.as_str().to_string()),
            None => rt_nil(),
        },
        Err(_) => rt_nil(),
    }
}

#[no_mangle]
pub extern "C" fn airl_regex_find_all(pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.clone(), _ => return rt_list(vec![]) };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => return rt_list(vec![]) };
    match regex::Regex::new(&pattern) {
        Ok(re) => {
            let items: Vec<*mut RtValue> = re.find_iter(&input).map(|m| rt_str(m.as_str().to_string())).collect();
            rt_list(items)
        }
        Err(_) => rt_list(vec![]),
    }
}

#[no_mangle]
pub extern "C" fn airl_regex_replace(pat: *mut RtValue, s: *mut RtValue, replacement: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.clone(), _ => return crate::memory::airl_value_clone(s) };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => return crate::memory::airl_value_clone(s) };
    let repl = match unsafe { &(*replacement).data } { RtData::Str(s) => s.clone(), _ => return crate::memory::airl_value_clone(s) };
    match regex::Regex::new(&pattern) {
        Ok(re) => rt_str(re.replace_all(&input, repl.as_str()).into_owned()),
        Err(_) => rt_str(input),
    }
}

#[no_mangle]
pub extern "C" fn airl_regex_split(pat: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let pattern = match unsafe { &(*pat).data } { RtData::Str(s) => s.clone(), _ => return rt_list(vec![]) };
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => return rt_list(vec![]) };
    match regex::Regex::new(&pattern) {
        Ok(re) => {
            let items: Vec<*mut RtValue> = re.split(&input).map(|s| rt_str(s.to_string())).collect();
            rt_list(items)
        }
        Err(_) => rt_list(vec![rt_str(input)]),
    }
}

// ── Crypto ──

#[no_mangle]
pub extern "C" fn airl_sha256(s: *mut RtValue) -> *mut RtValue {
    use sha2::{Digest, Sha256};
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let hash = Sha256::digest(&input);
    rt_str(hex::encode(hash))
}

#[no_mangle]
pub extern "C" fn airl_hmac_sha256(key: *mut RtValue, msg: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let k = match unsafe { &(*key).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let m = match unsafe { &(*msg).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let mut mac = Hmac::<Sha256>::new_from_slice(&k).unwrap();
    mac.update(&m);
    rt_str(hex::encode(mac.finalize().into_bytes()))
}

#[no_mangle]
pub extern "C" fn airl_base64_encode(s: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => String::new() };
    rt_str(base64::engine::general_purpose::STANDARD.encode(input.as_bytes()))
}

#[no_mangle]
pub extern "C" fn airl_base64_decode(s: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => String::new() };
    match base64::engine::general_purpose::STANDARD.decode(input.as_bytes()) {
        Ok(bytes) => rt_str(String::from_utf8_lossy(&bytes).into_owned()),
        Err(_) => rt_str(String::new()),
    }
}

#[no_mangle]
pub extern "C" fn airl_random_bytes(n: *mut RtValue) -> *mut RtValue {
    let count = match unsafe { &(*n).data } { RtData::Int(n) => *n as usize, _ => 0 };
    use rand::RngCore;
    let mut buf = vec![0u8; count];
    rand::thread_rng().fill_bytes(&mut buf);
    let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
    rt_str(hex)
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
    let s = if val.fract() == 0.0 && val.is_finite() { format!("{:.1}", val) } else { format!("{}", val) };
    rt_str(s)
}

#[no_mangle]
pub extern "C" fn airl_string_to_int(s: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => return err_variant("not a string") };
    match input.parse::<i64>() {
        Ok(v) => ok_variant(rt_int(v)),
        Err(e) => err_variant(&format!("invalid int: {}", e)),
    }
}

// ── System ──

#[no_mangle]
pub extern "C" fn airl_time_now() -> *mut RtValue {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    rt_int(ms)
}

#[no_mangle]
pub extern "C" fn airl_getenv(name: *mut RtValue) -> *mut RtValue {
    let key = match unsafe { &(*name).data } { RtData::Str(s) => s.clone(), _ => return err_variant("not a string") };
    match std::env::var(&key) {
        Ok(val) => ok_variant(rt_str(val)),
        Err(_) => err_variant(&format!("env var not found: {}", key)),
    }
}

#[no_mangle]
pub extern "C" fn airl_shell_exec(cmd: *mut RtValue, args_list: *mut RtValue) -> *mut RtValue {
    let command = match unsafe { &(*cmd).data } { RtData::Str(s) => s.clone(), _ => return err_variant("not a string") };
    let mut cmd_args = Vec::new();
    if let RtData::List(items) = unsafe { &(*args_list).data } {
        for item in items {
            if let RtData::Str(s) = unsafe { &(**item).data } { cmd_args.push(s.clone()); }
        }
    }
    match std::process::Command::new(&command).args(&cmd_args).output() {
        Ok(output) => ok_variant(rt_str(String::from_utf8_lossy(&output.stdout).into_owned())),
        Err(e) => err_variant(&format!("shell-exec: {}", e)),
    }
}

// ── HTTP stub ──

#[no_mangle]
pub extern "C" fn airl_http_request(_method: *mut RtValue, _url: *mut RtValue, _body: *mut RtValue, _headers: *mut RtValue) -> *mut RtValue {
    err_variant("http-request: not available in AOT — link with libcurl for HTTP support")
}

// ── JSON ──

#[no_mangle]
pub extern "C" fn airl_json_parse(text: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*text).data } { RtData::Str(s) => s.clone(), _ => return rt_nil() };
    let trimmed = input.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        rt_str(trimmed[1..trimmed.len()-1].to_string())
    } else if let Ok(n) = trimmed.parse::<i64>() {
        rt_int(n)
    } else if let Ok(f) = trimmed.parse::<f64>() {
        rt_float(f)
    } else if trimmed == "true" {
        rt_bool(true)
    } else if trimmed == "false" {
        rt_bool(false)
    } else if trimmed == "null" {
        rt_nil()
    } else {
        rt_str(input)
    }
}

#[no_mangle]
pub extern "C" fn airl_json_stringify(val: *mut RtValue) -> *mut RtValue {
    let v = unsafe { &*val };
    let s = match &v.data {
        RtData::Str(s) => format!("\"{}\"", s),
        RtData::Int(n) => n.to_string(),
        RtData::Float(f) => f.to_string(),
        RtData::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        RtData::Nil => "null".to_string(),
        _ => format!("{}", v),
    };
    rt_str(s)
}

// ── TCP sockets ──

use std::net::TcpStream;
use std::sync::atomic::{AtomicI64, Ordering};
use std::io::{Read, Write};

static NEXT_TCP_HANDLE: AtomicI64 = AtomicI64::new(1);

fn tcp_handles() -> &'static std::sync::Mutex<std::collections::HashMap<i64, TcpStream>> {
    use std::sync::{Mutex, OnceLock};
    static HANDLES: OnceLock<Mutex<std::collections::HashMap<i64, TcpStream>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

#[no_mangle]
pub extern "C" fn airl_tcp_connect(host: *mut RtValue, port: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*host).data } { RtData::Str(s) => s.clone(), _ => return err_variant("host must be string") };
    let p = match unsafe { &(*port).data } { RtData::Int(n) => *n as u16, _ => return err_variant("port must be int") };
    match TcpStream::connect(format!("{}:{}", h, p)) {
        Ok(stream) => { let handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst); tcp_handles().lock().unwrap().insert(handle, stream); ok_variant(rt_int(handle)) }
        Err(e) => err_variant(&format!("tcp-connect: {}", e)),
    }
}

#[no_mangle]
pub extern "C" fn airl_tcp_close(handle: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    match tcp_handles().lock().unwrap().remove(&h) { Some(_) => ok_variant(rt_nil()), None => err_variant("invalid handle") }
}

#[no_mangle]
pub extern "C" fn airl_tcp_send(handle: *mut RtValue, data: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let bytes: Vec<u8> = match unsafe { &(*data).data } {
        RtData::List(items) => items.iter().map(|i| match unsafe { &(**i).data } { RtData::Int(n) => *n as u8, _ => 0 }).collect(),
        _ => return err_variant("data must be list"),
    };
    let mut handles = tcp_handles().lock().unwrap();
    match handles.get_mut(&h) {
        Some(stream) => match stream.write_all(&bytes) { Ok(()) => ok_variant(rt_int(bytes.len() as i64)), Err(e) => err_variant(&format!("tcp-send: {}", e)) },
        None => err_variant("invalid handle"),
    }
}

#[no_mangle]
pub extern "C" fn airl_tcp_recv(handle: *mut RtValue, max_bytes: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let max = match unsafe { &(*max_bytes).data } { RtData::Int(n) => *n as usize, _ => return err_variant("max must be int") };
    let mut handles = tcp_handles().lock().unwrap();
    match handles.get_mut(&h) {
        Some(stream) => { let mut buf = vec![0u8; max]; match stream.read(&mut buf) { Ok(n) => { buf.truncate(n); ok_variant(rt_list(buf.iter().map(|b| rt_int(*b as i64)).collect())) }, Err(e) => err_variant(&format!("tcp-recv: {}", e)) } }
        None => err_variant("invalid handle"),
    }
}

#[no_mangle]
pub extern "C" fn airl_tcp_recv_exact(handle: *mut RtValue, count: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let n = match unsafe { &(*count).data } { RtData::Int(n) => *n as usize, _ => return err_variant("count must be int") };
    let mut handles = tcp_handles().lock().unwrap();
    match handles.get_mut(&h) {
        Some(stream) => { let mut buf = vec![0u8; n]; match stream.read_exact(&mut buf) { Ok(()) => ok_variant(rt_list(buf.iter().map(|b| rt_int(*b as i64)).collect())), Err(e) => err_variant(&format!("tcp-recv-exact: {}", e)) } }
        None => err_variant("invalid handle"),
    }
}

#[no_mangle]
pub extern "C" fn airl_tcp_set_timeout(handle: *mut RtValue, ms: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let millis = match unsafe { &(*ms).data } { RtData::Int(n) => *n, _ => return err_variant("ms must be int") };
    let timeout = if millis > 0 { Some(std::time::Duration::from_millis(millis as u64)) } else { None };
    let handles = tcp_handles().lock().unwrap();
    match handles.get(&h) {
        Some(stream) => { let _ = stream.set_read_timeout(timeout); let _ = stream.set_write_timeout(timeout); ok_variant(rt_nil()) }
        None => err_variant("invalid handle"),
    }
}

// ── Byte encoding ──

#[no_mangle]
pub extern "C" fn airl_bytes_from_int16(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n as i16, _ => 0 };
    rt_list(val.to_be_bytes().iter().map(|b| rt_int(*b as i64)).collect())
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int32(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n as i32, _ => 0 };
    rt_list(val.to_be_bytes().iter().map(|b| rt_int(*b as i64)).collect())
}

#[no_mangle]
pub extern "C" fn airl_bytes_from_int64(n: *mut RtValue) -> *mut RtValue {
    let val = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    rt_list(val.to_be_bytes().iter().map(|b| rt_int(*b as i64)).collect())
}

fn extract_bytes(list: *mut RtValue) -> Vec<u8> {
    match unsafe { &(*list).data } {
        RtData::List(items) => items.iter().map(|i| match unsafe { &(**i).data } { RtData::Int(n) => *n as u8, _ => 0 }).collect(),
        _ => vec![],
    }
}

fn byte_at(list: *mut RtValue, offset: usize) -> u8 {
    match unsafe { &(*list).data } {
        RtData::List(items) => if offset < items.len() { match unsafe { &(*items[offset]).data } { RtData::Int(n) => *n as u8, _ => 0 } } else { 0 },
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
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => String::new() };
    rt_list(input.as_bytes().iter().map(|b| rt_int(*b as i64)).collect())
}

#[no_mangle]
pub extern "C" fn airl_bytes_to_string(buf: *mut RtValue, offset: *mut RtValue, len: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let slen = match unsafe { &(*len).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let bytes = extract_bytes(buf);
    if off + slen > bytes.len() { return rt_str(String::new()); }
    rt_str(String::from_utf8_lossy(&bytes[off..off+slen]).into_owned())
}

#[no_mangle]
pub extern "C" fn airl_bytes_concat(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let mut ab = extract_bytes(a); ab.extend_from_slice(&extract_bytes(b));
    rt_list(ab.iter().map(|b| rt_int(*b as i64)).collect())
}

#[no_mangle]
pub extern "C" fn airl_bytes_slice(buf: *mut RtValue, offset: *mut RtValue, len: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let slen = match unsafe { &(*len).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let bytes = extract_bytes(buf);
    if off + slen > bytes.len() { return rt_list(vec![]); }
    rt_list(bytes[off..off+slen].iter().map(|b| rt_int(*b as i64)).collect())
}

#[no_mangle]
pub extern "C" fn airl_crc32c(buf: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(buf);
    rt_int(crc32c::crc32c(&bytes) as i64)
}

// airl_run_bytecode and airl_compile_to_executable are defined elsewhere in airl-runtime
