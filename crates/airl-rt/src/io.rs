use crate::value::{rt_bool, rt_nil, rt_str, RtData, RtValue};
use std::io::Write;

#[no_mangle]
pub extern "C" fn airl_print(v: *mut RtValue) -> *mut RtValue {
    let val = unsafe { &*v };
    match &val.data {
        RtData::Str(s) => print!("{}", s),
        _ => print!("{}", val),
    }
    rt_nil()
}

/// Variadic print: takes a pointer to an array of `*mut RtValue` and a count.
/// Prints all values space-separated with a trailing newline (matching
/// the interpreter's `builtin_print` semantics).
#[no_mangle]
pub extern "C" fn airl_print_values(args: *const *mut RtValue, count: i64) -> *mut RtValue {
    let count = count as usize;
    for i in 0..count {
        if i > 0 {
            print!(" ");
        }
        let v = unsafe { *args.add(i) };
        let val = unsafe { &*v };
        match &val.data {
            RtData::Str(s) => print!("{}", s),
            _ => print!("{}", val),
        }
    }
    println!();
    rt_nil()
}

/// Flush stdout — called at program exit to ensure all print output is visible.
#[no_mangle]
pub extern "C" fn airl_flush_stdout() {
    let _ = std::io::stdout().flush();
}

/// Read a file's contents as a string.  Takes a path (*mut RtValue Str),
/// returns the file contents as an RtValue Str, or calls rt_error on failure.
#[no_mangle]
pub extern "C" fn airl_read_file(path: *mut RtValue) -> *mut RtValue {
    let path_str = unsafe {
        match &(*path).data {
            RtData::Str(s) => s.clone(),
            _ => crate::error::rt_error("read-file: expected string path"),
        }
    };
    match std::fs::read_to_string(&path_str) {
        Ok(contents) => rt_str(contents),
        Err(e) => crate::error::rt_error(&format!("read-file: {}: {}", path_str, e)),
    }
}

/// Return command-line arguments as a List of Str values.
#[no_mangle]
pub extern "C" fn airl_get_args() -> *mut RtValue {
    let args: Vec<*mut RtValue> = std::env::args()
        .map(|a| rt_str(a))
        .collect();
    crate::value::rt_list(args)
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
