use crate::value::{rt_bool, rt_int, rt_nil, rt_str, rt_unit, rt_variant, RtData, RtValue};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[no_mangle]
pub extern "C" fn airl_print(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => print!("{}", s),
        _ => print!("{}", val),
    }
    rt_unit()
}

/// Display a value using its Display impl (strings are quoted) with trailing newline.
/// Matches the Rust driver's `println!("{}", val)` behavior for program results.
#[no_mangle]
pub extern "C" fn airl_println(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    println!("{}", val);
    rt_unit()
}

/// Variadic print: takes a pointer to an array of `*mut RtValue` and a count.
/// Prints all values space-separated with a trailing newline (matching
/// the interpreter's `builtin_print` semantics).
#[no_mangle]
pub extern "C" fn airl_print_values(args: *const *mut RtValue, count: i64) -> *mut RtValue {
    let count = count as usize;
    for i in 0..count {
        if i > 0 {
            print!(" ");
        }
        let v = unsafe { *args.add(i) };
        let val = unsafe { &*v };
        match &val.data {
            RtData::Str(s) => print!("{}", s),
            _ => print!("{}", val),
        }
    }
    println!();
    rt_unit()
}

/// Read one line from stdin (blocking). Returns the line as a Str with trailing newline stripped.
/// Returns (Ok line) on success, (Err msg) on EOF or I/O error.
#[no_mangle]
pub extern "C" fn airl_read_line() -> *mut RtValue {
    use std::io::BufRead;
    let _ = std::io::stdout().flush(); // flush any pending prompt
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) => rt_variant("Err".into(), rt_str("EOF".into())),
        Ok(_) => {
            if line.ends_with('\n') { line.pop(); }
            if line.ends_with('\r') { line.pop(); }
            rt_variant("Ok".into(), rt_str(line))
        }
        Err(e) => rt_variant("Err".into(), rt_str(format!("read-line: {}", e))),
    }
}

/// Read all of stdin as a single string (blocking, reads until EOF).
/// Returns (Ok contents) on success, (Err msg) on I/O error.
#[no_mangle]
pub extern "C" fn airl_read_stdin() -> *mut RtValue {
    let mut buf = String::new();
    match std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut buf) {
        Ok(_) => rt_variant("Ok".into(), rt_str(buf)),
        Err(e) => rt_variant("Err".into(), rt_str(format!("read-stdin: {}", e))),
    }
}

/// Print to stderr. Returns nil.
#[no_mangle]
pub extern "C" fn airl_eprint(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => eprint!("{}", s),
        _ => eprint!("{}", val),
    }
    rt_nil()
}

/// Print to stderr with newline. Returns nil.
#[no_mangle]
pub extern "C" fn airl_eprintln(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => eprintln!("{}", s),
        _ => eprintln!("{}", val),
    }
    rt_nil()
}

/// Flush stdout — called at program exit to ensure all print output is visible.
#[no_mangle]
pub extern "C" fn airl_flush_stdout() {
    let _ = std::io::stdout().flush();
}

/// Read a file's contents as a string.  Takes a path (*mut RtValue Str),
/// returns the file contents as an RtValue Str, or calls rt_error on failure.
#[no_mangle]
pub extern "C" fn airl_read_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("read-file: expected string path"),
        }
    };
    match std::fs::read_to_string(&path_str) {
        Ok(contents) => rt_str(contents),
        Err(e) => crate::error::rt_error(&format!("read-file: {}: {}", path_str, e)),
    }
}

/// Write a string to a file, creating parent directories if needed.
#[no_mangle]
pub extern "C" fn airl_write_file(path: *mut RtValue, content: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("write-file: expected string path"),
        }
    };
    let content_str = unsafe {
        match &(*content).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("write-file: expected string content"),
        }
    };
    if let Some(parent) = std::path::Path::new(&path_str).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                crate::error::rt_error(&format!("write-file: create dirs: {}: {}", path_str, e));
            }
        }
    }
    match std::fs::write(&path_str, &content_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("write-file: {}: {}", path_str, e)),
    }
}

/// Append a string to a file, creating it if it doesn't exist.
#[no_mangle]
pub extern "C" fn airl_append_file(path: *mut RtValue, content: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("append-file: expected string path"),
        }
    };
    let content_str = unsafe {
        match &(*content).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("append-file: expected string content"),
        }
    };
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path_str)
    {
        Ok(mut f) => match f.write_all(content_str.as_bytes()) {
            Ok(()) => rt_bool(true),
            Err(e) => crate::error::rt_error(&format!("append-file: write: {}: {}", path_str, e)),
        },
        Err(e) => crate::error::rt_error(&format!("append-file: open: {}: {}", path_str, e)),
    }
}

/// Delete a file.
#[no_mangle]
pub extern "C" fn airl_delete_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("delete-file: expected string path"),
        }
    };
    match std::fs::remove_file(&path_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("delete-file: {}: {}", path_str, e)),
    }
}

/// Delete a directory and all its contents.
#[no_mangle]
pub extern "C" fn airl_delete_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("delete-dir: expected string path"),
        }
    };
    match std::fs::remove_dir_all(&path_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("delete-dir: {}: {}", path_str, e)),
    }
}

/// List directory contents as a sorted list of filenames.
#[no_mangle]
pub extern "C" fn airl_read_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("read-dir: expected string path"),
        }
    };
    match std::fs::read_dir(&path_str) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            let items: Vec<*mut RtValue> = names.into_iter().map(|n| rt_str(n)).collect();
            crate::value::rt_list(items)
        }
        Err(e) => crate::error::rt_error(&format!("read-dir: {}: {}", path_str, e)),
    }
}

/// Create a directory and all parent directories.
#[no_mangle]
pub extern "C" fn airl_create_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("create-dir: expected string path"),
        }
    };
    match std::fs::create_dir_all(&path_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("create-dir: {}: {}", path_str, e)),
    }
}

/// Get the size of a file in bytes.
#[no_mangle]
pub extern "C" fn airl_file_size(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("file-size: expected string path"),
        }
    };
    match std::fs::metadata(&path_str) {
        Ok(meta) => crate::value::rt_int(meta.len() as i64),
        Err(e) => crate::error::rt_error(&format!("file-size: {}: {}", path_str, e)),
    }
}

/// Check if a path is a directory.
#[no_mangle]
pub extern "C" fn airl_is_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("is-dir: expected string path"),
        }
    };
    rt_bool(std::path::Path::new(&path_str).is_dir())
}

/// Check if a path exists.
#[no_mangle]
pub extern "C" fn airl_file_exists(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("file-exists?: expected string path"),
        }
    };
    rt_bool(std::path::Path::new(&path_str).exists())
}

/// Rename a file or directory.
#[no_mangle]
pub extern "C" fn airl_rename_file(old: *mut RtValue, new: *mut RtValue) -> *mut RtValue {
    let old_str = unsafe {
        match &(*old).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("rename-file: expected string path (old)"),
        }
    };
    let new_str = unsafe {
        match &(*new).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("rename-file: expected string path (new)"),
        }
    };
    match std::fs::rename(&old_str, &new_str) {
        Ok(()) => rt_bool(true),
        Err(e) => crate::error::rt_error(&format!("rename-file: {} -> {}: {}", old_str, new_str, e)),
    }
}

/// Return command-line arguments as a List of Str values.
#[no_mangle]
pub extern "C" fn airl_get_args() -> *mut RtValue {
    let args: Vec<*mut RtValue> = std::env::args()
        .map(|a| rt_str(a))
        .collect();
    crate::value::rt_list(args)
}

#[no_mangle]
pub extern "C" fn airl_type_of(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let name = match &val.data {
        RtData::Nil => "nil",
        RtData::Unit => "unit",
        RtData::Int(_) => "int",
        RtData::Float(_) => "float",
        RtData::Bool(_) => "bool",
        RtData::Str(_) => "string",
        RtData::List { .. } => "list",
        RtData::Map(_) => "map",
        RtData::Variant { .. } => "variant",
        RtData::Closure { .. } => "closure",
        RtData::Bytes(_) => "bytes",
    };
    rt_str(name.to_string())
}

#[no_mangle]
pub extern "C" fn airl_valid(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let is_nil = matches!(&val.data, RtData::Nil);
    rt_bool(!is_nil)
}

// ─────────────────────────────────────────────────────────────────────────────
// Temp files, file metadata
// ─────────────────────────────────────────────────────────────────────────────

/// `temp-file(prefix)` — create a temp file, return its path.
#[no_mangle]
pub extern "C" fn airl_temp_file(prefix: *mut RtValue) -> *mut RtValue {
    let pfx = match unsafe { &(*prefix).data } { RtData::Str(s) => s.clone(), _ => "airl".into() };
    let cnt = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{}-{}-{}", pfx, std::process::id(), cnt));
    std::fs::write(&path, "").unwrap_or(());
    rt_str(path.to_string_lossy().into_owned())
}

/// `temp-dir(prefix)` — create a temp directory, return its path.
#[no_mangle]
pub extern "C" fn airl_temp_dir(prefix: *mut RtValue) -> *mut RtValue {
    let pfx = match unsafe { &(*prefix).data } { RtData::Str(s) => s.clone(), _ => "airl".into() };
    let cnt = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{}-{}-{}-dir", pfx, std::process::id(), cnt));
    std::fs::create_dir_all(&path).unwrap_or(());
    rt_str(path.to_string_lossy().into_owned())
}

/// `file-mtime(path)` — return modification time as epoch millis, or -1 on error.
#[no_mangle]
pub extern "C" fn airl_file_mtime(path_val: *mut RtValue) -> *mut RtValue {
    let p = match unsafe { &(*path_val).data } { RtData::Str(s) => s.clone(), _ => return rt_int(-1) };
    match std::fs::metadata(&p).and_then(|m| m.modified()) {
        Ok(t) => {
            let ms = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            rt_int(ms)
        }
        Err(_) => rt_int(-1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_nil, rt_str};

    #[test]
    fn type_of_int() {
        unsafe {
            let v = rt_int(42);
            let r = airl_type_of(v);
            assert_eq!((*r).as_str(), "int");
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn type_of_str() {
        unsafe {
            let v = rt_str("hello".to_string());
            let r = airl_type_of(v);
            assert_eq!((*r).as_str(), "string");
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn valid_non_nil() {
        unsafe {
            let v = rt_int(1);
            let r = airl_valid(v);
            assert!((*r).as_bool());
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn valid_nil() {
        unsafe {
            let v = rt_nil();
            let r = airl_valid(v);
            assert!(!(*r).as_bool());
            airl_value_release(v);
            airl_value_release(r);
        }
    }
}
