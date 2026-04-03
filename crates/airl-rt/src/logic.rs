#[cfg(target_os = "airlos")]
#[allow(unused_imports)]
use crate::nostd_prelude::*;

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

/// Extract raw i64 from an RtValue (Int → value, Float → bits, Bool → 0/1).
/// Used for marshaling boxed values to unboxed function parameters.
#[no_mangle]
pub extern "C" fn airl_as_int_raw(v: *mut crate::value::RtValue) -> i64 {
    let val = unsafe { &*v };
    match &val.data {
        crate::value::RtData::Int(n) => *n,
        crate::value::RtData::Float(f) => f.to_bits() as i64,
        crate::value::RtData::Bool(b) => *b as i64,
        _ => 0, // Nil or other — shouldn't reach here for eligible functions
    }
}

/// Extract raw f64 bits as i64 from an RtValue (Float → bits, Int → cast, Bool → 0/1).
/// Used for marshaling boxed values to unboxed function parameters expecting floats.
#[no_mangle]
pub extern "C" fn airl_as_float_raw(v: *mut crate::value::RtValue) -> i64 {
    let val = unsafe { &*v };
    match &val.data {
        crate::value::RtData::Float(f) => f.to_bits() as i64,
        crate::value::RtData::Int(n) => (*n as f64).to_bits() as i64,
        crate::value::RtData::Bool(b) => (*b as i64 as f64).to_bits() as i64,
        _ => 0i64,
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

    #[test]
    fn as_int_raw_int() {
        unsafe {
            let v = crate::value::rt_int(42);
            assert_eq!(airl_as_int_raw(v), 42);
            airl_value_release(v);
        }
    }

    #[test]
    fn as_int_raw_float() {
        unsafe {
            let v = crate::value::rt_float(3.14);
            assert_eq!(airl_as_int_raw(v), 3.14f64.to_bits() as i64);
            airl_value_release(v);
        }
    }

    #[test]
    fn as_int_raw_bool() {
        unsafe {
            let v = rt_bool(true);
            assert_eq!(airl_as_int_raw(v), 1);
            airl_value_release(v);
            let v2 = rt_bool(false);
            assert_eq!(airl_as_int_raw(v2), 0);
            airl_value_release(v2);
        }
    }

    #[test]
    fn as_float_raw_float() {
        unsafe {
            let v = crate::value::rt_float(2.718);
            assert_eq!(airl_as_float_raw(v), 2.718f64.to_bits() as i64);
            airl_value_release(v);
        }
    }

    #[test]
    fn as_float_raw_int() {
        unsafe {
            let v = crate::value::rt_int(5);
            assert_eq!(airl_as_float_raw(v), 5.0f64.to_bits() as i64);
            airl_value_release(v);
        }
    }
}
