#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

use crate::value::{rt_bool, rt_int, rt_nil, rt_str, rt_unit, rt_variant, RtData, RtValue};
#[cfg(not(target_os = "airlos"))]
use std::io::Write;
#[cfg(not(target_os = "airlos"))]
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(not(target_os = "airlos"))]
use std::path::PathBuf;
#[cfg(not(target_os = "airlos"))]
use std::sync::OnceLock;

#[cfg(not(target_os = "airlos"))]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

// ─────────────────────────────────────────────────────────────────────────────
// SEC-5: File sandbox
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
fn sandbox_root() -> &'static Option<PathBuf> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        match std::env::var("AIRL_SANDBOX_ROOT") {
            Ok(val) if !val.is_empty() => {
                match std::fs::canonicalize(&val) {
                    Ok(canon) => Some(canon),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    })
}

#[cfg(not(target_os = "airlos"))]
pub(crate) fn sandbox_check(path: &str) -> Result<PathBuf, String> {
    match sandbox_root() {
        None => Ok(PathBuf::from(path)),
        Some(root) => {
            let p = PathBuf::from(path);
            let canonical = if p.exists() {
                std::fs::canonicalize(&p).map_err(|e| format!("sandbox: {}", e))?
            } else {
                p.clone()
            };
            if canonical.starts_with(root) {
                Ok(canonical)
            } else {
                Err(format!("sandbox: '{}' is outside sandbox root '{}'", path, root.display()))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// print
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_print(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => print!("{}", s),
        _ => print!("{}", val),
    }
    rt_unit()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_print(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let s = match &val.data {
        RtData::Str(s) => s.clone(),
        _ => format!("{}", val),
    };
    crate::airlos::vga_print(&s);
    rt_unit()
}

// ─────────────────────────────────────────────────────────────────────────────
// println
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_println(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    println!("{}", val);
    rt_unit()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_println(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let s = format!("{}\n", val);
    crate::airlos::vga_print(&s);
    rt_unit()
}

// ─────────────────────────────────────────────────────────────────────────────
// print_values (variadic)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_print_values(args: *const *mut RtValue, count: i64) -> *mut RtValue {
    use core::fmt::Write;
    let count = count as usize;
    let mut buf = String::new();
    for i in 0..count {
        if i > 0 {
            buf.push(' ');
        }
        let v = unsafe { *args.add(i) };
        let val = unsafe { &*v };
        match &val.data {
            RtData::Str(s) => buf.push_str(s),
            _ => { let _ = write!(buf, "{}", val); }
        }
    }
    buf.push('\n');
    crate::airlos::vga_print(&buf);
    rt_unit()
}

// ─────────────────────────────────────────────────────────────────────────────
// SIGINT handling for ash REPL
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
static SIGINT_PENDING: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_os = "airlos"))]
extern "C" fn ash_sigint_handler(_sig: libc::c_int) {
    SIGINT_PENDING.store(true, Ordering::SeqCst);
}

/// `ash-install-sigint` — install a SIGINT handler that records the signal
/// in an atomic flag instead of terminating the process. Call once at shell
/// startup. Returns Nil.
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_ash_install_sigint() -> *mut RtValue {
    unsafe {
        libc::signal(libc::SIGINT, ash_sigint_handler as *const () as libc::sighandler_t);
    }
    rt_nil()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_ash_install_sigint() -> *mut RtValue {
    rt_nil()
}

/// `ash-sigint-pending` — returns Bool true if SIGINT has fired since last
/// check. Atomically clears the flag.
#[no_mangle]
pub extern "C" fn airl_ash_sigint_pending() -> *mut RtValue {
    #[cfg(not(target_os = "airlos"))]
    {
        let was_set = SIGINT_PENDING.swap(false, Ordering::SeqCst);
        return rt_bool(was_set);
    }
    #[cfg(target_os = "airlos")]
    rt_bool(false)
}

// ─────────────────────────────────────────────────────────────────────────────
// read-line
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_read_line() -> *mut RtValue {
    use std::io::BufRead;
    let _ = std::io::stdout().flush(); // flush any pending prompt
    // If SIGINT fired before we enter the read, return interrupted immediately.
    if SIGINT_PENDING.swap(false, Ordering::SeqCst) {
        return rt_variant("Err".into(), rt_str("interrupted".into()));
    }
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) => rt_variant("Err".into(), rt_str("EOF".into())),
        Ok(_) => {
            if line.ends_with('\n') { line.pop(); }
            if line.ends_with('\r') { line.pop(); }
            rt_variant("Ok".into(), rt_str(line))
        }
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
            SIGINT_PENDING.store(false, Ordering::SeqCst);
            rt_variant("Err".into(), rt_str("interrupted".into()))
        }
        Err(e) => rt_variant("Err".into(), rt_str(format!("read-line: {}", e))),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_read_line() -> *mut RtValue {
    match crate::airlos::keyboard_read_line() {
        Some(line) => rt_variant("Ok".into(), rt_str(line)),
        None => rt_variant("Err".into(), rt_str("read-line: keyboard service unavailable".into())),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// read-stdin
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_read_stdin() -> *mut RtValue {
    let mut buf = String::new();
    match std::io::Read::read_to_string(&mut std::io::stdin().lock(), &mut buf) {
        Ok(_) => rt_variant("Ok".into(), rt_str(buf)),
        Err(e) => rt_variant("Err".into(), rt_str(format!("read-stdin: {}", e))),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_read_stdin() -> *mut RtValue {
    // No stdin concept on AIRLOS — read one line from keyboard
    match crate::airlos::keyboard_read_line() {
        Some(line) => rt_variant("Ok".into(), rt_str(line)),
        None => rt_variant("Err".into(), rt_str("read-stdin: not available on AIRLOS".into())),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// eprint / eprintln
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_eprint(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => eprint!("{}", s),
        _ => eprint!("{}", val),
    }
    rt_nil()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_eprint(v: *mut RtValue) -> *mut RtValue {
    // AIRLOS: send to VGA (no separate stderr)
    let val = unsafe { &*v };
    let s = match &val.data {
        RtData::Str(s) => s.clone(),
        _ => format!("{}", val),
    };
    crate::airlos::vga_print(&s);
    rt_nil()
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_eprintln(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => eprintln!("{}", s),
        _ => eprintln!("{}", val),
    }
    rt_nil()
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_eprintln(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let s = match &val.data {
        RtData::Str(s) => format!("{}\n", s),
        _ => format!("{}\n", val),
    };
    crate::airlos::vga_print(&s);
    rt_nil()
}

// ─────────────────────────────────────────────────────────────────────────────
// flush-stdout
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_flush_stdout() {
    let _ = std::io::stdout().flush();
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_flush_stdout() {
    // No-op: IPC is synchronous on AIRLOS
}

// ─────────────────────────────────────────────────────────────────────────────
// read-file
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_read_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("read-file: expected string path"),
        }
    };
    match crate::airlos::read_file(&path_str) {
        Ok(bytes) => {
            let contents = String::from_utf8_lossy(&bytes).into_owned();
            rt_str(contents)
        }
        Err(msg) => crate::error::rt_error(&format!("read-file: {}: {}", path_str, msg)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// write-file (not supported on AIRLOS MVP)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
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
    match crate::airlos::write_file(&path_str, content_str.as_bytes()) {
        Ok(()) => rt_bool(true),
        Err(msg) => crate::error::rt_error(&format!("write-file: {}: {}", path_str, msg)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// append-file (not supported on AIRLOS MVP)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_append_file(_path: *mut RtValue, _content: *mut RtValue) -> *mut RtValue {
    crate::error::rt_error("append-file: not supported on AIRLOS")
}

// ─────────────────────────────────────────────────────────────────────────────
// exec-file
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_exec_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("exec-file: expected string path"),
        }
    };
    match std::process::Command::new(&path_str).status() {
        Ok(status) => rt_int(status.code().unwrap_or(-1) as i64),
        Err(e) => crate::error::rt_error(&format!("exec-file: {}: {}", path_str, e)),
    }
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_exec_file(_path: *mut RtValue) -> *mut RtValue {
    crate::error::rt_error("exec-file: not yet supported on AIRLOS")
}

// ─────────────────────────────────────────────────────────────────────────────
// delete-file (not supported on AIRLOS MVP)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_delete_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("delete-file: expected string path"),
        }
    };
    match crate::airlos::delete_file(&path_str) {
        Ok(()) => rt_bool(true),
        Err(msg) => crate::error::rt_error(&format!("delete-file: {}: {}", path_str, msg)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// delete-dir (not supported on AIRLOS MVP)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_delete_dir(_path: *mut RtValue) -> *mut RtValue {
    crate::error::rt_error("delete-dir: not supported on AIRLOS")
}

// ─────────────────────────────────────────────────────────────────────────────
// read-dir
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_read_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("read-dir: expected string path"),
        }
    };
    let mut names = crate::airlos::read_dir(&path_str);
    names.sort();
    let items: Vec<*mut RtValue> = names.into_iter().map(|n| rt_str(n)).collect();
    crate::value::rt_list(items)
}

// ─────────────────────────────────────────────────────────────────────────────
// create-dir (not supported on AIRLOS MVP)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_create_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("create-dir: expected string path"),
        }
    };
    match crate::airlos::create_dir(&path_str) {
        Ok(()) => rt_bool(true),
        Err(msg) => crate::error::rt_error(&format!("create-dir: {}: {}", path_str, msg)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// file-size
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_file_size(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("file-size: expected string path"),
        }
    };
    match crate::airlos::file_size(&path_str) {
        Some(size) => rt_int(size as i64),
        None => crate::error::rt_error(&format!("file-size: {}: file not found", path_str)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// is-dir
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_is_dir(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("is-dir: expected string path"),
        }
    };
    rt_bool(crate::airlos::is_dir(&path_str))
}

// ─────────────────────────────────────────────────────────────────────────────
// file-exists?
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_file_exists(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("file-exists?: expected string path"),
        }
    };
    rt_bool(crate::airlos::file_exists(&path_str))
}

// ─────────────────────────────────────────────────────────────────────────────
// rename-file (not supported on AIRLOS MVP)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_rename_file(_old: *mut RtValue, _new: *mut RtValue) -> *mut RtValue {
    crate::error::rt_error("rename-file: not supported on AIRLOS")
}

// ─────────────────────────────────────────────────────────────────────────────
// get-args
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_get_args() -> *mut RtValue {
    let args: Vec<*mut RtValue> = std::env::args()
        .map(|a| rt_str(a))
        .collect();
    crate::value::rt_list(args)
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_get_args() -> *mut RtValue {
    // TBD: args may be passed via IPC at spawn time
    crate::value::rt_list(vec![])
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure functions — no OS dependency, shared across all targets
// ─────────────────────────────────────────────────────────────────────────────

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
        RtData::PartialApp { .. } => "partial-app",
        RtData::BCFuncNative(_) => "bcfunc",
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

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_temp_file(prefix: *mut RtValue) -> *mut RtValue {
    let pfx = match unsafe { &(*prefix).data } { RtData::Str(s) => s.clone(), _ => "airl".into() };
    let cnt = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{}-{}-{}", pfx, std::process::id(), cnt));
    std::fs::write(&path, "").unwrap_or(());
    rt_str(path.to_string_lossy().into_owned())
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_temp_file(_prefix: *mut RtValue) -> *mut RtValue {
    crate::error::rt_error("temp-file: not supported on AIRLOS")
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_temp_dir(prefix: *mut RtValue) -> *mut RtValue {
    let pfx = match unsafe { &(*prefix).data } { RtData::Str(s) => s.clone(), _ => "airl".into() };
    let cnt = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("{}-{}-{}-dir", pfx, std::process::id(), cnt));
    std::fs::create_dir_all(&path).unwrap_or(());
    rt_str(path.to_string_lossy().into_owned())
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_temp_dir(_prefix: *mut RtValue) -> *mut RtValue {
    crate::error::rt_error("temp-dir: not supported on AIRLOS")
}

#[cfg(not(target_os = "airlos"))]
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

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_file_mtime(_path_val: *mut RtValue) -> *mut RtValue {
    // No file modification times on AIRLOS ramdisk
    rt_int(-1)
}

// ─────────────────────────────────────────────────────────────────────────────
// exec-file — execute an ELF binary from the VFS and wait for it to exit
// (AIRLOS only — the non-AIRLOS implementation is defined earlier in this file)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_exec_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("exec-file: expected string path"),
        }
    };
    let exit_code = crate::airlos::exec_and_wait(&path_str);
    rt_int(exit_code as i64)
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
