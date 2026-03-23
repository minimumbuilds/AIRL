// crates/airl-runtime/src/bytecode_jit_full.rs
//! Full Cranelift JIT compiler that calls into the `airl-rt` runtime for all
//! value operations.  Unlike `bytecode_jit.rs` (primitive-only, unboxed), every
//! value here is a `*mut RtValue` pointer — a boxed, ref-counted heap allocation.
//!
//! This module is responsible for:
//!   1. Registering all `airl_*` runtime symbols with the JIT linker so that
//!      generated code can call them by name.
//!   2. Declaring every runtime function as a Cranelift `Import` so the compiler
//!      can emit `call` instructions.
//!   3. Building the `builtin_map` that maps AIRL builtin names ("+", "head", …)
//!      to their Cranelift `FuncId`s.
//!   4. Providing `value_to_rt` / `rt_to_value` marshaling helpers.
//!   5. Providing `try_call_native` for invoking already-compiled functions.

use std::collections::HashMap;

use cranelift_codegen::ir::{types, AbiParam};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use crate::value::Value;

// Re-export airl_rt types used by later compiler stages.
pub use airl_rt::value::RtValue;

// ─────────────────────────────────────────────────────────────────────────────
// RuntimeImports — one FuncId per runtime function
// ─────────────────────────────────────────────────────────────────────────────

/// Holds the declared Cranelift `FuncId` for every `airl_*` C function that
/// generated code may call.  The ids are obtained from a single `JITModule`
/// and remain valid for the lifetime of that module.
pub struct RuntimeImports {
    // Memory
    pub value_retain:  FuncId,
    pub value_release: FuncId,
    pub value_clone:   FuncId,

    // Constructors
    pub int_ctor:   FuncId,   // airl_int(i64) -> *mut RtValue
    pub float_ctor: FuncId,   // airl_float(f64) -> *mut RtValue  (declared but takes I64 bits)
    pub bool_ctor:  FuncId,   // airl_bool(i64) -> *mut RtValue   (bool passed as i64)
    pub nil_ctor:   FuncId,   // airl_nil() -> *mut RtValue
    pub unit_ctor:  FuncId,   // airl_unit() -> *mut RtValue
    pub str_ctor:   FuncId,   // airl_str(ptr, len) -> *mut RtValue

    // Logic
    pub as_bool_raw: FuncId,  // airl_as_bool_raw(*mut RtValue) -> i64

    // Arithmetic (arity-2: *mut, *mut → *mut)
    pub add: FuncId,
    pub sub: FuncId,
    pub mul: FuncId,
    pub div: FuncId,
    pub modulo: FuncId,

    // Comparison (arity-2: *mut, *mut → *mut)
    pub eq: FuncId,
    pub ne: FuncId,
    pub lt: FuncId,
    pub gt: FuncId,
    pub le: FuncId,
    pub ge: FuncId,

    // Logic (arity-1 / arity-2)
    pub not: FuncId,
    pub and: FuncId,
    pub or:  FuncId,
    pub xor: FuncId,

    // List operations
    pub head:     FuncId,   // (list) -> elem
    pub tail:     FuncId,   // (list) -> list
    pub cons:     FuncId,   // (elem, list) -> list
    pub empty:    FuncId,   // (list) -> bool
    pub length:   FuncId,   // (list|str|map) -> int
    pub at:       FuncId,   // (list, idx) -> elem
    pub append:   FuncId,   // (list, elem) -> list
    pub list_new: FuncId,   // (items_ptr, count) -> list  (special arity)

    // Variant / pattern
    pub make_variant: FuncId,  // (tag_str, inner) -> variant
    pub match_tag:    FuncId,  // (val, tag_str) -> inner | null

    // Closure
    pub make_closure: FuncId,  // (func_ptr, captures_ptr, count) -> closure
    pub call_closure: FuncId,  // (closure, args_ptr, argc) -> *mut RtValue

    // I/O / misc
    pub print:    FuncId,   // (val) -> nil
    pub type_of:  FuncId,   // (val) -> str
    pub valid:    FuncId,   // (val) -> bool

    // String builtins
    pub char_at:     FuncId,   // (str, int) -> str
    pub substring:   FuncId,   // (str, int, int) -> str
    pub chars:       FuncId,   // (str) -> list
    pub split:       FuncId,   // (str, str) -> list
    pub join:        FuncId,   // (list, str) -> str
    pub contains:    FuncId,   // (str, str) -> bool
    pub starts_with: FuncId,   // (str, str) -> bool
    pub ends_with:   FuncId,   // (str, str) -> bool
    pub index_of:    FuncId,   // (str, str) -> int
    pub trim:        FuncId,   // (str) -> str
    pub to_upper:    FuncId,   // (str) -> str
    pub to_lower:    FuncId,   // (str) -> str
    pub replace:     FuncId,   // (str, str, str) -> str

    // Map builtins
    pub map_new:    FuncId,   // () -> map
    pub map_from:   FuncId,   // (list) -> map
    pub map_get:    FuncId,   // (map, key) -> val
    pub map_get_or: FuncId,   // (map, key, default) -> val
    pub map_set:    FuncId,   // (map, key, val) -> map
    pub map_has:    FuncId,   // (map, key) -> bool
    pub map_remove: FuncId,   // (map, key) -> map
    pub map_keys:   FuncId,   // (map) -> list
    pub map_values: FuncId,   // (map) -> list
    pub map_size:   FuncId,   // (map) -> int
}

// ─────────────────────────────────────────────────────────────────────────────
// Signature helpers
// ─────────────────────────────────────────────────────────────────────────────

/// I64 pointer type (all `*mut RtValue` args / return values are word-sized).
const PTR: types::Type = types::I64;

fn sig_0_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_1_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_2_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_3_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

/// `(ptr) -> i64` — used for `airl_as_bool_raw`.
fn sig_1_ptr_ret_i64(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// `(ptr) -> void` — used for retain/release.
fn sig_1_ptr_ret_void(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig
}

/// `(ptr, ptr)` — 2 ptrs, *no* return value (retain/release take two ptrs variant doesn't apply;
/// this is just a placeholder for future use).
#[allow(dead_code)]
fn sig_2_ptr_ret_void(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig
}

/// `(i64) -> ptr` — used for `airl_int`.
fn sig_i64_ret_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

/// `(ptr, i64) -> ptr` — airl_str takes (u8*, usize).
fn sig_ptr_i64_ret_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

/// `(ptr, ptr, i64) -> ptr` — airl_make_closure / airl_call_closure.
fn sig_ptr_ptr_i64_ret_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

// ─────────────────────────────────────────────────────────────────────────────
// Declare helper
// ─────────────────────────────────────────────────────────────────────────────

fn declare_import(
    module: &mut JITModule,
    name: &str,
    sig: cranelift_codegen::ir::Signature,
) -> FuncId {
    module
        .declare_function(name, Linkage::Import, &sig)
        .unwrap_or_else(|e| panic!("failed to declare {}: {}", name, e))
}

// ─────────────────────────────────────────────────────────────────────────────
// BytecodeJitFull
// ─────────────────────────────────────────────────────────────────────────────

pub struct BytecodeJitFull {
    pub module: JITModule,
    pub rt: RuntimeImports,
    /// Cache of already-JIT-compiled functions: name → native code pointer.
    pub compiled: HashMap<String, *const u8>,
    /// Maps AIRL builtin names to their Cranelift FuncId for `CallBuiltin` dispatch.
    pub builtin_map: HashMap<String, FuncId>,
}

// SAFETY: The raw pointers in `compiled` are to JIT-allocated executable
// pages owned by `module`.  They live as long as `BytecodeJitFull`.
unsafe impl Send for BytecodeJitFull {}
unsafe impl Sync for BytecodeJitFull {}

impl BytecodeJitFull {
    /// Create a new JIT context, registering all `airl_*` runtime symbols.
    pub fn new() -> Result<Self, String> {
        // ── 1. Build the JITBuilder and register every runtime symbol ──────
        let mut builder =
            JITBuilder::new(cranelift_module::default_libcall_names())
                .map_err(|e| format!("JITBuilder error: {}", e))?;

        // Register symbols so Cranelift can resolve Import declarations.
        Self::register_symbols(&mut builder);

        // ── 2. Create the JITModule ────────────────────────────────────────
        let mut module = JITModule::new(builder);

        // ── 3. Declare all runtime functions ──────────────────────────────
        let rt = Self::declare_runtime_imports(&mut module);

        // ── 4. Build the builtin_map ───────────────────────────────────────
        let builtin_map = Self::build_builtin_map(&rt);

        Ok(Self {
            module,
            rt,
            compiled: HashMap::new(),
            builtin_map,
        })
    }

    // ──────────────────────────────────────────────────────────────────────
    // Symbol registration
    // ──────────────────────────────────────────────────────────────────────

    fn register_symbols(builder: &mut JITBuilder) {
        use airl_rt::*;

        // Memory
        builder.symbol("airl_value_retain",  memory::airl_value_retain  as *const u8);
        builder.symbol("airl_value_release", memory::airl_value_release as *const u8);
        builder.symbol("airl_value_clone",   memory::airl_value_clone   as *const u8);

        // Constructors (in value module)
        builder.symbol("airl_int",   value::airl_int   as *const u8);
        builder.symbol("airl_float", value::airl_float as *const u8);
        builder.symbol("airl_bool",  value::airl_bool  as *const u8);
        builder.symbol("airl_nil",   value::airl_nil   as *const u8);
        builder.symbol("airl_unit",  value::airl_unit  as *const u8);
        builder.symbol("airl_str",   value::airl_str   as *const u8);

        // Arithmetic
        builder.symbol("airl_add", arithmetic::airl_add as *const u8);
        builder.symbol("airl_sub", arithmetic::airl_sub as *const u8);
        builder.symbol("airl_mul", arithmetic::airl_mul as *const u8);
        builder.symbol("airl_div", arithmetic::airl_div as *const u8);
        builder.symbol("airl_mod", arithmetic::airl_mod as *const u8);

        // Comparison
        builder.symbol("airl_eq", comparison::airl_eq as *const u8);
        builder.symbol("airl_ne", comparison::airl_ne as *const u8);
        builder.symbol("airl_lt", comparison::airl_lt as *const u8);
        builder.symbol("airl_gt", comparison::airl_gt as *const u8);
        builder.symbol("airl_le", comparison::airl_le as *const u8);
        builder.symbol("airl_ge", comparison::airl_ge as *const u8);

        // Logic
        builder.symbol("airl_not",         logic::airl_not         as *const u8);
        builder.symbol("airl_and",         logic::airl_and         as *const u8);
        builder.symbol("airl_or",          logic::airl_or          as *const u8);
        builder.symbol("airl_xor",         logic::airl_xor         as *const u8);
        builder.symbol("airl_as_bool_raw", logic::airl_as_bool_raw as *const u8);

        // List
        builder.symbol("airl_head",     list::airl_head     as *const u8);
        builder.symbol("airl_tail",     list::airl_tail     as *const u8);
        builder.symbol("airl_cons",     list::airl_cons     as *const u8);
        builder.symbol("airl_empty",    list::airl_empty    as *const u8);
        builder.symbol("airl_length",   list::airl_length   as *const u8);
        builder.symbol("airl_at",       list::airl_at       as *const u8);
        builder.symbol("airl_append",   list::airl_append   as *const u8);
        builder.symbol("airl_list_new", list::airl_list_new as *const u8);

        // Variant / pattern
        builder.symbol("airl_make_variant", variant::airl_make_variant as *const u8);
        builder.symbol("airl_match_tag",    variant::airl_match_tag    as *const u8);

        // Closure
        builder.symbol("airl_make_closure", closure::airl_make_closure as *const u8);
        builder.symbol("airl_call_closure", closure::airl_call_closure as *const u8);

        // I/O
        builder.symbol("airl_print",   io::airl_print   as *const u8);
        builder.symbol("airl_type_of", io::airl_type_of as *const u8);
        builder.symbol("airl_valid",   io::airl_valid   as *const u8);

        // String
        builder.symbol("airl_char_at",     string::airl_char_at     as *const u8);
        builder.symbol("airl_substring",   string::airl_substring   as *const u8);
        builder.symbol("airl_chars",       string::airl_chars       as *const u8);
        builder.symbol("airl_split",       string::airl_split       as *const u8);
        builder.symbol("airl_join",        string::airl_join        as *const u8);
        builder.symbol("airl_contains",    string::airl_contains    as *const u8);
        builder.symbol("airl_starts_with", string::airl_starts_with as *const u8);
        builder.symbol("airl_ends_with",   string::airl_ends_with   as *const u8);
        builder.symbol("airl_index_of",    string::airl_index_of    as *const u8);
        builder.symbol("airl_trim",        string::airl_trim        as *const u8);
        builder.symbol("airl_to_upper",    string::airl_to_upper    as *const u8);
        builder.symbol("airl_to_lower",    string::airl_to_lower    as *const u8);
        builder.symbol("airl_replace",     string::airl_replace     as *const u8);

        // Map
        builder.symbol("airl_map_new",    map::airl_map_new    as *const u8);
        builder.symbol("airl_map_from",   map::airl_map_from   as *const u8);
        builder.symbol("airl_map_get",    map::airl_map_get    as *const u8);
        builder.symbol("airl_map_get_or", map::airl_map_get_or as *const u8);
        builder.symbol("airl_map_set",    map::airl_map_set    as *const u8);
        builder.symbol("airl_map_has",    map::airl_map_has    as *const u8);
        builder.symbol("airl_map_remove", map::airl_map_remove as *const u8);
        builder.symbol("airl_map_keys",   map::airl_map_keys   as *const u8);
        builder.symbol("airl_map_values", map::airl_map_values as *const u8);
        builder.symbol("airl_map_size",   map::airl_map_size   as *const u8);
    }

    // ──────────────────────────────────────────────────────────────────────
    // Declare all runtime imports in the JITModule
    // ──────────────────────────────────────────────────────────────────────

    fn declare_runtime_imports(m: &mut JITModule) -> RuntimeImports {
        // Memory — retain/release take a ptr, return nothing
        let void_1  = sig_1_ptr_ret_void(m);
        let value_retain  = declare_import(m, "airl_value_retain",  void_1.clone());
        let value_release = declare_import(m, "airl_value_release", void_1);
        let value_clone   = declare_import(m, "airl_value_clone",   sig_1_ptr(m));

        // Constructors
        let int_ctor   = declare_import(m, "airl_int",   sig_i64_ret_ptr(m));
        // airl_float takes an f64 — we transmit it as I64 bits at call sites
        let float_ctor = declare_import(m, "airl_float", sig_i64_ret_ptr(m));
        // airl_bool takes a bool (C _Bool / i8), but we use i64 for uniformity
        let bool_ctor  = declare_import(m, "airl_bool",  sig_i64_ret_ptr(m));
        let nil_ctor   = declare_import(m, "airl_nil",   sig_0_ptr(m));
        let unit_ctor  = declare_import(m, "airl_unit",  sig_0_ptr(m));
        let str_ctor   = declare_import(m, "airl_str",   sig_ptr_i64_ret_ptr(m));

        // Logic helper
        let as_bool_raw = declare_import(m, "airl_as_bool_raw", sig_1_ptr_ret_i64(m));

        // Arithmetic — all (ptr, ptr) -> ptr
        let s2 = sig_2_ptr(m);
        let add    = declare_import(m, "airl_add", s2.clone());
        let sub    = declare_import(m, "airl_sub", s2.clone());
        let mul    = declare_import(m, "airl_mul", s2.clone());
        let div    = declare_import(m, "airl_div", s2.clone());
        let modulo = declare_import(m, "airl_mod", s2.clone());

        // Comparison — all (ptr, ptr) -> ptr
        let eq = declare_import(m, "airl_eq", s2.clone());
        let ne = declare_import(m, "airl_ne", s2.clone());
        let lt = declare_import(m, "airl_lt", s2.clone());
        let gt = declare_import(m, "airl_gt", s2.clone());
        let le = declare_import(m, "airl_le", s2.clone());
        let ge = declare_import(m, "airl_ge", s2.clone());

        // Logic — (ptr) -> ptr  and  (ptr, ptr) -> ptr
        let s1 = sig_1_ptr(m);
        let not = declare_import(m, "airl_not", s1.clone());
        let and = declare_import(m, "airl_and", s2.clone());
        let or  = declare_import(m, "airl_or",  s2.clone());
        let xor = declare_import(m, "airl_xor", s2.clone());

        // List operations
        let head   = declare_import(m, "airl_head",   s1.clone());
        let tail   = declare_import(m, "airl_tail",   s1.clone());
        let cons   = declare_import(m, "airl_cons",   s2.clone());
        let empty  = declare_import(m, "airl_empty",  s1.clone());
        let length = declare_import(m, "airl_length", s1.clone());
        let at     = declare_import(m, "airl_at",     s2.clone());
        let append = declare_import(m, "airl_append", s2.clone());
        // airl_list_new(items_ptr: *const *mut RtValue, count: usize) -> *mut RtValue
        let list_new = declare_import(m, "airl_list_new", sig_ptr_i64_ret_ptr(m));

        // Variant / pattern
        let make_variant = declare_import(m, "airl_make_variant", s2.clone());
        let match_tag    = declare_import(m, "airl_match_tag",    s2.clone());

        // Closure
        // airl_make_closure(func_ptr, captures_ptr, count) -> *mut RtValue
        let make_closure = declare_import(m, "airl_make_closure", sig_ptr_ptr_i64_ret_ptr(m));
        // airl_call_closure(closure, args_ptr, argc) -> *mut RtValue
        let call_closure = declare_import(m, "airl_call_closure", sig_ptr_ptr_i64_ret_ptr(m));

        // I/O / misc
        let print   = declare_import(m, "airl_print",   s1.clone());
        let type_of = declare_import(m, "airl_type_of", s1.clone());
        let valid   = declare_import(m, "airl_valid",   s1.clone());

        // String builtins
        let char_at     = declare_import(m, "airl_char_at",     s2.clone());
        let substring   = declare_import(m, "airl_substring",   sig_3_ptr(m));
        let chars       = declare_import(m, "airl_chars",       s1.clone());
        let split       = declare_import(m, "airl_split",       s2.clone());
        let join        = declare_import(m, "airl_join",        s2.clone());
        let contains    = declare_import(m, "airl_contains",    s2.clone());
        let starts_with = declare_import(m, "airl_starts_with", s2.clone());
        let ends_with   = declare_import(m, "airl_ends_with",   s2.clone());
        let index_of    = declare_import(m, "airl_index_of",    s2.clone());
        let trim        = declare_import(m, "airl_trim",        s1.clone());
        let to_upper    = declare_import(m, "airl_to_upper",    s1.clone());
        let to_lower    = declare_import(m, "airl_to_lower",    s1.clone());
        let replace     = declare_import(m, "airl_replace",     sig_3_ptr(m));

        // Map builtins
        let map_new    = declare_import(m, "airl_map_new",    sig_0_ptr(m));
        let map_from   = declare_import(m, "airl_map_from",   s1.clone());
        let map_get    = declare_import(m, "airl_map_get",    s2.clone());
        let map_get_or = declare_import(m, "airl_map_get_or", sig_3_ptr(m));
        let map_set    = declare_import(m, "airl_map_set",    sig_3_ptr(m));
        let map_has    = declare_import(m, "airl_map_has",    s2.clone());
        let map_remove = declare_import(m, "airl_map_remove", s2.clone());
        let map_keys   = declare_import(m, "airl_map_keys",   s1.clone());
        let map_values = declare_import(m, "airl_map_values", s1.clone());
        let map_size   = declare_import(m, "airl_map_size",   s1.clone());

        RuntimeImports {
            value_retain, value_release, value_clone,
            int_ctor, float_ctor, bool_ctor, nil_ctor, unit_ctor, str_ctor,
            as_bool_raw,
            add, sub, mul, div, modulo,
            eq, ne, lt, gt, le, ge,
            not, and, or, xor,
            head, tail, cons, empty, length, at, append, list_new,
            make_variant, match_tag,
            make_closure, call_closure,
            print, type_of, valid,
            char_at, substring, chars, split, join, contains, starts_with,
            ends_with, index_of, trim, to_upper, to_lower, replace,
            map_new, map_from, map_get, map_get_or, map_set, map_has,
            map_remove, map_keys, map_values, map_size,
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Build the AIRL-name → FuncId map for CallBuiltin dispatch
    // ──────────────────────────────────────────────────────────────────────

    fn build_builtin_map(rt: &RuntimeImports) -> HashMap<String, FuncId> {
        let mut m = HashMap::new();

        // Arithmetic
        m.insert("+".into(),  rt.add);
        m.insert("-".into(),  rt.sub);
        m.insert("*".into(),  rt.mul);
        m.insert("/".into(),  rt.div);
        m.insert("%".into(),  rt.modulo);

        // Comparison
        m.insert("=".into(),  rt.eq);
        m.insert("!=".into(), rt.ne);
        m.insert("<".into(),  rt.lt);
        m.insert(">".into(),  rt.gt);
        m.insert("<=".into(), rt.le);
        m.insert(">=".into(), rt.ge);

        // Logic
        m.insert("not".into(), rt.not);
        m.insert("and".into(), rt.and);
        m.insert("or".into(),  rt.or);
        m.insert("xor".into(), rt.xor);

        // List
        m.insert("head".into(),   rt.head);
        m.insert("tail".into(),   rt.tail);
        m.insert("cons".into(),   rt.cons);
        m.insert("empty?".into(), rt.empty);
        m.insert("length".into(), rt.length);
        m.insert("at".into(),     rt.at);
        m.insert("append".into(), rt.append);

        // I/O
        m.insert("print".into(),   rt.print);
        m.insert("type-of".into(), rt.type_of);
        m.insert("valid".into(),   rt.valid);

        // String builtins
        m.insert("char-at".into(),     rt.char_at);
        m.insert("substring".into(),   rt.substring);
        m.insert("chars".into(),       rt.chars);
        m.insert("split".into(),       rt.split);
        m.insert("join".into(),        rt.join);
        m.insert("contains".into(),    rt.contains);
        m.insert("starts-with".into(), rt.starts_with);
        m.insert("ends-with".into(),   rt.ends_with);
        m.insert("index-of".into(),    rt.index_of);
        m.insert("trim".into(),        rt.trim);
        m.insert("to-upper".into(),    rt.to_upper);
        m.insert("to-lower".into(),    rt.to_lower);
        m.insert("replace".into(),     rt.replace);

        // Map builtins
        m.insert("map-new".into(),    rt.map_new);
        m.insert("map-from".into(),   rt.map_from);
        m.insert("map-get".into(),    rt.map_get);
        m.insert("map-get-or".into(), rt.map_get_or);
        m.insert("map-set".into(),    rt.map_set);
        m.insert("map-has".into(),    rt.map_has);
        m.insert("map-remove".into(), rt.map_remove);
        m.insert("map-keys".into(),   rt.map_keys);
        m.insert("map-values".into(), rt.map_values);
        m.insert("map-size".into(),   rt.map_size);

        m
    }

    // ──────────────────────────────────────────────────────────────────────
    // Value marshaling: interpreter Value ↔ airl_rt RtValue
    // ──────────────────────────────────────────────────────────────────────

    /// Convert an interpreter `Value` into a heap-allocated `RtValue`.
    /// The caller owns one reference (rc=1).
    pub fn value_to_rt(v: &Value) -> *mut RtValue {
        use airl_rt::value::*;
        match v {
            Value::Int(n)   => rt_int(*n),
            Value::UInt(n)  => rt_int(*n as i64),
            Value::Float(f) => rt_float(*f),
            Value::Bool(b)  => rt_bool(*b),
            Value::Str(s)   => rt_str(s.clone()),
            Value::Nil      => rt_nil(),
            Value::Unit     => rt_unit(),
            Value::List(items) => {
                let ptrs: Vec<*mut RtValue> = items.iter().map(Self::value_to_rt).collect();
                rt_list(ptrs)
            }
            Value::Tuple(items) => {
                // Represent tuples as lists at the RT level.
                let ptrs: Vec<*mut RtValue> = items.iter().map(Self::value_to_rt).collect();
                rt_list(ptrs)
            }
            Value::Variant(tag, inner) => {
                let inner_ptr = Self::value_to_rt(inner);
                rt_variant(tag.clone(), inner_ptr)
            }
            Value::Map(map) => {
                use std::collections::HashMap as HM;
                let mut rt_map_data: HM<String, *mut RtValue> = HM::new();
                for (k, val) in map {
                    rt_map_data.insert(k.clone(), Self::value_to_rt(val));
                }
                rt_map(rt_map_data)
            }
            // Anything we cannot represent → nil
            _ => rt_nil(),
        }
    }

    /// Convert an `RtValue` pointer back to an interpreter `Value`.
    /// Releases (decrements rc of) the pointer after conversion.
    ///
    /// # Safety
    /// `ptr` must be a valid, non-null `*mut RtValue` with rc >= 1.
    pub fn rt_to_value(ptr: *mut RtValue) -> Value {
        use airl_rt::value::RtData;
        use airl_rt::memory::airl_value_release;

        if ptr.is_null() {
            return Value::Nil;
        }

        let result = unsafe {
            match &(*ptr).data {
                RtData::Nil      => Value::Nil,
                RtData::Unit     => Value::Unit,
                RtData::Int(n)   => Value::Int(*n),
                RtData::Float(f) => Value::Float(*f),
                RtData::Bool(b)  => Value::Bool(*b),
                RtData::Str(s)   => Value::Str(s.clone()),
                RtData::List(items) => {
                    let vals: Vec<Value> = items
                        .iter()
                        .map(|&item| {
                            // Retain so we can safely convert; release happens in recursive call.
                            airl_rt::memory::airl_value_retain(item);
                            Self::rt_to_value(item)
                        })
                        .collect();
                    Value::List(vals)
                }
                RtData::Map(map) => {
                    let mut result_map = std::collections::HashMap::new();
                    for (k, &val) in map {
                        airl_rt::memory::airl_value_retain(val);
                        result_map.insert(k.clone(), Self::rt_to_value(val));
                    }
                    Value::Map(result_map)
                }
                RtData::Variant { tag_name, inner } => {
                    let inner_copy = *inner;
                    airl_rt::memory::airl_value_retain(inner_copy);
                    Value::Variant(tag_name.clone(), Box::new(Self::rt_to_value(inner_copy)))
                }
                RtData::Closure { .. } => {
                    // Cannot round-trip closures through interpreter Value easily.
                    Value::Nil
                }
            }
        };

        // Release the original pointer (we've extracted what we needed).
        // SAFETY: ptr was non-null and valid; we are done with it.
        airl_value_release(ptr);
        result
    }

    // ──────────────────────────────────────────────────────────────────────
    // Call native JIT-compiled function
    // ──────────────────────────────────────────────────────────────────────

    /// Look up a compiled function by name and call it with `args`.
    /// Returns `Some(Value)` on success, `None` if the function is not compiled.
    ///
    /// Arity 0–8 is supported via transmute dispatch.
    pub fn try_call_native(&self, name: &str, args: &[Value]) -> Option<Value> {
        let fn_ptr = *self.compiled.get(name)?;

        // Marshal args → RtValue pointers
        let mut rt_args: Vec<*mut RtValue> =
            args.iter().map(|v| Self::value_to_rt(v)).collect();

        // Call through the function pointer, dispatch by arity
        let result_ptr = unsafe { Self::dispatch_call(fn_ptr, &mut rt_args) };

        // Release the argument RtValues we allocated for the call.
        // SAFETY: each ptr was freshly allocated by value_to_rt; rc=1.
        for &ptr in &rt_args {
            airl_rt::memory::airl_value_release(ptr);
        }

        Some(Self::rt_to_value(result_ptr))
    }

    /// Dispatch a call to a C-ABI function pointer by arity (0–8).
    ///
    /// # Safety
    /// `fn_ptr` must point to a C-ABI function whose arity matches `args.len()`.
    unsafe fn dispatch_call(fn_ptr: *const u8, args: &mut Vec<*mut RtValue>) -> *mut RtValue {
        match args.len() {
            0 => {
                let f: extern "C" fn() -> *mut RtValue = std::mem::transmute(fn_ptr);
                f()
            }
            1 => {
                let f: extern "C" fn(*mut RtValue) -> *mut RtValue =
                    std::mem::transmute(fn_ptr);
                f(args[0])
            }
            2 => {
                let f: extern "C" fn(*mut RtValue, *mut RtValue) -> *mut RtValue =
                    std::mem::transmute(fn_ptr);
                f(args[0], args[1])
            }
            3 => {
                let f: extern "C" fn(*mut RtValue, *mut RtValue, *mut RtValue) -> *mut RtValue =
                    std::mem::transmute(fn_ptr);
                f(args[0], args[1], args[2])
            }
            4 => {
                let f: extern "C" fn(
                    *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue,
                ) -> *mut RtValue = std::mem::transmute(fn_ptr);
                f(args[0], args[1], args[2], args[3])
            }
            5 => {
                let f: extern "C" fn(
                    *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue,
                ) -> *mut RtValue = std::mem::transmute(fn_ptr);
                f(args[0], args[1], args[2], args[3], args[4])
            }
            6 => {
                let f: extern "C" fn(
                    *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue,
                    *mut RtValue, *mut RtValue,
                ) -> *mut RtValue = std::mem::transmute(fn_ptr);
                f(args[0], args[1], args[2], args[3], args[4], args[5])
            }
            7 => {
                let f: extern "C" fn(
                    *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue,
                    *mut RtValue, *mut RtValue, *mut RtValue,
                ) -> *mut RtValue = std::mem::transmute(fn_ptr);
                f(args[0], args[1], args[2], args[3], args[4], args[5], args[6])
            }
            8 => {
                let f: extern "C" fn(
                    *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue,
                    *mut RtValue, *mut RtValue, *mut RtValue, *mut RtValue,
                ) -> *mut RtValue = std::mem::transmute(fn_ptr);
                f(args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7])
            }
            n => {
                // Arity > 8 not supported — return nil
                eprintln!("[JIT-full] try_call_native: arity {} > 8 not supported", n);
                airl_rt::value::rt_nil()
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_succeeds() {
        let jit = BytecodeJitFull::new();
        assert!(jit.is_ok(), "BytecodeJitFull::new() failed: {:?}", jit.err());
    }

    #[test]
    fn builtin_map_contains_expected_keys() {
        let jit = BytecodeJitFull::new().unwrap();
        for key in ["+", "-", "*", "/", "%",
                    "=", "!=", "<", ">", "<=", ">=",
                    "not", "and", "or", "xor",
                    "head", "tail", "cons", "empty?", "length", "at", "append",
                    "print", "type-of", "valid",
                    "char-at", "substring", "chars", "split", "join",
                    "contains", "starts-with", "ends-with", "index-of",
                    "trim", "to-upper", "to-lower", "replace",
                    "map-new", "map-from", "map-get", "map-get-or", "map-set",
                    "map-has", "map-remove", "map-keys", "map-values", "map-size"] {
            assert!(jit.builtin_map.contains_key(key), "missing builtin: {}", key);
        }
    }

    #[test]
    fn value_to_rt_and_back_int() {
        let v = Value::Int(42);
        let ptr = BytecodeJitFull::value_to_rt(&v);
        assert!(!ptr.is_null());
        let back = BytecodeJitFull::rt_to_value(ptr);
        assert_eq!(back, Value::Int(42));
    }

    #[test]
    fn value_to_rt_and_back_str() {
        let v = Value::Str("hello".into());
        let ptr = BytecodeJitFull::value_to_rt(&v);
        assert!(!ptr.is_null());
        let back = BytecodeJitFull::rt_to_value(ptr);
        assert_eq!(back, Value::Str("hello".into()));
    }

    #[test]
    fn value_to_rt_and_back_bool() {
        for b in [true, false] {
            let ptr = BytecodeJitFull::value_to_rt(&Value::Bool(b));
            let back = BytecodeJitFull::rt_to_value(ptr);
            assert_eq!(back, Value::Bool(b));
        }
    }

    #[test]
    fn value_to_rt_and_back_nil() {
        let ptr = BytecodeJitFull::value_to_rt(&Value::Nil);
        let back = BytecodeJitFull::rt_to_value(ptr);
        assert_eq!(back, Value::Nil);
    }

    #[test]
    fn value_to_rt_and_back_list() {
        let v = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let ptr = BytecodeJitFull::value_to_rt(&v);
        let back = BytecodeJitFull::rt_to_value(ptr);
        assert_eq!(back, v);
    }

    #[test]
    fn value_to_rt_and_back_variant() {
        let v = Value::Variant("Ok".into(), Box::new(Value::Int(99)));
        let ptr = BytecodeJitFull::value_to_rt(&v);
        let back = BytecodeJitFull::rt_to_value(ptr);
        assert_eq!(back, v);
    }
}
