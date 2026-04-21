#[cfg(target_os = "airlos")]
#[allow(unused_imports)]
use crate::nostd_prelude::*;

use crate::error::rt_error;
use crate::memory::airl_value_retain;
use crate::value::{rt_variant_at, RtData, RtValue};

#[cfg(not(target_os = "airlos"))]
static SITE_MAKE_VARIANT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

/// Construct a variant value: (tag inner).
/// `tag` must be a Str; `inner` can be any value.
/// The variant takes ownership of one reference to `inner` (caller must retain separately if needed).
#[no_mangle]
pub extern "C" fn airl_make_variant(tag: *mut RtValue, inner: *mut RtValue) -> *mut RtValue {
    let tag_name = unsafe { &*tag }.as_str_owned();
    airl_value_retain(inner);
    #[cfg(not(target_os = "airlos"))]
    let sid = *SITE_MAKE_VARIANT.get_or_init(|| crate::diag::register_site("variant.rs:airl_make_variant"));
    #[cfg(target_os = "airlos")]
    let sid = 0u16;
    rt_variant_at(tag_name, inner, sid)
}

/// Attempt to match a variant against an expected tag.
/// Returns the inner value (with incremented refcount) on match,
/// or null if the tag does not match or `val` is not a Variant.
#[no_mangle]
pub extern "C" fn airl_match_tag(val: *mut RtValue, expected_tag: *mut RtValue) -> *mut RtValue {
    let expected = unsafe { &*expected_tag }.as_str_owned();
    let v = unsafe { &*val };
    match &v.data {
        RtData::Variant { tag_name, inner } if *tag_name == expected => {
            airl_value_retain(*inner);
            *inner
        }
        RtData::Variant { .. } => core::ptr::null_mut(),
        _ => rt_error("airl_match_tag: not a Variant"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::airl_value_release;
    use crate::value::{rt_int, rt_str, rt_unit};

    #[test]
    fn make_and_match() {
        unsafe {
            let tag = rt_str("Ok".to_string());
            let inner = rt_int(42);
            let variant = airl_make_variant(tag, inner);

            // Display check
            assert_eq!(format!("{}", *variant), "(Ok 42)");

            let expected = rt_str("Ok".to_string());
            let result = airl_match_tag(variant, expected);
            assert!(!result.is_null());
            assert_eq!((*result).as_int(), 42);

            airl_value_release(result);
            airl_value_release(expected);
            airl_value_release(inner);
            airl_value_release(tag);
            airl_value_release(variant);
        }
    }

    #[test]
    fn match_wrong_tag() {
        unsafe {
            let tag = rt_str("Ok".to_string());
            let inner = rt_int(42);
            let variant = airl_make_variant(tag, inner);

            let wrong_tag = rt_str("Err".to_string());
            let result = airl_match_tag(variant, wrong_tag);
            assert!(result.is_null());

            airl_value_release(wrong_tag);
            airl_value_release(inner);
            airl_value_release(tag);
            airl_value_release(variant);
        }
    }

    #[test]
    fn nullary_variant() {
        unsafe {
            let tag = rt_str("None".to_string());
            let unit = rt_unit();
            let variant = airl_make_variant(tag, unit);

            assert_eq!(format!("{}", *variant), "(None ())");

            airl_value_release(unit);
            airl_value_release(tag);
            airl_value_release(variant);
        }
    }
}
