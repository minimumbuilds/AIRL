// crates/airl-runtime/src/bytecode_jit_full.rs
//! Full Cranelift JIT compiler that calls into the `airl-rt` runtime for all
//! value operations.  Every value here is a `*mut RtValue` pointer — a boxed,
//! ref-counted heap allocation.
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

use std::collections::{BTreeSet, HashMap};

use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder, MemFlags, StackSlotData};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use crate::bytecode::*;
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
    pub print:    FuncId,        // (val) -> nil
    pub println:  FuncId,        // (val) -> nil  (Display-formatted with newline)
    pub print_values: FuncId,    // (args_ptr, count) -> nil  (variadic print)
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

    // File I/O
    pub read_file:   FuncId,      // (path_str) -> str
    pub write_file:  FuncId,      // (path, content) -> result
    pub file_exists: FuncId,      // (path) -> bool
    pub append_file: FuncId,      // (path, content) -> result
    pub delete_file: FuncId,      // (path) -> result
    pub delete_dir:  FuncId,      // (path) -> result
    pub rename_file: FuncId,      // (old, new) -> result
    pub read_dir:    FuncId,      // (path) -> list
    pub create_dir:  FuncId,      // (path) -> result
    pub file_size:   FuncId,      // (path) -> int
    pub is_dir:      FuncId,      // (path) -> bool
    pub get_args:    FuncId,      // () -> list
    pub run_bytecode: FuncId,   // (list_of_bcfuncs) -> result
    pub compile_bc_to_exe: FuncId, // (list_of_bcfuncs, output_path) -> result

    // Misc builtins
    pub char_count: FuncId,
    pub str_variadic: FuncId,
    pub format_variadic: FuncId,
    pub assert_fn: FuncId,
    pub panic_fn: FuncId,
    pub exit_fn: FuncId,
    pub sleep_fn: FuncId,
    pub format_time: FuncId,
    pub read_lines: FuncId,
    pub concat_lists: FuncId,
    pub range_fn: FuncId,
    pub reverse_list: FuncId,
    pub take_fn: FuncId,
    pub drop_fn: FuncId,
    pub zip_fn: FuncId,
    pub flatten_fn: FuncId,
    pub enumerate_fn: FuncId,
    pub map_ho: FuncId,
    pub filter_ho: FuncId,
    pub fold_ho: FuncId,
    pub sort_ho: FuncId,
    pub any_ho: FuncId,
    pub all_ho: FuncId,
    pub find_ho: FuncId,
    pub path_join: FuncId,
    pub path_parent: FuncId,
    pub path_filename: FuncId,
    pub path_extension: FuncId,
    pub is_absolute: FuncId,
    pub regex_match: FuncId,
    pub regex_find_all: FuncId,
    pub regex_replace: FuncId,
    pub regex_split: FuncId,
    pub sha256: FuncId,
    pub hmac_sha256: FuncId,
    pub base64_encode: FuncId,
    pub base64_decode: FuncId,
    pub random_bytes: FuncId,
    pub string_to_float: FuncId,

    // Crypto (byte-oriented)
    pub sha512: FuncId,
    pub hmac_sha512: FuncId,
    pub sha256_bytes: FuncId,
    pub sha512_bytes: FuncId,
    pub hmac_sha256_bytes: FuncId,
    pub hmac_sha512_bytes: FuncId,
    pub pbkdf2_sha256: FuncId,
    pub pbkdf2_sha512: FuncId,
    pub base64_decode_bytes: FuncId,
    pub base64_encode_bytes: FuncId,
    pub bitwise_xor: FuncId,
    pub bitwise_and: FuncId,
    pub bitwise_or: FuncId,
    pub bitwise_shr: FuncId,
    pub bitwise_shl: FuncId,

    // Byte encoding
    pub bytes_from_int16: FuncId,
    pub bytes_from_int32: FuncId,
    pub bytes_from_int64: FuncId,
    pub bytes_to_int16: FuncId,
    pub bytes_to_int32: FuncId,
    pub bytes_to_int64: FuncId,
    pub bytes_from_string: FuncId,
    pub bytes_to_string: FuncId,
    pub bytes_concat: FuncId,
    pub bytes_slice: FuncId,
    pub crc32c: FuncId,

    // Compression
    pub gzip_compress: FuncId,
    pub gzip_decompress: FuncId,
    pub snappy_compress: FuncId,
    pub snappy_decompress: FuncId,
    pub lz4_compress: FuncId,
    pub lz4_decompress: FuncId,
    pub zstd_compress: FuncId,
    pub zstd_decompress: FuncId,

    // TCP sockets
    pub tcp_connect: FuncId,
    pub tcp_close: FuncId,
    pub tcp_send: FuncId,
    pub tcp_recv: FuncId,
    pub tcp_recv_exact: FuncId,
    pub tcp_set_timeout: FuncId,
    pub tcp_connect_tls: FuncId,

    // Contract failure
    pub contract_fail: FuncId,  // (kind: i64, fn_name_idx: i64, clause_idx: i64) -> i64
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

fn sig_4_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
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

/// `(f64) -> ptr` — used for `airl_float` which takes an actual f64.
fn sig_f64_ret_ptr(m: &JITModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(types::F64));
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
    /// Stable storage for string constants whose raw pointers are baked into JIT code.
    /// Strings here live as long as the JIT module, preventing dangling pointer bugs.
    stable_strings: Vec<String>,
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
            stable_strings: Vec::new(),
        })
    }

    /// Intern a string into stable storage, returning (ptr, len).
    /// The pointer is valid for the lifetime of `self`.
    fn intern_string(&mut self, s: &str) -> (*const u8, usize) {
        self.stable_strings.push(s.to_string());
        let stored = self.stable_strings.last().unwrap();
        (stored.as_ptr(), stored.len())
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
        builder.symbol("airl_print",        io::airl_print        as *const u8);
        builder.symbol("airl_println",      io::airl_println      as *const u8);
        builder.symbol("airl_print_values", io::airl_print_values as *const u8);
        builder.symbol("airl_type_of",      io::airl_type_of      as *const u8);
        builder.symbol("airl_valid",        io::airl_valid        as *const u8);
        builder.symbol("airl_read_file",    io::airl_read_file    as *const u8);
        builder.symbol("airl_write_file",   io::airl_write_file   as *const u8);
        builder.symbol("airl_file_exists",  io::airl_file_exists  as *const u8);
        builder.symbol("airl_append_file",  io::airl_append_file  as *const u8);
        builder.symbol("airl_delete_file",  io::airl_delete_file  as *const u8);
        builder.symbol("airl_delete_dir",   io::airl_delete_dir   as *const u8);
        builder.symbol("airl_rename_file",  io::airl_rename_file  as *const u8);
        builder.symbol("airl_read_dir",     io::airl_read_dir     as *const u8);
        builder.symbol("airl_create_dir",   io::airl_create_dir   as *const u8);
        builder.symbol("airl_file_size",    io::airl_file_size    as *const u8);
        builder.symbol("airl_is_dir",       io::airl_is_dir       as *const u8);
        builder.symbol("airl_get_args",     io::airl_get_args     as *const u8);
        builder.symbol("airl_run_bytecode", crate::bytecode_marshal::airl_run_bytecode as *const u8);
        #[cfg(feature = "aot")]
        builder.symbol("airl_compile_to_executable", crate::bytecode_aot::airl_compile_to_executable as *const u8);
        #[cfg(feature = "aot")]
        builder.symbol("airl_compile_bytecode_to_executable", crate::bytecode_marshal::airl_compile_bytecode_to_executable as *const u8);

        // Contract failure signaling (shared module)
        builder.symbol("airl_jit_contract_fail", crate::jit_contract::airl_jit_contract_fail as *const u8);

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
        // airl_float takes a real f64 (passed in XMM register on x86-64)
        let float_ctor = declare_import(m, "airl_float", sig_f64_ret_ptr(m));
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
        let print        = declare_import(m, "airl_print",        s1.clone());
        let println      = declare_import(m, "airl_println",      s1.clone());
        let print_values = declare_import(m, "airl_print_values", sig_ptr_i64_ret_ptr(m));
        let type_of      = declare_import(m, "airl_type_of",      s1.clone());
        let valid        = declare_import(m, "airl_valid",        s1.clone());

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

        // File I/O
        let read_file   = declare_import(m, "airl_read_file",   s1.clone());
        let write_file  = declare_import(m, "airl_write_file",  s2.clone());
        let file_exists = declare_import(m, "airl_file_exists", s1.clone());
        let append_file = declare_import(m, "airl_append_file", s2.clone());
        let delete_file = declare_import(m, "airl_delete_file", s1.clone());
        let delete_dir  = declare_import(m, "airl_delete_dir",  s1.clone());
        let rename_file = declare_import(m, "airl_rename_file", s2.clone());
        let read_dir    = declare_import(m, "airl_read_dir",    s1.clone());
        let create_dir  = declare_import(m, "airl_create_dir",  s1.clone());
        let file_size   = declare_import(m, "airl_file_size",   s1.clone());
        let is_dir      = declare_import(m, "airl_is_dir",      s1.clone());
        let get_args    = declare_import(m, "airl_get_args",    sig_0_ptr(m));
        let run_bytecode = declare_import(m, "airl_run_bytecode", s1.clone());
        let compile_bc_to_exe = declare_import(m, "airl_compile_bytecode_to_executable", s2.clone());

        // Misc builtins
        let char_count = declare_import(m, "airl_char_count", s1.clone());
        let str_variadic = declare_import(m, "airl_str_variadic", sig_ptr_i64_ret_ptr(m));
        let format_variadic = declare_import(m, "airl_format_variadic", sig_ptr_i64_ret_ptr(m));
        let assert_fn = declare_import(m, "airl_assert", s2.clone());
        let panic_fn = declare_import(m, "airl_panic", s1.clone());
        let exit_fn = declare_import(m, "airl_exit", s1.clone());
        let sleep_fn = declare_import(m, "airl_sleep", s1.clone());
        let format_time = declare_import(m, "airl_format_time", s2.clone());
        let read_lines = declare_import(m, "airl_read_lines", s1.clone());
        let concat_lists = declare_import(m, "airl_concat_lists", s2.clone());
        let range_fn = declare_import(m, "airl_range", s2.clone());
        let reverse_list = declare_import(m, "airl_reverse_list", s1.clone());
        let take_fn = declare_import(m, "airl_take", s2.clone());
        let drop_fn = declare_import(m, "airl_drop", s2.clone());
        let zip_fn = declare_import(m, "airl_zip", s2.clone());
        let flatten_fn = declare_import(m, "airl_flatten", s1.clone());
        let enumerate_fn = declare_import(m, "airl_enumerate", s1.clone());
        let map_ho = declare_import(m, "airl_map", s2.clone());
        let filter_ho = declare_import(m, "airl_filter", s2.clone());
        let mut s3 = m.make_signature();
        for _ in 0..3 { s3.params.push(AbiParam::new(PTR)); }
        s3.returns.push(AbiParam::new(PTR));
        let fold_ho = declare_import(m, "airl_fold", s3);
        let sort_ho = declare_import(m, "airl_sort", s2.clone());
        let any_ho = declare_import(m, "airl_any", s2.clone());
        let all_ho = declare_import(m, "airl_all", s2.clone());
        let find_ho = declare_import(m, "airl_find", s2.clone());
        let path_join = declare_import(m, "airl_path_join", s1.clone());
        let path_parent = declare_import(m, "airl_path_parent", s1.clone());
        let path_filename = declare_import(m, "airl_path_filename", s1.clone());
        let path_extension = declare_import(m, "airl_path_extension", s1.clone());
        let is_absolute = declare_import(m, "airl_is_absolute", s1.clone());
        let regex_match = declare_import(m, "airl_regex_match", s2.clone());
        let regex_find_all = declare_import(m, "airl_regex_find_all", s2.clone());
        let regex_replace = declare_import(m, "airl_regex_replace", sig_3_ptr(m));
        let regex_split = declare_import(m, "airl_regex_split", s2.clone());
        let sha256 = declare_import(m, "airl_sha256", s1.clone());
        let hmac_sha256 = declare_import(m, "airl_hmac_sha256", s2.clone());
        let base64_encode = declare_import(m, "airl_base64_encode", s1.clone());
        let base64_decode = declare_import(m, "airl_base64_decode", s1.clone());
        let random_bytes = declare_import(m, "airl_random_bytes", s1.clone());
        let string_to_float = declare_import(m, "airl_string_to_float", s1.clone());

        // Crypto (byte-oriented)
        let sha512 = declare_import(m, "airl_sha512", s1.clone());
        let hmac_sha512 = declare_import(m, "airl_hmac_sha512", s2.clone());
        let sha256_bytes = declare_import(m, "airl_sha256_bytes", s1.clone());
        let sha512_bytes = declare_import(m, "airl_sha512_bytes", s1.clone());
        let hmac_sha256_bytes = declare_import(m, "airl_hmac_sha256_bytes", s2.clone());
        let hmac_sha512_bytes = declare_import(m, "airl_hmac_sha512_bytes", s2.clone());
        let pbkdf2_sha256 = declare_import(m, "airl_pbkdf2_sha256", sig_4_ptr(m));
        let pbkdf2_sha512 = declare_import(m, "airl_pbkdf2_sha512", sig_4_ptr(m));
        let base64_decode_bytes = declare_import(m, "airl_base64_decode_bytes", s1.clone());
        let base64_encode_bytes = declare_import(m, "airl_base64_encode_bytes", s1.clone());
        let bitwise_xor = declare_import(m, "airl_bitwise_xor", s2.clone());
        let bitwise_and = declare_import(m, "airl_bitwise_and", s2.clone());
        let bitwise_or = declare_import(m, "airl_bitwise_or", s2.clone());
        let bitwise_shr = declare_import(m, "airl_bitwise_shr", s2.clone());
        let bitwise_shl = declare_import(m, "airl_bitwise_shl", s2.clone());

        // Byte encoding
        let bytes_from_int16 = declare_import(m, "airl_bytes_from_int16", s1.clone());
        let bytes_from_int32 = declare_import(m, "airl_bytes_from_int32", s1.clone());
        let bytes_from_int64 = declare_import(m, "airl_bytes_from_int64", s1.clone());
        let bytes_to_int16 = declare_import(m, "airl_bytes_to_int16", s2.clone());
        let bytes_to_int32 = declare_import(m, "airl_bytes_to_int32", s2.clone());
        let bytes_to_int64 = declare_import(m, "airl_bytes_to_int64", s2.clone());
        let bytes_from_string = declare_import(m, "airl_bytes_from_string", s1.clone());
        let bytes_to_string = declare_import(m, "airl_bytes_to_string", sig_3_ptr(m));
        let bytes_concat = declare_import(m, "airl_bytes_concat", s2.clone());
        let bytes_slice = declare_import(m, "airl_bytes_slice", sig_3_ptr(m));
        let crc32c = declare_import(m, "airl_crc32c", s1.clone());

        // Compression
        let gzip_compress = declare_import(m, "airl_gzip_compress", s1.clone());
        let gzip_decompress = declare_import(m, "airl_gzip_decompress", s1.clone());
        let snappy_compress = declare_import(m, "airl_snappy_compress", s1.clone());
        let snappy_decompress = declare_import(m, "airl_snappy_decompress", s1.clone());
        let lz4_compress = declare_import(m, "airl_lz4_compress", s1.clone());
        let lz4_decompress = declare_import(m, "airl_lz4_decompress", s1.clone());
        let zstd_compress = declare_import(m, "airl_zstd_compress", s1.clone());
        let zstd_decompress = declare_import(m, "airl_zstd_decompress", s1.clone());

        // TCP sockets
        let tcp_connect = declare_import(m, "airl_tcp_connect", s2.clone());
        let tcp_close = declare_import(m, "airl_tcp_close", s1.clone());
        let tcp_send = declare_import(m, "airl_tcp_send", s2.clone());
        let tcp_recv = declare_import(m, "airl_tcp_recv", s2.clone());
        let tcp_recv_exact = declare_import(m, "airl_tcp_recv_exact", s2.clone());
        let tcp_set_timeout = declare_import(m, "airl_tcp_set_timeout", s2.clone());
        let mut sig5 = m.make_signature();
        for _ in 0..5 { sig5.params.push(AbiParam::new(PTR)); }
        sig5.returns.push(AbiParam::new(PTR));
        let tcp_connect_tls = declare_import(m, "airl_tcp_connect_tls", sig5);

        // Contract failure: (i64, i64, i64) -> i64
        let mut cf_sig = m.make_signature();
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.returns.push(AbiParam::new(types::I64));
        let contract_fail = declare_import(m, "airl_jit_contract_fail", cf_sig);

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
            print, println, print_values, type_of, valid,
            char_at, substring, chars, split, join, contains, starts_with,
            ends_with, index_of, trim, to_upper, to_lower, replace,
            map_new, map_from, map_get, map_get_or, map_set, map_has,
            map_remove, map_keys, map_values, map_size,
            read_file, write_file, file_exists,
            append_file, delete_file, delete_dir, rename_file,
            read_dir, create_dir, file_size, is_dir,
            get_args, run_bytecode, compile_bc_to_exe,
            char_count, str_variadic, format_variadic,
            assert_fn, panic_fn, exit_fn, sleep_fn, format_time, read_lines,
            concat_lists, range_fn, reverse_list, take_fn, drop_fn, zip_fn,
            flatten_fn, enumerate_fn,
            map_ho, filter_ho, fold_ho, sort_ho, any_ho, all_ho, find_ho,
            path_join, path_parent, path_filename, path_extension, is_absolute,
            regex_match, regex_find_all, regex_replace, regex_split,
            sha256, hmac_sha256, base64_encode, base64_decode, random_bytes,
            string_to_float,
            sha512, hmac_sha512, sha256_bytes, sha512_bytes,
            hmac_sha256_bytes, hmac_sha512_bytes,
            pbkdf2_sha256, pbkdf2_sha512,
            base64_decode_bytes, base64_encode_bytes,
            bitwise_xor, bitwise_and, bitwise_or, bitwise_shr, bitwise_shl,
            bytes_from_int16, bytes_from_int32, bytes_from_int64,
            bytes_to_int16, bytes_to_int32, bytes_to_int64,
            bytes_from_string, bytes_to_string, bytes_concat, bytes_slice, crc32c,
            gzip_compress, gzip_decompress, snappy_compress, snappy_decompress,
            lz4_compress, lz4_decompress, zstd_compress, zstd_decompress,
            tcp_connect, tcp_close, tcp_send, tcp_recv, tcp_recv_exact, tcp_set_timeout, tcp_connect_tls,
            contract_fail,
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
        m.insert("println".into(), rt.println);
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

        // File I/O
        m.insert("read-file".into(),    rt.read_file);
        m.insert("write-file".into(),   rt.write_file);
        m.insert("file-exists?".into(), rt.file_exists);
        m.insert("append-file".into(),  rt.append_file);
        m.insert("delete-file".into(),  rt.delete_file);
        m.insert("delete-dir".into(),   rt.delete_dir);
        m.insert("rename-file".into(),  rt.rename_file);
        m.insert("read-dir".into(),     rt.read_dir);
        m.insert("create-dir".into(),   rt.create_dir);
        m.insert("file-size".into(),    rt.file_size);
        m.insert("is-dir?".into(),      rt.is_dir);
        m.insert("get-args".into(),     rt.get_args);
        m.insert("run-bytecode".into(), rt.run_bytecode);
        m.insert("compile-bytecode-to-executable".into(), rt.compile_bc_to_exe);

        // Misc builtins
        m.insert("char-count".into(),    rt.char_count);
        m.insert("assert".into(),        rt.assert_fn);
        m.insert("panic".into(),         rt.panic_fn);
        m.insert("exit".into(),          rt.exit_fn);
        m.insert("sleep".into(),         rt.sleep_fn);
        m.insert("format-time".into(),   rt.format_time);
        m.insert("read-lines".into(),    rt.read_lines);
        m.insert("concat".into(),        rt.concat_lists);
        m.insert("range".into(),         rt.range_fn);
        m.insert("reverse".into(),       rt.reverse_list);
        m.insert("take".into(),          rt.take_fn);
        m.insert("drop".into(),          rt.drop_fn);
        m.insert("zip".into(),           rt.zip_fn);
        m.insert("flatten".into(),       rt.flatten_fn);
        m.insert("enumerate".into(),     rt.enumerate_fn);
        m.insert("path-join".into(),     rt.path_join);
        m.insert("path-parent".into(),   rt.path_parent);
        m.insert("path-filename".into(), rt.path_filename);
        m.insert("path-extension".into(),rt.path_extension);
        m.insert("is-absolute?".into(),  rt.is_absolute);
        m.insert("regex-match".into(),   rt.regex_match);
        m.insert("regex-find-all".into(),rt.regex_find_all);
        m.insert("regex-replace".into(), rt.regex_replace);
        m.insert("regex-split".into(),   rt.regex_split);
        m.insert("sha256".into(),        rt.sha256);
        m.insert("hmac-sha256".into(),   rt.hmac_sha256);
        m.insert("base64-encode".into(), rt.base64_encode);
        m.insert("base64-decode".into(), rt.base64_decode);
        m.insert("random-bytes".into(),  rt.random_bytes);
        m.insert("string-to-float".into(), rt.string_to_float);
        m.insert("sha512".into(),              rt.sha512);
        m.insert("hmac-sha512".into(),         rt.hmac_sha512);
        m.insert("sha256-bytes".into(),        rt.sha256_bytes);
        m.insert("sha512-bytes".into(),        rt.sha512_bytes);
        m.insert("hmac-sha256-bytes".into(),   rt.hmac_sha256_bytes);
        m.insert("hmac-sha512-bytes".into(),   rt.hmac_sha512_bytes);
        m.insert("pbkdf2-sha256".into(),       rt.pbkdf2_sha256);
        m.insert("pbkdf2-sha512".into(),       rt.pbkdf2_sha512);
        m.insert("base64-decode-bytes".into(), rt.base64_decode_bytes);
        m.insert("base64-encode-bytes".into(), rt.base64_encode_bytes);
        m.insert("bitwise-xor".into(),         rt.bitwise_xor);
        m.insert("bitwise-and".into(),         rt.bitwise_and);
        m.insert("bitwise-or".into(),          rt.bitwise_or);
        m.insert("bitwise-shr".into(),         rt.bitwise_shr);
        m.insert("bitwise-shl".into(),         rt.bitwise_shl);
        m.insert("str".into(),           rt.str_variadic);
        m.insert("format".into(),        rt.format_variadic);

        // Byte encoding
        m.insert("bytes-from-int16".into(),  rt.bytes_from_int16);
        m.insert("bytes-from-int32".into(),  rt.bytes_from_int32);
        m.insert("bytes-from-int64".into(),  rt.bytes_from_int64);
        m.insert("bytes-to-int16".into(),    rt.bytes_to_int16);
        m.insert("bytes-to-int32".into(),    rt.bytes_to_int32);
        m.insert("bytes-to-int64".into(),    rt.bytes_to_int64);
        m.insert("bytes-from-string".into(), rt.bytes_from_string);
        m.insert("bytes-to-string".into(),   rt.bytes_to_string);
        m.insert("bytes-concat".into(),      rt.bytes_concat);
        m.insert("bytes-slice".into(),       rt.bytes_slice);
        m.insert("crc32c".into(),            rt.crc32c);

        // Compression
        m.insert("gzip-compress".into(),    rt.gzip_compress);
        m.insert("gzip-decompress".into(),  rt.gzip_decompress);
        m.insert("snappy-compress".into(),  rt.snappy_compress);
        m.insert("snappy-decompress".into(),rt.snappy_decompress);
        m.insert("lz4-compress".into(),     rt.lz4_compress);
        m.insert("lz4-decompress".into(),   rt.lz4_decompress);
        m.insert("zstd-compress".into(),    rt.zstd_compress);
        m.insert("zstd-decompress".into(),  rt.zstd_decompress);

        // TCP sockets
        m.insert("tcp-connect".into(),     rt.tcp_connect);
        m.insert("tcp-close".into(),       rt.tcp_close);
        m.insert("tcp-send".into(),        rt.tcp_send);
        m.insert("tcp-recv".into(),        rt.tcp_recv);
        m.insert("tcp-recv-exact".into(),  rt.tcp_recv_exact);
        m.insert("tcp-set-timeout".into(), rt.tcp_set_timeout);
        m.insert("tcp-connect-tls".into(), rt.tcp_connect_tls);

        // NOTE: map/filter/fold/sort/any/all/find resolve to AIRL stdlib
        // definitions, not extern C functions. See bytecode_aot.rs comment.

        m
    }

    // ──────────────────────────────────────────────────────────────────────
    // Value marshaling: interpreter Value ↔ airl_rt RtValue
    // ──────────────────────────────────────────────────────────────────────

    /// Convert an interpreter `Value` into a heap-allocated `RtValue`.
    /// The caller owns one reference (rc=1).
    pub fn value_to_rt(v: &Value) -> *mut RtValue {
        Self::value_to_rt_with_compiled(v, None)
    }

    /// Like `value_to_rt`, but with access to the compiled function map so that
    /// `BytecodeClosure` values can be converted to proper `RtData::Closure`.
    pub fn value_to_rt_with_compiled(v: &Value, compiled: Option<&HashMap<String, *const u8>>) -> *mut RtValue {
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
                let ptrs: Vec<*mut RtValue> = items.iter().map(|i| Self::value_to_rt_with_compiled(i, compiled)).collect();
                rt_list(ptrs)
            }
            Value::IntList(ints) => {
                // Promote to boxed list for JIT interop
                let ptrs: Vec<*mut RtValue> = ints.iter().map(|n| Self::value_to_rt_with_compiled(&Value::Int(*n), compiled)).collect();
                rt_list(ptrs)
            }
            Value::Tuple(items) => {
                let ptrs: Vec<*mut RtValue> = items.iter().map(|i| Self::value_to_rt_with_compiled(i, compiled)).collect();
                rt_list(ptrs)
            }
            Value::Variant(tag, inner) => {
                let inner_ptr = Self::value_to_rt_with_compiled(inner, compiled);
                rt_variant(tag.clone(), inner_ptr)
            }
            Value::Map(map) => {
                use std::collections::HashMap as HM;
                let mut rt_map_data: HM<String, *mut RtValue> = HM::new();
                for (k, val) in map {
                    rt_map_data.insert(k.clone(), Self::value_to_rt_with_compiled(val, compiled));
                }
                rt_map(rt_map_data)
            }
            Value::BytecodeClosure(bc) => {
                // Look up the compiled function pointer for this closure
                let fn_ptr = compiled
                    .and_then(|c| c.get(&bc.func_name).copied())
                    .unwrap_or(std::ptr::null());
                // Marshal captured values
                let caps: Vec<*mut RtValue> = bc.captured.iter()
                    .map(|c| Self::value_to_rt_with_compiled(c, compiled))
                    .collect();
                let cap_ptrs: Vec<*mut RtValue> = caps.clone();
                // Call airl_make_closure to build a proper closure RtValue
                airl_rt::closure::airl_make_closure(
                    fn_ptr,
                    if cap_ptrs.is_empty() { std::ptr::null() } else { cap_ptrs.as_ptr() },
                    cap_ptrs.len(),
                )
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

        // Marshal args → RtValue pointers (pass compiled map for BytecodeClosure → Closure)
        let mut rt_args: Vec<*mut RtValue> =
            args.iter().map(|v| Self::value_to_rt_with_compiled(v, Some(&self.compiled))).collect();

        // Call through the function pointer, dispatch by arity
        let result_ptr = unsafe { Self::dispatch_call(fn_ptr, &mut rt_args) };

        // Retain the result before releasing args, because the result may alias
        // one of the input arg pointers (e.g., `max2` returns one of its args).
        if !result_ptr.is_null() {
            airl_rt::memory::airl_value_retain(result_ptr);
        }

        // Release the argument RtValues we allocated for the call.
        // SAFETY: each ptr was freshly allocated by value_to_rt; rc=1.
        for &ptr in &rt_args {
            airl_rt::memory::airl_value_release(ptr);
        }

        // rt_to_value will release the retained reference.
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

    // ──────────────────────────────────────────────────────────────────────
    // Compile orchestration
    // ──────────────────────────────────────────────────────────────────────

    /// Try to compile a function (and its call dependencies) to native code.
    /// There is no eligibility check — every function is compilable because
    /// all value operations go through runtime calls.
    /// Collect dependency-ordered compilation list (iterative, no recursion).
    fn collect_compile_order(
        &self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
    ) -> Vec<String> {
        let mut order = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![func.name.clone()];

        while let Some(name) = stack.pop() {
            if visited.contains(&name) || self.compiled.contains_key(&name) {
                continue;
            }
            visited.insert(name.clone());

            if let Some(f) = all_functions.get(&name) {
                // Check for unresolvable targets
                let mut resolvable = true;
                for instr in &f.instructions {
                    if instr.op == Op::Call || instr.op == Op::TailCall {
                        if let Value::Str(callee) = &f.constants[instr.a as usize] {
                            if callee != &f.name
                                && !self.compiled.contains_key(callee)
                                && !all_functions.contains_key(callee)
                                && !self.builtin_map.contains_key(callee)
                                && callee != "print"
                            {
                                resolvable = false;
                                break;
                            }
                        }
                    }
                }
                if !resolvable {
                    continue;
                }

                // Push dependencies (they'll be compiled first due to order reversal)
                for instr in &f.instructions {
                    if instr.op == Op::Call || instr.op == Op::MakeClosure {
                        if let Value::Str(callee) = &f.constants[instr.a as usize] {
                            if callee != &f.name && !visited.contains(callee) {
                                stack.push(callee.clone());
                            }
                        }
                    }
                }
                order.push(name);
            }
        }
        // Reverse: DFS post-order means dependencies come after their dependents.
        // We need callees compiled before callers.
        order.reverse();
        order
    }

    pub fn try_compile_full(
        &mut self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
    ) {
        // Build an extended map that includes the target function itself
        // (unit tests may pass an empty all_functions map)
        let mut extended = all_functions.clone();
        extended.entry(func.name.clone()).or_insert_with(|| func.clone());

        // Iteratively collect all functions to compile in dependency order
        let compile_order = self.collect_compile_order(func, &extended);

        for name in &compile_order {
            if self.compiled.contains_key(name) {
                continue;
            }
            let f = match extended.get(name) {
                Some(f) => f.clone(),
                None => continue,
            };
            match self.compile_func(&f, all_functions) {
                Ok(ptr) => {
                    if std::env::var("AIRL_JIT_DEBUG").as_deref() == Ok("1") {
                        eprintln!("[JIT-full] compiled {}", name);
                    }
                    self.compiled.insert(name.clone(), ptr);
                }
                Err(e) => {
                    if std::env::var("AIRL_JIT_DEBUG").as_deref() == Ok("1") {
                        eprintln!("[JIT-full] {} compile error: {}", name, e);
                    }
                }
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Core Cranelift IR emitter
    // ──────────────────────────────────────────────────────────────────────

    /// Compile a single `BytecodeFunc` to native code.  Every value is an
    /// `i64`-sized `*mut RtValue` pointer; all operations go through `airl_*`
    /// runtime helper calls.
    pub fn compile_func(&mut self, func: &BytecodeFunc, all_functions: &HashMap<String, BytecodeFunc>) -> Result<*const u8, String> {
        // ── 1. Build Cranelift signature (all params & return are I64 ptrs) ─
        let mut sig = self.module.make_signature();
        for _ in 0..func.arity {
            sig.params.push(AbiParam::new(PTR));
        }
        sig.returns.push(AbiParam::new(PTR));

        // ── 2. Declare function in JIT module ──────────────────────────────
        let func_id = self
            .module
            .declare_function(&func.name, Linkage::Local, &sig)
            .map_err(|e| format!("declare: {}", e))?;

        // ── 3. Build function body ─────────────────────────────────────────
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();

        // Pre-declare call targets for Op::Call (before builder scope).
        // If the callee is a known runtime builtin, reuse its already-imported FuncId.
        // If the callee is a user/stdlib function, declare it as Local.
        let mut call_targets: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for instr in &func.instructions {
            if instr.op == Op::Call || instr.op == Op::TailCall {
                if let Value::Str(callee_name) = &func.constants[instr.a as usize] {
                    if callee_name != &func.name && !call_targets.contains_key(callee_name) {
                        let argc = instr.b as usize;
                        // "print" with argc != 1 is variadic — handled inline
                        // via airl_print_values, so skip pre-declaration.
                        let is_variadic_print = callee_name == "print" && argc != 1;
                        if is_variadic_print {
                            // no-op: handled inline in Op::Call emission
                        } else if let Some(&builtin_id) = self.builtin_map.get(callee_name.as_str()) {
                            call_targets.insert(callee_name.clone(), builtin_id);
                        } else {
                            let mut call_sig = self.module.make_signature();
                            for _ in 0..argc {
                                call_sig.params.push(AbiParam::new(PTR));
                            }
                            call_sig.returns.push(AbiParam::new(PTR));
                            let callee_id = self.module
                                .declare_function(callee_name, Linkage::Local, &call_sig)
                                .map_err(|e| format!("call declare: {}", e))?;
                            call_targets.insert(callee_name.clone(), callee_id);
                        }
                    }
                }
            }
        }

        let instrs = &func.instructions;
        let reg_count = func.register_count as usize;

        // ── Pass 1: Find basic block boundaries ────────────────────────────
        let mut block_starts: BTreeSet<usize> = BTreeSet::new();
        block_starts.insert(0);

        for (i, instr) in instrs.iter().enumerate() {
            match instr.op {
                Op::Jump => {
                    let offset = instr.a as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                }
                Op::JumpIfFalse | Op::JumpIfTrue => {
                    let offset = instr.b as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                    block_starts.insert(i + 1);
                }
                Op::JumpIfNoMatch => {
                    let offset = instr.a as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                    block_starts.insert(i + 1);
                }
                Op::TryUnwrap => {
                    // err_offset creates a branch target
                    let offset = instr.b as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                    block_starts.insert(i + 1);
                }
                Op::Return | Op::TailCall => {
                    // Terminators: code after them (if any) must start a new block.
                    if i + 1 < instrs.len() {
                        block_starts.insert(i + 1);
                    }
                }
                _ => {}
            }
        }

        // Map instruction-index → Cranelift Block
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let mut index_to_block: HashMap<usize, ir::Block> = HashMap::new();
        for &start in &block_starts {
            let blk = builder.create_block();
            index_to_block.insert(start, blk);
        }

        // Entry block receives function parameters.
        let entry_block = index_to_block[&0];
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        // ── Declare Cranelift Variables for every bytecode register ─────────
        let mut vars: Vec<Variable> = Vec::with_capacity(reg_count + 1);
        for _ in 0..reg_count {
            let var = builder.declare_var(PTR);
            vars.push(var);
        }
        // Extra variable for match_flag (used by MatchTag / JumpIfNoMatch)
        let match_flag_var = builder.declare_var(types::I64);

        // Bind function params to the first `arity` variables.
        {
            let params: Vec<ir::Value> = builder.block_params(entry_block).to_vec();
            for (i, &param_val) in params.iter().enumerate() {
                if i < func.arity as usize {
                    builder.def_var(vars[i], param_val);
                }
            }
        }
        // Initialize remaining registers to null (0).
        for r in func.arity as usize..reg_count {
            let zero = builder.ins().iconst(PTR, 0);
            builder.def_var(vars[r], zero);
        }
        // Initialize match_flag to 0.
        {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(match_flag_var, zero);
        }

        // ── Create loop_block for TailCall back-edges ──────────────────────
        let loop_block = builder.create_block();
        index_to_block.insert(0, loop_block);
        builder.ins().jump(loop_block, &[]);
        builder.switch_to_block(loop_block);
        let mut last_was_terminator = true;

        // ── Pass 2: Emit IR for each instruction ───────────────────────────
        for (i, instr) in instrs.iter().enumerate() {
            // Block boundary — emit fallthrough if needed.
            if let Some(&blk) = index_to_block.get(&i) {
                if !last_was_terminator {
                    builder.ins().jump(blk, &[]);
                }
                builder.switch_to_block(blk);
                last_was_terminator = false;
            }

            match instr.op {
                // ── Literals ────────────────────────────────────────────
                Op::LoadConst => {
                    let dst = instr.dst as usize;
                    let cidx = instr.a as usize;
                    match &func.constants[cidx] {
                        Value::Int(n) => {
                            let int_ref = self.module.declare_func_in_func(self.rt.int_ctor, builder.func);
                            let n_val = builder.ins().iconst(types::I64, *n);
                            let call = builder.ins().call(int_ref, &[n_val]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
                        }
                        Value::Float(f) => {
                            let float_ref = self.module.declare_func_in_func(self.rt.float_ctor, builder.func);
                            let f_val = builder.ins().f64const(*f);
                            let call = builder.ins().call(float_ref, &[f_val]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
                        }
                        Value::Bool(b) => {
                            let bool_ref = self.module.declare_func_in_func(self.rt.bool_ctor, builder.func);
                            let b_val = builder.ins().iconst(types::I64, *b as i64);
                            let call = builder.ins().call(bool_ref, &[b_val]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
                        }
                        Value::Str(s) => {
                            let (sptr, slen) = self.intern_string(s);
                            let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                            let ptr_val = builder.ins().iconst(types::I64, sptr as i64);
                            let len_val = builder.ins().iconst(types::I64, slen as i64);
                            let call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
                        }
                        _ => {
                            // Unsupported constant type — load nil
                            let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
                            let call = builder.ins().call(nil_ref, &[]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
                        }
                    }
                    last_was_terminator = false;
                }

                Op::LoadNil => {
                    let dst = instr.dst as usize;
                    let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
                    let call = builder.ins().call(nil_ref, &[]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                Op::LoadTrue => {
                    let dst = instr.dst as usize;
                    let bool_ref = self.module.declare_func_in_func(self.rt.bool_ctor, builder.func);
                    let one = builder.ins().iconst(types::I64, 1);
                    let call = builder.ins().call(bool_ref, &[one]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                Op::LoadFalse => {
                    let dst = instr.dst as usize;
                    let bool_ref = self.module.declare_func_in_func(self.rt.bool_ctor, builder.func);
                    let zero = builder.ins().iconst(types::I64, 0);
                    let call = builder.ins().call(bool_ref, &[zero]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                Op::Move => {
                    let dst = instr.dst as usize;
                    let src = instr.a as usize;
                    let v = builder.use_var(vars[src]);
                    builder.def_var(vars[dst], v);
                    last_was_terminator = false;
                }

                // ── Arithmetic ──────────────────────────────────────────
                Op::Add => {
                    let dst = instr.dst as usize;
                    let add_ref = self.module.declare_func_in_func(self.rt.add, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(add_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Sub => {
                    let dst = instr.dst as usize;
                    let sub_ref = self.module.declare_func_in_func(self.rt.sub, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(sub_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Mul => {
                    let dst = instr.dst as usize;
                    let mul_ref = self.module.declare_func_in_func(self.rt.mul, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(mul_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Div => {
                    let dst = instr.dst as usize;
                    let div_ref = self.module.declare_func_in_func(self.rt.div, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(div_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Mod => {
                    let dst = instr.dst as usize;
                    let mod_ref = self.module.declare_func_in_func(self.rt.modulo, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(mod_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Neg => {
                    let dst = instr.dst as usize;
                    // Negate by computing 0 - a
                    let int_ref = self.module.declare_func_in_func(self.rt.int_ctor, builder.func);
                    let zero_raw = builder.ins().iconst(types::I64, 0);
                    let call_zero = builder.ins().call(int_ref, &[zero_raw]);
                    let zero_ptr = builder.inst_results(call_zero)[0];
                    let sub_ref = self.module.declare_func_in_func(self.rt.sub, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let call = builder.ins().call(sub_ref, &[zero_ptr, va]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Comparison ──────────────────────────────────────────
                Op::Eq => {
                    let dst = instr.dst as usize;
                    let eq_ref = self.module.declare_func_in_func(self.rt.eq, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(eq_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Ne => {
                    let dst = instr.dst as usize;
                    let ne_ref = self.module.declare_func_in_func(self.rt.ne, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(ne_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Lt => {
                    let dst = instr.dst as usize;
                    let lt_ref = self.module.declare_func_in_func(self.rt.lt, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(lt_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Le => {
                    let dst = instr.dst as usize;
                    let le_ref = self.module.declare_func_in_func(self.rt.le, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(le_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Gt => {
                    let dst = instr.dst as usize;
                    let gt_ref = self.module.declare_func_in_func(self.rt.gt, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(gt_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Ge => {
                    let dst = instr.dst as usize;
                    let ge_ref = self.module.declare_func_in_func(self.rt.ge, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let vb = builder.use_var(vars[instr.b as usize]);
                    let call = builder.ins().call(ge_ref, &[va, vb]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Logic ───────────────────────────────────────────────
                Op::Not => {
                    let dst = instr.dst as usize;
                    let not_ref = self.module.declare_func_in_func(self.rt.not, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let call = builder.ins().call(not_ref, &[va]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Control flow ────────────────────────────────────────
                Op::Jump => {
                    let offset = instr.a as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let target_blk = index_to_block[&target_idx];
                    builder.ins().jump(target_blk, &[]);
                    last_was_terminator = true;
                }

                Op::JumpIfFalse => {
                    let cond_reg = instr.a as usize;
                    let offset = instr.b as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];
                    // Extract raw bool from boxed value
                    let as_bool_ref = self.module.declare_func_in_func(self.rt.as_bool_raw, builder.func);
                    let cond_ptr = builder.use_var(vars[cond_reg]);
                    let call = builder.ins().call(as_bool_ref, &[cond_ptr]);
                    let raw = builder.inst_results(call)[0];
                    // brif: first block if nonzero; JumpIfFalse = jump when zero → target is second.
                    builder.ins().brif(raw, fallthrough_blk, &[], target_blk, &[]);
                    last_was_terminator = true;
                }

                Op::JumpIfTrue => {
                    let cond_reg = instr.a as usize;
                    let offset = instr.b as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];
                    let as_bool_ref = self.module.declare_func_in_func(self.rt.as_bool_raw, builder.func);
                    let cond_ptr = builder.use_var(vars[cond_reg]);
                    let call = builder.ins().call(as_bool_ref, &[cond_ptr]);
                    let raw = builder.inst_results(call)[0];
                    // brif: first block if nonzero → target is first.
                    builder.ins().brif(raw, target_blk, &[], fallthrough_blk, &[]);
                    last_was_terminator = true;
                }

                Op::Return => {
                    let src = instr.a as usize;
                    let v = builder.use_var(vars[src]);
                    builder.ins().return_(&[v]);
                    last_was_terminator = true;
                }

                // ── Function calls ──────────────────────────────────────
                Op::Call => {
                    let callee_name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("call: func name must be string".into()),
                    };
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;

                    // Variadic print: pack args onto stack slot, call airl_print_values
                    if callee_name == "print" && argc != 1 && !call_targets.contains_key("print") {
                        // Allocate a stack slot for the argument array (argc pointers)
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            ir::StackSlotKind::ExplicitSlot,
                            (argc as u32) * 8,
                            3, // align 8
                        ));
                        for j in 0..argc {
                            let arg = builder.use_var(vars[dst + 1 + j]);
                            builder.ins().stack_store(arg, slot, (j as i32) * 8);
                        }
                        let slot_addr = builder.ins().stack_addr(PTR, slot, 0);
                        let count_val = builder.ins().iconst(types::I64, argc as i64);
                        let pv_ref = self.module.declare_func_in_func(self.rt.print_values, builder.func);
                        let call = builder.ins().call(pv_ref, &[slot_addr, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                        last_was_terminator = false;
                    } else {
                        let callee_func_id = if callee_name == func.name {
                            func_id
                        } else if let Some(&id) = call_targets.get(&callee_name) {
                            id
                        } else {
                            return Err(format!("call target '{}' not declared", callee_name));
                        };
                        let func_ref = self.module.declare_func_in_func(callee_func_id, builder.func);

                        let mut call_args = Vec::new();
                        for j in 0..argc {
                            let arg = builder.use_var(vars[dst + 1 + j]);
                            call_args.push(arg);
                        }
                        let call = builder.ins().call(func_ref, &call_args);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                        last_was_terminator = false;
                    }
                }

                Op::TailCall => {
                    let callee_name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("tailcall: func name must be string".into()),
                    };
                    if callee_name != func.name {
                        return Err(format!("cross-function TailCall to '{}' not supported", callee_name));
                    }
                    builder.ins().jump(loop_block, &[]);
                    last_was_terminator = true;
                }

                // ── CallBuiltin ─────────────────────────────────────────
                Op::CallBuiltin => {
                    let name_idx = instr.a as usize;
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;
                    let builtin_name = match &func.constants[name_idx] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("callbuiltin: name must be string".into()),
                    };
                    // Variadic print via CallBuiltin
                    if builtin_name == "print" && argc != 1 {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            ir::StackSlotKind::ExplicitSlot,
                            (argc as u32) * 8,
                            3,
                        ));
                        for j in 0..argc {
                            let arg = builder.use_var(vars[dst + 1 + j]);
                            builder.ins().stack_store(arg, slot, (j as i32) * 8);
                        }
                        let slot_addr = builder.ins().stack_addr(PTR, slot, 0);
                        let count_val = builder.ins().iconst(types::I64, argc as i64);
                        let pv_ref = self.module.declare_func_in_func(self.rt.print_values, builder.func);
                        let call = builder.ins().call(pv_ref, &[slot_addr, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else if let Some(&builtin_func_id) = self.builtin_map.get(&builtin_name) {
                        let builtin_ref = self.module.declare_func_in_func(builtin_func_id, builder.func);
                        let mut call_args = Vec::new();
                        for j in 0..argc {
                            let arg = builder.use_var(vars[dst + 1 + j]);
                            call_args.push(arg);
                        }
                        let call = builder.ins().call(builtin_ref, &call_args);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else {
                        // Unknown builtin — fail loudly instead of silently returning nil
                        return Err(format!("JIT: unregistered builtin '{}'. Add to build_builtin_map() in bytecode_jit_full.rs", builtin_name));
                    }
                    last_was_terminator = false;
                }

                // ── CallReg (closure call) ──────────────────────────────
                Op::CallReg => {
                    let dst = instr.dst as usize;
                    let callee_reg = instr.a as usize;
                    let argc = instr.b as usize;

                    let call_closure_ref = self.module.declare_func_in_func(self.rt.call_closure, builder.func);

                    if argc > 0 {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            (argc * 8) as u32,
                            0,
                        ));
                        let base = builder.ins().stack_addr(PTR, slot, 0);
                        for j in 0..argc {
                            let val = builder.use_var(vars[dst + 1 + j]);
                            let offset = (j * 8) as i32;
                            builder.ins().store(MemFlags::new(), val, base, offset);
                        }
                        let argc_val = builder.ins().iconst(types::I64, argc as i64);
                        let closure_val = builder.use_var(vars[callee_reg]);
                        let call = builder.ins().call(call_closure_ref, &[closure_val, base, argc_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else {
                        // Zero args — pass null ptr and 0
                        let null = builder.ins().iconst(PTR, 0);
                        let zero = builder.ins().iconst(types::I64, 0);
                        let closure_val = builder.use_var(vars[callee_reg]);
                        let call = builder.ins().call(call_closure_ref, &[closure_val, null, zero]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    }
                    last_was_terminator = false;
                }

                // ── Data operations ─────────────────────────────────────
                Op::MakeList => {
                    let dst = instr.dst as usize;
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let list_new_ref = self.module.declare_func_in_func(self.rt.list_new, builder.func);

                    if count > 0 {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            (count * 8) as u32,
                            0,
                        ));
                        let base = builder.ins().stack_addr(PTR, slot, 0);
                        for j in 0..count {
                            let val = builder.use_var(vars[start + j]);
                            let offset = (j * 8) as i32;
                            builder.ins().store(MemFlags::new(), val, base, offset);
                        }
                        let count_val = builder.ins().iconst(types::I64, count as i64);
                        let call = builder.ins().call(list_new_ref, &[base, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else {
                        let null = builder.ins().iconst(PTR, 0);
                        let zero = builder.ins().iconst(types::I64, 0);
                        let call = builder.ins().call(list_new_ref, &[null, zero]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    }
                    last_was_terminator = false;
                }

                Op::MakeVariant => {
                    let dst = instr.dst as usize;
                    let tag_idx = instr.a as usize;
                    let inner_reg = instr.b as usize;
                    let tag_src = match &func.constants[tag_idx] {
                        Value::Str(s) => s.as_str(),
                        _ => return Err("MakeVariant: tag must be string".into()),
                    };
                    let (sptr, slen) = self.intern_string(tag_src);
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let ptr_val = builder.ins().iconst(types::I64, sptr as i64);
                    let len_val = builder.ins().iconst(types::I64, slen as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let mv_ref = self.module.declare_func_in_func(self.rt.make_variant, builder.func);
                    let inner_val = builder.use_var(vars[inner_reg]);
                    let call = builder.ins().call(mv_ref, &[tag_rt, inner_val]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                Op::MakeVariant0 => {
                    let dst = instr.dst as usize;
                    let tag_idx = instr.a as usize;
                    let tag_src = match &func.constants[tag_idx] {
                        Value::Str(s) => s.as_str(),
                        _ => return Err("MakeVariant0: tag must be string".into()),
                    };
                    let (sptr, slen) = self.intern_string(tag_src);
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let ptr_val = builder.ins().iconst(types::I64, sptr as i64);
                    let len_val = builder.ins().iconst(types::I64, slen as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let unit_ref = self.module.declare_func_in_func(self.rt.unit_ctor, builder.func);
                    let unit_call = builder.ins().call(unit_ref, &[]);
                    let unit_val = builder.inst_results(unit_call)[0];

                    let mv_ref = self.module.declare_func_in_func(self.rt.make_variant, builder.func);
                    let call = builder.ins().call(mv_ref, &[tag_rt, unit_val]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                Op::MakeClosure => {
                    let dst = instr.dst as usize;
                    let func_idx = instr.a as usize;
                    let capture_start = instr.b as usize;

                    let closure_func_name = match &func.constants[func_idx] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("MakeClosure: func name must be string".into()),
                    };

                    // Get the compiled function pointer (should already be compiled)
                    let fn_ptr = self.compiled.get(&closure_func_name)
                        .copied()
                        .unwrap_or(std::ptr::null());

                    // Get capture count from the target function
                    let capture_count = all_functions.get(&closure_func_name)
                        .map(|f| f.capture_count as usize)
                        .unwrap_or(0);

                    let make_closure_ref = self.module.declare_func_in_func(self.rt.make_closure, builder.func);
                    let fn_ptr_val = builder.ins().iconst(PTR, fn_ptr as i64);

                    if capture_count > 0 {
                        // Pack captured values into a stack slot
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            ir::StackSlotKind::ExplicitSlot,
                            (capture_count as u32) * 8,
                            3, // align 8
                        ));
                        for j in 0..capture_count {
                            let cap_val = builder.use_var(vars[capture_start + j]);
                            builder.ins().stack_store(cap_val, slot, (j as i32) * 8);
                        }
                        let cap_addr = builder.ins().stack_addr(PTR, slot, 0);
                        let count_val = builder.ins().iconst(types::I64, capture_count as i64);
                        let call = builder.ins().call(make_closure_ref, &[fn_ptr_val, cap_addr, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else {
                        let null = builder.ins().iconst(PTR, 0);
                        let zero = builder.ins().iconst(types::I64, 0);
                        let call = builder.ins().call(make_closure_ref, &[fn_ptr_val, null, zero]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    }
                    last_was_terminator = false;
                }

                // ── Pattern matching ────────────────────────────────────
                Op::MatchTag => {
                    let dst = instr.dst as usize;
                    let scrutinee_reg = instr.a as usize;
                    let tag_idx = instr.b as usize;

                    let tag_src = match &func.constants[tag_idx] {
                        Value::Str(s) => s.as_str(),
                        _ => return Err("MatchTag: tag must be string".into()),
                    };
                    let (sptr, slen) = self.intern_string(tag_src);

                    // Build tag string RtValue
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let ptr_val = builder.ins().iconst(types::I64, sptr as i64);
                    let len_val = builder.ins().iconst(types::I64, slen as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    // Call airl_match_tag — returns inner ptr on match, null on no-match
                    let mt_ref = self.module.declare_func_in_func(self.rt.match_tag, builder.func);
                    let scrutinee = builder.use_var(vars[scrutinee_reg]);
                    let call = builder.ins().call(mt_ref, &[scrutinee, tag_rt]);
                    let match_result = builder.inst_results(call)[0];

                    // Check if result is null
                    let zero = builder.ins().iconst(PTR, 0);
                    let is_null = builder.ins().icmp(ir::condcodes::IntCC::Equal, match_result, zero);
                    let is_null_i64 = builder.ins().uextend(types::I64, is_null);

                    // Create match/no-match/continue blocks
                    let match_blk = builder.create_block();
                    let nomatch_blk = builder.create_block();
                    let cont_blk = builder.create_block();

                    // brif is_null: nomatch if nonzero (null), match if zero (not null)
                    builder.ins().brif(is_null_i64, nomatch_blk, &[], match_blk, &[]);

                    // Match block: store result, set flag=1
                    builder.switch_to_block(match_blk);
                    builder.def_var(vars[dst], match_result);
                    let one = builder.ins().iconst(types::I64, 1);
                    builder.def_var(match_flag_var, one);
                    builder.ins().jump(cont_blk, &[]);

                    // No-match block: set flag=0
                    builder.switch_to_block(nomatch_blk);
                    let zero_flag = builder.ins().iconst(types::I64, 0);
                    builder.def_var(match_flag_var, zero_flag);
                    builder.ins().jump(cont_blk, &[]);

                    // Continue block
                    builder.switch_to_block(cont_blk);
                    last_was_terminator = false;
                }

                Op::JumpIfNoMatch => {
                    let offset = instr.a as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];

                    let flag = builder.use_var(match_flag_var);
                    // If flag is nonzero (matched), fallthrough; if zero (no match), jump to target.
                    builder.ins().brif(flag, fallthrough_blk, &[], target_blk, &[]);
                    last_was_terminator = true;
                }

                Op::MatchWild => {
                    let dst = instr.dst as usize;
                    let scrutinee_reg = instr.a as usize;
                    let v = builder.use_var(vars[scrutinee_reg]);
                    builder.def_var(vars[dst], v);
                    let one = builder.ins().iconst(types::I64, 1);
                    builder.def_var(match_flag_var, one);
                    last_was_terminator = false;
                }

                Op::TryUnwrap => {
                    let dst = instr.dst as usize;
                    let src_reg = instr.a as usize;
                    let err_offset = instr.b as i16 as isize;
                    let target_idx = (i as isize + 1 + err_offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];

                    // Create "Ok" tag string
                    let ok_str = "Ok";
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let ptr_val = builder.ins().iconst(types::I64, ok_str.as_ptr() as i64);
                    let len_val = builder.ins().iconst(types::I64, ok_str.len() as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let mt_ref = self.module.declare_func_in_func(self.rt.match_tag, builder.func);
                    let src_val = builder.use_var(vars[src_reg]);
                    let call = builder.ins().call(mt_ref, &[src_val, tag_rt]);
                    let match_result = builder.inst_results(call)[0];

                    let zero = builder.ins().iconst(PTR, 0);
                    let is_null = builder.ins().icmp(ir::condcodes::IntCC::Equal, match_result, zero);
                    let is_null_i64 = builder.ins().uextend(types::I64, is_null);

                    let ok_blk = builder.create_block();

                    // If null → error path; if non-null → ok path
                    builder.ins().brif(is_null_i64, target_blk, &[], ok_blk, &[]);

                    builder.switch_to_block(ok_blk);
                    builder.def_var(vars[dst], match_result);
                    builder.ins().jump(fallthrough_blk, &[]);
                    last_was_terminator = true;
                }

                // ── Contract assertions ──────────────────────────────────
                // Happy path: one conditional branch (predicted taken).
                // Sad path: call airl_jit_contract_fail, return nil.
                Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
                    let bool_reg = instr.a as usize;
                    let bool_ptr = builder.use_var(vars[bool_reg]);

                    // Unbox the boolean: airl_as_bool_raw(*mut RtValue) -> i64
                    let as_bool_ref = self.module.declare_func_in_func(self.rt.as_bool_raw, builder.func);
                    let call = builder.ins().call(as_bool_ref, &[bool_ptr]);
                    let raw_bool = builder.inst_results(call)[0];

                    let fail_blk = builder.create_block();
                    let cont_blk = builder.create_block();

                    // if raw_bool != 0 (true) → continue; else → fail
                    builder.ins().brif(raw_bool, cont_blk, &[], fail_blk, &[]);

                    // Fail block: call airl_jit_contract_fail(kind, fn_name_idx, clause_idx)
                    builder.switch_to_block(fail_blk);
                    let kind_val = builder.ins().iconst(types::I64, match instr.op {
                        Op::AssertRequires => 0,
                        Op::AssertEnsures => 1,
                        _ => 2, // Invariant
                    });
                    let fn_idx_val = builder.ins().iconst(types::I64, instr.dst as i64);
                    let clause_val = builder.ins().iconst(types::I64, instr.b as i64);
                    let fail_ref = self.module.declare_func_in_func(self.rt.contract_fail, builder.func);
                    builder.ins().call(fail_ref, &[kind_val, fn_idx_val, clause_val]);
                    // Return nil to signal failure
                    let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
                    let nil_call = builder.ins().call(nil_ref, &[]);
                    let nil_val = builder.inst_results(nil_call)[0];
                    builder.ins().return_(&[nil_val]);

                    // Continue block: contract passed
                    builder.switch_to_block(cont_blk);
                    last_was_terminator = false;
                }
                Op::MarkMoved | Op::CheckNotMoved => {
                    // No-op in JIT-full — ownership is enforced at the bytecode VM level
                }
            }
        }

        // If the last instruction didn't terminate, add implicit return nil.
        if !last_was_terminator {
            let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
            let call = builder.ins().call(nil_ref, &[]);
            let result = builder.inst_results(call)[0];
            builder.ins().return_(&[result]);
        }

        // ── Seal all blocks ────────────────────────────────────────────────
        builder.seal_all_blocks();
        builder.finalize();

        // Debug: print Cranelift IR if AIRL_JIT_DEBUG is set
        if std::env::var("AIRL_JIT_DEBUG").as_deref() == Ok("1") {
            eprintln!("[JIT-full] Cranelift IR for {}:\n{}", func.name, ctx.func.display());
        }

        // ── Define function, finalize, extract pointer ──────────────────────
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| format!("define: {}", e))?;
        self.module
            .finalize_definitions()
            .map_err(|e| format!("finalize: {}", e))?;

        let ptr = self.module.get_finalized_function(func_id);
        Ok(ptr)
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

    #[test]
    fn test_full_jit_add() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("add", &["a".into(), "b".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]));
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        assert!(jit.compiled.contains_key("add"));
        let result = jit.try_call_native("add", &[Value::Int(3), Value::Int(4)]);
        assert_eq!(result, Some(Value::Int(7)));
    }

    #[test]
    fn test_full_jit_string_concat() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("greet", &["a".into(), "b".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]));
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        let result = jit.try_call_native("greet", &[Value::Str("hello ".into()), Value::Str("world".into())]);
        assert_eq!(result, Some(Value::Str("hello world".into())));
    }

    #[test]
    fn test_full_jit_gt_only() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        // Just return the result of (> a b) — no branching
        let func = compiler.compile_function("gt2", &["a".into(), "b".into()],
            &IRNode::Call(">".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]));
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        assert!(jit.compiled.contains_key("gt2"));
        let result = jit.try_call_native("gt2", &[Value::Int(10), Value::Int(3)]);
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn test_full_jit_if_simple_true() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("ift", &[],
            &IRNode::If(
                Box::new(IRNode::Bool(true)),
                Box::new(IRNode::Int(1)),
                Box::new(IRNode::Int(2)),
            ));
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        assert!(jit.compiled.contains_key("ift"));
        assert_eq!(jit.try_call_native("ift", &[]), Some(Value::Int(1)));
    }

    #[test]
    fn test_full_jit_if_branch() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("max2", &["a".into(), "b".into()],
            &IRNode::If(
                Box::new(IRNode::Call(">".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())])),
                Box::new(IRNode::Load("a".into())),
                Box::new(IRNode::Load("b".into())),
            ));
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        assert_eq!(jit.try_call_native("max2", &[Value::Int(10), Value::Int(3)]), Some(Value::Int(10)));
        assert_eq!(jit.try_call_native("max2", &[Value::Int(2), Value::Int(8)]), Some(Value::Int(8)));
    }

    #[test]
    fn test_full_jit_list_creation() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("make", &[],
            &IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]));
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        let result = jit.try_call_native("make", &[]);
        assert_eq!(result, Some(Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])));
    }

    #[test]
    fn test_full_jit_factorial() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let body = IRNode::If(
            Box::new(IRNode::Call("<=".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)])),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Call("*".into(), vec![
                IRNode::Load("n".into()),
                IRNode::Call("fact".into(), vec![
                    IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
                ]),
            ])),
        );
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("fact", &["n".into()], &body);
        let all = HashMap::new();
        let mut jit = BytecodeJitFull::new().unwrap();
        jit.try_compile_full(&func, &all);
        assert_eq!(jit.try_call_native("fact", &[Value::Int(5)]), Some(Value::Int(120)));
    }
}
