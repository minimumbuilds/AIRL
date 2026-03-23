use crate::error::rt_error;
use crate::value::{rt_bool, RtData, RtValue};

fn as_bool(ptr: *mut RtValue) -> bool {
    let v = unsafe { &*ptr };
    match &v.data {
        RtData::Bool(b) => *b,
        _ => rt_error("logic: expected Bool"),
    }
}

#[no_mangle]
pub extern "C" fn airl_not(a: *mut RtValue) -> *mut RtValue {
    rt_bool(!as_bool(a))
}

#[no_mangle]
pub extern "C" fn airl_and(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(as_bool(a) && as_bool(b))
}

#[no_mangle]
pub extern "C" fn airl_or(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(as_bool(a) || as_bool(b))
}

#[no_mangle]
pub extern "C" fn airl_xor(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(as_bool(a) ^ as_bool(b))
}

/// Return 1 if the value is truthy (Bool(true) or any non-nil non-bool), 0 for Bool(false)/Nil.
/// Used by JIT-compiled code to convert a boxed bool to a raw i64 branch condition.
#[no_mangle]
pub extern "C" fn airl_as_bool_raw(v: *mut crate::value::RtValue) -> i64 {
    let val = unsafe { &*v };
    match &val.data {
        crate::value::RtData::Bool(b) => *b as i64,
        crate::value::RtData::Nil => 0,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::rt_bool;

    #[test]
    fn not_true() {
        unsafe {
            let a = rt_bool(true);
            let r = airl_not(a);
            assert!(!(*r).as_bool());
            airl_value_release(a);
            airl_value_release(r);
        }
    }

    #[test]
    fn and_tt() {
        unsafe {
            let a = rt_bool(true);
            let b = rt_bool(true);
            let r = airl_and(a, b);
            assert!((*r).as_bool());
            airl_value_release(a);
            airl_value_release(b);
            airl_value_release(r);
        }
    }

    #[test]
    fn or_tf() {
        unsafe {
            let a = rt_bool(true);
            let b = rt_bool(false);
            let r = airl_or(a, b);
            assert!((*r).as_bool());
            airl_value_release(a);
            airl_value_release(b);
            airl_value_release(r);
        }
    }

    #[test]
    fn xor_tf() {
        unsafe {
            let a = rt_bool(true);
            let b = rt_bool(false);
            let r = airl_xor(a, b);
            assert!((*r).as_bool());
            airl_value_release(a);
            airl_value_release(b);
            airl_value_release(r);
        }
    }
}
