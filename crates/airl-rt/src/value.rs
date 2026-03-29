use std::collections::HashMap;
use std::fmt;

use crate::error::rt_error;

// Tag constants
pub const TAG_NIL: u8 = 0;
pub const TAG_INT: u8 = 1;
pub const TAG_FLOAT: u8 = 2;
pub const TAG_BOOL: u8 = 3;
pub const TAG_STR: u8 = 4;
pub const TAG_LIST: u8 = 5;
pub const TAG_MAP: u8 = 6;
pub const TAG_VARIANT: u8 = 7;
pub const TAG_CLOSURE: u8 = 8;
pub const TAG_UNIT: u8 = 9;
pub const TAG_BYTES: u8 = 10;

/// Variant order MUST match TAG_* constants (0-10).
/// The Rust compiler assigns discriminants by position,
/// and airl-rt functions match on RtData using these discriminants.
/// AOT-compiled code checks the `tag` byte directly.
/// If these diverge, AOT binaries will misidentify value types.
pub enum RtData {
    Nil,                                                    // 0 = TAG_NIL
    Int(i64),                                               // 1 = TAG_INT
    Float(f64),                                             // 2 = TAG_FLOAT
    Bool(bool),                                             // 3 = TAG_BOOL
    Str(String),                                            // 4 = TAG_STR
    List(Vec<*mut RtValue>),                                // 5 = TAG_LIST
    Map(HashMap<String, *mut RtValue>),                     // 6 = TAG_MAP
    Variant { tag_name: String, inner: *mut RtValue },      // 7 = TAG_VARIANT
    Closure { func_ptr: *const u8, captures: Vec<*mut RtValue> }, // 8 = TAG_CLOSURE
    Unit,                                                   // 9 = TAG_UNIT
    Bytes(Vec<u8>),                                         // 10 = TAG_BYTES
}

#[repr(C)]
pub struct RtValue {
    pub tag: u8,
    pub rc: u32,
    pub data: RtData,
}

// Safety: RtValue is manually ref-counted. Thread-safety is managed by
// retaining before send and releasing after receive. Required for
// thread-spawn and channel builtins.
unsafe impl Send for RtValue {}
unsafe impl Sync for RtValue {}

impl RtValue {
    pub fn alloc(tag: u8, data: RtData) -> *mut RtValue {
        let v = RtValue { tag, rc: 1, data };
        Box::into_raw(Box::new(v))
    }
}

// Rust-side constructors
pub fn rt_nil() -> *mut RtValue {
    RtValue::alloc(TAG_NIL, RtData::Nil)
}

pub fn rt_unit() -> *mut RtValue {
    RtValue::alloc(TAG_UNIT, RtData::Unit)
}

pub fn rt_int(v: i64) -> *mut RtValue {
    RtValue::alloc(TAG_INT, RtData::Int(v))
}

pub fn rt_float(v: f64) -> *mut RtValue {
    RtValue::alloc(TAG_FLOAT, RtData::Float(v))
}

pub fn rt_bool(v: bool) -> *mut RtValue {
    RtValue::alloc(TAG_BOOL, RtData::Bool(v))
}

pub fn rt_str(v: String) -> *mut RtValue {
    RtValue::alloc(TAG_STR, RtData::Str(v))
}

pub fn rt_list(items: Vec<*mut RtValue>) -> *mut RtValue {
    RtValue::alloc(TAG_LIST, RtData::List(items))
}

pub fn rt_map(m: HashMap<String, *mut RtValue>) -> *mut RtValue {
    RtValue::alloc(TAG_MAP, RtData::Map(m))
}

pub fn rt_variant(tag_name: String, inner: *mut RtValue) -> *mut RtValue {
    RtValue::alloc(TAG_VARIANT, RtData::Variant { tag_name, inner })
}

pub fn rt_bytes(v: Vec<u8>) -> *mut RtValue {
    RtValue::alloc(TAG_BYTES, RtData::Bytes(v))
}

// C-ABI constructors
#[no_mangle]
pub extern "C" fn airl_int(v: i64) -> *mut RtValue {
    rt_int(v)
}

#[no_mangle]
pub extern "C" fn airl_float(v: f64) -> *mut RtValue {
    rt_float(v)
}

#[no_mangle]
pub extern "C" fn airl_bool(v: bool) -> *mut RtValue {
    rt_bool(v)
}

#[no_mangle]
pub extern "C" fn airl_nil() -> *mut RtValue {
    rt_nil()
}

#[no_mangle]
pub extern "C" fn airl_unit() -> *mut RtValue {
    rt_unit()
}

#[no_mangle]
pub extern "C" fn airl_str(ptr: *const u8, len: usize) -> *mut RtValue {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    let s = std::str::from_utf8(slice).unwrap_or_else(|_| rt_error("airl_str: invalid utf8"));
    rt_str(s.to_string())
}

#[no_mangle]
pub extern "C" fn airl_bytes_new(ptr: *const u8, len: usize) -> *mut RtValue {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    rt_bytes(slice.to_vec())
}

// Rust-side accessors
impl RtValue {
    pub fn as_int(&self) -> i64 {
        match &self.data {
            RtData::Int(v) => *v,
            _ => rt_error("as_int: not an Int"),
        }
    }

    pub fn as_float(&self) -> f64 {
        match &self.data {
            RtData::Float(v) => *v,
            _ => rt_error("as_float: not a Float"),
        }
    }

    pub fn as_bool(&self) -> bool {
        match &self.data {
            RtData::Bool(v) => *v,
            _ => rt_error("as_bool: not a Bool"),
        }
    }

    pub fn as_str(&self) -> &str {
        match &self.data {
            RtData::Str(s) => s.as_str(),
            _ => rt_error("as_str: not a Str"),
        }
    }

    pub fn as_str_owned(&self) -> String {
        self.as_str().to_string()
    }

    pub fn as_list(&self) -> &Vec<*mut RtValue> {
        match &self.data {
            RtData::List(items) => items,
            _ => rt_error("as_list: not a List"),
        }
    }

    pub fn as_map(&self) -> &HashMap<String, *mut RtValue> {
        match &self.data {
            RtData::Map(m) => m,
            _ => rt_error("as_map: not a Map"),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match &self.data {
            RtData::Bytes(v) => v,
            _ => rt_error("as_bytes: not Bytes"),
        }
    }
}

impl fmt::Display for RtValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.data {
            RtData::Nil => write!(f, "nil"),
            RtData::Unit => write!(f, "()"),
            RtData::Int(v) => write!(f, "{}", v),
            RtData::Float(v) => {
                if v.fract() == 0.0 && v.is_finite() {
                    write!(f, "{:.1}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            RtData::Bool(v) => write!(f, "{}", v),
            RtData::Str(s) => write!(f, "\"{}\"", s),
            RtData::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    let val = unsafe { &**item };
                    write!(f, "{}", val)?;
                }
                write!(f, "]")
            }
            RtData::Map(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                write!(f, "{{")?;
                for (i, key) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    let val = unsafe { &*m[*key] };
                    write!(f, "{}: {}", key, val)?;
                }
                write!(f, "}}")
            }
            RtData::Variant { tag_name, inner } => {
                let val = unsafe { &**inner };
                write!(f, "({} {})", tag_name, val)
            }
            RtData::Closure { .. } => write!(f, "<closure>"),
            RtData::Bytes(v) => write!(f, "<Bytes len={}>", v.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn free_value(ptr: *mut RtValue) {
        drop(Box::from_raw(ptr));
    }

    #[test]
    fn test_int_roundtrip() {
        unsafe {
            let v = rt_int(42);
            assert_eq!((*v).as_int(), 42);
            assert_eq!(format!("{}", *v), "42");
            free_value(v);
        }
    }

    #[test]
    fn test_float_whole_number_display() {
        unsafe {
            let v = rt_float(3.0);
            assert_eq!(format!("{}", *v), "3.0");
            free_value(v);
        }
    }

    #[test]
    fn test_float_fractional_display() {
        unsafe {
            let v = rt_float(3.14);
            assert_eq!(format!("{}", *v), "3.14");
            free_value(v);
        }
    }

    #[test]
    fn test_bool_roundtrip() {
        unsafe {
            let t = rt_bool(true);
            let f = rt_bool(false);
            assert!((*t).as_bool());
            assert!(!(*f).as_bool());
            assert_eq!(format!("{}", *t), "true");
            assert_eq!(format!("{}", *f), "false");
            free_value(t);
            free_value(f);
        }
    }

    #[test]
    fn test_str_roundtrip() {
        unsafe {
            let v = rt_str("hello".to_string());
            assert_eq!((*v).as_str(), "hello");
            assert_eq!(format!("{}", *v), "\"hello\"");
            free_value(v);
        }
    }

    #[test]
    fn test_nil_display() {
        unsafe {
            let v = rt_nil();
            assert_eq!(format!("{}", *v), "nil");
            free_value(v);
        }
    }

    #[test]
    fn test_list_display() {
        unsafe {
            let a = rt_int(1);
            let b = rt_int(2);
            let c = rt_int(3);
            let list = rt_list(vec![a, b, c]);
            assert_eq!(format!("{}", *list), "[1 2 3]");
            // Free items then list (shallow free for test)
            let items = (*list).as_list().clone();
            drop(Box::from_raw(list));
            for item in items {
                free_value(item);
            }
        }
    }

    #[test]
    fn test_variant_display() {
        unsafe {
            let inner = rt_int(42);
            let v = rt_variant("Ok".to_string(), inner);
            assert_eq!(format!("{}", *v), "(Ok 42)");
            // Free inner then variant
            let inner_ptr = match &(*v).data {
                RtData::Variant { inner, .. } => *inner,
                _ => panic!(),
            };
            drop(Box::from_raw(v));
            free_value(inner_ptr);
        }
    }

    #[test]
    fn test_bytes_roundtrip() {
        unsafe {
            let v = rt_bytes(vec![1, 2, 3, 255]);
            assert_eq!((*v).tag, TAG_BYTES);
            assert_eq!((*v).as_bytes(), &[1, 2, 3, 255]);
            assert_eq!(format!("{}", *v), "<Bytes len=4>");
            free_value(v);
        }
    }

    #[test]
    fn test_bytes_empty() {
        unsafe {
            let v = rt_bytes(vec![]);
            assert_eq!((*v).as_bytes(), &[] as &[u8]);
            assert_eq!(format!("{}", *v), "<Bytes len=0>");
            free_value(v);
        }
    }
}
