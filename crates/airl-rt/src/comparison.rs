use crate::error::rt_error;
use crate::value::{rt_bool, RtData, RtValue};

/// Deep equality comparison between two RtValues.
pub fn rt_values_equal(a: *mut RtValue, b: *mut RtValue) -> bool {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Nil, RtData::Nil) => true,
        (RtData::Unit, RtData::Unit) => true,
        (RtData::Int(x), RtData::Int(y)) => x == y,
        (RtData::Float(x), RtData::Float(y)) => x.to_bits() == y.to_bits(),
        (RtData::Bool(x), RtData::Bool(y)) => x == y,
        (RtData::Str(x), RtData::Str(y)) => x == y,
        (RtData::List(xs), RtData::List(ys)) => {
            if xs.len() != ys.len() {
                return false;
            }
            xs.iter().zip(ys.iter()).all(|(&x, &y)| rt_values_equal(x, y))
        }
        (RtData::Variant { tag_name: tn_a, inner: inner_a },
         RtData::Variant { tag_name: tn_b, inner: inner_b }) => {
            tn_a == tn_b && rt_values_equal(*inner_a, *inner_b)
        }
        // Cross-type = false
        _ => false,
    }
}

#[no_mangle]
pub extern "C" fn airl_eq(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(rt_values_equal(a, b))
}

#[no_mangle]
pub extern "C" fn airl_ne(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    rt_bool(!rt_values_equal(a, b))
}

#[no_mangle]
pub extern "C" fn airl_lt(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_bool(x < y),
        (RtData::Float(x), RtData::Float(y)) => rt_bool(x < y),
        (RtData::Str(x), RtData::Str(y)) => rt_bool(x < y),
        _ => rt_error("airl_lt: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_gt(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_bool(x > y),
        (RtData::Float(x), RtData::Float(y)) => rt_bool(x > y),
        (RtData::Str(x), RtData::Str(y)) => rt_bool(x > y),
        _ => rt_error("airl_gt: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_le(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_bool(x <= y),
        (RtData::Float(x), RtData::Float(y)) => rt_bool(x <= y),
        (RtData::Str(x), RtData::Str(y)) => rt_bool(x <= y),
        _ => rt_error("airl_le: type mismatch"),
    }
}

#[no_mangle]
pub extern "C" fn airl_ge(a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let va = unsafe { &*a };
    let vb = unsafe { &*b };
    match (&va.data, &vb.data) {
        (RtData::Int(x), RtData::Int(y)) => rt_bool(x >= y),
        (RtData::Float(x), RtData::Float(y)) => rt_bool(x >= y),
        (RtData::Str(x), RtData::Str(y)) => rt_bool(x >= y),
        _ => rt_error("airl_ge: type mismatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_list, rt_str, rt_variant, RtData};

    unsafe fn check_bool(ptr: *mut RtValue, expected: bool) {
        assert_eq!((*ptr).as_bool(), expected);
        airl_value_release(ptr);
    }

    #[test]
    fn eq_ints() {
        unsafe {
            let a = rt_int(5);
            let b = rt_int(5);
            let c = rt_int(6);
            let r1 = airl_eq(a, b);
            check_bool(r1, true);
            let r2 = airl_eq(a, c);
            check_bool(r2, false);
            airl_value_release(a);
            airl_value_release(b);
            airl_value_release(c);
        }
    }

    #[test]
    fn ne_ints() {
        unsafe {
            let a = rt_int(3);
            let b = rt_int(4);
            let r = airl_ne(a, b);
            check_bool(r, true);
            let r2 = airl_ne(a, a);
            check_bool(r2, false);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn lt_ints() {
        unsafe {
            let a = rt_int(2);
            let b = rt_int(5);
            let r = airl_lt(a, b);
            check_bool(r, true);
            let r2 = airl_lt(b, a);
            check_bool(r2, false);
            airl_value_release(a);
            airl_value_release(b);
        }
    }

    #[test]
    fn ge_ints() {
        unsafe {
            let a = rt_int(5);
            let b = rt_int(5);
            let c = rt_int(3);
            let r1 = airl_ge(a, b);
            check_bool(r1, true);
            let r2 = airl_ge(a, c);
            check_bool(r2, true);
            let r3 = airl_ge(c, a);
            check_bool(r3, false);
            airl_value_release(a);
            airl_value_release(b);
            airl_value_release(c);
        }
    }

    #[test]
    fn eq_strings() {
        unsafe {
            let a = rt_str("foo".to_string());
            let b = rt_str("foo".to_string());
            let c = rt_str("bar".to_string());
            let r1 = airl_eq(a, b);
            check_bool(r1, true);
            let r2 = airl_eq(a, c);
            check_bool(r2, false);
            airl_value_release(a);
            airl_value_release(b);
            airl_value_release(c);
        }
    }

    #[test]
    fn eq_lists() {
        unsafe {
            // [1, 2] == [1, 2]
            let a1 = rt_int(1);
            let a2 = rt_int(2);
            let b1 = rt_int(1);
            let b2 = rt_int(2);
            let c1 = rt_int(1);
            let c2 = rt_int(3);

            let list_a = rt_list(vec![a1, a2]);
            let list_b = rt_list(vec![b1, b2]);
            let list_c = rt_list(vec![c1, c2]);

            let r1 = airl_eq(list_a, list_b);
            check_bool(r1, true);
            let r2 = airl_eq(list_a, list_c);
            check_bool(r2, false);

            airl_value_release(list_a);
            airl_value_release(list_b);
            airl_value_release(list_c);
        }
    }

    #[test]
    fn eq_variants() {
        unsafe {
            let inner_a = rt_int(42);
            let inner_b = rt_int(42);
            let inner_c = rt_int(99);

            let v1 = rt_variant("Ok".to_string(), inner_a);
            let v2 = rt_variant("Ok".to_string(), inner_b);
            let v3 = rt_variant("Ok".to_string(), inner_c);
            let v4 = rt_variant("Err".to_string(), rt_int(42));

            let r1 = airl_eq(v1, v2);
            check_bool(r1, true);
            let r2 = airl_eq(v1, v3);
            check_bool(r2, false);
            let r3 = airl_eq(v1, v4);
            check_bool(r3, false);

            airl_value_release(v1);
            airl_value_release(v2);
            airl_value_release(v3);
            airl_value_release(v4);
        }
    }
}
