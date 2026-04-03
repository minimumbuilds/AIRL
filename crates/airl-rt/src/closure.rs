#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

use crate::error::rt_error;
use crate::memory::airl_value_retain;
use crate::value::{RtData, RtValue, TAG_CLOSURE};

/// Build a closure value from a raw function pointer and an array of captured values.
/// Each capture is retained (the closure takes ownership of one reference to each).
#[no_mangle]
pub extern "C" fn airl_make_closure(
    func_ptr: *const u8,
    captures: *const *mut RtValue,
    capture_count: usize,
) -> *mut RtValue {
    let mut cap_vec: Vec<*mut RtValue> = Vec::with_capacity(capture_count);
    if capture_count > 0 {
        let slice = unsafe { core::slice::from_raw_parts(captures, capture_count) };
        for &cap in slice {
            airl_value_retain(cap);
            cap_vec.push(cap);
        }
    }
    RtValue::alloc(TAG_CLOSURE, RtData::Closure { func_ptr, captures: cap_vec })
}

/// Invoke a closure with the provided arguments.
/// The full parameter list passed to the underlying function is: captures first, then args.
/// Supports arities 0 through 8; returns rt_error for higher arities.
#[no_mangle]
pub extern "C" fn airl_call_closure(
    closure: *mut RtValue,
    args: *const *mut RtValue,
    argc: usize,
) -> *mut RtValue {
    let (func_ptr, captures) = unsafe {
        match &(*closure).data {
            RtData::Closure { func_ptr, captures } => (*func_ptr, captures.clone()),
            _ => rt_error("airl_call_closure: not a Closure"),
        }
    };

    let arg_slice = if argc > 0 {
        unsafe { core::slice::from_raw_parts(args, argc) }
    } else {
        &[]
    };

    // Build full parameter list: captures first, then args
    let mut params: Vec<*mut RtValue> = Vec::with_capacity(captures.len() + argc);
    params.extend_from_slice(&captures);
    params.extend_from_slice(arg_slice);

    let total = params.len();

    // Dispatch by total arity using transmute to typed function pointers
    unsafe {
        match total {
            0 => {
                let f: extern "C" fn() -> *mut RtValue = core::mem::transmute(func_ptr);
                f()
            }
            1 => {
                let f: extern "C" fn(*mut RtValue) -> *mut RtValue =
                    core::mem::transmute(func_ptr);
                f(params[0])
            }
            2 => {
                let f: extern "C" fn(*mut RtValue, *mut RtValue) -> *mut RtValue =
                    core::mem::transmute(func_ptr);
                f(params[0], params[1])
            }
            3 => {
                let f: extern "C" fn(
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                ) -> *mut RtValue = core::mem::transmute(func_ptr);
                f(params[0], params[1], params[2])
            }
            4 => {
                let f: extern "C" fn(
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                ) -> *mut RtValue = core::mem::transmute(func_ptr);
                f(params[0], params[1], params[2], params[3])
            }
            5 => {
                let f: extern "C" fn(
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                ) -> *mut RtValue = core::mem::transmute(func_ptr);
                f(params[0], params[1], params[2], params[3], params[4])
            }
            6 => {
                let f: extern "C" fn(
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                ) -> *mut RtValue = core::mem::transmute(func_ptr);
                f(params[0], params[1], params[2], params[3], params[4], params[5])
            }
            7 => {
                let f: extern "C" fn(
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                ) -> *mut RtValue = core::mem::transmute(func_ptr);
                f(params[0], params[1], params[2], params[3], params[4], params[5], params[6])
            }
            8 => {
                let f: extern "C" fn(
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                    *mut RtValue,
                ) -> *mut RtValue = core::mem::transmute(func_ptr);
                f(
                    params[0], params[1], params[2], params[3], params[4], params[5], params[6],
                    params[7],
                )
            }
            _ => rt_error("airl_call_closure: arity > 8 not supported"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::Ordering;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, TAG_CLOSURE};

    #[test]
    fn make_closure_no_captures() {
        unsafe {
            let closure = airl_make_closure(core::ptr::null(), core::ptr::null(), 0);
            assert!(!closure.is_null());
            assert_eq!((*closure).tag, TAG_CLOSURE);
            match &(*closure).data {
                RtData::Closure { captures, .. } => {
                    assert_eq!(captures.len(), 0);
                }
                _ => panic!("expected Closure"),
            }
            airl_value_release(closure);
        }
    }

    #[test]
    fn make_closure_with_captures() {
        unsafe {
            let cap = rt_int(99);
            assert_eq!((*cap).rc.load(Ordering::Relaxed), 1);

            let caps: *const *mut RtValue = &cap as *const *mut RtValue;
            let closure = airl_make_closure(core::ptr::null(), caps, 1);

            // Capture was retained — rc should be 2
            assert_eq!((*cap).rc.load(Ordering::Relaxed), 2);

            match &(*closure).data {
                RtData::Closure { captures, .. } => {
                    assert_eq!(captures.len(), 1);
                    assert_eq!((*captures[0]).as_int(), 99);
                }
                _ => panic!("expected Closure"),
            }

            // Releasing the closure decrements the capture's rc back to 1
            airl_value_release(closure);
            assert_eq!((*cap).rc.load(Ordering::Relaxed), 1);

            // Clean up the original reference
            airl_value_release(cap);
        }
    }
}
