#[cfg(target_os = "airlos")]
#[allow(unused_imports)]
use crate::nostd_prelude::*;

#[cfg(not(target_os = "airlos"))]
use std::process;

/// Fatal runtime error — prints message and exits.
/// Used by AOT-compiled binaries where there's no VM to propagate errors.
#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn airl_runtime_error(msg: *const u8, len: usize) -> ! {
    let slice = unsafe { core::slice::from_raw_parts(msg, len) };
    let s = core::str::from_utf8(slice).unwrap_or("<invalid utf8>");
    eprintln!("Runtime error: {}", s);
    process::exit(1);
}

#[cfg(target_os = "airlos")]
#[no_mangle]
pub extern "C" fn airl_runtime_error(msg: *const u8, len: usize) -> ! {
    let slice = unsafe { core::slice::from_raw_parts(msg, len) };
    let s = core::str::from_utf8(slice).unwrap_or("<invalid utf8>");
    let text = format!("Runtime error: {}\n", s);
    crate::airlos::vga_print(&text);
    crate::airlos::exit(1);
}

/// Rust-side helper: abort with a &str message.
// Intentional process::exit: AOT-compiled extern "C" builtins have no error return path.
// Type errors in builtins are prevented by the type checker at compile time.
// A future improvement would use thread-local error cells (like contract failures).
#[cfg(not(target_os = "airlos"))]
pub(crate) fn rt_error(msg: &str) -> ! {
    eprintln!("Runtime error: {}", msg);
    process::exit(1);
}

#[cfg(target_os = "airlos")]
pub(crate) fn rt_error(msg: &str) -> ! {
    let text = format!("Runtime error: {}\n", msg);
    crate::airlos::vga_print(&text);
    crate::airlos::exit(1);
}
