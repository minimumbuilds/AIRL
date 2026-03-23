use crate::value::{rt_bool, rt_nil, rt_str, RtData, RtValue};

#[no_mangle]
pub extern "C" fn airl_print(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => print!("{}", s),
        _ => print!("{}", val),
    }
    rt_nil()
}

#[no_mangle]
pub extern "C" fn airl_type_of(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let name = match &val.data {
        RtData::Nil => "nil",
        RtData::Unit => "unit",
        RtData::Int(_) => "int",
        RtData::Float(_) => "float",
        RtData::Bool(_) => "bool",
        RtData::Str(_) => "string",
        RtData::List(_) => "list",
        RtData::Map(_) => "map",
        RtData::Variant { .. } => "variant",
        RtData::Closure { .. } => "closure",
    };
    rt_str(name.to_string())
}

#[no_mangle]
pub extern "C" fn airl_valid(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    let is_nil = matches!(&val.data, RtData::Nil);
    rt_bool(!is_nil)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_nil, rt_str};

    #[test]
    fn type_of_int() {
        unsafe {
            let v = rt_int(42);
            let r = airl_type_of(v);
            assert_eq!((*r).as_str(), "int");
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn type_of_str() {
        unsafe {
            let v = rt_str("hello".to_string());
            let r = airl_type_of(v);
            assert_eq!((*r).as_str(), "string");
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn valid_non_nil() {
        unsafe {
            let v = rt_int(1);
            let r = airl_valid(v);
            assert!((*r).as_bool());
            airl_value_release(v);
            airl_value_release(r);
        }
    }

    #[test]
    fn valid_nil() {
        unsafe {
            let v = rt_nil();
            let r = airl_valid(v);
            assert!(!(*r).as_bool());
            airl_value_release(v);
            airl_value_release(r);
        }
    }
}
