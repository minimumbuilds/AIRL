use std::process;

/// Fatal runtime error — prints message and exits.
#[no_mangle]
pub extern "C" fn airl_runtime_error(msg: *const u8, len: usize) -> ! {
    let slice = unsafe { std::slice::from_raw_parts(msg, len) };
    let s = std::str::from_utf8(slice).unwrap_or("<invalid utf8>");
    eprintln!("Runtime error: {}", s);
    process::exit(1);
}

/// Rust-side helper: abort with a &str message.
pub(crate) fn rt_error(msg: &str) -> ! {
    eprintln!("Runtime error: {}", msg);
    process::exit(1);
}
