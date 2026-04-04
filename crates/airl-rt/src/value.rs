#[cfg(target_os = "airlos")]
use crate::nostd_prelude::*;

#[cfg(not(target_os = "airlos"))]
use std::collections::HashMap;
#[cfg(target_os = "airlos")]
use alloc::collections::BTreeMap as HashMap;

#[cfg(not(target_os = "airlos"))]
use std::fmt;
#[cfg(target_os = "airlos")]
use core::fmt;
use core::sync::atomic::AtomicU32;

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
    List { items: Vec<*mut RtValue>, offset: usize, parent: Option<*mut RtValue> }, // 5 = TAG_LIST
    Map(HashMap<String, *mut RtValue>),                     // 6 = TAG_MAP
    Variant { tag_name: String, inner: *mut RtValue },      // 7 = TAG_VARIANT
    Closure { func_ptr: *const u8, captures: Vec<*mut RtValue> }, // 8 = TAG_CLOSURE
    Unit,                                                   // 9 = TAG_UNIT
    Bytes(Vec<u8>),                                         // 10 = TAG_BYTES
}

#[repr(C)]
pub struct RtValue {
    pub tag: u8,
    pub rc: AtomicU32,
    pub data: RtData,
}

// SAFETY: RtValue uses manual reference counting (the `rc` field) for
// lifetime management. Cross-thread transfers are safe under this protocol:
//
//   Pre-condition:  Caller MUST call `airl_value_retain` before sending
//                   an `*mut RtValue` across a thread boundary (channel-send,
//                   thread-spawn captured values, etc.).
//   Post-condition: Receiver MUST call `airl_value_release` when it no
//                   longer needs the value.
//   Invariant:      No two threads may hold a *mutable* reference to the
//                   same `RtValue` simultaneously. Shared read-only access
//                   is permitted when refcount >= 2.
//
// Violation of any of the above causes use-after-free or data races.
// Prefer `SendableRtValue` (below) for cross-thread transfers — it
// enforces retain-on-construct / release-on-drop at the type level.
unsafe impl Send for RtValue {}
unsafe impl Sync for RtValue {}

/// A wrapper that enforces the retain/release protocol for cross-thread
/// transfers of `*mut RtValue`. Retains on construction, releases on drop.
///
/// Use `SendableRtValue::new(ptr)` to safely wrap a value for sending.
/// The receiver calls `into_raw()` to take ownership of the retained
/// pointer (and becomes responsible for releasing it).
pub struct SendableRtValue(*mut RtValue);

impl SendableRtValue {
    /// Retains the value and wraps it for safe cross-thread transfer.
    ///
    /// # Safety
    ///
    /// `v` must be a valid, non-null `*mut RtValue` with rc >= 1.
    pub fn new(v: *mut RtValue) -> Self {
        assert!(!v.is_null(), "SendableRtValue::new called with null pointer");
        crate::memory::airl_value_retain(v);
        Self(v)
    }

    /// Wraps a pointer that has *already* been retained by the caller.
    /// Does NOT call retain again. The wrapper will release on drop.
    ///
    /// # Safety
    ///
    /// `v` must be a valid, non-null `*mut RtValue` whose refcount already
    /// accounts for this wrapper's ownership.
    pub unsafe fn from_retained(v: *mut RtValue) -> Self {
        assert!(!v.is_null(), "SendableRtValue::from_retained called with null pointer");
        Self(v)
    }

    /// Returns the underlying pointer without consuming or releasing.
    pub fn as_ptr(&self) -> *mut RtValue {
        self.0
    }

    /// Consumes the wrapper and returns the raw pointer.
    /// The caller becomes responsible for calling `airl_value_release`.
    pub fn into_raw(self) -> *mut RtValue {
        let ptr = self.0;
        core::mem::forget(self); // prevent Drop from releasing
        ptr
    }
}

impl Drop for SendableRtValue {
    fn drop(&mut self) {
        crate::memory::airl_value_release(self.0);
    }
}

// SAFETY: SendableRtValue enforces the retain/release protocol at the
// type level — the value is retained on construction and released on drop.
unsafe impl Send for SendableRtValue {}

impl RtValue {
    pub fn alloc(tag: u8, data: RtData) -> *mut RtValue {
        let v = RtValue { tag, rc: AtomicU32::new(1), data };
        Box::into_raw(Box::new(v))
    }
}

// ── Static singletons for frequently-created immutable values ─────
// These avoid heap allocation entirely. The rc is set to u32::MAX
// (immortal), so retain/release are no-ops and free_value is never called.

static NIL_SINGLETON: RtValue = RtValue {
    tag: TAG_NIL,
    rc: AtomicU32::new(u32::MAX),
    data: RtData::Nil,
};

static UNIT_SINGLETON: RtValue = RtValue {
    tag: TAG_UNIT,
    rc: AtomicU32::new(u32::MAX),
    data: RtData::Unit,
};

static TRUE_SINGLETON: RtValue = RtValue {
    tag: TAG_BOOL,
    rc: AtomicU32::new(u32::MAX),
    data: RtData::Bool(true),
};

static FALSE_SINGLETON: RtValue = RtValue {
    tag: TAG_BOOL,
    rc: AtomicU32::new(u32::MAX),
    data: RtData::Bool(false),
};

// Rust-side constructors
pub fn rt_nil() -> *mut RtValue {
    &NIL_SINGLETON as *const RtValue as *mut RtValue
}

pub fn rt_unit() -> *mut RtValue {
    &UNIT_SINGLETON as *const RtValue as *mut RtValue
}

pub fn rt_int(v: i64) -> *mut RtValue {
    RtValue::alloc(TAG_INT, RtData::Int(v))
}

pub fn rt_float(v: f64) -> *mut RtValue {
    RtValue::alloc(TAG_FLOAT, RtData::Float(v))
}

pub fn rt_bool(v: bool) -> *mut RtValue {
    if v {
        &TRUE_SINGLETON as *const RtValue as *mut RtValue
    } else {
        &FALSE_SINGLETON as *const RtValue as *mut RtValue
    }
}

pub fn rt_str(v: String) -> *mut RtValue {
    RtValue::alloc(TAG_STR, RtData::Str(v))
}

pub fn rt_list(items: Vec<*mut RtValue>) -> *mut RtValue {
    RtValue::alloc(TAG_LIST, RtData::List { items, offset: 0, parent: None })
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
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    let s = core::str::from_utf8(slice).unwrap_or_else(|_| rt_error("airl_str: invalid utf8"));
    rt_str(s.to_string())
}

#[no_mangle]
pub extern "C" fn airl_bytes_new(ptr: *const u8, len: usize) -> *mut RtValue {
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    rt_bytes(slice.to_vec())
}

// ── Safe Option-returning accessors ────────────────────────────────
//
// These return `None` on type mismatch instead of panicking, making them
// suitable for use after a single `unsafe { &*ptr }` dereference.  The
// bytecode VM uses these to minimise the number of distinct unsafe blocks.
impl RtValue {
    /// Returns `Some(n)` if this value is `Int(n)`, else `None`.
    pub fn try_as_int(&self) -> Option<i64> {
        match &self.data {
            RtData::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns `Some(f)` if this value is `Float(f)`, else `None`.
    pub fn try_as_float(&self) -> Option<f64> {
        match &self.data {
            RtData::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns `Some(b)` if this value is `Bool(b)`, else `None`.
    pub fn try_as_bool(&self) -> Option<bool> {
        match &self.data {
            RtData::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns `Some(s)` if this value is `Str(s)`, else `None`.
    pub fn try_as_str(&self) -> Option<&str> {
        match &self.data {
            RtData::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns `true` if this value is `Nil`.
    pub fn is_nil(&self) -> bool {
        matches!(&self.data, RtData::Nil)
    }

    /// Returns `true` if this value is `Unit`.
    pub fn is_unit(&self) -> bool {
        matches!(&self.data, RtData::Unit)
    }

    /// If this is a `Variant`, returns `(tag_name, inner)`.
    pub fn try_as_variant(&self) -> Option<(&str, *mut RtValue)> {
        match &self.data {
            RtData::Variant { tag_name, inner } => Some((tag_name.as_str(), *inner)),
            _ => None,
        }
    }

    /// Returns the `RtData` enum reference for direct pattern matching.
    pub fn data(&self) -> &RtData {
        &self.data
    }
}

// ── Panicking accessors (used by extern "C" builtins) ─────────────
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

    pub fn as_list(&self) -> &[*mut RtValue] {
        match &self.data {
            RtData::List { items, offset, parent } => {
                if let Some(p) = parent {
                    let root = unsafe { &**p };
                    match &root.data {
                        RtData::List { items: root_items, .. } => &root_items[*offset..],
                        _ => rt_error("as_list: view parent is not a List"),
                    }
                } else {
                    &items[*offset..]
                }
            }
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
                if *v == (*v as i64 as f64) && v.is_finite() {
                    write!(f, "{:.1}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            RtData::Bool(v) => write!(f, "{}", v),
            RtData::Str(s) => write!(f, "\"{}\"", s),
            RtData::List { items, offset, parent } => {
                let slice = if let Some(p) = parent {
                    let root = unsafe { &**p };
                    match &root.data {
                        RtData::List { items: root_items, .. } => &root_items[*offset..],
                        _ => return write!(f, "[<invalid view>]"),
                    }
                } else {
                    &items[*offset..]
                };
                write!(f, "[")?;
                for (i, item) in slice.iter().enumerate() {
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
    use core::sync::atomic::Ordering;

    unsafe fn free_value(ptr: *mut RtValue) {
        // Skip static singletons (immortal rc) — they cannot be freed.
        if (*ptr).rc.load(Ordering::Relaxed) == u32::MAX {
            return;
        }
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
            let items = (*list).as_list().to_vec();
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

    // ── Option-returning accessor tests ───────────────────────────

    #[test]
    fn test_try_as_int_happy() {
        unsafe {
            let v = rt_int(99);
            assert_eq!((*v).try_as_int(), Some(99));
            free_value(v);
        }
    }

    #[test]
    fn test_try_as_int_wrong_type() {
        unsafe {
            let v = rt_str("nope".into());
            assert_eq!((*v).try_as_int(), None);
            free_value(v);
        }
    }

    #[test]
    fn test_try_as_float_happy() {
        unsafe {
            let v = rt_float(2.718);
            assert_eq!((*v).try_as_float(), Some(2.718));
            free_value(v);
        }
    }

    #[test]
    fn test_try_as_float_wrong_type() {
        unsafe {
            let v = rt_int(1);
            assert_eq!((*v).try_as_float(), None);
            free_value(v);
        }
    }

    #[test]
    fn test_try_as_bool_happy() {
        unsafe {
            let t = rt_bool(true);
            let f = rt_bool(false);
            assert_eq!((*t).try_as_bool(), Some(true));
            assert_eq!((*f).try_as_bool(), Some(false));
            free_value(t);
            free_value(f);
        }
    }

    #[test]
    fn test_try_as_bool_wrong_type() {
        unsafe {
            let v = rt_nil();
            assert_eq!((*v).try_as_bool(), None);
            free_value(v);
        }
    }

    #[test]
    fn test_try_as_str_happy() {
        unsafe {
            let v = rt_str("hello".into());
            assert_eq!((*v).try_as_str(), Some("hello"));
            free_value(v);
        }
    }

    #[test]
    fn test_try_as_str_wrong_type() {
        unsafe {
            let v = rt_int(42);
            assert_eq!((*v).try_as_str(), None);
            free_value(v);
        }
    }

    #[test]
    fn test_is_nil() {
        unsafe {
            let n = rt_nil();
            let i = rt_int(0);
            assert!((*n).is_nil());
            assert!(!(*i).is_nil());
            free_value(n);
            free_value(i);
        }
    }

    #[test]
    fn test_is_unit() {
        unsafe {
            let u = rt_unit();
            let i = rt_int(0);
            assert!((*u).is_unit());
            assert!(!(*i).is_unit());
            free_value(u);
            free_value(i);
        }
    }

    #[test]
    fn test_try_as_variant_happy() {
        unsafe {
            let inner = rt_int(42);
            let v = rt_variant("Ok".into(), inner);
            let (tag, inner_ptr) = (*v).try_as_variant().unwrap();
            assert_eq!(tag, "Ok");
            assert_eq!((*inner_ptr).as_int(), 42);
            drop(Box::from_raw(v));
            free_value(inner);
        }
    }

    #[test]
    fn test_try_as_variant_wrong_type() {
        unsafe {
            let v = rt_int(1);
            assert!((*v).try_as_variant().is_none());
            free_value(v);
        }
    }

    #[test]
    fn test_data_accessor() {
        unsafe {
            let v = rt_int(7);
            assert!(matches!((*v).data(), &RtData::Int(7)));
            free_value(v);
        }
    }

    // ── SendableRtValue tests ─────────────────────────────────────

    #[test]
    fn test_sendable_retains_on_new() {
        unsafe {
            let v = rt_int(10);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            let sv = SendableRtValue::new(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 2);
            drop(sv); // should release
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            crate::memory::airl_value_release(v);
        }
    }

    #[test]
    fn test_sendable_into_raw_no_double_release() {
        unsafe {
            let v = rt_int(20);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            let sv = SendableRtValue::new(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 2);
            let raw = sv.into_raw();
            // into_raw consumed the wrapper without releasing
            assert_eq!((*v).rc.load(Ordering::Relaxed), 2);
            assert_eq!(raw, v);
            // Manually release the extra ref
            crate::memory::airl_value_release(v);
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            crate::memory::airl_value_release(v);
        }
    }

    #[test]
    fn test_sendable_from_retained() {
        unsafe {
            let v = rt_int(30);
            crate::memory::airl_value_retain(v); // manually retain
            assert_eq!((*v).rc.load(Ordering::Relaxed), 2);
            let sv = SendableRtValue::from_retained(v);
            // from_retained does NOT retain again
            assert_eq!((*v).rc.load(Ordering::Relaxed), 2);
            drop(sv); // releases once
            assert_eq!((*v).rc.load(Ordering::Relaxed), 1);
            crate::memory::airl_value_release(v);
        }
    }

    #[test]
    fn test_sendable_as_ptr() {
        unsafe {
            let v = rt_int(40);
            let sv = SendableRtValue::new(v);
            assert_eq!(sv.as_ptr(), v);
            drop(sv);
            crate::memory::airl_value_release(v);
        }
    }

    #[test]
    #[should_panic(expected = "SendableRtValue::new called with null")]
    fn test_sendable_null_panics() {
        let _sv = SendableRtValue::new(core::ptr::null_mut());
    }

    #[test]
    fn test_sendable_is_send() {
        // Compile-time check that SendableRtValue implements Send.
        fn assert_send<T: Send>() {}
        assert_send::<SendableRtValue>();
    }

    // ── Singleton contract tests ─────────────────────────────────

    #[test]
    fn test_nil_singleton_same_pointer() {
        let a = rt_nil();
        let b = rt_nil();
        assert_eq!(a, b, "rt_nil() should return the same pointer each time");
    }

    #[test]
    fn test_bool_singleton_same_pointer() {
        let t1 = rt_bool(true);
        let t2 = rt_bool(true);
        let f1 = rt_bool(false);
        let f2 = rt_bool(false);
        assert_eq!(t1, t2, "rt_bool(true) should return the same pointer");
        assert_eq!(f1, f2, "rt_bool(false) should return the same pointer");
        assert_ne!(t1, f1, "true and false should be different pointers");
    }

    #[test]
    fn test_unit_singleton_same_pointer() {
        let a = rt_unit();
        let b = rt_unit();
        assert_eq!(a, b, "rt_unit() should return the same pointer each time");
    }

    #[test]
    fn test_singleton_retain_release_are_noops() {
        unsafe {
            let n = rt_nil();
            let rc_before = (*n).rc.load(Ordering::Relaxed);
            assert_eq!(rc_before, u32::MAX, "singleton rc should be immortal");
            crate::memory::airl_value_retain(n);
            assert_eq!((*n).rc.load(Ordering::Relaxed), u32::MAX);
            crate::memory::airl_value_release(n);
            assert_eq!((*n).rc.load(Ordering::Relaxed), u32::MAX);
        }
    }
}
