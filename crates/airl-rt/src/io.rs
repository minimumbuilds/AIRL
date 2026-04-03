use crate::value::{rt_bool, rt_int, rt_nil, rt_str, rt_unit, rt_variant, RtData, RtValue};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

// ─────────────────────────────────────────────────────────────────────────────
// SEC-5: File sandbox — restrict file I/O to a root directory when configured
// ─────────────────────────────────────────────────────────────────────────────

/// Cached sandbox root from AIRL_SANDBOX_ROOT env var.
/// None = no sandbox (all paths allowed). Some(path) = enforce sandbox.
fn sandbox_root() -> &'static Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        match std::env::var("AIRL_SANDBOX_ROOT") {
            Ok(val) if !val.is_empty() => {
                match std::fs::canonicalize(&val) {
                    Ok(canon) => Some(canon),
                    Err(_) => {
                        eprintln!("warning: AIRL_SANDBOX_ROOT '{}' cannot be canonicalized, sandbox disabled", val);
                        None
                    }
                }
            }
            _ => None,
        }
    })
}

/// Validate that `path` is under the sandbox root (if configured).
/// Returns the canonicalized PathBuf on success, or an error message on violation.
/// When AIRL_SANDBOX_ROOT is not set, returns Ok(PathBuf::from(path)) for backward compat.
pub(crate) fn sandbox_check(path: &str) -> Result<PathBuf, String> {
    match sandbox_root() {
        None => Ok(PathBuf::from(path)),
        Some(root) => {
            // For paths that don't exist yet (write-file, temp-file, etc.),
            // canonicalize the parent directory and append the filename.
            let p = PathBuf::from(path);
            let canonical = if p.exists() {
                std::fs::canonicalize(&p).map_err(|e| format!("sandbox: cannot canonicalize '{}': {}", path, e))?
            } else {
                // Canonicalize the longest existing prefix
                let mut base = p.clone();
                let mut tail_parts = Vec::new();
                loop {
                    if base.exists() {
                        let mut canon = std::fs::canonicalize(&base)
                            .map_err(|e| format!("sandbox: cannot canonicalize '{}': {}", base.display(), e))?;
                        for part in tail_parts.into_iter().rev() {
                            canon.push(part);
                        }
                        break canon;
                    }
                    match base.file_name() {
                        Some(name) => {
                            tail_parts.push(name.to_os_string());
                            base.pop();
                        }
                        None => {
                            // No existing prefix at all — use the path as-is for the check
                            break p;
                        }
                    }
                }
            };
            if canonical.starts_with(root) {
                Ok(canonical)
            } else {
                Err(format!("sandbox violation: '{}' is outside AIRL_SANDBOX_ROOT '{}'", path, root.display()))
            }
        }
    }
}

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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("read-file: {}", msg)),
    };
    match std::fs::read_to_string(&checked) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("write-file: {}", msg)),
    };
    if let Some(parent) = checked.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                crate::error::rt_error(&format!("write-file: create dirs: {}: {}", path_str, e));
            }
        }
    }
    match std::fs::write(&checked, &content_str) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("append-file: {}", msg)),
    };
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&checked)
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("delete-file: {}", msg)),
    };
    match std::fs::remove_file(&checked) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("delete-dir: {}", msg)),
    };
    match std::fs::remove_dir_all(&checked) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("read-dir: {}", msg)),
    };
    match std::fs::read_dir(&checked) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("create-dir: {}", msg)),
    };
    match std::fs::create_dir_all(&checked) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("file-size: {}", msg)),
    };
    match std::fs::metadata(&checked) {
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("is-dir: {}", msg)),
    };
    rt_bool(checked.is_dir())
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
    let checked = match sandbox_check(&path_str) {
        Ok(p) => p,
        Err(_) => return rt_bool(false), // Outside sandbox = does not exist from AIRL's perspective
    };
    rt_bool(checked.exists())
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
    let checked_old = match sandbox_check(&old_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("rename-file: {}", msg)),
    };
    let checked_new = match sandbox_check(&new_str) {
        Ok(p) => p,
        Err(msg) => crate::error::rt_error(&format!("rename-file: {}", msg)),
    };
    match std::fs::rename(&checked_old, &checked_new) {
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
/// When AIRL_SANDBOX_ROOT is set, temp files are created under the sandbox root.
#[no_mangle]
pub extern "C" fn airl_temp_file(prefix: *mut RtValue) -> *mut RtValue {
    let pfx = match unsafe { &(*prefix).data } { RtData::Str(s) => s.clone(), _ => "airl".into() };
    let cnt = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = match sandbox_root() {
        Some(root) => root.join("tmp"),
        None => std::env::temp_dir(),
    };
    let path = base.join(format!("{}-{}-{}", pfx, std::process::id(), cnt));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or(());
    }
    std::fs::write(&path, "").unwrap_or(());
    rt_str(path.to_string_lossy().into_owned())
}

/// `temp-dir(prefix)` — create a temp directory, return its path.
/// When AIRL_SANDBOX_ROOT is set, temp dirs are created under the sandbox root.
#[no_mangle]
pub extern "C" fn airl_temp_dir(prefix: *mut RtValue) -> *mut RtValue {
    let pfx = match unsafe { &(*prefix).data } { RtData::Str(s) => s.clone(), _ => "airl".into() };
    let cnt = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = match sandbox_root() {
        Some(root) => root.join("tmp"),
        None => std::env::temp_dir(),
    };
    let path = base.join(format!("{}-{}-{}-dir", pfx, std::process::id(), cnt));
    std::fs::create_dir_all(&path).unwrap_or(());
    rt_str(path.to_string_lossy().into_owned())
}

/// `file-mtime(path)` — return modification time as epoch millis, or -1 on error.
#[no_mangle]
pub extern "C" fn airl_file_mtime(path_val: *mut RtValue) -> *mut RtValue {
    let p = match unsafe { &(*path_val).data } { RtData::Str(s) => s.clone(), _ => return rt_int(-1) };
    let checked = match sandbox_check(&p) {
        Ok(p) => p,
        Err(_) => return rt_int(-1),
    };
    match std::fs::metadata(&checked).and_then(|m| m.modified()) {
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

    // ── SEC-5: sandbox_check unit tests ──

    #[test]
    fn sandbox_check_no_sandbox_allows_any_path() {
        // When AIRL_SANDBOX_ROOT is not set (the default for tests),
        // sandbox_check returns Ok with the original path.
        let result = sandbox_check("/etc/passwd");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), std::path::PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn sandbox_check_nonexistent_path_without_sandbox() {
        let result = sandbox_check("/nonexistent/path/file.txt");
        assert!(result.is_ok());
    }

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
