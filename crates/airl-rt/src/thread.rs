use crate::value::{RtData, RtValue, TAG_INT, TAG_BOOL};
use crate::value::{rt_str as rt_str_alloc, rt_variant, rt_bool as rt_bool_alloc};
use crate::list::airl_list_new as airl_list_new_raw;
use crate::closure::airl_call_closure;
use crate::memory::airl_value_retain;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Wrapper to send *mut RtValue across threads.
/// Safety: RtValue is ref-counted and we retain before sending, release after receiving.
struct SendPtr(*mut RtValue);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

// ── Thread handle storage ──────────────────────────────────────────

static NEXT_THREAD_HANDLE: AtomicI64 = AtomicI64::new(1);

type ThreadResult = Result<SendPtr, String>;
type ThreadHandle = std::thread::JoinHandle<ThreadResult>;

fn thread_handles() -> &'static Mutex<HashMap<i64, ThreadHandle>> {
    static HANDLES: OnceLock<Mutex<HashMap<i64, ThreadHandle>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── Channel storage ────────────────────────────────────────────────

static NEXT_CHANNEL_HANDLE: AtomicI64 = AtomicI64::new(1);

// Channels pass *mut RtValue between threads. We retain values on send,
// so the receiver owns a reference.
fn channel_senders() -> &'static Mutex<HashMap<i64, std::sync::mpsc::Sender<SendPtr>>> {
    static SENDERS: OnceLock<Mutex<HashMap<i64, std::sync::mpsc::Sender<SendPtr>>>> = OnceLock::new();
    SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn channel_receivers() -> &'static Mutex<HashMap<i64, std::sync::mpsc::Receiver<SendPtr>>> {
    static RECEIVERS: OnceLock<Mutex<HashMap<i64, std::sync::mpsc::Receiver<SendPtr>>>> = OnceLock::new();
    RECEIVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── Helpers ────────────────────────────────────────────────────────

fn rt_int(n: i64) -> *mut RtValue {
    RtValue::alloc(TAG_INT, RtData::Int(n))
}

fn rt_str(s: String) -> *mut RtValue {
    rt_str_alloc(s)
}

fn rt_bool(b: bool) -> *mut RtValue {
    rt_bool_alloc(b)
}

fn rt_ok(inner: *mut RtValue) -> *mut RtValue {
    rt_variant("Ok".into(), inner)
}

fn rt_err(msg: &str) -> *mut RtValue {
    rt_variant("Err".into(), rt_str(msg.into()))
}

fn extract_int(ptr: *mut RtValue) -> Option<i64> {
    if ptr.is_null() { return None; }
    unsafe {
        match &(*ptr).data {
            RtData::Int(n) => Some(*n),
            _ => None,
        }
    }
}

// ── thread-spawn ───────────────────────────────────────────────────

/// Spawn a new OS thread running the given 0-arg closure.
/// Returns an integer thread handle.
#[no_mangle]
pub extern "C" fn airl_thread_spawn(closure: *mut RtValue) -> *mut RtValue {
    if closure.is_null() {
        return rt_err("thread-spawn: requires 1 argument");
    }

    // Retain the closure so the child thread owns a reference
    airl_value_retain(closure);
    let closure_send = SendPtr(closure);

    // Safety: RtValue is ref-counted. We retained the closure above and release in the thread.
    // The spawned thread gets exclusive access to the closure.
    let handle = unsafe {
        let raw_closure = closure as usize; // usize is Send
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || -> ThreadResult {
                let cl = raw_closure as *mut RtValue;
                let result = airl_call_closure(cl, std::ptr::null(), 0);
                crate::memory::airl_value_release(cl);
                Ok(SendPtr(result))
            })
    };

    match handle {
        Ok(jh) => {
            let id = NEXT_THREAD_HANDLE.fetch_add(1, Ordering::SeqCst);
            thread_handles().lock().unwrap().insert(id, jh);
            rt_int(id)
        }
        Err(e) => rt_err(&format!("thread-spawn: {}", e)),
    }
}

/// Join a thread by handle. Returns Result[Value, Str].
#[no_mangle]
pub extern "C" fn airl_thread_join(handle: *mut RtValue) -> *mut RtValue {
    let handle_id = match extract_int(handle) {
        Some(n) => n,
        None => return rt_err("thread-join: handle must be Int"),
    };

    let join_handle = match thread_handles().lock().unwrap().remove(&handle_id) {
        Some(h) => h,
        None => return rt_err(&format!("thread-join: invalid or already-joined handle {}", handle_id)),
    };

    match join_handle.join() {
        Ok(Ok(SendPtr(val))) => rt_ok(val),
        Ok(Err(msg)) => rt_err(&msg),
        Err(_) => rt_err("thread panicked"),
    }
}

// ── channel-new ────────────────────────────────────────────────────

/// Create an unbounded channel. Returns [sender-handle, receiver-handle].
#[no_mangle]
pub extern "C" fn airl_channel_new() -> *mut RtValue {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    let rx_id = NEXT_CHANNEL_HANDLE.fetch_add(1, Ordering::SeqCst);
    channel_senders().lock().unwrap().insert(tx_id, tx);
    channel_receivers().lock().unwrap().insert(rx_id, rx);
    let items = vec![rt_int(tx_id), rt_int(rx_id)];
    airl_list_new_raw(items.as_ptr(), items.len())
}

/// Send a value on a channel. Returns Result[Bool, Str].
#[no_mangle]
pub extern "C" fn airl_channel_send(tx_handle: *mut RtValue, value: *mut RtValue) -> *mut RtValue {
    let tx_id = match extract_int(tx_handle) {
        Some(n) => n,
        None => return rt_err("channel-send: handle must be Int"),
    };

    // Retain the value so the receiver owns a reference
    airl_value_retain(value);

    let senders = channel_senders().lock().unwrap();
    match senders.get(&tx_id) {
        Some(tx) => match tx.send(SendPtr(value)) {
            Ok(()) => rt_ok(rt_bool(true)),
            Err(_) => {
                crate::memory::airl_value_release(value);
                rt_err("channel closed")
            }
        },
        None => {
            crate::memory::airl_value_release(value);
            rt_err(&format!("channel-send: invalid sender handle {}", tx_id))
        }
    }
}

/// Blocking receive on a channel. Returns Result[Value, Str].
#[no_mangle]
pub extern "C" fn airl_channel_recv(rx_handle: *mut RtValue) -> *mut RtValue {
    let rx_id = match extract_int(rx_handle) {
        Some(n) => n,
        None => return rt_err("channel-recv: handle must be Int"),
    };

    let rx = channel_receivers().lock().unwrap().remove(&rx_id);
    match rx {
        Some(rx) => {
            let result = match rx.recv() {
                Ok(SendPtr(val)) => rt_ok(val),
                Err(_) => rt_err("channel closed"),
            };
            channel_receivers().lock().unwrap().insert(rx_id, rx);
            result
        }
        None => rt_err(&format!("channel-recv: invalid receiver handle {}", rx_id)),
    }
}

/// Receive with timeout (milliseconds). Returns Result[Value, Str].
#[no_mangle]
pub extern "C" fn airl_channel_recv_timeout(rx_handle: *mut RtValue, timeout_ms: *mut RtValue) -> *mut RtValue {
    let rx_id = match extract_int(rx_handle) {
        Some(n) => n,
        None => return rt_err("channel-recv-timeout: handle must be Int"),
    };
    let ms = match extract_int(timeout_ms) {
        Some(n) => n,
        None => return rt_err("channel-recv-timeout: timeout must be Int"),
    };

    let rx = channel_receivers().lock().unwrap().remove(&rx_id);
    match rx {
        Some(rx) => {
            let duration = std::time::Duration::from_millis(ms as u64);
            let result = match rx.recv_timeout(duration) {
                Ok(SendPtr(val)) => rt_ok(val),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => rt_err("timeout"),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => rt_err("channel closed"),
            };
            channel_receivers().lock().unwrap().insert(rx_id, rx);
            result
        }
        None => rt_err(&format!("channel-recv-timeout: invalid receiver handle {}", rx_id)),
    }
}

/// Close a channel handle (sender or receiver). Returns Bool.
#[no_mangle]
pub extern "C" fn airl_channel_close(handle: *mut RtValue) -> *mut RtValue {
    let handle_id = match extract_int(handle) {
        Some(n) => n,
        None => return rt_bool(false),
    };
    let removed_tx = channel_senders().lock().unwrap().remove(&handle_id).is_some();
    let removed_rx = channel_receivers().lock().unwrap().remove(&handle_id).is_some();
    rt_bool(removed_tx || removed_rx)
}
