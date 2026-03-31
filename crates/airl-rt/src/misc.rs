use std::collections::HashMap;
use crate::value::{rt_bool, rt_bytes, rt_float, rt_int, rt_list, rt_map, rt_nil, rt_str, rt_variant, RtData, RtValue};

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
        // Intentional process::exit: AIRL `assert` semantics require program termination on failure.
        // This is not a library error — the AIRL program explicitly requested abort.
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
    // Intentional process::exit: AIRL `panic` semantics require program termination.
    // This is not a library error — the AIRL program explicitly requested abort.
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
    // Intentional process::exit: AIRL `exit` semantics require process termination.
    // This is not a library error — the AIRL program explicitly requested exit with a code.
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

// ── Crypto (byte-oriented) ──

#[no_mangle]
pub extern "C" fn airl_sha512(s: *mut RtValue) -> *mut RtValue {
    use sha2::{Digest, Sha512};
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let hash = Sha512::digest(&input);
    rt_str(hex::encode(hash))
}

#[no_mangle]
pub extern "C" fn airl_hmac_sha512(key: *mut RtValue, msg: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    let k = match unsafe { &(*key).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let m = match unsafe { &(*msg).data } { RtData::Str(s) => s.as_bytes().to_vec(), _ => vec![] };
    let mut mac = Hmac::<Sha512>::new_from_slice(&k).unwrap();
    mac.update(&m);
    rt_str(hex::encode(mac.finalize().into_bytes()))
}

#[no_mangle]
pub extern "C" fn airl_sha256_bytes(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&bytes);
    rt_bytes(hash.to_vec())
}

#[no_mangle]
pub extern "C" fn airl_sha512_bytes(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    use sha2::Digest;
    let hash = sha2::Sha512::digest(&bytes);
    rt_bytes(hash.to_vec())
}

#[no_mangle]
pub extern "C" fn airl_hmac_sha256_bytes(key: *mut RtValue, data: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    let k = extract_bytes(key);
    let d = extract_bytes(data);
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&k).unwrap();
    mac.update(&d);
    rt_bytes(mac.finalize().into_bytes().to_vec())
}

#[no_mangle]
pub extern "C" fn airl_hmac_sha512_bytes(key: *mut RtValue, data: *mut RtValue) -> *mut RtValue {
    use hmac::{Hmac, Mac};
    let k = extract_bytes(key);
    let d = extract_bytes(data);
    let mut mac = Hmac::<sha2::Sha512>::new_from_slice(&k).unwrap();
    mac.update(&d);
    rt_bytes(mac.finalize().into_bytes().to_vec())
}

#[no_mangle]
pub extern "C" fn airl_pbkdf2_sha256(password: *mut RtValue, salt: *mut RtValue, iterations: *mut RtValue, key_len: *mut RtValue) -> *mut RtValue {
    let pw = match unsafe { &(*password).data } { RtData::Str(s) => s.clone(), _ => return rt_bytes(vec![]) };
    let salt_bytes = extract_bytes(salt);
    let iters = match unsafe { &(*iterations).data } { RtData::Int(n) => *n as u32, _ => 4096 };
    let klen = match unsafe { &(*key_len).data } { RtData::Int(n) => *n as usize, _ => 32 };
    let mut derived = vec![0u8; klen];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(pw.as_bytes(), &salt_bytes, iters, &mut derived);
    rt_bytes(derived)
}

#[no_mangle]
pub extern "C" fn airl_pbkdf2_sha512(password: *mut RtValue, salt: *mut RtValue, iterations: *mut RtValue, key_len: *mut RtValue) -> *mut RtValue {
    let pw = match unsafe { &(*password).data } { RtData::Str(s) => s.clone(), _ => return rt_bytes(vec![]) };
    let salt_bytes = extract_bytes(salt);
    let iters = match unsafe { &(*iterations).data } { RtData::Int(n) => *n as u32, _ => 4096 };
    let klen = match unsafe { &(*key_len).data } { RtData::Int(n) => *n as usize, _ => 64 };
    let mut derived = vec![0u8; klen];
    pbkdf2::pbkdf2_hmac::<sha2::Sha512>(pw.as_bytes(), &salt_bytes, iters, &mut derived);
    rt_bytes(derived)
}

#[no_mangle]
pub extern "C" fn airl_base64_decode_bytes(data: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let bytes = extract_bytes(data);
    match base64::engine::general_purpose::STANDARD.decode(&bytes) {
        Ok(decoded) => rt_bytes(decoded),
        Err(_) => rt_bytes(vec![]),
    }
}

#[no_mangle]
pub extern "C" fn airl_base64_encode_bytes(data: *mut RtValue) -> *mut RtValue {
    use base64::Engine;
    let bytes = extract_bytes(data);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
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
    rt_int(((va as u64) >> (vn as u64)) as i64)
}

#[no_mangle]
pub extern "C" fn airl_bitwise_shl(a: *mut RtValue, n: *mut RtValue) -> *mut RtValue {
    let va = match unsafe { &(*a).data } { RtData::Int(n) => *n, _ => 0 };
    let vn = match unsafe { &(*n).data } { RtData::Int(n) => *n, _ => 0 };
    rt_int(((va as u64) << (vn as u64)) as i64)
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
pub extern "C" fn airl_cpu_count() -> *mut RtValue {
    rt_int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1))
}

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

// ── Radix Parsing ──

#[no_mangle]
pub extern "C" fn airl_parse_int_radix(s: *mut RtValue, base: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*s).data } { RtData::Str(s) => s.clone(), _ => return err_variant("parse-int-radix: not a string") };
    let radix = match unsafe { &(*base).data } { RtData::Int(n) => *n as u32, _ => return err_variant("parse-int-radix: base not an int") };
    if !(2..=36).contains(&radix) { return err_variant("parse-int-radix: base must be 2-36"); }
    match i64::from_str_radix(&input, radix) {
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

#[no_mangle]
pub extern "C" fn airl_get_cwd() -> *mut RtValue {
    match std::env::current_dir() {
        Ok(p) => rt_str(p.to_string_lossy().into_owned()),
        Err(e) => rt_str(format!("<cwd-error: {}>", e)),
    }
}

// ── JSON ──

#[no_mangle]
pub extern "C" fn airl_json_parse(text: *mut RtValue) -> *mut RtValue {
    let input = match unsafe { &(*text).data } { RtData::Str(s) => s.clone(), _ => return err_variant("json-parse: not a string") };
    match parse_json_value(input.trim()) {
        Some((val, _)) => ok_variant(val),
        None => err_variant(&format!("json-parse: invalid JSON: {}", input)),
    }
}

/// Minimal recursive-descent JSON parser returning (*mut RtValue, remaining_input).
fn parse_json_value(input: &str) -> Option<(*mut RtValue, &str)> {
    let s = input.trim_start();
    if s.is_empty() { return None; }
    match s.as_bytes()[0] {
        b'"' => parse_json_string(s),
        b'{' => parse_json_object(s),
        b'[' => parse_json_array(s),
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

fn parse_json_array(s: &str) -> Option<(*mut RtValue, &str)> {
    let mut rest = s[1..].trim_start(); // skip '['
    let mut items: Vec<*mut RtValue> = Vec::new();
    if rest.starts_with(']') { return Some((rt_list(items), &rest[1..])); }
    loop {
        let (val, r) = parse_json_value(rest)?;
        items.push(val);
        rest = r.trim_start();
        if rest.starts_with(',') { rest = rest[1..].trim_start(); }
        else if rest.starts_with(']') { return Some((rt_list(items), &rest[1..])); }
        else { return None; }
    }
}

fn parse_json_object(s: &str) -> Option<(*mut RtValue, &str)> {
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
        let (val, r) = parse_json_value(rest)?;
        map.insert(key, val);
        rest = r.trim_start();
        if rest.starts_with(',') { rest = rest[1..].trim_start(); }
        else if rest.starts_with('}') { return Some((rt_map(map), &rest[1..])); }
        else { return None; }
    }
}

#[no_mangle]
pub extern "C" fn airl_json_stringify(val: *mut RtValue) -> *mut RtValue {
    fn to_json(v: &RtValue) -> String {
        match &v.data {
            RtData::Str(s) => format!("\"{}\"", s),
            RtData::Int(n) => n.to_string(),
            RtData::Float(f) => f.to_string(),
            RtData::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            RtData::Nil | RtData::Unit => "null".to_string(),
            RtData::List(items) => {
                let parts: Vec<String> = items.iter().map(|&p| {
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
                    format!("\"{}\":{}", k, to_json(val))
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
        }
    }
    let v = unsafe { &*val };
    rt_str(to_json(v))
}

// ── TCP sockets ──

use std::net::TcpStream;
use std::sync::atomic::{AtomicI64, Ordering};
use std::io::{Read, Write};

enum RtTcpHandle {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for RtTcpHandle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self { RtTcpHandle::Plain(s) => s.read(buf), RtTcpHandle::Tls(s) => s.read(buf) }
    }
}

impl Write for RtTcpHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self { RtTcpHandle::Plain(s) => s.write(buf), RtTcpHandle::Tls(s) => s.write(buf) }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self { RtTcpHandle::Plain(s) => s.flush(), RtTcpHandle::Tls(s) => s.flush() }
    }
}

impl RtTcpHandle {
    fn set_timeout(&self, timeout: Option<std::time::Duration>) -> std::io::Result<()> {
        let stream = match self { RtTcpHandle::Plain(s) => s, RtTcpHandle::Tls(s) => s.get_ref() };
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)?;
        Ok(())
    }
}

static NEXT_TCP_HANDLE: AtomicI64 = AtomicI64::new(1);

fn tcp_handles() -> &'static std::sync::Mutex<std::collections::HashMap<i64, RtTcpHandle>> {
    use std::sync::{Mutex, OnceLock};
    static HANDLES: OnceLock<Mutex<std::collections::HashMap<i64, RtTcpHandle>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

// ── TCP server (listen/accept) ──────────────────────────────────────────

static NEXT_LISTENER_HANDLE: AtomicI64 = AtomicI64::new(1);

fn tcp_listeners() -> &'static std::sync::Mutex<std::collections::HashMap<i64, std::net::TcpListener>> {
    use std::sync::{Mutex, OnceLock};
    static LISTENERS: OnceLock<Mutex<std::collections::HashMap<i64, std::net::TcpListener>>> = OnceLock::new();
    LISTENERS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Bind a TCP server socket. Returns Result[handle, error].
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
#[no_mangle]
pub extern "C" fn airl_tcp_accept(listener_handle: *mut RtValue) -> *mut RtValue {
    let lh = match unsafe { &(*listener_handle).data } { RtData::Int(n) => *n, _ => return err_variant("tcp-accept: handle must be int") };
    // Remove listener from map so we can block on accept without holding the lock
    let listener = tcp_listeners().lock().unwrap().remove(&lh);
    match listener {
        Some(listener) => {
            let result = match listener.accept() {
                Ok((stream, _addr)) => {
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

#[no_mangle]
pub extern "C" fn airl_tcp_connect(host: *mut RtValue, port: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*host).data } { RtData::Str(s) => s.clone(), _ => return err_variant("host must be string") };
    let p = match unsafe { &(*port).data } { RtData::Int(n) => *n as u16, _ => return err_variant("port must be int") };
    match TcpStream::connect(format!("{}:{}", h, p)) {
        Ok(stream) => { let handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst); tcp_handles().lock().unwrap().insert(handle, RtTcpHandle::Plain(stream)); ok_variant(rt_int(handle)) }
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
        RtData::Bytes(v) => v.clone(),
        RtData::List(items) => items.iter().map(|i| match unsafe { &(**i).data } { RtData::Int(n) => *n as u8, _ => 0 }).collect(),
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

#[no_mangle]
pub extern "C" fn airl_tcp_recv(handle: *mut RtValue, max_bytes: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let max = match unsafe { &(*max_bytes).data } { RtData::Int(n) => *n as usize, _ => return err_variant("max must be int") };
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

#[no_mangle]
pub extern "C" fn airl_tcp_recv_exact(handle: *mut RtValue, count: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*handle).data } { RtData::Int(n) => *n, _ => return err_variant("handle must be int") };
    let n = match unsafe { &(*count).data } { RtData::Int(n) => *n as usize, _ => return err_variant("count must be int") };
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

#[no_mangle]
pub extern "C" fn airl_tcp_connect_tls(host: *mut RtValue, port: *mut RtValue, ca_path: *mut RtValue, cert_path: *mut RtValue, key_path: *mut RtValue) -> *mut RtValue {
    let h = match unsafe { &(*host).data } { RtData::Str(s) => s.clone(), _ => return err_variant("host must be string") };
    let p = match unsafe { &(*port).data } { RtData::Int(n) => *n as u16, _ => return err_variant("port must be int") };
    let ca = match unsafe { &(*ca_path).data } { RtData::Str(s) => s.clone(), _ => return err_variant("ca-path must be string") };
    let cert = match unsafe { &(*cert_path).data } { RtData::Str(s) => s.clone(), _ => return err_variant("cert-path must be string") };
    let key = match unsafe { &(*key_path).data } { RtData::Str(s) => s.clone(), _ => return err_variant("key-path must be string") };

    let mut root_store = rustls::RootCertStore::empty();
    if ca.is_empty() {
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
    let server_name = match rustls::pki_types::ServerName::try_from(h.clone()) { Ok(n) => n, Err(e) => return err_variant(&format!("invalid hostname: {}", e)) };
    let conn = match rustls::ClientConnection::new(std::sync::Arc::new(config), server_name) { Ok(c) => c, Err(e) => return err_variant(&format!("tls init: {}", e)) };
    let tls_stream = rustls::StreamOwned::new(conn, tcp);

    let handle = NEXT_TCP_HANDLE.fetch_add(1, Ordering::SeqCst);
    tcp_handles().lock().unwrap().insert(handle, RtTcpHandle::Tls(Box::new(tls_stream)));
    ok_variant(rt_int(handle))
}

// ── Byte encoding ──

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
        RtData::List(items) => items.iter().map(|i| match unsafe { &(**i).data } { RtData::Int(n) => *n as u8, _ => 0 }).collect(),
        _ => vec![],
    }
}

fn byte_at(val: *mut RtValue, offset: usize) -> u8 {
    match unsafe { &(*val).data } {
        RtData::Bytes(v) => if offset < v.len() { v[offset] } else { 0 },
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
    rt_bytes(input.into_bytes())
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
    rt_bytes(ab)
}

/// Concatenate a list of byte lists in one O(n) pass.
/// Input: List[List[u8]]. Output: List[u8].
/// Replaces the O(n²) `(fold (fn [acc part] (bytes-concat acc part)) [] parts)` pattern.
#[no_mangle]
pub extern "C" fn airl_bytes_concat_all(parts: *mut RtValue) -> *mut RtValue {
    let part_lists = match unsafe { &(*parts).data } {
        RtData::List(items) => items,
        _ => return rt_bytes(vec![]),
    };
    // Measure total size, allocate once
    let mut total = 0usize;
    let extracted: Vec<Vec<u8>> = part_lists.iter().map(|p| {
        let bytes = extract_bytes(*p);
        total += bytes.len();
        bytes
    }).collect();
    // Build result in one pass
    let mut result = Vec::with_capacity(total);
    for bytes in &extracted {
        result.extend_from_slice(bytes);
    }
    rt_bytes(result)
}

#[no_mangle]
pub extern "C" fn airl_bytes_slice(buf: *mut RtValue, offset: *mut RtValue, len: *mut RtValue) -> *mut RtValue {
    let off = match unsafe { &(*offset).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let slen = match unsafe { &(*len).data } { RtData::Int(n) => *n as usize, _ => 0 };
    let bytes = extract_bytes(buf);
    if off + slen > bytes.len() { return rt_bytes(vec![]); }
    rt_bytes(bytes[off..off+slen].to_vec())
}

#[no_mangle]
pub extern "C" fn airl_crc32c(buf: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(buf);
    rt_int(crc32c::crc32c(&bytes) as i64)
}

// ── Compression builtins ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn airl_gzip_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::Write::write_all(&mut encoder, &bytes).unwrap();
    let compressed = encoder.finish().unwrap();
    rt_bytes(compressed)
}

#[no_mangle]
pub extern "C" fn airl_gzip_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    use flate2::read::GzDecoder;
    let mut decoder = GzDecoder::new(&bytes[..]);
    let mut decompressed = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut decompressed).unwrap();
    rt_bytes(decompressed)
}

#[no_mangle]
pub extern "C" fn airl_snappy_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    let compressed = snap::raw::Encoder::new().compress_vec(&bytes).unwrap();
    rt_bytes(compressed)
}

#[no_mangle]
pub extern "C" fn airl_snappy_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    let decompressed = snap::raw::Decoder::new().decompress_vec(&bytes).unwrap();
    rt_bytes(decompressed)
}

#[no_mangle]
pub extern "C" fn airl_lz4_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    let compressed = lz4_flex::compress_prepend_size(&bytes);
    rt_bytes(compressed)
}

#[no_mangle]
pub extern "C" fn airl_lz4_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    let decompressed = lz4_flex::decompress_size_prepended(&bytes).unwrap();
    rt_bytes(decompressed)
}

#[no_mangle]
pub extern "C" fn airl_zstd_compress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    let compressed = zstd::encode_all(&bytes[..], 3).unwrap();
    rt_bytes(compressed)
}

#[no_mangle]
pub extern "C" fn airl_zstd_decompress(data: *mut RtValue) -> *mut RtValue {
    let bytes = extract_bytes(data);
    let decompressed = zstd::decode_all(&bytes[..]).unwrap();
    rt_bytes(decompressed)
}

// airl_run_bytecode and airl_compile_to_executable are defined elsewhere in airl-runtime
