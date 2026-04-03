#[cfg(target_os = "airlos")]
#[allow(unused_imports)]
use crate::nostd_prelude::*;

use crate::error::rt_error;
use crate::value::{rt_float, rt_int, rt_str, RtData, RtValue};

#[no_mangle]
pub extern "C" fn airl_add(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_add(*y)),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x + y),
        (RtData::Str(x), RtData::Str(y)) => rt_str(format!("{}{}", x, y)),
        _ => rt_error("airl_add: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_sub(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_sub(*y)),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x - y),
        _ => rt_error("airl_sub: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_mul(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_int(x.wrapping_mul(*y)),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x * y),
        _ => rt_error("airl_mul: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_div(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(_), RtData::Int(0)) => rt_error("division by zero"),
        (RtData::Int(x), RtData::Int(y)) => rt_int(x / y),
        (RtData::Float(x), RtData::Float(y)) => rt_float(x / y),
        _ => rt_error("airl_div: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_mod(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(_), RtData::Int(0)) => rt_error("modulo by zero"),
        (RtData::Int(x), RtData::Int(y)) => rt_int(x % y),
        _ => rt_error("airl_mod: type mismatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_float, rt_int, rt_str, RtData};

    unsafe fn check_int(ptr: *mut RtValue, expected: i64) {
        assert_eq!((*ptr).as_int(), expected);
        airl_value_release(ptr);
    }

    unsafe fn check_float(ptr: *mut RtValue, expected: f64) {
        assert_eq!((*ptr).as_float(), expected);
        airl_value_release(ptr);
    }

    unsafe fn check_str(ptr: *mut RtValue, expected: &str) {
        assert_eq!((*ptr).as_str(), expected);
        airl_value_release(ptr);
    }

    #[test]
    fn add_ints() {
        unsafe {
            let a = rt_int(3);
            let b = rt_int(4);
            let r = airl_add(a, b);
            check_int(r, 7);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn add_floats() {
        unsafe {
            let a = rt_float(1.5);
            let b = rt_float(2.5);
            let r = airl_add(a, b);
            check_float(r, 4.0);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn add_strings() {
        unsafe {
            let a = rt_str("hello".to_string());
            let b = rt_str(" world".to_string());
            let r = airl_add(a, b);
            check_str(r, "hello world");
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn sub_ints() {
        unsafe {
            let a = rt_int(10);
            let b = rt_int(3);
            let r = airl_sub(a, b);
            check_int(r, 7);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn mul_ints() {
        unsafe {
            let a = rt_int(6);
            let b = rt_int(7);
            let r = airl_mul(a, b);
            check_int(r, 42);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn div_ints() {
        unsafe {
            let a = rt_int(20);
            let b = rt_int(4);
            let r = airl_div(a, b);
            check_int(r, 5);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn mod_ints() {
        unsafe {
            let a = rt_int(17);
            let b = rt_int(5);
            let r = airl_mod(a, b);
            check_int(r, 2);
            airl_value_release(a);
            airl_value_release(b);
        }
    }
}
