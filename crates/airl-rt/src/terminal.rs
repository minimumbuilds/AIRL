//! Canopy terminal I/O primitives.
//!
//! Seven extern "C" functions for raw terminal access:
//! raw mode, stdin byte reads, stdout writes, terminal size, SIGWINCH.
//! AOT linkage wrappers (`__airl_fn_canopy_*`) are appended at the bottom.

#[cfg(not(target_os = "airlos"))]
use crate::value::{rt_int, rt_list, RtValue};

#[cfg(not(target_os = "airlos"))]
use std::sync::Mutex;

#[cfg(not(target_os = "airlos"))]
use std::sync::OnceLock;

// ── Saved termios for raw mode ────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
static SAVED_TERMIOS: OnceLock<Mutex<Option<libc::termios>>> = OnceLock::new();

#[cfg(not(target_os = "airlos"))]
fn saved_termios() -> &'static Mutex<Option<libc::termios>> {
    SAVED_TERMIOS.get_or_init(|| Mutex::new(None))
}

// ── canopy_raw_mode_enable ────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_raw_mode_enable() -> *mut RtValue {
    unsafe {
        let mut orig: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut orig) != 0 {
            return rt_int(-1);
        }
        // Save original
        *saved_termios().lock().unwrap() = Some(orig);

        let mut raw = orig;
        // Disable canonical mode, echo, and signal generation
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
        // Disable input processing
        raw.c_iflag &= !(libc::IXON | libc::ICRNL | libc::BRKINT | libc::INPCK | libc::ISTRIP);
        // Disable output processing
        raw.c_oflag &= !libc::OPOST;
        // Read returns after 1 byte, no timeout
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;

        if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw) != 0 {
            return rt_int(-1);
        }
        rt_int(0)
    }
}

// ── canopy_raw_mode_disable ───────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_raw_mode_disable() -> *mut RtValue {
    let guard = saved_termios().lock().unwrap();
    match *guard {
        Some(ref orig) => unsafe {
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, orig) != 0 {
                rt_int(-1)
            } else {
                rt_int(0)
            }
        },
        None => rt_int(-1),
    }
}

// ── canopy_stdin_read_byte ────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_stdin_read_byte() -> *mut RtValue {
    let mut buf = [0u8; 1];
    unsafe {
        let n = libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, 1);
        if n <= 0 {
            rt_int(-1)
        } else {
            rt_int(buf[0] as i64)
        }
    }
}

// ── canopy_stdin_read_available ───────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_stdin_read_available() -> *mut RtValue {
    unsafe {
        let mut available: libc::c_int = 0;
        if libc::ioctl(libc::STDIN_FILENO, libc::FIONREAD, &mut available) != 0 || available <= 0 {
            return rt_list(vec![]);
        }
        let n = available as usize;
        let mut buf = vec![0u8; n];
        let read = libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, n);
        if read <= 0 {
            return rt_list(vec![]);
        }
        let items: Vec<*mut RtValue> = buf[..read as usize]
            .iter()
            .map(|&b| rt_int(b as i64))
            .collect();
        rt_list(items)
    }
}

// ── canopy_stdout_write ───────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_stdout_write(s: *mut RtValue) -> *mut RtValue {
    use std::io::Write;
    let val = unsafe { &*s };
    match &val.data {
        crate::value::RtData::Str(text) => {
            let out = std::io::stdout();
            let mut handle = out.lock();
            match handle.write_all(text.as_bytes()) {
                Ok(()) => {
                    let _ = handle.flush();
                    rt_int(text.len() as i64)
                }
                Err(_) => rt_int(-1),
            }
        }
        _ => rt_int(-1),
    }
}

// ── canopy_terminal_size ──────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_terminal_size() -> *mut RtValue {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) != 0 {
            // Fallback
            return rt_list(vec![rt_int(80), rt_int(24)]);
        }
        rt_list(vec![rt_int(ws.ws_col as i64), rt_int(ws.ws_row as i64)])
    }
}

// ── canopy_on_resize ──────────────────────────────────────────────────
// Stores the channel tx handle for SIGWINCH. On resize, the signal
// handler sends [width, height] to the channel.

#[cfg(not(target_os = "airlos"))]
static RESIZE_TX: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn canopy_on_resize(tx: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*tx };
    if let crate::value::RtData::Int(handle) = &val.data {
        RESIZE_TX.store(*handle, std::sync::atomic::Ordering::SeqCst);
        unsafe {
            libc::signal(libc::SIGWINCH, sigwinch_handler as *const () as libc::sighandler_t);
        }
        rt_int(0)
    } else {
        rt_int(-1)
    }
}

#[cfg(not(target_os = "airlos"))]
extern "C" fn sigwinch_handler(_sig: libc::c_int) {
    // Note: This is a signal handler — must be async-signal-safe.
    // We store the tx handle and let the coordinator poll for resize
    // on next drain. The actual channel-send happens outside the handler.
    //
    // For Phase A, resize is detected by polling canopy_terminal_size()
    // in the coordinator loop rather than signal-based push.
    // This handler is a placeholder for Phase B.
}

// ── AOT linkage wrappers ──────────────────────────────────────────────
//
// The g3 AOT compiler emits calls to `__airl_fn_<name>` for unregistered
// extern symbols.  These thin wrappers expose the mangled names so that
// canopy AIRL sources link without `(extern-c ...)` declarations.

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_stdout_write(s: *mut RtValue) -> *mut RtValue {
    canopy_stdout_write(s)
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_stdin_read_byte() -> *mut RtValue {
    canopy_stdin_read_byte()
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_stdin_read_available() -> *mut RtValue {
    canopy_stdin_read_available()
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_terminal_size() -> *mut RtValue {
    canopy_terminal_size()
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_raw_mode_enable() -> *mut RtValue {
    canopy_raw_mode_enable()
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_raw_mode_disable() -> *mut RtValue {
    canopy_raw_mode_disable()
}

#[cfg(not(target_os = "airlos"))]
#[no_mangle]
pub extern "C" fn __airl_fn_canopy_on_resize(tx: *mut RtValue) -> *mut RtValue {
    canopy_on_resize(tx)
}
