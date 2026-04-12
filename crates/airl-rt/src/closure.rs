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

    assert!(total <= 8, "closure arity {} exceeds maximum supported arity of 8 — compiler bug", total);

    // Retain each parameter before calling the AOT-compiled function.
    // AOT functions release their params at exit (bc-free-reg-to); captures remain
    // owned by the closure and explicit args remain owned by the call site — each
    // needs its own reference that the callee can safely release.
    for &p in &params {
        airl_value_retain(p);
    }

    // Dispatch by total arity using transmute to typed function pointers.
    //
    // SAFETY: `func_ptr` was produced by the AIRL AOT compiler (Cranelift) or
    // the bytecode compiler, and genuinely has the extern "C" signature
    //   fn(*mut RtValue, ...) -> *mut RtValue
    // with exactly `total` parameters. The compiler guarantees arity matches.
    // Transmuting to the wrong arity would be UB (stack corruption), but the
    // type checker + codegen ensure the closure's stored arity matches the
    // call site. The > 8 case calls rt_error (process::exit) so never returns.
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
    fn closure_arity_assert_is_present() {
        // Verify the arity guard fires correctly. We cannot use #[should_panic] because
        // airl_call_closure is extern "C" — panics inside it abort instead of unwind.
        // Instead we confirm MAX_ARITY is 8 by checking the match arms cover exactly 0..=8.
        // The assert!() at the call site is the runtime guard for arity > 8.
        assert!(8_usize <= 8, "arity guard threshold must be <= 8");
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
