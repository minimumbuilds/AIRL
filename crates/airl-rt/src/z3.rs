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
    fn Z3_mk_real_sort(c: *mut c_void) -> *mut c_void;
    fn Z3_mk_string_sort(c: *mut c_void) -> *mut c_void;
    fn Z3_mk_string_symbol(c: *mut c_void, s: *const i8) -> *mut c_void;
    fn Z3_mk_const(c: *mut c_void, s: *mut c_void, ty: *mut c_void) -> *mut c_void;
    fn Z3_mk_int(c: *mut c_void, v: c_int, ty: *mut c_void) -> *mut c_void;
    fn Z3_mk_real(c: *mut c_void, num: c_int, den: c_int) -> *mut c_void;
    fn Z3_mk_int2real(c: *mut c_void, a: *mut c_void) -> *mut c_void;
    fn Z3_mk_string(c: *mut c_void, s: *const i8) -> *mut c_void;
    fn Z3_mk_seq_sort(c: *mut c_void, elem_sort: *mut c_void) -> *mut c_void;
    fn Z3_mk_seq_unit(c: *mut c_void, elem: *mut c_void) -> *mut c_void;
    fn Z3_mk_seq_length(c: *mut c_void, s: *mut c_void) -> *mut c_void;
    fn Z3_mk_seq_contains(c: *mut c_void, a: *mut c_void, b: *mut c_void) -> *mut c_void;
    fn Z3_mk_seq_concat(c: *mut c_void, num: c_uint, args: *const *mut c_void) -> *mut c_void;
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
    fn Z3_mk_forall_const(c: *mut c_void, weight: c_uint, num_bound: c_uint, bound: *const *mut c_void, num_patterns: c_uint, patterns: *const *mut c_void, body: *mut c_void) -> *mut c_void;
    fn Z3_mk_exists_const(c: *mut c_void, weight: c_uint, num_bound: c_uint, bound: *const *mut c_void, num_patterns: c_uint, patterns: *const *mut c_void, body: *mut c_void) -> *mut c_void;
    fn Z3_mk_func_decl(c: *mut c_void, s: *mut c_void, domain_size: c_uint, domain: *const *mut c_void, range: *mut c_void) -> *mut c_void;
    fn Z3_mk_app(c: *mut c_void, d: *mut c_void, num_args: c_uint, args: *const *mut c_void) -> *mut c_void;
    fn Z3_solver_assert(c: *mut c_void, s: *mut c_void, a: *mut c_void);
    fn Z3_solver_check(c: *mut c_void, s: *mut c_void) -> c_int;
    fn Z3_solver_get_model(c: *mut c_void, s: *mut c_void) -> *mut c_void;
    fn Z3_model_to_string(c: *mut c_void, m: *mut c_void) -> *const i8;
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

// ── Real sort (issue-133) ─────────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_real_sort(ctx: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_real_sort(extract_handle(ctx)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_real(ctx: *mut RtValue, num: *mut RtValue, den: *mut RtValue) -> *mut RtValue {
    let n = extract_int(num) as c_int;
    let d = extract_int(den) as c_int;
    handle_to_rt(unsafe { Z3_mk_real(extract_handle(ctx), n, d) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_int2real(ctx: *mut RtValue, ast: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_int2real(extract_handle(ctx), extract_handle(ast)) })
}

// ── String sort (issue-133) ───────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_string_sort(ctx: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_string_sort(extract_handle(ctx)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_string_val(ctx: *mut RtValue, s: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let val = extract_str(s);
    let cs = match CString::new(val) {
        Ok(v) => v,
        Err(_) => return rt_int(0),
    };
    handle_to_rt(unsafe { Z3_mk_string(c, cs.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_seq_length(ctx: *mut RtValue, ast: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_seq_length(extract_handle(ctx), extract_handle(ast)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_seq_contains(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_seq_contains(extract_handle(ctx), extract_handle(a), extract_handle(b)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_seq_concat2(ctx: *mut RtValue, a: *mut RtValue, b: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a), extract_handle(b)];
    handle_to_rt(unsafe { Z3_mk_seq_concat(c, 2, args.as_ptr()) })
}

// ── Quantifiers (issue-134) ───────────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_forall_const1(ctx: *mut RtValue, bound: *mut RtValue, body: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let bound_arr = [extract_handle(bound)];
    let empty_patterns: [*mut c_void; 0] = [];
    handle_to_rt(unsafe {
        Z3_mk_forall_const(c, 0, 1, bound_arr.as_ptr(), 0, empty_patterns.as_ptr(), extract_handle(body))
    })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_exists_const1(ctx: *mut RtValue, bound: *mut RtValue, body: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let bound_arr = [extract_handle(bound)];
    let empty_patterns: [*mut c_void; 0] = [];
    handle_to_rt(unsafe {
        Z3_mk_exists_const(c, 0, 1, bound_arr.as_ptr(), 0, empty_patterns.as_ptr(), extract_handle(body))
    })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_forall_const2(ctx: *mut RtValue, b1: *mut RtValue, b2: *mut RtValue, body: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let bound_arr = [extract_handle(b1), extract_handle(b2)];
    let empty_patterns: [*mut c_void; 0] = [];
    handle_to_rt(unsafe {
        Z3_mk_forall_const(c, 0, 2, bound_arr.as_ptr(), 0, empty_patterns.as_ptr(), extract_handle(body))
    })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_exists_const2(ctx: *mut RtValue, b1: *mut RtValue, b2: *mut RtValue, body: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let bound_arr = [extract_handle(b1), extract_handle(b2)];
    let empty_patterns: [*mut c_void; 0] = [];
    handle_to_rt(unsafe {
        Z3_mk_exists_const(c, 0, 2, bound_arr.as_ptr(), 0, empty_patterns.as_ptr(), extract_handle(body))
    })
}

// ── Seq sort / unit (issue-137) ───────────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_seq_sort(ctx: *mut RtValue, elem_sort: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_seq_sort(extract_handle(ctx), extract_handle(elem_sort)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_seq_unit(ctx: *mut RtValue, elem: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_mk_seq_unit(extract_handle(ctx), extract_handle(elem)) })
}

// ── Uninterpreted functions (issue-140) ───────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_func_decl1(ctx: *mut RtValue, name: *mut RtValue, domain: *mut RtValue, range: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let s = extract_str(name);
    let cs = match CString::new(s) {
        Ok(v) => v,
        Err(_) => return rt_int(0),
    };
    let sym = unsafe { Z3_mk_string_symbol(c, cs.as_ptr()) };
    let domain_arr = [extract_handle(domain)];
    handle_to_rt(unsafe { Z3_mk_func_decl(c, sym, 1, domain_arr.as_ptr(), extract_handle(range)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_func_decl2(ctx: *mut RtValue, name: *mut RtValue, d1: *mut RtValue, d2: *mut RtValue, range: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let s = extract_str(name);
    let cs = match CString::new(s) {
        Ok(v) => v,
        Err(_) => return rt_int(0),
    };
    let sym = unsafe { Z3_mk_string_symbol(c, cs.as_ptr()) };
    let domain_arr = [extract_handle(d1), extract_handle(d2)];
    handle_to_rt(unsafe { Z3_mk_func_decl(c, sym, 2, domain_arr.as_ptr(), extract_handle(range)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_app1(ctx: *mut RtValue, decl: *mut RtValue, arg: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(arg)];
    handle_to_rt(unsafe { Z3_mk_app(c, extract_handle(decl), 1, args.as_ptr()) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_mk_app2(ctx: *mut RtValue, decl: *mut RtValue, a1: *mut RtValue, a2: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let args = [extract_handle(a1), extract_handle(a2)];
    handle_to_rt(unsafe { Z3_mk_app(c, extract_handle(decl), 2, args.as_ptr()) })
}

// ── Model / counterexample (issue-136) ────────────────────────────

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_solver_get_model(ctx: *mut RtValue, solver: *mut RtValue) -> *mut RtValue {
    handle_to_rt(unsafe { Z3_solver_get_model(extract_handle(ctx), extract_handle(solver)) })
}

#[no_mangle]
#[cfg(not(target_os = "airlos"))]
pub extern "C" fn airl_z3_model_to_string(ctx: *mut RtValue, model: *mut RtValue) -> *mut RtValue {
    let c = extract_handle(ctx);
    let m = extract_handle(model);
    if m.is_null() {
        return rt_int(0);
    }
    let cstr = unsafe { Z3_model_to_string(c, m) };
    if cstr.is_null() {
        return rt_int(0);
    }
    let s = unsafe { std::ffi::CStr::from_ptr(cstr) };
    let owned = s.to_string_lossy().into_owned();
    crate::value::rt_str(owned)
}
