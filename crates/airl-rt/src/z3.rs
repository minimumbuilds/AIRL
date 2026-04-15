//! Z3 SMT solver wrappers for airl-rt.
//! Provides `airl_z3_*` extern "C" functions callable from AIRL via extern-c.
//! All Z3 opaque pointers are passed as i64 handles (raw pointer casts).
//! Array-taking Z3 functions (mk_add, mk_and, etc.) are wrapped as binary ops.

#[cfg(not(target_os = "airlos"))]
use crate::value::{rt_int, rt_nil, RtData, RtValue};

#[cfg(not(target_os = "airlos"))]
use std::ffi::{c_int, c_uint, c_void, CString};

// ── Raw Z3 C API bindings ──────────────────────────────────────────
// Linked at final binary stage via -lz3.
// All Z3 handle types are opaque pointers; we use *mut c_void.
#[cfg(not(target_os = "airlos"))]
type Z3ErrorHandler = Option<extern "C" fn(*mut c_void, u32)>;

#[cfg(not(target_os = "airlos"))]
extern "C" fn z3_noop_error_handler(_ctx: *mut c_void, _err: u32) {
    // Silently ignore Z3 errors — caller checks results
}

#[cfg(not(target_os = "airlos"))]
extern "C" {
    fn Z3_mk_config() -> *mut c_void;
    fn Z3_del_config(c: *mut c_void);
    fn Z3_mk_context(c: *mut c_void) -> *mut c_void;
    fn Z3_set_error_handler(c: *mut c_void, h: Z3ErrorHandler);
    fn Z3_del_context(c: *mut c_void);
    fn Z3_mk_solver(c: *mut c_void) -> *mut c_void;
    fn Z3_solver_inc_ref(c: *mut c_void, s: *mut c_void);
    fn Z3_solver_dec_ref(c: *mut c_void, s: *mut c_void);
    fn Z3_mk_int_sort(c: *mut c_void) -> *mut c_void;
    fn Z3_mk_bool_sort(c: *mut c_void) -> *mut c_void;
    fn Z3_mk_string_symbol(c: *mut c_void, s: *const i8) -> *mut c_void;
    fn Z3_mk_const(c: *mut c_void, s: *mut c_void, ty: *mut c_void) -> *mut c_void;
    fn Z3_mk_int(c: *mut c_void, v: c_int, ty: *mut c_void) -> *mut c_void;
    fn Z3_mk_true(c: *mut c_void) -> *mut c_void;
    fn Z3_mk_false(c: *mut c_void) -> *mut c_void;
    fn Z3_mk_eq(c: *mut c_void, l: *mut c_void, r: *mut c_void) -> *mut c_void;
    fn Z3_mk_not(c: *mut c_void, a: *mut c_void) -> *mut c_void;
    fn Z3_mk_ite(c: *mut c_void, t1: *mut c_void, t2: *mut c_void, t3: *mut c_void) -> *mut c_void;
    fn Z3_mk_implies(c: *mut c_void, t1: *mut c_void, t2: *mut c_void) -> *mut c_void;
    fn Z3_mk_and(c: *mut c_void, num: c_uint, args: *const *mut c_void) -> *mut c_void;
    fn Z3_mk_or(c: *mut c_void, num: c_uint, args: *const *mut c_void) -> *mut c_void;
    fn Z3_mk_add(c: *mut c_void, num: c_uint, args: *const *mut c_void) -> *mut c_void;
    fn Z3_mk_sub(c: *mut c_void, num: c_uint, args: *const *mut c_void) -> *mut c_void;
    fn Z3_mk_mul(c: *mut c_void, num: c_uint, args: *const *mut c_void) -> *mut c_void;
    fn Z3_mk_div(c: *mut c_void, a: *mut c_void, b: *mut c_void) -> *mut c_void;
    fn Z3_mk_mod(c: *mut c_void, a: *mut c_void, b: *mut c_void) -> *mut c_void;
    fn Z3_mk_lt(c: *mut c_void, t1: *mut c_void, t2: *mut c_void) -> *mut c_void;
    fn Z3_mk_le(c: *mut c_void, t1: *mut c_void, t2: *mut c_void) -> *mut c_void;
    fn Z3_mk_gt(c: *mut c_void, t1: *mut c_void, t2: *mut c_void) -> *mut c_void;
    fn Z3_mk_ge(c: *mut c_void, t1: *mut c_void, t2: *mut c_void) -> *mut c_void;
    fn Z3_solver_assert(c: *mut c_void, s: *mut c_void, a: *mut c_void);
    fn Z3_solver_check(c: *mut c_void, s: *mut c_void) -> c_int;
}

// ── Helpers ────────────────────────────────────────────────────────

#[cfg(not(target_os = "airlos"))]
fn extract_handle(v: *mut RtValue) -> *mut c_void {
    unsafe {
        match &(*v).data {
            RtData::Int(n) => *n as *mut c_void,
            _ => std::ptr::null_mut(),
        }
    }
}

#[cfg(not(target_os = "airlos"))]
fn extract_int(v: *mut RtValue) -> i64 {
    unsafe {
        match &(*v).data {
            RtData::Int(n) => *n,
            _ => 0,
        }
    }
}

#[cfg(not(target_os = "airlos"))]
fn extract_str(v: *mut RtValue) -> String {
    unsafe {
        match &(*v).data {
            RtData::Str(s) => s.clone(),
            _ => String::new(),
        }
    }
}

#[cfg(not(target_os = "airlos"))]
fn handle_to_rt(ptr: *mut c_void) -> *mut RtValue {
    rt_int(ptr as i64)
}

// ── Context / Config ───────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_config() -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_config() })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_del_config(cfg: *mut RtValue) -> *mut RtValue {
    unsafe { Z3_del_config(extract_handle(cfg)) };
    rt_nil()
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_context(cfg: *mut RtValue) -> *mut RtValue {
    let ctx = unsafe { Z3_mk_context(extract_handle(cfg)) };
    unsafe { Z3_set_error_handler(ctx, Some(z3_noop_error_handler)) };
    handle_to_rt(ctx)
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_del_context(ctx: *mut RtValue) -> *mut RtValue {
    unsafe { Z3_del_context(extract_handle(ctx)) };
    rt_nil()
}

// ── Solver ─────────────────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_solver(ctx: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let s = unsafe { Z3_mk_solver(c) };
    // With Z3_mk_context (auto GC), inc_ref/dec_ref are optional
    // but we call inc_ref to prevent premature collection during long-lived usage
    unsafe { Z3_solver_inc_ref(c, s) };
    handle_to_rt(s)
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_del_solver(ctx: *mut RtValue, solver: *mut RtValue) -> *mut RtValue {
    unsafe { Z3_solver_dec_ref(extract_handle(ctx), extract_handle(solver)) };
    rt_nil()
}

// ── Sorts ──────────────────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_int_sort(ctx: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_int_sort(extract_handle(ctx)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_bool_sort(ctx: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_bool_sort(extract_handle(ctx)) })
}

// ── Symbol / Constant ──────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_string_symbol(ctx: *mut RtValue, name: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let s = extract_str(name);
    let cs = match CString::new(s) {
        Ok(v) => v,
        Err(_) => return rt_int(0),
    };
    handle_to_rt(unsafe { Z3_mk_string_symbol(c, cs.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_const(ctx: *mut RtValue, sym: *mut RtValue, sort: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_const(extract_handle(ctx), extract_handle(sym), extract_handle(sort)) })
}

// ── Literals ───────────────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_int_val(ctx: *mut RtValue, val: *mut RtValue, sort: *mut RtValue) -> *mut RtValue {
    let v = extract_int(val) as c_int;
    handle_to_rt(unsafe { Z3_mk_int(extract_handle(ctx), v, extract_handle(sort)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_true(ctx: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_true(extract_handle(ctx)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_false(ctx: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_false(extract_handle(ctx)) })
}

// ── Arithmetic (binary wrappers for array-taking Z3 functions) ─────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_add2(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a), extract_handle(b)];
    handle_to_rt(unsafe { Z3_mk_add(c, 2, args.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_sub2(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a), extract_handle(b)];
    handle_to_rt(unsafe { Z3_mk_sub(c, 2, args.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_mul2(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a), extract_handle(b)];
    handle_to_rt(unsafe { Z3_mk_mul(c, 2, args.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_div(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_div(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_mod(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_mod(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

// ── Comparison ─────────────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_lt(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_lt(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_le(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_le(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_gt(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_gt(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_ge(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_ge(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_eq(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_eq(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

// ── Logic ──────────────────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_and2(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a), extract_handle(b)];
    handle_to_rt(unsafe { Z3_mk_and(c, 2, args.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_or2(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a), extract_handle(b)];
    handle_to_rt(unsafe { Z3_mk_or(c, 2, args.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_not(ctx: *mut RtValue, a: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_not(extract_handle(ctx), extract_handle(a)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_implies(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_implies(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_ite(ctx: *mut RtValue, cond: *mut RtValue, t: *mut RtValue, e: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_ite(extract_handle(ctx), extract_handle(cond), extract_handle(t), extract_handle(e)) })
}

// ── Solver operations ──────────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_solver_assert(ctx: *mut RtValue, solver: *mut RtValue, ast: *mut RtValue) -> *mut RtValue {
    unsafe { Z3_solver_assert(extract_handle(ctx), extract_handle(solver), extract_handle(ast)) };
    rt_nil()
}

/// Returns: 1 = SAT (Z3_L_TRUE), -1 = UNSAT (Z3_L_FALSE), 0 = UNKNOWN (Z3_L_UNDEF)
#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_solver_check(ctx: *mut RtValue, solver: *mut RtValue) -> *mut RtValue {
    let result = unsafe { Z3_solver_check(extract_handle(ctx), extract_handle(solver)) };
    rt_int(result as i64)
}
