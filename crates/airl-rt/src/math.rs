use crate::value::{rt_bool, rt_float, rt_int, RtData, RtValue};

fn as_f64(name: &str, v: *mut RtValue) -> f64 {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Float(f) => *f,
        RtData::Int(n) => *n as f64,
        _ => crate::error::rt_error(&format!("{}: expected number", name)),
    }
}

#[no_mangle]
pub extern "C" fn airl_sqrt(v: *mut RtValue) -> *mut RtValue {
    rt_float(as_f64("sqrt", v).sqrt())
}

#[no_mangle]
pub extern "C" fn airl_sin(v: *mut RtValue) -> *mut RtValue {
    rt_float(as_f64("sin", v).sin())
}

#[no_mangle]
pub extern "C" fn airl_cos(v: *mut RtValue) -> *mut RtValue {
    rt_float(as_f64("cos", v).cos())
}

#[no_mangle]
pub extern "C" fn airl_tan(v: *mut RtValue) -> *mut RtValue {
    rt_float(as_f64("tan", v).tan())
}

#[no_mangle]
pub extern "C" fn airl_log(v: *mut RtValue) -> *mut RtValue {
    rt_float(as_f64("log", v).ln())
}

#[no_mangle]
pub extern "C" fn airl_exp(v: *mut RtValue) -> *mut RtValue {
    rt_float(as_f64("exp", v).exp())
}

#[no_mangle]
pub extern "C" fn airl_floor(v: *mut RtValue) -> *mut RtValue {
    rt_int(as_f64("floor", v).floor() as i64)
}

#[no_mangle]
pub extern "C" fn airl_ceil(v: *mut RtValue) -> *mut RtValue {
    rt_int(as_f64("ceil", v).ceil() as i64)
}

#[no_mangle]
pub extern "C" fn airl_round(v: *mut RtValue) -> *mut RtValue {
    rt_int(as_f64("round", v).round() as i64)
}

#[no_mangle]
pub extern "C" fn airl_float_to_int(v: *mut RtValue) -> *mut RtValue {
    rt_int(as_f64("float-to-int", v).trunc() as i64)
}

#[no_mangle]
pub extern "C" fn airl_int_to_float(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Int(n) => rt_float(*n as f64),
        RtData::Float(f) => rt_float(*f),
        _ => crate::error::rt_error("int-to-float: expected integer"),
    }
}

#[no_mangle]
pub extern "C" fn airl_infinity() -> *mut RtValue {
    rt_float(f64::INFINITY)
}

#[no_mangle]
pub extern "C" fn airl_nan() -> *mut RtValue {
    rt_float(f64::NAN)
}

#[no_mangle]
pub extern "C" fn airl_is_nan(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Float(f) => rt_bool(f.is_nan()),
        _ => rt_bool(false),
    }
}

#[no_mangle]
pub extern "C" fn airl_is_infinite(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Float(f) => rt_bool(f.is_infinite()),
        _ => rt_bool(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;

    #[test]
    fn sqrt_of_4() {
        unsafe {
            let v = rt_float(4.0);
            let r = airl_sqrt(v);
            assert_eq!((*r).as_float(), 2.0);
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn floor_rounds_down() {
        unsafe {
            let v = rt_float(3.7);
            let r = airl_floor(v);
            assert_eq!((*r).as_int(), 3);
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn ceil_rounds_up() {
        unsafe {
            let v = rt_float(3.2);
            let r = airl_ceil(v);
            assert_eq!((*r).as_int(), 4);
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn is_nan_true() {
        unsafe {
            let v = airl_nan();
            let r = airl_is_nan(v);
            assert!((*r).as_bool());
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn is_infinite_true() {
        unsafe {
            let v = airl_infinity();
            let r = airl_is_infinite(v);
            assert!((*r).as_bool());
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn int_to_float_converts() {
        unsafe {
            let v = rt_int(42);
            let r = airl_int_to_float(v);
            assert_eq!((*r).as_float(), 42.0);
            airl_value_release(v);
            airl_value_release(r);
        }
    }
}
