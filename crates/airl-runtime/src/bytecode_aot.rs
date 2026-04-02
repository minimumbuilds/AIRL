// crates/airl-runtime/src/bytecode_aot.rs
//! Cranelift AOT compiler that produces a native object file (`.o`)
//! via `ObjectModule`.
//!
//!   - String constants live in data sections.
//!   - Closure function pointers use `func_addr`.
//!   - Emits a C `main()` entry point that calls `__main__()`.
//!   - `finish()` returns the object file bytes.

use std::collections::{BTreeSet, HashMap, HashSet};

use cranelift_codegen::ir::{self, condcodes::{FloatCC, IntCC}, types, AbiParam, InstBuilder, MemFlags, StackSlotData};
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::bytecode::*;
use crate::value::Value;

// ─────────────────────────────────────────────────────────────────────────────
// RuntimeImports — one FuncId per runtime function (same shape as JIT)
// ─────────────────────────────────────────────────────────────────────────────

pub struct RuntimeImports {
    // Memory
    pub value_retain:  FuncId,
    pub value_release: FuncId,
    pub value_clone:   FuncId,

    // Constructors
    pub int_ctor:   FuncId,
    pub float_ctor: FuncId,
    pub bool_ctor:  FuncId,
    pub nil_ctor:   FuncId,
    pub unit_ctor:  FuncId,
    pub str_ctor:   FuncId,
    pub bytes_ctor: FuncId,

    // Logic
    pub as_bool_raw: FuncId,
    pub as_int_raw: FuncId,

    // Arithmetic
    pub add: FuncId,
    pub sub: FuncId,
    pub mul: FuncId,
    pub div: FuncId,
    pub modulo: FuncId,

    // Comparison
    pub eq: FuncId,
    pub ne: FuncId,
    pub lt: FuncId,
    pub gt: FuncId,
    pub le: FuncId,
    pub ge: FuncId,

    // Logic
    pub not: FuncId,
    pub and: FuncId,
    pub or:  FuncId,
    pub xor: FuncId,

    // List operations
    pub head:     FuncId,
    pub tail:     FuncId,
    pub cons:     FuncId,
    pub empty:    FuncId,
    pub length:   FuncId,
    pub at:       FuncId,
    pub append:   FuncId,
    pub list_new: FuncId,
    pub at_or: FuncId,
    pub set_at: FuncId,
    pub list_contains: FuncId,

    // Variant / pattern
    pub make_variant: FuncId,
    pub match_tag:    FuncId,

    // Closure
    pub make_closure: FuncId,
    pub call_closure: FuncId,

    // I/O / misc
    pub print:        FuncId,
    pub println:      FuncId,
    pub eprint:       FuncId,
    pub eprintln:     FuncId,
    pub read_line:    FuncId,
    pub read_stdin:   FuncId,
    pub print_values: FuncId,
    pub type_of:      FuncId,
    pub valid:        FuncId,

    // String builtins
    pub char_at:     FuncId,
    pub substring:   FuncId,
    pub chars:       FuncId,
    pub split:       FuncId,
    pub join:        FuncId,
    pub replace:     FuncId,
    pub char_upper:  FuncId,
    pub char_lower:  FuncId,
    pub string_ci_eq: FuncId,

    // Map builtins
    pub map_new:    FuncId,
    pub map_get:    FuncId,
    pub map_set:    FuncId,
    pub map_has:    FuncId,
    pub map_remove: FuncId,
    pub map_keys:   FuncId,

    // File I/O
    pub read_file:   FuncId,
    pub write_file:  FuncId,
    pub file_exists: FuncId,
    pub append_file: FuncId,
    pub delete_file: FuncId,
    pub delete_dir:  FuncId,
    pub rename_file: FuncId,
    pub read_dir:    FuncId,
    pub create_dir:  FuncId,
    pub file_size:   FuncId,
    pub is_dir:      FuncId,
    pub temp_file:   FuncId,
    pub temp_dir:    FuncId,
    pub file_mtime:  FuncId,
    pub get_args:    FuncId,
    pub run_bytecode: FuncId,
    pub compile_to_exe: FuncId,
    pub compile_bc_to_exe: FuncId,
    pub compile_bc_to_exe_with_target: FuncId,

    // Type conversions
    pub int_to_string: FuncId,
    pub float_to_string: FuncId,
    pub string_to_int: FuncId,
    pub string_to_float: FuncId,
    pub char_code: FuncId,
    pub char_from_code: FuncId,

    // Float math
    pub sqrt: FuncId,
    pub sin: FuncId,
    pub cos: FuncId,
    pub tan: FuncId,
    pub log: FuncId,
    pub exp: FuncId,
    pub floor: FuncId,
    pub ceil: FuncId,
    pub round: FuncId,
    pub float_to_int: FuncId,
    pub int_to_float: FuncId,
    pub infinity: FuncId,
    pub nan_ctor: FuncId,
    pub is_nan: FuncId,
    pub is_infinite: FuncId,

    // System
    pub cpu_count: FuncId,
    pub time_now: FuncId,

    // Environment
    pub getenv: FuncId,

    // Process
    pub shell_exec: FuncId,
    pub shell_exec_with_stdin: FuncId,

    // Radix / system utilities
    pub parse_int_radix: FuncId,
    pub int_to_string_radix: FuncId,
    pub get_cwd: FuncId,

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
    // Higher-order list ops (closure-accepting)
    pub map_ho: FuncId,
    pub filter_ho: FuncId,
    pub fold_ho: FuncId,
    pub sort_ho: FuncId,
    pub any_ho: FuncId,
    pub all_ho: FuncId,
    pub find_ho: FuncId,
    pub regex_match: FuncId,
    pub regex_find_all: FuncId,
    pub regex_replace: FuncId,
    pub regex_split: FuncId,
    pub random_bytes: FuncId,

    // Crypto (byte-oriented)
    pub sha512: FuncId,
    pub hmac_sha512: FuncId,
    pub sha512_bytes: FuncId,
    pub hmac_sha512_bytes: FuncId,
    pub pbkdf2_sha512: FuncId,
    pub bitwise_xor: FuncId,
    pub bitwise_and: FuncId,
    pub bitwise_or: FuncId,
    pub bitwise_shr: FuncId,
    pub bitwise_shl: FuncId,

    // Byte encoding
    pub bytes_alloc: FuncId,
    pub bytes_get: FuncId,
    pub bytes_set: FuncId,
    pub bytes_length: FuncId,
    pub bytes_new: FuncId,
    pub bytes_from_int8: FuncId,
    pub bytes_from_int16: FuncId,
    pub bytes_from_int32: FuncId,
    pub bytes_from_int64: FuncId,
    pub bytes_to_int16: FuncId,
    pub bytes_to_int32: FuncId,
    pub bytes_to_int64: FuncId,
    pub bytes_from_string: FuncId,
    pub bytes_to_string: FuncId,
    pub bytes_concat: FuncId,
    pub bytes_concat_all: FuncId,
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
    pub tcp_listen: FuncId,
    pub tcp_accept: FuncId,
    pub tcp_accept_tls: FuncId,

    // Threads and channels
    pub thread_spawn: FuncId,
    pub thread_join: FuncId,
    pub thread_set_affinity: FuncId,
    pub channel_new: FuncId,
    pub channel_send: FuncId,
    pub channel_recv: FuncId,
    pub channel_recv_timeout: FuncId,
    pub channel_drain: FuncId,
    pub channel_close: FuncId,

    // Contract failure
    pub contract_fail: FuncId,
}

// ─────────────────────────────────────────────────────────────────────────────
// Signature helpers (ObjectModule versions)
// ─────────────────────────────────────────────────────────────────────────────

fn ptr_type_for(triple: &target_lexicon::Triple) -> types::Type {
    if triple.pointer_width().unwrap().bits() == 32 {
        types::I32
    } else {
        types::I64
    }
}

fn sig_0_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_1_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_2_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_3_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_4_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_1_ptr_ret_i64(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn sig_1_ptr_ret_void(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig
}

fn sig_i64_ret_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_ptr_i64_ret_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_ptr_ptr_i64_ret_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(ptr));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

fn sig_f64_ret_ptr(m: &ObjectModule, ptr: types::Type) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(ptr));
    sig
}

// ─────────────────────────────────────────────────────────────────────────────
// Declare helper
// ─────────────────────────────────────────────────────────────────────────────

fn declare_import(
    module: &mut ObjectModule,
    name: &str,
    sig: cranelift_codegen::ir::Signature,
) -> FuncId {
    module
        .declare_function(name, Linkage::Import, &sig)
        .unwrap_or_else(|e| panic!("failed to declare {}: {}", name, e))
}

// ─────────────────────────────────────────────────────────────────────────────
// BytecodeAot
// ─────────────────────────────────────────────────────────────────────────────

/// Type hint for marshaling unboxed native values (same as JIT TypeHint).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeHint {
    Int,
    Float,
    Bool,
}

pub struct BytecodeAot {
    module: ObjectModule,
    rt: RuntimeImports,
    builtin_map: HashMap<String, FuncId>,
    /// Target triple (stored for linker decisions)
    target_triple: target_lexicon::Triple,
    /// Pointer type for the target (I32 for i686, I64 for x86-64)
    ptr: types::Type,
    /// String constants stored as (DataId, byte_length) in the object file's
    /// data section — replaces the JIT's heap-pointer approach.
    stable_strings: Vec<(cranelift_module::DataId, usize)>,
    /// Compiled function names → FuncId for `func_addr` in closures.
    compiled_funcs: HashMap<String, FuncId>,
    /// Functions compiled via the unboxed (primitive) path.
    eligible_funcs: HashSet<String>,
    /// Return type hints for eligible (unboxed) functions, used for
    /// reboxing when a boxed caller invokes an unboxed callee.
    eligible_return_hints: HashMap<String, TypeHint>,
    /// Functions referenced by MakeClosure — must be compiled boxed
    /// because airl_call_closure invokes them with *mut RtValue args.
    closure_targets: HashSet<String>,
}


/// Remap user function names to avoid clashes with C symbols.
/// In particular, user "main" → "__airl_user_main" so C entry point can be emitted.
fn aot_symbol_name(name: &str) -> String {
    if name == "main" {
        "__airl_user_main".to_string()
    } else {
        name.to_string()
    }
}

impl BytecodeAot {
    /// Create a new AOT compiler context. If `target` is None, targets the host.
    /// Accepted target strings: "x86-64", "i686", "i686-airlos", "x86_64-airlos", or any valid triple.
    pub fn new_with_target(target: Option<&str>) -> Result<Self, String> {
        let is_freestanding = matches!(target, Some("i686-airlos") | Some("x86_64-airlos"));
        let mut settings_builder = cranelift_codegen::settings::builder();
        let _ = settings_builder.set("unwind_info", "false");
        let _ = settings_builder.set("is_pic", if is_freestanding { "false" } else { "true" });
        let _ = settings_builder.set("enable_probestack", "false");

        let triple = match target {
            Some("x86-64") | None => target_lexicon::Triple::host(),
            Some("i686") => "i686-unknown-linux-gnu".parse::<target_lexicon::Triple>()
                .map_err(|e| format!("invalid triple: {}", e))?,
            Some("i686-airlos") => {
                let mut t = "i686-unknown-none".parse::<target_lexicon::Triple>()
                    .map_err(|e| format!("invalid triple: {}", e))?;
                t.binary_format = target_lexicon::BinaryFormat::Elf;
                t
            }
            Some("x86_64-airlos") => {
                let mut t = "x86_64-unknown-none".parse::<target_lexicon::Triple>()
                    .map_err(|e| format!("invalid triple: {}", e))?;
                t.binary_format = target_lexicon::BinaryFormat::Elf;
                t
            }
            Some("aarch64") => "aarch64-unknown-linux-gnu".parse::<target_lexicon::Triple>()
                .map_err(|e| format!("invalid triple: {}", e))?,
            Some(t) => t.parse::<target_lexicon::Triple>()
                .map_err(|e| format!("invalid target triple '{}': {}", t, e))?,
        };

        let ptr = ptr_type_for(&triple);

        let isa = cranelift_codegen::isa::lookup(triple.clone())
            .map_err(|e| format!("ISA lookup for {:?}: {}", triple, e))?
            .finish(cranelift_codegen::settings::Flags::new(settings_builder))
            .map_err(|e| format!("ISA build: {}", e))?;

        let builder = ObjectBuilder::new(
            isa,
            "airl_program",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| format!("ObjectBuilder: {}", e))?;

        let mut module = ObjectModule::new(builder);
        let rt = Self::declare_runtime_imports(&mut module, ptr);
        let builtin_map = Self::build_builtin_map(&rt);

        Ok(Self {
            module,
            rt,
            builtin_map,
            target_triple: triple,
            ptr,
            stable_strings: Vec::new(),
            compiled_funcs: HashMap::new(),
            eligible_funcs: HashSet::new(),
            eligible_return_hints: HashMap::new(),
            closure_targets: HashSet::new(),
        })
    }

    /// Create a new AOT compiler context targeting the host triple.
    pub fn new() -> Result<Self, String> {
        Self::new_with_target(None)
    }

    /// Register an `extern-c` declaration: declare the C symbol as an imported
    /// function and add it to the builtin map so that normal `Call` opcodes
    /// resolve to it.
    pub fn register_extern_c(&mut self, c_name: &str, arity: usize) {
        if self.builtin_map.contains_key(c_name) {
            return; // already registered (e.g. via runtime imports)
        }
        let mut sig = self.module.make_signature();
        for _ in 0..arity {
            sig.params.push(AbiParam::new(self.ptr));
        }
        sig.returns.push(AbiParam::new(self.ptr));
        let func_id = self.module
            .declare_function(c_name, Linkage::Import, &sig)
            .unwrap_or_else(|e| panic!("extern-c declare '{}': {}", c_name, e));
        self.builtin_map.insert(c_name.to_string(), func_id);
    }

    /// Intern a string constant into the object file's data section.
    /// Returns `(DataId, byte_length)`.
    fn intern_string(&mut self, s: &str) -> (cranelift_module::DataId, usize) {
        let name = format!(".str.{}", self.stable_strings.len());
        let data_id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .unwrap();
        let mut desc = DataDescription::new();
        desc.define(s.as_bytes().to_vec().into_boxed_slice());
        self.module.define_data(data_id, &desc).unwrap();
        let len = s.len();
        self.stable_strings.push((data_id, len));
        (data_id, len)
    }

    // ──────────────────────────────────────────────────────────────────────
    // Declare all runtime imports
    // ──────────────────────────────────────────────────────────────────────

    fn declare_runtime_imports(m: &mut ObjectModule, ptr: types::Type) -> RuntimeImports {
        let void_1 = sig_1_ptr_ret_void(m, ptr);
        let value_retain  = declare_import(m, "airl_value_retain",  void_1.clone());
        let value_release = declare_import(m, "airl_value_release", void_1);
        let value_clone   = declare_import(m, "airl_value_clone",   sig_1_ptr(m, ptr));

        let int_ctor   = declare_import(m, "airl_int",   sig_i64_ret_ptr(m, ptr));
        let float_ctor = declare_import(m, "airl_float", sig_f64_ret_ptr(m, ptr));
        let bool_ctor  = declare_import(m, "airl_bool",  sig_i64_ret_ptr(m, ptr));
        let nil_ctor   = declare_import(m, "airl_nil",   sig_0_ptr(m, ptr));
        let unit_ctor  = declare_import(m, "airl_unit",  sig_0_ptr(m, ptr));
        let str_ctor   = declare_import(m, "airl_str",   sig_ptr_i64_ret_ptr(m, ptr));
        let bytes_ctor = declare_import(m, "airl_bytes_new", sig_ptr_i64_ret_ptr(m, ptr));

        let as_bool_raw = declare_import(m, "airl_as_bool_raw", sig_1_ptr_ret_i64(m, ptr));
        let as_int_raw  = declare_import(m, "airl_as_int_raw",  sig_1_ptr_ret_i64(m, ptr));

        let s2 = sig_2_ptr(m, ptr);
        let add    = declare_import(m, "airl_add", s2.clone());
        let sub    = declare_import(m, "airl_sub", s2.clone());
        let mul    = declare_import(m, "airl_mul", s2.clone());
        let div    = declare_import(m, "airl_div", s2.clone());
        let modulo = declare_import(m, "airl_mod", s2.clone());

        let eq = declare_import(m, "airl_eq", s2.clone());
        let ne = declare_import(m, "airl_ne", s2.clone());
        let lt = declare_import(m, "airl_lt", s2.clone());
        let gt = declare_import(m, "airl_gt", s2.clone());
        let le = declare_import(m, "airl_le", s2.clone());
        let ge = declare_import(m, "airl_ge", s2.clone());

        let s1 = sig_1_ptr(m, ptr);
        let not = declare_import(m, "airl_not", s1.clone());
        let and = declare_import(m, "airl_and", s2.clone());
        let or  = declare_import(m, "airl_or",  s2.clone());
        let xor = declare_import(m, "airl_xor", s2.clone());

        let head   = declare_import(m, "airl_head",   s1.clone());
        let tail   = declare_import(m, "airl_tail",   s1.clone());
        let cons   = declare_import(m, "airl_cons",   s2.clone());
        let empty  = declare_import(m, "airl_empty",  s1.clone());
        let length = declare_import(m, "airl_length", s1.clone());
        let at     = declare_import(m, "airl_at",     s2.clone());
        let append = declare_import(m, "airl_append", s2.clone());
        let list_new = declare_import(m, "airl_list_new", sig_ptr_i64_ret_ptr(m, ptr));
        let at_or = declare_import(m, "airl_at_or", sig_3_ptr(m, ptr));
        let set_at = declare_import(m, "airl_set_at", sig_3_ptr(m, ptr));
        let list_contains = declare_import(m, "airl_list_contains", s2.clone());

        let make_variant = declare_import(m, "airl_make_variant", s2.clone());
        let match_tag    = declare_import(m, "airl_match_tag",    s2.clone());

        let make_closure = declare_import(m, "airl_make_closure", sig_ptr_ptr_i64_ret_ptr(m, ptr));
        let call_closure = declare_import(m, "airl_call_closure", sig_ptr_ptr_i64_ret_ptr(m, ptr));

        let print        = declare_import(m, "airl_print",        s1.clone());
        let println      = declare_import(m, "airl_println",      s1.clone());
        let eprint       = declare_import(m, "airl_eprint",       s1.clone());
        let eprintln     = declare_import(m, "airl_eprintln",     s1.clone());
        let read_line    = declare_import(m, "airl_read_line",    sig_0_ptr(m, ptr));
        let read_stdin   = declare_import(m, "airl_read_stdin",   sig_0_ptr(m, ptr));
        let print_values = declare_import(m, "airl_print_values", sig_ptr_i64_ret_ptr(m, ptr));
        let type_of      = declare_import(m, "airl_type_of",      s1.clone());
        let valid        = declare_import(m, "airl_valid",        s1.clone());

        let char_at     = declare_import(m, "airl_char_at",     s2.clone());
        let substring   = declare_import(m, "airl_substring",   sig_3_ptr(m, ptr));
        let chars       = declare_import(m, "airl_chars",       s1.clone());
        let split       = declare_import(m, "airl_split",       s2.clone());
        let join        = declare_import(m, "airl_join",        s2.clone());
        let replace     = declare_import(m, "airl_replace",     sig_3_ptr(m, ptr));
        let char_upper  = declare_import(m, "airl_char_upper",  s1.clone());
        let char_lower  = declare_import(m, "airl_char_lower",  s1.clone());
        let string_ci_eq = declare_import(m, "airl_string_ci_eq", s2.clone());

        let map_new    = declare_import(m, "airl_map_new",    sig_0_ptr(m, ptr));
        let map_get    = declare_import(m, "airl_map_get",    s2.clone());
        let map_set    = declare_import(m, "airl_map_set",    sig_3_ptr(m, ptr));
        let map_has    = declare_import(m, "airl_map_has",    s2.clone());
        let map_remove = declare_import(m, "airl_map_remove", s2.clone());
        let map_keys   = declare_import(m, "airl_map_keys",   s1.clone());

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
        let temp_file   = declare_import(m, "airl_temp_file",   s1.clone());
        let temp_dir    = declare_import(m, "airl_temp_dir",    s1.clone());
        let file_mtime  = declare_import(m, "airl_file_mtime",  s1.clone());
        let get_args    = declare_import(m, "airl_get_args",    sig_0_ptr(m, ptr));
        let run_bytecode = declare_import(m, "airl_run_bytecode", s1.clone());
        let compile_to_exe = declare_import(m, "airl_compile_to_executable", s2.clone());
        let compile_bc_to_exe = declare_import(m, "airl_compile_bytecode_to_executable", s2.clone());
        let compile_bc_to_exe_with_target = declare_import(m, "airl_compile_bytecode_to_executable_with_target", sig_3_ptr(m, ptr));

        // Type conversions
        let int_to_string = declare_import(m, "airl_int_to_string", s1.clone());
        let float_to_string = declare_import(m, "airl_float_to_string", s1.clone());
        let string_to_int = declare_import(m, "airl_string_to_int", s1.clone());
        let string_to_float = declare_import(m, "airl_string_to_float", s1.clone());
        let char_code = declare_import(m, "airl_char_code", s1.clone());
        let char_from_code = declare_import(m, "airl_char_from_code", s1.clone());

        // Float math
        let sqrt = declare_import(m, "airl_sqrt", s1.clone());
        let sin = declare_import(m, "airl_sin", s1.clone());
        let cos = declare_import(m, "airl_cos", s1.clone());
        let tan = declare_import(m, "airl_tan", s1.clone());
        let log = declare_import(m, "airl_log", s1.clone());
        let exp = declare_import(m, "airl_exp", s1.clone());
        let floor = declare_import(m, "airl_floor", s1.clone());
        let ceil = declare_import(m, "airl_ceil", s1.clone());
        let round = declare_import(m, "airl_round", s1.clone());
        let float_to_int = declare_import(m, "airl_float_to_int", s1.clone());
        let int_to_float = declare_import(m, "airl_int_to_float", s1.clone());
        let infinity = declare_import(m, "airl_infinity", sig_0_ptr(m, ptr));
        let nan_ctor = declare_import(m, "airl_nan", sig_0_ptr(m, ptr));
        let is_nan = declare_import(m, "airl_is_nan", s1.clone());
        let is_infinite = declare_import(m, "airl_is_infinite", s1.clone());

        // System
        let cpu_count = declare_import(m, "airl_cpu_count", sig_0_ptr(m, ptr));
        let time_now = declare_import(m, "airl_time_now", sig_0_ptr(m, ptr));

        // Environment
        let getenv = declare_import(m, "airl_getenv", s1.clone());

        // Process
        let shell_exec = declare_import(m, "airl_shell_exec", s2.clone());
        let shell_exec_with_stdin = declare_import(m, "airl_shell_exec_with_stdin", sig_3_ptr(m, ptr));
        let parse_int_radix = declare_import(m, "airl_parse_int_radix", s2.clone());
        let int_to_string_radix = declare_import(m, "airl_int_to_string_radix", s2.clone());
        let get_cwd = declare_import(m, "airl_get_cwd", sig_0_ptr(m, ptr));

        // Misc builtins
        let char_count = declare_import(m, "airl_char_count", s1.clone());
        let str_variadic = declare_import(m, "airl_str_variadic", sig_ptr_i64_ret_ptr(m, ptr));
        let format_variadic = declare_import(m, "airl_format_variadic", sig_ptr_i64_ret_ptr(m, ptr));
        let assert_fn = declare_import(m, "airl_assert", s2.clone());
        let panic_fn = declare_import(m, "airl_panic", s1.clone());
        let exit_fn = declare_import(m, "airl_exit", s1.clone());
        let sleep_fn = declare_import(m, "airl_sleep", s1.clone());
        let format_time = declare_import(m, "airl_format_time", s2.clone());
        let read_lines = declare_import(m, "airl_read_lines", s1.clone());
        // Higher-order list ops (closure + list -> result)
        let map_ho = declare_import(m, "airl_map", s2.clone());
        let filter_ho = declare_import(m, "airl_filter", s2.clone());
        let s3 = sig_3_ptr(m, ptr);
        let fold_ho = declare_import(m, "airl_fold", s3);
        let sort_ho = declare_import(m, "airl_sort", s2.clone());
        let any_ho = declare_import(m, "airl_any", s2.clone());
        let all_ho = declare_import(m, "airl_all", s2.clone());
        let find_ho = declare_import(m, "airl_find", s2.clone());
        let regex_match = declare_import(m, "airl_regex_match", s2.clone());
        let regex_find_all = declare_import(m, "airl_regex_find_all", s2.clone());
        let regex_replace = declare_import(m, "airl_regex_replace", sig_3_ptr(m, ptr));
        let regex_split = declare_import(m, "airl_regex_split", s2.clone());
        let random_bytes = declare_import(m, "airl_random_bytes", s1.clone());

        // Crypto (byte-oriented)
        let sha512 = declare_import(m, "airl_sha512", s1.clone());
        let hmac_sha512 = declare_import(m, "airl_hmac_sha512", s2.clone());
        let sha512_bytes = declare_import(m, "airl_sha512_bytes", s1.clone());
        let hmac_sha512_bytes = declare_import(m, "airl_hmac_sha512_bytes", s2.clone());
        let pbkdf2_sha512 = declare_import(m, "airl_pbkdf2_sha512", sig_4_ptr(m, ptr));
        let bitwise_xor = declare_import(m, "airl_bitwise_xor", s2.clone());
        let bitwise_and = declare_import(m, "airl_bitwise_and", s2.clone());
        let bitwise_or = declare_import(m, "airl_bitwise_or", s2.clone());
        let bitwise_shr = declare_import(m, "airl_bitwise_shr", s2.clone());
        let bitwise_shl = declare_import(m, "airl_bitwise_shl", s2.clone());

        // Byte-array intrinsics
        let bytes_alloc = declare_import(m, "airl_bytes_alloc", s1.clone());
        let bytes_get = declare_import(m, "airl_bytes_get", s2.clone());
        let bytes_set = declare_import(m, "airl_bytes_set", sig_3_ptr(m, ptr));
        let bytes_length = length; // reuses airl_length which already handles Bytes

        // Byte encoding
        let bytes_new = declare_import(m, "airl_bytes_new_empty", sig_0_ptr(m, ptr));
        let bytes_from_int8 = declare_import(m, "airl_bytes_from_int8", s1.clone());
        let bytes_from_int16 = declare_import(m, "airl_bytes_from_int16", s1.clone());
        let bytes_from_int32 = declare_import(m, "airl_bytes_from_int32", s1.clone());
        let bytes_from_int64 = declare_import(m, "airl_bytes_from_int64", s1.clone());
        let bytes_to_int16 = declare_import(m, "airl_bytes_to_int16", s2.clone());
        let bytes_to_int32 = declare_import(m, "airl_bytes_to_int32", s2.clone());
        let bytes_to_int64 = declare_import(m, "airl_bytes_to_int64", s2.clone());
        let bytes_from_string = declare_import(m, "airl_bytes_from_string", s1.clone());
        let bytes_to_string = declare_import(m, "airl_bytes_to_string", sig_3_ptr(m, ptr));
        let bytes_concat = declare_import(m, "airl_bytes_concat", s2.clone());
        let bytes_concat_all = declare_import(m, "airl_bytes_concat_all", s1.clone());
        let bytes_slice = declare_import(m, "airl_bytes_slice", sig_3_ptr(m, ptr));
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
        for _ in 0..5 { sig5.params.push(AbiParam::new(ptr)); }
        sig5.returns.push(AbiParam::new(ptr));
        let tcp_connect_tls = declare_import(m, "airl_tcp_connect_tls", sig5);
        let tcp_listen = declare_import(m, "airl_tcp_listen", s2.clone());
        let tcp_accept = declare_import(m, "airl_tcp_accept", s1.clone());
        let tcp_accept_tls = declare_import(m, "airl_tcp_accept_tls", sig_3_ptr(m, ptr));

        // Threads and channels
        let thread_spawn = declare_import(m, "airl_thread_spawn", s1.clone());
        let thread_join = declare_import(m, "airl_thread_join", s1.clone());
        let thread_set_affinity = declare_import(m, "airl_thread_set_affinity", s1.clone());
        let s0_ret = sig_0_ptr(m, ptr);
        let channel_new = declare_import(m, "airl_channel_new", s0_ret);
        let channel_send = declare_import(m, "airl_channel_send", s2.clone());
        let channel_recv = declare_import(m, "airl_channel_recv", s1.clone());
        let channel_recv_timeout = declare_import(m, "airl_channel_recv_timeout", s2.clone());
        let channel_drain = declare_import(m, "airl_channel_drain", s1.clone());
        let channel_close = declare_import(m, "airl_channel_close", s1.clone());

        // Contract failure: (kind: i64, fn_name_idx: i64, clause_idx: i64) -> i64
        let mut cf_sig = m.make_signature();
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.returns.push(AbiParam::new(types::I64));
        let contract_fail = declare_import(m, "airl_jit_contract_fail", cf_sig);

        RuntimeImports {
            value_retain, value_release, value_clone,
            int_ctor, float_ctor, bool_ctor, nil_ctor, unit_ctor, str_ctor, bytes_ctor,
            as_bool_raw, as_int_raw,
            add, sub, mul, div, modulo,
            eq, ne, lt, gt, le, ge,
            not, and, or, xor,
            head, tail, cons, empty, length, at, append, list_new, at_or, set_at, list_contains,
            make_variant, match_tag,
            make_closure, call_closure,
            print, println, eprint, eprintln, read_line, read_stdin, print_values, type_of, valid,
            char_at, substring, chars, split, join, replace,
            char_upper, char_lower, string_ci_eq,
            map_new, map_get, map_set, map_has,
            map_remove, map_keys,
            read_file, write_file, file_exists,
            append_file, delete_file, delete_dir, rename_file,
            read_dir, create_dir, file_size, is_dir,
            temp_file, temp_dir, file_mtime,
            get_args, run_bytecode, compile_to_exe, compile_bc_to_exe, compile_bc_to_exe_with_target,
            int_to_string, float_to_string, string_to_int, string_to_float,
            char_code, char_from_code,
            sqrt, sin, cos, tan, log, exp, floor, ceil, round,
            float_to_int, int_to_float, infinity, nan_ctor, is_nan, is_infinite,
            cpu_count, time_now, getenv, shell_exec, shell_exec_with_stdin,
            parse_int_radix, int_to_string_radix, get_cwd,
            char_count, str_variadic, format_variadic,
            assert_fn, panic_fn, exit_fn, sleep_fn, format_time, read_lines,
            map_ho, filter_ho, fold_ho, sort_ho, any_ho, all_ho, find_ho,
            regex_match, regex_find_all, regex_replace, regex_split,
            random_bytes,
            sha512, hmac_sha512, sha512_bytes,
            hmac_sha512_bytes,
            pbkdf2_sha512,
            bitwise_xor, bitwise_and, bitwise_or, bitwise_shr, bitwise_shl,
            bytes_alloc, bytes_get, bytes_set, bytes_length,
            bytes_new, bytes_from_int8, bytes_from_int16, bytes_from_int32, bytes_from_int64,
            bytes_to_int16, bytes_to_int32, bytes_to_int64,
            bytes_from_string, bytes_to_string, bytes_concat, bytes_concat_all, bytes_slice, crc32c,
            gzip_compress, gzip_decompress, snappy_compress, snappy_decompress,
            lz4_compress, lz4_decompress, zstd_compress, zstd_decompress,
            tcp_connect, tcp_close, tcp_send, tcp_recv, tcp_recv_exact, tcp_set_timeout, tcp_connect_tls, tcp_listen, tcp_accept, tcp_accept_tls,
            thread_spawn, thread_join, thread_set_affinity, channel_new, channel_send, channel_recv, channel_recv_timeout, channel_drain, channel_close,
            contract_fail,
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // Build the AIRL-name → FuncId map for CallBuiltin dispatch
    // ──────────────────────────────────────────────────────────────────────

    fn build_builtin_map(rt: &RuntimeImports) -> HashMap<String, FuncId> {
        let mut m = HashMap::new();

        m.insert("+".into(),  rt.add);
        m.insert("-".into(),  rt.sub);
        m.insert("*".into(),  rt.mul);
        m.insert("/".into(),  rt.div);
        m.insert("%".into(),  rt.modulo);

        m.insert("=".into(),  rt.eq);
        m.insert("!=".into(), rt.ne);
        m.insert("<".into(),  rt.lt);
        m.insert(">".into(),  rt.gt);
        m.insert("<=".into(), rt.le);
        m.insert(">=".into(), rt.ge);

        m.insert("not".into(), rt.not);
        m.insert("and".into(), rt.and);
        m.insert("or".into(),  rt.or);
        m.insert("xor".into(), rt.xor);

        m.insert("head".into(),   rt.head);
        m.insert("tail".into(),   rt.tail);
        m.insert("cons".into(),   rt.cons);
        m.insert("empty?".into(), rt.empty);
        m.insert("length".into(), rt.length);
        m.insert("at".into(),     rt.at);
        m.insert("append".into(), rt.append);
        m.insert("at-or".into(),          rt.at_or);
        m.insert("set-at".into(),         rt.set_at);
        m.insert("list-contains?".into(), rt.list_contains);

        m.insert("print".into(),      rt.print);
        m.insert("println".into(),    rt.println);
        m.insert("eprint".into(),     rt.eprint);
        m.insert("eprintln".into(),   rt.eprintln);
        // read-line, read-stdin
        // deregistered — AIRL stdlib equivalents in io.airl take over
        m.insert("type-of".into(), rt.type_of);
        m.insert("valid".into(),   rt.valid);

        m.insert("char-at".into(),     rt.char_at);
        m.insert("substring".into(),   rt.substring);
        m.insert("chars".into(),       rt.chars);
        m.insert("split".into(),       rt.split);
        m.insert("join".into(),        rt.join);
        m.insert("replace".into(),     rt.replace);
        // contains, starts-with, ends-with, index-of, trim, to-upper, to-lower,
        // char-alpha?, char-digit?, char-whitespace?
        // deregistered — AIRL stdlib equivalents in string.airl take over
        m.insert("char-upper?".into(),    rt.char_upper);
        m.insert("char-lower?".into(),    rt.char_lower);
        m.insert("string-ci=?".into(),    rt.string_ci_eq);

        m.insert("map-new".into(),    rt.map_new);
        m.insert("map-get".into(),    rt.map_get);
        m.insert("map-set".into(),    rt.map_set);
        m.insert("map-has".into(),    rt.map_has);
        m.insert("map-remove".into(), rt.map_remove);
        m.insert("map-keys".into(),   rt.map_keys);
        // map-from, map-get-or, map-values, map-size
        // deregistered — AIRL stdlib equivalents in map.airl take over

        // File I/O, Directory I/O (read-file, write-file, file-exists?, append-file,
        // delete-file, delete-dir, rename-file, read-dir, create-dir, file-size,
        // is-dir?, temp-file, temp-dir, file-mtime, get-args)
        // deregistered — AIRL stdlib equivalents in io.airl take over
        m.insert("run-bytecode".into(), rt.run_bytecode);
        m.insert("compile-to-executable".into(), rt.compile_to_exe);
        m.insert("compile-bytecode-to-executable".into(), rt.compile_bc_to_exe);
        m.insert("compile-bytecode-to-executable-with-target".into(), rt.compile_bc_to_exe_with_target);

        // Type conversions
        m.insert("int-to-string".into(),   rt.int_to_string);
        m.insert("float-to-string".into(), rt.float_to_string);
        m.insert("string-to-int".into(),   rt.string_to_int);
        m.insert("string-to-float".into(), rt.string_to_float);
        m.insert("char-code".into(),       rt.char_code);
        m.insert("char-from-code".into(),  rt.char_from_code);

        // Float math
        m.insert("sqrt".into(),         rt.sqrt);
        m.insert("sin".into(),          rt.sin);
        m.insert("cos".into(),          rt.cos);
        m.insert("tan".into(),          rt.tan);
        m.insert("log".into(),          rt.log);
        m.insert("exp".into(),          rt.exp);
        m.insert("floor".into(),        rt.floor);
        m.insert("ceil".into(),         rt.ceil);
        m.insert("round".into(),        rt.round);
        m.insert("float-to-int".into(), rt.float_to_int);
        m.insert("int-to-float".into(), rt.int_to_float);
        m.insert("infinity".into(),     rt.infinity);
        m.insert("nan".into(),          rt.nan_ctor);
        m.insert("is-nan?".into(),      rt.is_nan);
        m.insert("is-infinite?".into(), rt.is_infinite);

        // System (cpu-count, time-now), Environment (getenv)
        // deregistered — AIRL stdlib equivalents in io.airl take over

        // json-parse, json-stringify
        // deregistered — AIRL stdlib equivalents in json.airl take over

        // Process
        m.insert("shell-exec".into(), rt.shell_exec);
        m.insert("shell-exec-with-stdin".into(), rt.shell_exec_with_stdin);
        m.insert("parse-int-radix".into(),    rt.parse_int_radix);
        m.insert("int-to-string-radix".into(), rt.int_to_string_radix);
        // get-cwd deregistered — AIRL stdlib equivalent in io.airl takes over

        // Misc builtins
        m.insert("char-count".into(),    rt.char_count);
        m.insert("assert".into(),        rt.assert_fn);
        m.insert("panic".into(),         rt.panic_fn);
        // exit, sleep, format-time, read-lines
        // deregistered — AIRL stdlib equivalents in io.airl take over
        // concat, range, reverse, take, drop, zip, flatten, enumerate
        // deregistered — AIRL stdlib equivalents in prelude.airl take over
        // path-join, path-parent, path-filename, path-extension, is-absolute?
        // deregistered — AIRL stdlib equivalents in path.airl take over
        m.insert("regex-match".into(),   rt.regex_match);
        m.insert("regex-find-all".into(),rt.regex_find_all);
        m.insert("regex-replace".into(), rt.regex_replace);
        m.insert("regex-split".into(),   rt.regex_split);
        // sha256, hmac-sha256, sha256-bytes, hmac-sha256-bytes, pbkdf2-sha256,
        // base64-encode, base64-decode, base64-encode-bytes, base64-decode-bytes
        // deregistered — AIRL stdlib equivalents take over
        m.insert("random-bytes".into(),  rt.random_bytes);
        m.insert("sha512".into(),              rt.sha512);
        m.insert("hmac-sha512".into(),         rt.hmac_sha512);
        m.insert("sha512-bytes".into(),        rt.sha512_bytes);
        m.insert("hmac-sha512-bytes".into(),   rt.hmac_sha512_bytes);
        m.insert("pbkdf2-sha512".into(),       rt.pbkdf2_sha512);
        m.insert("bitwise-xor".into(),         rt.bitwise_xor);
        m.insert("bitwise-and".into(),         rt.bitwise_and);
        m.insert("bitwise-or".into(),          rt.bitwise_or);
        m.insert("bitwise-shr".into(),         rt.bitwise_shr);
        m.insert("bitwise-shl".into(),         rt.bitwise_shl);
        // str and format are variadic — handled specially in compile_func like print
        // They use (ptr*, i64) -> ptr signature (same as print_values)
        m.insert("str".into(),            rt.str_variadic);
        m.insert("format".into(),         rt.format_variadic);

        // Byte-array intrinsics
        m.insert("bytes-alloc".into(),       rt.bytes_alloc);
        m.insert("bytes-get".into(),         rt.bytes_get);
        m.insert("bytes-set!".into(),        rt.bytes_set);
        m.insert("bytes-length".into(),      rt.bytes_length);

        // Byte encoding
        m.insert("bytes-new".into(),         rt.bytes_new);
        m.insert("bytes-from-int8".into(),   rt.bytes_from_int8);
        m.insert("bytes-from-int16".into(),  rt.bytes_from_int16);
        m.insert("bytes-from-int32".into(),  rt.bytes_from_int32);
        m.insert("bytes-from-int64".into(),  rt.bytes_from_int64);
        m.insert("bytes-to-int16".into(),    rt.bytes_to_int16);
        m.insert("bytes-to-int32".into(),    rt.bytes_to_int32);
        m.insert("bytes-to-int64".into(),    rt.bytes_to_int64);
        m.insert("bytes-from-string".into(), rt.bytes_from_string);
        m.insert("bytes-to-string".into(),   rt.bytes_to_string);
        m.insert("bytes-concat".into(),      rt.bytes_concat);
        m.insert("bytes-concat-all".into(), rt.bytes_concat_all);
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
        m.insert("tcp-listen".into(),      rt.tcp_listen);
        m.insert("tcp-accept".into(),      rt.tcp_accept);
        m.insert("tcp-accept-tls".into(), rt.tcp_accept_tls);

        // Threads and channels
        m.insert("thread-spawn".into(),         rt.thread_spawn);
        m.insert("thread-set-affinity".into(),  rt.thread_set_affinity);
        m.insert("thread-join".into(),          rt.thread_join);
        m.insert("channel-new".into(),          rt.channel_new);
        m.insert("channel-send".into(),         rt.channel_send);
        m.insert("channel-recv".into(),         rt.channel_recv);
        m.insert("channel-recv-timeout".into(), rt.channel_recv_timeout);
        m.insert("channel-drain".into(),        rt.channel_drain);
        m.insert("channel-close".into(),        rt.channel_close);

        // NOTE: map/filter/fold/sort/any/all/find are NOT registered here.
        // They resolve to AIRL stdlib definitions (from prelude.airl) which
        // use head/tail recursion. The airl_map/airl_fold/etc extern C
        // functions exist in airl-rt for potential future use, but registering
        // them causes conflicts when the stdlib's own recursive versions are
        // also compiled — the closure calling convention differs.

        m
    }

    // ──────────────────────────────────────────────────────────────────────
    // Compile all functions
    // ──────────────────────────────────────────────────────────────────────

    /// Compile all bytecode functions into the object module.
    pub fn compile_all(
        &mut self,
        funcs: &[BytecodeFunc],
        all_functions: &HashMap<String, BytecodeFunc>,
    ) -> Result<(), String> {
        // Pre-scan: identify functions referenced by MakeClosure or IRFuncRef.
        // These must be compiled boxed (airl_call_closure uses RtValue* ABI).
        for func in all_functions.values() {
            for instr in &func.instructions {
                if instr.op == Op::MakeClosure {
                    if let Some(Value::Str(target)) = func.constants.get(instr.a as usize) {
                        self.closure_targets.insert(target.clone());
                    }
                }
                // IRFuncRef loaded via LoadConst — function used as a value
                if instr.op == Op::LoadConst {
                    if let Some(Value::IRFuncRef(target)) = func.constants.get(instr.a as usize) {
                        self.closure_targets.insert(target.clone());
                    }
                }
            }
        }
        let mut in_progress = HashSet::new();
        let mut eligible_cache = HashSet::new();
        let mut ineligible_cache = HashSet::new();
        for func in funcs {
            if !self.compiled_funcs.contains_key(&func.name) {
                self.compile_with_deps(func, all_functions, &mut in_progress, &mut eligible_cache, &mut ineligible_cache)?;
            }
        }
        Ok(())
    }

    /// Compile a function and its dependencies (recursively).
    fn compile_with_deps(
        &mut self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
        in_progress: &mut HashSet<String>,
        eligible_cache: &mut HashSet<String>,
        ineligible_cache: &mut HashSet<String>,
    ) -> Result<(), String> {
        let name = func.name.clone();
        if self.compiled_funcs.contains_key(&name) {
            return Ok(());
        }
        // Prevent infinite recursion on mutual dependencies
        if in_progress.contains(&name) {
            // Already being compiled (forward declaration will resolve at link time)
            return Ok(());
        }
        in_progress.insert(name.clone());

        // Compile call dependencies first.
        for instr in &func.instructions {
            if instr.op == Op::Call || instr.op == Op::MakeClosure {
                if let Value::Str(callee_name) = &func.constants[instr.a as usize] {
                    if callee_name != &func.name
                        && !self.compiled_funcs.contains_key(callee_name)
                    {
                        if let Some(callee) = all_functions.get(callee_name).cloned() {
                            self.compile_with_deps(&callee, all_functions, in_progress, eligible_cache, ineligible_cache)?;
                        }
                    }
                }
            }
            // IRFuncRef loaded via LoadConst — function used as a value (higher-order)
            if instr.op == Op::LoadConst {
                if let Some(Value::IRFuncRef(callee_name)) = func.constants.get(instr.a as usize) {
                    if callee_name != &func.name
                        && !self.compiled_funcs.contains_key(callee_name)
                    {
                        if let Some(callee) = all_functions.get(callee_name).cloned() {
                            self.compile_with_deps(&callee, all_functions, in_progress, eligible_cache, ineligible_cache)?;
                        }
                    }
                }
            }
        }

        // v0.6.0: All functions compile boxed for uniform calling convention.
        // Unboxed compilation (raw i64/f64 register ops) is disabled to eliminate
        // calling convention mismatches that caused the G3 self-compilation crash.
        // Can be re-enabled as an optimization in a future version.
        let is_eligible = false;

        if std::env::var("AIRL_AOT_DEBUG").as_deref() == Ok("1") {
            eprintln!("[AOT] compiling {} ({} instrs, boxed)", name, func.instructions.len());
        }
        if std::env::var("AIRL_BC_DUMP").as_deref() == Ok("1") {
            eprintln!("[BC] {} (arity={}, regs={}, consts={})", name, func.arity, func.register_count, func.constants.len());
            for (i, instr) in func.instructions.iter().enumerate() {
                let const_hint = match instr.op {
                    Op::LoadConst | Op::Call | Op::CallBuiltin | Op::MakeVariant | Op::MakeVariant0 | Op::MakeClosure | Op::MatchTag => {
                        if (instr.a as usize) < func.constants.len() {
                            format!("  ; const[{}]={:?}", instr.a, func.constants[instr.a as usize])
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                };
                eprintln!("  {:3}: {:?} dst={} a={} b={}{}", i, instr.op, instr.dst, instr.a, instr.b, const_hint);
            }
        }

        if is_eligible {
            self.compile_func_unboxed(func, all_functions)?;
        } else {
            self.compile_func(func, all_functions)?;
        }

        in_progress.remove(&name);
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Unboxed Cranelift IR emitter (primitive-only functions)
    // ──────────────────────────────────────────────────────────────────────

    /// Compile a single eligible `BytecodeFunc` using raw I64 values (no boxing).
    /// AOT unboxed compilation for primitive-typed functions.
    fn compile_func_unboxed(
        &mut self,
        func: &BytecodeFunc,
        _all_functions: &HashMap<String, BytecodeFunc>,
    ) -> Result<(), String> {
        // ── 1. Build Cranelift signature (all I64) ──────────────────────────
        let mut sig = self.module.make_signature();
        for _ in 0..func.arity {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));

        // ── 2. Declare function in object module ─────────────────────────
        let func_id = self
            .module
            .declare_function(&aot_symbol_name(&func.name), Linkage::Local, &sig)
            .map_err(|e| format!("declare: {}", e))?;
        self.compiled_funcs.insert(func.name.clone(), func_id);
        self.eligible_funcs.insert(func.name.clone());

        // ── 3. Build function body ───────────────────────────────────────
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();

        // Set of builtin names that will be inlined as native ops (no call needed).
        let inlined_builtins: HashSet<&str> = [
            "+", "-", "*", "/", "%",
            "=", "!=", "<", ">", "<=", ">=",
            "and", "or", "not", "xor",
            "valid",
        ].into_iter().collect();

        // Pre-declare call targets with I64 signatures (not self.ptr).
        // Skip builtins that will be inlined as native instructions.
        let mut call_targets: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for instr in &func.instructions {
            if instr.op == Op::Call {
                if let Value::Str(callee_name) = &func.constants[instr.a as usize] {
                    if callee_name != &func.name
                        && !call_targets.contains_key(callee_name)
                        && !inlined_builtins.contains(callee_name.as_str())
                    {
                        let argc = instr.b as usize;
                        let mut call_sig = self.module.make_signature();
                        for _ in 0..argc {
                            call_sig.params.push(AbiParam::new(types::I64));
                        }
                        call_sig.returns.push(AbiParam::new(types::I64));
                        let callee_id = self
                            .module
                            .declare_function(&aot_symbol_name(callee_name), Linkage::Local, &call_sig)
                            .map_err(|e| format!("call declare: {}", e))?;
                        call_targets.insert(callee_name.clone(), callee_id);
                    }
                }
            }
        }

        // Type hints per register — used to decide int vs float ops.
        let reg_count = func.register_count as usize;
        let mut type_hints: Vec<TypeHint> = vec![TypeHint::Int; reg_count];

        let instrs = &func.instructions;

        // ── Pass 1: Find basic block boundaries ──────────────────────────
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

        let entry_block = index_to_block[&0];
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        // ── Declare Cranelift Variables ───────────────────────────────────
        let mut vars: Vec<Variable> = Vec::with_capacity(reg_count);
        for _r in 0..reg_count {
            let var = builder.declare_var(types::I64);
            vars.push(var);
        }

        // Bind function params.
        {
            let params: Vec<ir::Value> = builder.block_params(entry_block).to_vec();
            for (i, &param_val) in params.iter().enumerate() {
                if i < func.arity as usize {
                    builder.def_var(vars[i], param_val);
                }
            }
        }
        // Initialize remaining registers to zero.
        for r in func.arity as usize..reg_count {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(vars[r], zero);
        }

        // ── Create loop_block for TailCall ───────────────────────────────
        let loop_block = builder.create_block();
        index_to_block.insert(0, loop_block);
        builder.ins().jump(loop_block, &[]);
        builder.switch_to_block(loop_block);
        let mut last_was_terminator = true;

        // ── Pass 2: Emit IR for each instruction ─────────────────────────
        for (i, instr) in instrs.iter().enumerate() {
            if let Some(&blk) = index_to_block.get(&i) {
                if !last_was_terminator {
                    builder.ins().jump(blk, &[]);
                }
                builder.switch_to_block(blk);
            }

            match instr.op {
                // ── Literals ──────────────────────────────────────────
                Op::LoadConst => {
                    let dst = instr.dst as usize;
                    let cidx = instr.a as usize;
                    let val = match &func.constants[cidx] {
                        Value::Int(n) => {
                            type_hints[dst] = TypeHint::Int;
                            builder.ins().iconst(types::I64, *n)
                        }
                        Value::Float(f) => {
                            type_hints[dst] = TypeHint::Float;
                            let fv = builder.ins().f64const(*f);
                            builder.ins().bitcast(types::I64, MemFlags::new(), fv)
                        }
                        Value::Bool(b) => {
                            type_hints[dst] = TypeHint::Bool;
                            builder.ins().iconst(types::I64, *b as i64)
                        }
                        _ => return Err("LoadConst: unsupported constant type in unboxed function".into()),
                    };
                    builder.def_var(vars[dst], val);
                    last_was_terminator = false;
                }
                Op::LoadNil => {
                    let dst = instr.dst as usize;
                    type_hints[dst] = TypeHint::Int;
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.def_var(vars[dst], zero);
                    last_was_terminator = false;
                }
                Op::LoadTrue => {
                    let dst = instr.dst as usize;
                    type_hints[dst] = TypeHint::Bool;
                    let one = builder.ins().iconst(types::I64, 1);
                    builder.def_var(vars[dst], one);
                    last_was_terminator = false;
                }
                Op::LoadFalse => {
                    let dst = instr.dst as usize;
                    type_hints[dst] = TypeHint::Bool;
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.def_var(vars[dst], zero);
                    last_was_terminator = false;
                }
                Op::Move => {
                    let dst = instr.dst as usize;
                    let src = instr.a as usize;
                    type_hints[dst] = type_hints[src];
                    let v = builder.use_var(vars[src]);
                    builder.def_var(vars[dst], v);
                    last_was_terminator = false;
                }

                // ── Arithmetic ────────────────────────────────────────
                Op::Add | Op::Sub | Op::Mul | Op::Div => {
                    let dst = instr.dst as usize;
                    let a = instr.a as usize;
                    let b = instr.b as usize;
                    let is_float =
                        type_hints[a] == TypeHint::Float || type_hints[b] == TypeHint::Float;
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let result = if is_float {
                        let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                        let fb = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                        let fr = match instr.op {
                            Op::Add => builder.ins().fadd(fa, fb),
                            Op::Sub => builder.ins().fsub(fa, fb),
                            Op::Mul => builder.ins().fmul(fa, fb),
                            Op::Div => builder.ins().fdiv(fa, fb),
                            _ => unreachable!(),
                        };
                        type_hints[dst] = TypeHint::Float;
                        builder.ins().bitcast(types::I64, MemFlags::new(), fr)
                    } else {
                        type_hints[dst] = TypeHint::Int;
                        match instr.op {
                            Op::Add => builder.ins().iadd(va, vb),
                            Op::Sub => builder.ins().isub(va, vb),
                            Op::Mul => builder.ins().imul(va, vb),
                            Op::Div => builder.ins().sdiv(va, vb),
                            _ => unreachable!(),
                        }
                    };
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Mod => {
                    let dst = instr.dst as usize;
                    let a = instr.a as usize;
                    let b = instr.b as usize;
                    type_hints[dst] = TypeHint::Int;
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let result = builder.ins().srem(va, vb);
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::Neg => {
                    let dst = instr.dst as usize;
                    let a = instr.a as usize;
                    let is_float = type_hints[a] == TypeHint::Float;
                    let va = builder.use_var(vars[a]);
                    let result = if is_float {
                        let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                        let fr = builder.ins().fneg(fa);
                        type_hints[dst] = TypeHint::Float;
                        builder.ins().bitcast(types::I64, MemFlags::new(), fr)
                    } else {
                        type_hints[dst] = TypeHint::Int;
                        builder.ins().ineg(va)
                    };
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Comparisons ───────────────────────────────────────
                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let dst = instr.dst as usize;
                    let a = instr.a as usize;
                    let b = instr.b as usize;
                    let is_float =
                        type_hints[a] == TypeHint::Float || type_hints[b] == TypeHint::Float;
                    let va = builder.use_var(vars[a]);
                    let vb = builder.use_var(vars[b]);
                    let cmp_i8 = if is_float {
                        let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                        let fb = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                        let fcc = match instr.op {
                            Op::Eq => FloatCC::Equal,
                            Op::Ne => FloatCC::NotEqual,
                            Op::Lt => FloatCC::LessThan,
                            Op::Le => FloatCC::LessThanOrEqual,
                            Op::Gt => FloatCC::GreaterThan,
                            Op::Ge => FloatCC::GreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        builder.ins().fcmp(fcc, fa, fb)
                    } else {
                        let icc = match instr.op {
                            Op::Eq => IntCC::Equal,
                            Op::Ne => IntCC::NotEqual,
                            Op::Lt => IntCC::SignedLessThan,
                            Op::Le => IntCC::SignedLessThanOrEqual,
                            Op::Gt => IntCC::SignedGreaterThan,
                            Op::Ge => IntCC::SignedGreaterThanOrEqual,
                            _ => unreachable!(),
                        };
                        builder.ins().icmp(icc, va, vb)
                    };
                    // icmp/fcmp produce I8; uextend to I64.
                    let result = builder.ins().uextend(types::I64, cmp_i8);
                    type_hints[dst] = TypeHint::Bool;
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Logic ─────────────────────────────────────────────
                Op::Not => {
                    let dst = instr.dst as usize;
                    let a = instr.a as usize;
                    type_hints[dst] = TypeHint::Bool;
                    let va = builder.use_var(vars[a]);
                    let one = builder.ins().iconst(types::I64, 1);
                    let result = builder.ins().isub(one, va);
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Control flow ──────────────────────────────────────
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
                    let cond = builder.use_var(vars[cond_reg]);
                    builder.ins().brif(cond, fallthrough_blk, &[], target_blk, &[]);
                    last_was_terminator = true;
                }
                Op::JumpIfTrue => {
                    let cond_reg = instr.a as usize;
                    let offset = instr.b as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];
                    let cond = builder.use_var(vars[cond_reg]);
                    builder.ins().brif(cond, target_blk, &[], fallthrough_blk, &[]);
                    last_was_terminator = true;
                }
                Op::Return => {
                    let src = instr.a as usize;
                    let v = builder.use_var(vars[src]);
                    builder.ins().return_(&[v]);
                    last_was_terminator = true;
                }

                // ── Calls ────────────────────────────────────────────
                Op::Call => {
                    let callee_name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("call: func name must be string".into()),
                    };
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;

                    // Check if this is a whitelisted builtin that should be inlined
                    if inlined_builtins.contains(callee_name.as_str()) {
                        // Inline the builtin as native instructions (same logic as CallBuiltin)
                        match callee_name.as_str() {
                            "+" | "-" | "*" | "/" => {
                                let a = dst + 1;
                                let b = dst + 2;
                                let is_float = type_hints.get(a).copied() == Some(TypeHint::Float)
                                    || type_hints.get(b).copied() == Some(TypeHint::Float);
                                let va = builder.use_var(vars[a]);
                                let vb = builder.use_var(vars[b]);
                                let result = if is_float {
                                    let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                                    let fb = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                                    let fr = match callee_name.as_str() {
                                        "+" => builder.ins().fadd(fa, fb),
                                        "-" => builder.ins().fsub(fa, fb),
                                        "*" => builder.ins().fmul(fa, fb),
                                        "/" => builder.ins().fdiv(fa, fb),
                                        _ => unreachable!(),
                                    };
                                    type_hints[dst] = TypeHint::Float;
                                    builder.ins().bitcast(types::I64, MemFlags::new(), fr)
                                } else {
                                    type_hints[dst] = TypeHint::Int;
                                    match callee_name.as_str() {
                                        "+" => builder.ins().iadd(va, vb),
                                        "-" => builder.ins().isub(va, vb),
                                        "*" => builder.ins().imul(va, vb),
                                        "/" => builder.ins().sdiv(va, vb),
                                        _ => unreachable!(),
                                    }
                                };
                                builder.def_var(vars[dst], result);
                            }
                            "%" => {
                                let a = dst + 1;
                                let b = dst + 2;
                                type_hints[dst] = TypeHint::Int;
                                let va = builder.use_var(vars[a]);
                                let vb = builder.use_var(vars[b]);
                                let result = builder.ins().srem(va, vb);
                                builder.def_var(vars[dst], result);
                            }
                            "=" | "!=" | "<" | ">" | "<=" | ">=" => {
                                let a = dst + 1;
                                let b = dst + 2;
                                let is_float = type_hints.get(a).copied() == Some(TypeHint::Float)
                                    || type_hints.get(b).copied() == Some(TypeHint::Float);
                                let va = builder.use_var(vars[a]);
                                let vb = builder.use_var(vars[b]);
                                let cmp_i8 = if is_float {
                                    let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                                    let fb = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                                    let fcc = match callee_name.as_str() {
                                        "="  => FloatCC::Equal,
                                        "!=" => FloatCC::NotEqual,
                                        "<"  => FloatCC::LessThan,
                                        ">"  => FloatCC::GreaterThan,
                                        "<=" => FloatCC::LessThanOrEqual,
                                        ">=" => FloatCC::GreaterThanOrEqual,
                                        _ => unreachable!(),
                                    };
                                    builder.ins().fcmp(fcc, fa, fb)
                                } else {
                                    let icc = match callee_name.as_str() {
                                        "="  => IntCC::Equal,
                                        "!=" => IntCC::NotEqual,
                                        "<"  => IntCC::SignedLessThan,
                                        ">"  => IntCC::SignedGreaterThan,
                                        "<=" => IntCC::SignedLessThanOrEqual,
                                        ">=" => IntCC::SignedGreaterThanOrEqual,
                                        _ => unreachable!(),
                                    };
                                    builder.ins().icmp(icc, va, vb)
                                };
                                let result = builder.ins().uextend(types::I64, cmp_i8);
                                type_hints[dst] = TypeHint::Bool;
                                builder.def_var(vars[dst], result);
                            }
                            "not" => {
                                let a = dst + 1;
                                type_hints[dst] = TypeHint::Bool;
                                let va = builder.use_var(vars[a]);
                                let one = builder.ins().iconst(types::I64, 1);
                                let result = builder.ins().isub(one, va);
                                builder.def_var(vars[dst], result);
                            }
                            "and" => {
                                let a = dst + 1;
                                let b = dst + 2;
                                type_hints[dst] = TypeHint::Bool;
                                let va = builder.use_var(vars[a]);
                                let vb = builder.use_var(vars[b]);
                                let result = builder.ins().band(va, vb);
                                builder.def_var(vars[dst], result);
                            }
                            "or" => {
                                let a = dst + 1;
                                let b = dst + 2;
                                type_hints[dst] = TypeHint::Bool;
                                let va = builder.use_var(vars[a]);
                                let vb = builder.use_var(vars[b]);
                                let result = builder.ins().bor(va, vb);
                                builder.def_var(vars[dst], result);
                            }
                            "xor" => {
                                let a = dst + 1;
                                let b = dst + 2;
                                type_hints[dst] = TypeHint::Bool;
                                let va = builder.use_var(vars[a]);
                                let vb = builder.use_var(vars[b]);
                                let result = builder.ins().bxor(va, vb);
                                builder.def_var(vars[dst], result);
                            }
                            "valid" => {
                                type_hints[dst] = TypeHint::Bool;
                                let one = builder.ins().iconst(types::I64, 1);
                                builder.def_var(vars[dst], one);
                            }
                            _ => {
                                return Err(format!(
                                    "Op::Call to inlined builtin '{}' not handled in unboxed codegen",
                                    callee_name
                                ));
                            }
                        }
                    } else {
                        // Regular function call (user-defined or self-call)
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
                    }
                    last_was_terminator = false;
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

                // ── Contract assertions ──────────────────────────────
                // Happy path: one conditional branch (predicted taken).
                // Sad path: call runtime helper, return sentinel.
                Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {
                    let bool_reg = instr.a as usize;
                    let bool_val = builder.use_var(vars[bool_reg]);

                    let fail_block = builder.create_block();
                    let cont_block = builder.create_block();

                    // if bool_val != 0 (true) → continue; else → fail
                    builder.ins().brif(bool_val, cont_block, &[], fail_block, &[]);

                    // Fail block: call airl_jit_contract_fail(kind, fn_name_idx, clause_idx)
                    builder.switch_to_block(fail_block);
                    let kind_val = builder.ins().iconst(types::I64, match instr.op {
                        Op::AssertRequires => 0,
                        Op::AssertEnsures => 1,
                        _ => 2, // Invariant
                    });
                    let fn_idx_val = builder.ins().iconst(types::I64, instr.dst as i64);
                    let clause_val = builder.ins().iconst(types::I64, instr.b as i64);
                    let fail_ref = self.module.declare_func_in_func(self.rt.contract_fail, builder.func);
                    let call = builder.ins().call(fail_ref, &[kind_val, fn_idx_val, clause_val]);
                    let sentinel = builder.inst_results(call)[0];
                    builder.ins().return_(&[sentinel]);

                    // Continue block: contract passed
                    builder.switch_to_block(cont_block);
                    last_was_terminator = false;
                }

                // ── CallBuiltin (whitelisted arithmetic/comparison/logic) ──
                // These are inlined as native instructions, identical to the
                // direct Op::Add/Sub/Eq/etc. paths above.
                Op::CallBuiltin => {
                    let name_idx = instr.a as usize;
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;
                    let builtin_name = match &func.constants[name_idx] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("callbuiltin: name must be string in unboxed path".into()),
                    };

                    match builtin_name.as_str() {
                        // ── Binary arithmetic ────────────────────────────
                        "+" | "-" | "*" | "/" => {
                            if argc != 2 { return Err(format!("CallBuiltin '{}': expected 2 args, got {}", builtin_name, argc)); }
                            let a = dst + 1;
                            let b = dst + 2;
                            let is_float = type_hints.get(a).copied() == Some(TypeHint::Float)
                                || type_hints.get(b).copied() == Some(TypeHint::Float);
                            let va = builder.use_var(vars[a]);
                            let vb = builder.use_var(vars[b]);
                            let result = if is_float {
                                let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                                let fb = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                                let fr = match builtin_name.as_str() {
                                    "+" => builder.ins().fadd(fa, fb),
                                    "-" => builder.ins().fsub(fa, fb),
                                    "*" => builder.ins().fmul(fa, fb),
                                    "/" => builder.ins().fdiv(fa, fb),
                                    _ => unreachable!(),
                                };
                                type_hints[dst] = TypeHint::Float;
                                builder.ins().bitcast(types::I64, MemFlags::new(), fr)
                            } else {
                                type_hints[dst] = TypeHint::Int;
                                match builtin_name.as_str() {
                                    "+" => builder.ins().iadd(va, vb),
                                    "-" => builder.ins().isub(va, vb),
                                    "*" => builder.ins().imul(va, vb),
                                    "/" => builder.ins().sdiv(va, vb),
                                    _ => unreachable!(),
                                }
                            };
                            builder.def_var(vars[dst], result);
                        }
                        "%" => {
                            if argc != 2 { return Err(format!("CallBuiltin '%': expected 2 args, got {}", argc)); }
                            let a = dst + 1;
                            let b = dst + 2;
                            type_hints[dst] = TypeHint::Int;
                            let va = builder.use_var(vars[a]);
                            let vb = builder.use_var(vars[b]);
                            let result = builder.ins().srem(va, vb);
                            builder.def_var(vars[dst], result);
                        }

                        // ── Comparisons ──────────────────────────────────
                        "=" | "!=" | "<" | ">" | "<=" | ">=" => {
                            if argc != 2 { return Err(format!("CallBuiltin '{}': expected 2 args, got {}", builtin_name, argc)); }
                            let a = dst + 1;
                            let b = dst + 2;
                            let is_float = type_hints.get(a).copied() == Some(TypeHint::Float)
                                || type_hints.get(b).copied() == Some(TypeHint::Float);
                            let va = builder.use_var(vars[a]);
                            let vb = builder.use_var(vars[b]);
                            let cmp_i8 = if is_float {
                                let fa = builder.ins().bitcast(types::F64, MemFlags::new(), va);
                                let fb = builder.ins().bitcast(types::F64, MemFlags::new(), vb);
                                let fcc = match builtin_name.as_str() {
                                    "="  => FloatCC::Equal,
                                    "!=" => FloatCC::NotEqual,
                                    "<"  => FloatCC::LessThan,
                                    ">"  => FloatCC::GreaterThan,
                                    "<=" => FloatCC::LessThanOrEqual,
                                    ">=" => FloatCC::GreaterThanOrEqual,
                                    _ => unreachable!(),
                                };
                                builder.ins().fcmp(fcc, fa, fb)
                            } else {
                                let icc = match builtin_name.as_str() {
                                    "="  => IntCC::Equal,
                                    "!=" => IntCC::NotEqual,
                                    "<"  => IntCC::SignedLessThan,
                                    ">"  => IntCC::SignedGreaterThan,
                                    "<=" => IntCC::SignedLessThanOrEqual,
                                    ">=" => IntCC::SignedGreaterThanOrEqual,
                                    _ => unreachable!(),
                                };
                                builder.ins().icmp(icc, va, vb)
                            };
                            let result = builder.ins().uextend(types::I64, cmp_i8);
                            type_hints[dst] = TypeHint::Bool;
                            builder.def_var(vars[dst], result);
                        }

                        // ── Logic ────────────────────────────────────────
                        "not" => {
                            if argc != 1 { return Err(format!("CallBuiltin 'not': expected 1 arg, got {}", argc)); }
                            let a = dst + 1;
                            type_hints[dst] = TypeHint::Bool;
                            let va = builder.use_var(vars[a]);
                            let one = builder.ins().iconst(types::I64, 1);
                            let result = builder.ins().isub(one, va);
                            builder.def_var(vars[dst], result);
                        }
                        "and" => {
                            if argc != 2 { return Err(format!("CallBuiltin 'and': expected 2 args, got {}", argc)); }
                            let a = dst + 1;
                            let b = dst + 2;
                            type_hints[dst] = TypeHint::Bool;
                            let va = builder.use_var(vars[a]);
                            let vb = builder.use_var(vars[b]);
                            let result = builder.ins().band(va, vb);
                            builder.def_var(vars[dst], result);
                        }
                        "or" => {
                            if argc != 2 { return Err(format!("CallBuiltin 'or': expected 2 args, got {}", argc)); }
                            let a = dst + 1;
                            let b = dst + 2;
                            type_hints[dst] = TypeHint::Bool;
                            let va = builder.use_var(vars[a]);
                            let vb = builder.use_var(vars[b]);
                            let result = builder.ins().bor(va, vb);
                            builder.def_var(vars[dst], result);
                        }
                        "xor" => {
                            if argc != 2 { return Err(format!("CallBuiltin 'xor': expected 2 args, got {}", argc)); }
                            let a = dst + 1;
                            let b = dst + 2;
                            type_hints[dst] = TypeHint::Bool;
                            let va = builder.use_var(vars[a]);
                            let vb = builder.use_var(vars[b]);
                            let result = builder.ins().bxor(va, vb);
                            builder.def_var(vars[dst], result);
                        }

                        // ── valid (contract helper) ──────────────────────
                        // `valid` checks if a value is non-nil. In unboxed context,
                        // any non-zero i64 is "valid". Returns 1 (true) or 0 (false).
                        "valid" => {
                            if argc != 1 { return Err(format!("CallBuiltin 'valid': expected 1 arg, got {}", argc)); }
                            // In the unboxed tier, all values are raw i64. A value is
                            // "valid" if it's anything — since unboxed values can't be
                            // nil in the traditional sense, we always return true.
                            // This is safe because if we got here, the value exists.
                            type_hints[dst] = TypeHint::Bool;
                            let one = builder.ins().iconst(types::I64, 1);
                            builder.def_var(vars[dst], one);
                        }

                        _ => {
                            return Err(format!(
                                "CallBuiltin '{}' is not supported in unboxed AOT (should have been rejected by is_eligible)",
                                builtin_name
                            ));
                        }
                    }
                    last_was_terminator = false;
                }

                // Any other opcode should have been caught by is_eligible.
                op => {
                    return Err(format!("unhandled opcode {:?} in unboxed AOT", op));
                }
            }
        }

        // If the last instruction didn't terminate the block, add an implicit return 0.
        if !last_was_terminator {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().return_(&[zero]);
        }

        // ── Record return type hint for boundary marshaling ─────────────
        // Scan ALL Return instructions: use Float if any return is Float, else Int.
        let mut return_hint = TypeHint::Int; // default
        for instr in &func.instructions {
            if instr.op == Op::Return {
                let src = instr.a as usize;
                if src < type_hints.len() {
                    let hint = type_hints[src];
                    if matches!(hint, TypeHint::Float) {
                        return_hint = TypeHint::Float;
                        break; // Float takes priority
                    }
                    return_hint = hint;
                }
            }
        }
        self.eligible_return_hints.insert(func.name.clone(), return_hint);

        // ── Seal all blocks ──────────────────────────────────────────────
        builder.seal_all_blocks();
        builder.finalize();

        // Debug output
        if std::env::var("AIRL_AOT_DEBUG").as_deref() == Ok("1") {
            eprintln!("[AOT] Cranelift IR (unboxed) for {}:\n{}", func.name, ctx.func.display());
        }

        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| format!("define: {}", e))?;

        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Core Cranelift IR emitter (boxed — existing path)
    // ──────────────────────────────────────────────────────────────────────

    /// Compile a single `BytecodeFunc` into the object module.
    pub fn compile_func(
        &mut self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
    ) -> Result<(), String> {
        // ── 1. Build Cranelift signature ──────────────────────────────────
        let mut sig = self.module.make_signature();
        for _ in 0..func.arity {
            sig.params.push(AbiParam::new(self.ptr));
        }
        sig.returns.push(AbiParam::new(self.ptr));

        // ── 2. Declare function in object module ─────────────────────────
        let func_id = self
            .module
            .declare_function(&aot_symbol_name(&func.name), Linkage::Local, &sig)
            .map_err(|e| format!("declare: {}", e))?;
        // NOTE: insert into compiled_funcs BEFORE body compilation so that
        // self-recursive calls and emit_entry_point can find the function.
        // If body compilation fails, the function will be declared but undefined,
        // which is caught by the error propagation from compile_func → compile_all.
        self.compiled_funcs.insert(func.name.clone(), func_id);

        // ── 3. Build function body ───────────────────────────────────────
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();

        // Pre-declare call targets.
        let mut call_targets: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for instr in &func.instructions {
            if instr.op == Op::Call || instr.op == Op::TailCall {
                if let Value::Str(callee_name) = &func.constants[instr.a as usize] {
                    if callee_name != &func.name && !call_targets.contains_key(callee_name) {
                        let argc = instr.b as usize;
                        let is_variadic_special = (callee_name == "print" && argc != 1)
                            || callee_name == "str" || callee_name == "format";
                        if is_variadic_special {
                            // handled inline via stack slot + variadic call
                        } else if let Some(&builtin_id) = self.builtin_map.get(callee_name.as_str()) {
                            if all_functions.contains_key(callee_name.as_str())
                                || self.compiled_funcs.contains_key(callee_name.as_str())
                            {
                                eprintln!("[g3] WARNING: function '{}' shadows builtin — builtin will be used", callee_name);
                            }
                            call_targets.insert(callee_name.clone(), builtin_id);
                        } else if self.eligible_funcs.contains(callee_name) {
                            // Callee was compiled unboxed — use I64 signature
                            let mut call_sig = self.module.make_signature();
                            for _ in 0..argc {
                                call_sig.params.push(AbiParam::new(types::I64));
                            }
                            call_sig.returns.push(AbiParam::new(types::I64));
                            let callee_id = self
                                .module
                                .declare_function(&aot_symbol_name(callee_name), Linkage::Local, &call_sig)
                                .map_err(|e| format!("call declare (eligible): {}", e))?;
                            call_targets.insert(callee_name.clone(), callee_id);
                        } else if all_functions.contains_key(callee_name.as_str())
                            || self.compiled_funcs.contains_key(callee_name.as_str())
                        {
                            let mut call_sig = self.module.make_signature();
                            for _ in 0..argc {
                                call_sig.params.push(AbiParam::new(self.ptr));
                            }
                            call_sig.returns.push(AbiParam::new(self.ptr));
                            let callee_id = self
                                .module
                                .declare_function(&aot_symbol_name(callee_name), Linkage::Local, &call_sig)
                                .map_err(|e| format!("call declare: {}", e))?;
                            call_targets.insert(callee_name.clone(), callee_id);
                        } else {
                            // Warn about unresolved function but still declare it.
                            // This allows G3 self-compilation (where bootstrap module
                            // functions are linked separately) while flagging typos.
                            eprintln!(
                                "warning: unresolved function '{}' (called from '{}') — \
                                 not found in compiled functions or builtins",
                                callee_name, func.name
                            );
                            let mut call_sig = self.module.make_signature();
                            for _ in 0..argc {
                                call_sig.params.push(AbiParam::new(self.ptr));
                            }
                            call_sig.returns.push(AbiParam::new(self.ptr));
                            let callee_id = self
                                .module
                                .declare_function(&aot_symbol_name(callee_name), Linkage::Local, &call_sig)
                                .map_err(|e| format!("call declare: {}", e))?;
                            call_targets.insert(callee_name.clone(), callee_id);
                        }
                    }
                }
            }
        }

        let instrs = &func.instructions;
        let reg_count = func.register_count as usize;

        // ── Pass 1: Find basic block boundaries ──────────────────────────
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
                    let offset = instr.b as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                    block_starts.insert(i + 1);
                }
                Op::Return | Op::TailCall => {
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

        let entry_block = index_to_block[&0];
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        // ── Declare Cranelift Variables ───────────────────────────────────
        let mut vars: Vec<Variable> = Vec::with_capacity(reg_count + 1);
        for _ in 0..reg_count {
            let var = builder.declare_var(self.ptr);
            vars.push(var);
        }
        let match_flag_var = builder.declare_var(types::I64);

        // Bind function params.
        {
            let params: Vec<ir::Value> = builder.block_params(entry_block).to_vec();
            for (i, &param_val) in params.iter().enumerate() {
                if i < func.arity as usize {
                    builder.def_var(vars[i], param_val);
                }
            }
        }
        for r in func.arity as usize..reg_count {
            let zero = builder.ins().iconst(self.ptr, 0);
            builder.def_var(vars[r], zero);
        }
        {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(match_flag_var, zero);
        }

        // ── Create loop_block for TailCall ───────────────────────────────
        let loop_block = builder.create_block();
        index_to_block.insert(0, loop_block);
        builder.ins().jump(loop_block, &[]);
        builder.switch_to_block(loop_block);
        let mut last_was_terminator = true;

        // ── Pass 2: Emit IR for each instruction ─────────────────────────
        for (i, instr) in instrs.iter().enumerate() {
            if let Some(&blk) = index_to_block.get(&i) {
                if !last_was_terminator {
                    builder.ins().jump(blk, &[]);
                }
                builder.switch_to_block(blk);
            }

            match instr.op {
                // ── Literals ──────────────────────────────────────────
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
                            // AOT: string goes into data section
                            let (data_id, slen) = self.intern_string(s);
                            let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                            let gv = self.module.declare_data_in_func(data_id, builder.func);
                            let ptr_val = builder.ins().global_value(self.ptr, gv);
                            let len_val = builder.ins().iconst(types::I64, slen as i64);
                            let call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
                        }
                        Value::IRFuncRef(name) => {
                            // Function reference: create a closure with zero captures
                            if let Some(&func_id) = self.compiled_funcs.get(name) {
                                let callee_ref = self.module.declare_func_in_func(func_id, builder.func);
                                let fn_ptr_val = builder.ins().func_addr(self.ptr, callee_ref);
                                let make_closure_ref = self.module.declare_func_in_func(self.rt.make_closure, builder.func);
                                let null = builder.ins().iconst(self.ptr, 0);
                                let zero = builder.ins().iconst(types::I64, 0);
                                let call = builder.ins().call(make_closure_ref, &[fn_ptr_val, null, zero]);
                                let result = builder.inst_results(call)[0];
                                builder.def_var(vars[dst], result);
                            } else {
                                // Unknown function — emit nil as fallback
                                let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
                                let call = builder.ins().call(nil_ref, &[]);
                                let result = builder.inst_results(call)[0];
                                builder.def_var(vars[dst], result);
                            }
                        }
                        _ => {
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

                // ── Arithmetic ────────────────────────────────────────
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

                // ── Comparison ────────────────────────────────────────
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

                // ── Logic ─────────────────────────────────────────────
                Op::Not => {
                    let dst = instr.dst as usize;
                    let not_ref = self.module.declare_func_in_func(self.rt.not, builder.func);
                    let va = builder.use_var(vars[instr.a as usize]);
                    let call = builder.ins().call(not_ref, &[va]);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }

                // ── Control flow ──────────────────────────────────────
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
                    let as_bool_ref = self.module.declare_func_in_func(self.rt.as_bool_raw, builder.func);
                    let cond_ptr = builder.use_var(vars[cond_reg]);
                    let call = builder.ins().call(as_bool_ref, &[cond_ptr]);
                    let raw = builder.inst_results(call)[0];
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
                    builder.ins().brif(raw, target_blk, &[], fallthrough_blk, &[]);
                    last_was_terminator = true;
                }

                Op::Return => {
                    let src = instr.a as usize;
                    let v = builder.use_var(vars[src]);
                    builder.ins().return_(&[v]);
                    last_was_terminator = true;
                }

                // ── Function calls ────────────────────────────────────
                Op::Call => {
                    let callee_name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("call: func name must be string".into()),
                    };
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;

                    // Variadic builtins: print (multi-arg), str, format
                    // These use (ptr_to_args, count) -> ptr calling convention
                    let variadic_func_id = if callee_name == "print" && argc != 1 {
                        Some(self.rt.print_values)
                    } else if callee_name == "str" {
                        Some(self.rt.str_variadic)
                    } else if callee_name == "format" {
                        Some(self.rt.format_variadic)
                    } else {
                        None
                    };

                    if let Some(var_func) = variadic_func_id {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            ir::StackSlotKind::ExplicitSlot,
                            (argc as u32) * 8,
                            3,
                        ));
                        for j in 0..argc {
                            let arg = builder.use_var(vars[dst + 1 + j]);
                            builder.ins().stack_store(arg, slot, (j as i32) * 8);
                        }
                        let slot_addr = builder.ins().stack_addr(self.ptr, slot, 0);
                        let count_val = builder.ins().iconst(types::I64, argc as i64);
                        let var_ref = self.module.declare_func_in_func(var_func, builder.func);
                        let call = builder.ins().call(var_ref, &[slot_addr, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                        last_was_terminator = false;
                    } else if self.eligible_funcs.contains(&callee_name) {
                        // ── Boundary marshal: boxed caller → unboxed callee ──
                        // 1. Extract raw i64 from each boxed arg via airl_as_int_raw
                        // 2. Call the unboxed function with raw I64 args
                        // 3. Rebox the raw i64 result based on the callee's return hint

                        let callee_func_id = if let Some(&id) = call_targets.get(&callee_name) {
                            id
                        } else {
                            return Err(format!("call target '{}' not declared", callee_name));
                        };
                        let func_ref = self.module.declare_func_in_func(callee_func_id, builder.func);
                        let as_int_raw_ref = self.module.declare_func_in_func(self.rt.as_int_raw, builder.func);

                        // Unbox each arg: *mut RtValue → i64
                        let mut call_args = Vec::new();
                        for j in 0..argc {
                            let arg_ptr = builder.use_var(vars[dst + 1 + j]);
                            let raw_call = builder.ins().call(as_int_raw_ref, &[arg_ptr]);
                            let raw_val = builder.inst_results(raw_call)[0];
                            call_args.push(raw_val);
                        }

                        // Call the unboxed function
                        let call = builder.ins().call(func_ref, &call_args);
                        let raw_result = builder.inst_results(call)[0];

                        // Rebox the result based on the callee's return type hint
                        let return_hint = self.eligible_return_hints
                            .get(&callee_name)
                            .copied()
                            .unwrap_or(TypeHint::Int);

                        let boxed_result = match return_hint {
                            TypeHint::Int => {
                                let ctor_ref = self.module.declare_func_in_func(self.rt.int_ctor, builder.func);
                                let c = builder.ins().call(ctor_ref, &[raw_result]);
                                builder.inst_results(c)[0]
                            }
                            TypeHint::Float => {
                                // raw_result is f64 bits as i64 — bitcast to f64 for airl_float
                                let f_val = builder.ins().bitcast(types::F64, MemFlags::new(), raw_result);
                                let ctor_ref = self.module.declare_func_in_func(self.rt.float_ctor, builder.func);
                                let c = builder.ins().call(ctor_ref, &[f_val]);
                                builder.inst_results(c)[0]
                            }
                            TypeHint::Bool => {
                                let ctor_ref = self.module.declare_func_in_func(self.rt.bool_ctor, builder.func);
                                let c = builder.ins().call(ctor_ref, &[raw_result]);
                                builder.inst_results(c)[0]
                            }
                        };
                        builder.def_var(vars[dst], boxed_result);
                        last_was_terminator = false;
                    } else {
                        // Normal boxed-to-boxed call
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
                        return Err(format!(
                            "cross-function TailCall to '{}' not supported",
                            callee_name
                        ));
                    }
                    builder.ins().jump(loop_block, &[]);
                    last_was_terminator = true;
                }

                // ── CallBuiltin ───────────────────────────────────────
                Op::CallBuiltin => {
                    let name_idx = instr.a as usize;
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;
                    let builtin_name = match &func.constants[name_idx] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("callbuiltin: name must be string".into()),
                    };
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
                        let slot_addr = builder.ins().stack_addr(self.ptr, slot, 0);
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
                        return Err(format!("AOT: unregistered builtin '{}'. Add to build_builtin_map() in bytecode_aot.rs", builtin_name));
                    }
                    last_was_terminator = false;
                }

                // ── CallReg (closure call) ────────────────────────────
                Op::CallReg => {
                    let dst = instr.dst as usize;
                    let callee_reg = instr.a as usize;
                    let argc = instr.b as usize;

                    let call_closure_ref =
                        self.module.declare_func_in_func(self.rt.call_closure, builder.func);

                    if argc > 0 {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            (argc * 8) as u32,
                            0,
                        ));
                        let base = builder.ins().stack_addr(self.ptr, slot, 0);
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
                        let null = builder.ins().iconst(self.ptr, 0);
                        let zero = builder.ins().iconst(types::I64, 0);
                        let closure_val = builder.use_var(vars[callee_reg]);
                        let call = builder.ins().call(call_closure_ref, &[closure_val, null, zero]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    }
                    last_was_terminator = false;
                }

                // ── Data operations ───────────────────────────────────
                Op::MakeList => {
                    let dst = instr.dst as usize;
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let list_new_ref =
                        self.module.declare_func_in_func(self.rt.list_new, builder.func);

                    if count > 0 {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            (count * 8) as u32,
                            0,
                        ));
                        let base = builder.ins().stack_addr(self.ptr, slot, 0);
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
                        let null = builder.ins().iconst(self.ptr, 0);
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
                    // AOT: string tag via data section
                    let (data_id, slen) = self.intern_string(tag_src);
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let gv = self.module.declare_data_in_func(data_id, builder.func);
                    let ptr_val = builder.ins().global_value(self.ptr, gv);
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
                    let (data_id, slen) = self.intern_string(tag_src);
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let gv = self.module.declare_data_in_func(data_id, builder.func);
                    let ptr_val = builder.ins().global_value(self.ptr, gv);
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

                    // AOT: use func_addr instead of raw code pointer
                    let callee_func_id = self
                        .compiled_funcs
                        .get(&closure_func_name)
                        .copied()
                        .ok_or_else(|| {
                            format!("MakeClosure: target '{}' not compiled", closure_func_name)
                        })?;
                    let callee_ref =
                        self.module.declare_func_in_func(callee_func_id, builder.func);
                    let fn_ptr_val = builder.ins().func_addr(self.ptr, callee_ref);

                    let capture_count = all_functions
                        .get(&closure_func_name)
                        .map(|f| f.capture_count as usize)
                        .unwrap_or(0);

                    let make_closure_ref =
                        self.module.declare_func_in_func(self.rt.make_closure, builder.func);

                    if capture_count > 0 {
                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            ir::StackSlotKind::ExplicitSlot,
                            (capture_count as u32) * 8,
                            3,
                        ));
                        for j in 0..capture_count {
                            let cap_val = builder.use_var(vars[capture_start + j]);
                            builder.ins().stack_store(cap_val, slot, (j as i32) * 8);
                        }
                        let cap_addr = builder.ins().stack_addr(self.ptr, slot, 0);
                        let count_val =
                            builder.ins().iconst(types::I64, capture_count as i64);
                        let call = builder
                            .ins()
                            .call(make_closure_ref, &[fn_ptr_val, cap_addr, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else {
                        let null = builder.ins().iconst(self.ptr, 0);
                        let zero = builder.ins().iconst(types::I64, 0);
                        let call =
                            builder.ins().call(make_closure_ref, &[fn_ptr_val, null, zero]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    }
                    last_was_terminator = false;
                }

                // ── Pattern matching ──────────────────────────────────
                Op::MatchTag => {
                    let dst = instr.dst as usize;
                    let scrutinee_reg = instr.a as usize;
                    let tag_idx = instr.b as usize;

                    let tag_src = match &func.constants[tag_idx] {
                        Value::Str(s) => s.as_str(),
                        _ => return Err("MatchTag: tag must be string".into()),
                    };
                    let (data_id, slen) = self.intern_string(tag_src);

                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let gv = self.module.declare_data_in_func(data_id, builder.func);
                    let ptr_val = builder.ins().global_value(self.ptr, gv);
                    let len_val = builder.ins().iconst(types::I64, slen as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let mt_ref = self.module.declare_func_in_func(self.rt.match_tag, builder.func);
                    let scrutinee = builder.use_var(vars[scrutinee_reg]);
                    let call = builder.ins().call(mt_ref, &[scrutinee, tag_rt]);
                    let match_result = builder.inst_results(call)[0];

                    let zero = builder.ins().iconst(self.ptr, 0);
                    let is_null =
                        builder
                            .ins()
                            .icmp(ir::condcodes::IntCC::Equal, match_result, zero);
                    let is_null_i64 = builder.ins().uextend(types::I64, is_null);

                    let match_blk = builder.create_block();
                    let nomatch_blk = builder.create_block();
                    let cont_blk = builder.create_block();

                    builder.ins().brif(is_null_i64, nomatch_blk, &[], match_blk, &[]);

                    builder.switch_to_block(match_blk);
                    builder.def_var(vars[dst], match_result);
                    let one = builder.ins().iconst(types::I64, 1);
                    builder.def_var(match_flag_var, one);
                    builder.ins().jump(cont_blk, &[]);

                    builder.switch_to_block(nomatch_blk);
                    let zero_flag = builder.ins().iconst(types::I64, 0);
                    builder.def_var(match_flag_var, zero_flag);
                    builder.ins().jump(cont_blk, &[]);

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

                    // "Ok" tag via data section
                    let (ok_data_id, ok_len) = self.intern_string("Ok");
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let gv = self.module.declare_data_in_func(ok_data_id, builder.func);
                    let ptr_val = builder.ins().global_value(self.ptr, gv);
                    let len_val = builder.ins().iconst(types::I64, ok_len as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let mt_ref = self.module.declare_func_in_func(self.rt.match_tag, builder.func);
                    let src_val = builder.use_var(vars[src_reg]);
                    let call = builder.ins().call(mt_ref, &[src_val, tag_rt]);
                    let match_result = builder.inst_results(call)[0];

                    let zero = builder.ins().iconst(self.ptr, 0);
                    let is_null =
                        builder
                            .ins()
                            .icmp(ir::condcodes::IntCC::Equal, match_result, zero);
                    let is_null_i64 = builder.ins().uextend(types::I64, is_null);

                    let ok_blk = builder.create_block();

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
                    // No-op in AOT — ownership is enforced at the bytecode VM level
                }
            }
        }

        // Implicit return nil if last instruction didn't terminate.
        if !last_was_terminator {
            let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
            let call = builder.ins().call(nil_ref, &[]);
            let result = builder.inst_results(call)[0];
            builder.ins().return_(&[result]);
        }

        builder.seal_all_blocks();
        builder.finalize();

        // Debug output
        if std::env::var("AIRL_AOT_DEBUG").as_deref() == Ok("1") {
            eprintln!("[AOT] Cranelift IR for {}:\n{}", func.name, ctx.func.display());
        }

        // Pre-verify to get detailed error
        let mut vb = cranelift_codegen::settings::builder();
        let _ = vb.set("unwind_info", "false");
        let _ = vb.set("is_pic", "true");
        let _ = vb.set("enable_probestack", "false");
        let flags = cranelift_codegen::settings::Flags::new(vb);
        if let Err(errs) = cranelift_codegen::verify_function(&ctx.func, &flags) {
            eprintln!("[AOT] VERIFIER ERRORS for '{}':\n{}", func.name, errs);
            eprintln!("[AOT] Full IR:\n{}", ctx.func.display());
            return Err(format!("verify: {}", errs));
        }

        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| {
                let err_str = format!("{:#}", e);
                eprintln!("[AOT] DEFINE FAILED for '{}': {}", func.name, err_str);
                format!("define: {}", err_str)
            })?;

        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Entry point
    // ──────────────────────────────────────────────────────────────────────

    /// Emit entry point: `main()` for hosted, `_start()` for freestanding.
    pub fn emit_entry_point(&mut self) -> Result<(), String> {
        let is_freestanding = self.target_triple.operating_system == target_lexicon::OperatingSystem::None_;
        let entry_name = if is_freestanding { "_start" } else { "main" };

        let mut sig = self.module.make_signature();
        sig.returns.push(AbiParam::new(types::I32));

        let main_id = self
            .module
            .declare_function(entry_name, Linkage::Export, &sig)
            .map_err(|e| format!("declare {}: {}", entry_name, e))?;

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        let entry = builder.create_block();
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        // Call __main__()
        let main_func_id = self
            .compiled_funcs
            .get("__main__")
            .ok_or("no __main__ function found")?;
        let main_ref = self.module.declare_func_in_func(*main_func_id, builder.func);
        builder.ins().call(main_ref, &[]);

        // Flush stdout before exit
        let flush_sig = self.module.make_signature(); // () -> void
        let flush_id = self.module
            .declare_function("airl_flush_stdout", Linkage::Import, &flush_sig)
            .map_err(|e| format!("declare flush: {}", e))?;
        let flush_ref = self.module.declare_func_in_func(flush_id, builder.func);
        builder.ins().call(flush_ref, &[]);

        // Return 0
        let zero = builder.ins().iconst(types::I32, 0);
        builder.ins().return_(&[zero]);
        builder.finalize();

        self.module
            .define_function(main_id, &mut ctx)
            .map_err(|e| format!("define main: {}", e))?;

        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Finalize
    // ──────────────────────────────────────────────────────────────────────

    /// Finalize the object module and return the raw object file bytes.
    pub fn finish(self) -> Vec<u8> {
        let product = self.module.finish();
        product.emit().unwrap()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// C-ABI: Full compile-to-executable pipeline
// ─────────────────────────────────────────────────────────────────────────────

// Embedded stdlib sources
pub const COLLECTIONS_SOURCE: &str = include_str!("../../../stdlib/prelude.airl");
pub const MATH_SOURCE: &str = include_str!("../../../stdlib/math.airl");
pub const RESULT_SOURCE: &str = include_str!("../../../stdlib/result.airl");
pub const STRING_SOURCE: &str = include_str!("../../../stdlib/string.airl");
pub const MAP_SOURCE: &str = include_str!("../../../stdlib/map.airl");
pub const SET_SOURCE: &str = include_str!("../../../stdlib/set.airl");
pub const IO_SOURCE: &str = include_str!("../../../stdlib/io.airl");

/// An extern-c declaration extracted from source: C symbol name + arity.
#[derive(Debug, Clone)]
pub struct ExternCInfo {
    pub c_name: String,
    pub arity: usize,
}

/// Compile source string to bytecode functions via the Rust-side pipeline.
pub fn compile_source_to_bytecode(
    source: &str,
    prefix: &str,
) -> Result<(Vec<BytecodeFunc>, BytecodeFunc), String> {
    let (funcs, main, _externs) = compile_source_to_bytecode_with_externs(source, prefix)?;
    Ok((funcs, main))
}

/// Like `compile_source_to_bytecode` but also returns extern-c declarations.
pub fn compile_source_to_bytecode_with_externs(
    source: &str,
    prefix: &str,
) -> Result<(Vec<BytecodeFunc>, BytecodeFunc, Vec<ExternCInfo>), String> {
    use airl_syntax::*;
    use crate::bytecode_compiler::BytecodeCompiler;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(|d| format!("lex: {}", d.message))?;
    let sexprs = parse_sexpr_all(&tokens).map_err(|d| format!("parse: {}", d.message))?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    let mut extern_c_decls = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => {
                if let airl_syntax::ast::TopLevel::ExternC(ref decl) = top {
                    extern_c_decls.push(ExternCInfo {
                        c_name: decl.c_name.clone(),
                        arity: decl.params.len(),
                    });
                }
                tops.push(top);
            }
            Err(d) => {
                let mut diags2 = Diagnostics::new();
                match parser::parse_expr(sexpr, &mut diags2) {
                    Ok(expr) => tops.push(airl_syntax::ast::TopLevel::Expr(expr)),
                    Err(_) => return Err(format!("parse: {}", d.message)),
                }
            }
        }
    }

    let ir_nodes: Vec<crate::ir::IRNode> = tops.iter()
        .map(crate::ast_to_ir::compile_top_level)
        .collect();
    let mut bc_compiler = BytecodeCompiler::with_prefix(prefix);
    let (funcs, main) = bc_compiler.compile_program(&ir_nodes);
    Ok((funcs, main, extern_c_decls))
}

/// Full pipeline: source files → native executable.
/// Called from AOT-compiled native binaries (the self-hosting compiler).
pub fn compile_to_executable_impl(
    source_paths: &[String],
    output_path: &str,
) -> Result<(), String> {
    let mut all_funcs: Vec<BytecodeFunc> = Vec::new();

    // 1. Compile stdlib (pure AIRL — no extern-c)
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
    ] {
        let (funcs, _stdlib_main) = compile_source_to_bytecode(src, name)?;
        all_funcs.extend(funcs);
    }

    // 1b. Compile stdlib with extern-c declarations (io.airl)
    let mut stdlib_extern_c_decls: Vec<ExternCInfo> = Vec::new();
    for (src, name) in &[
        (IO_SOURCE, "io"),
    ] {
        let (funcs, _stdlib_main, externs) = compile_source_to_bytecode_with_externs(src, name)?;
        all_funcs.extend(funcs);
        stdlib_extern_c_decls.extend(externs);
    }

    // 2. Compile user sources
    let mut all_source = String::new();
    for path in source_paths {
        let s = std::fs::read_to_string(path)
            .map_err(|e| format!("{}: {}", path, e))?;
        all_source.push_str(&s);
        all_source.push('\n');
    }
    let (funcs, main_func, extern_c_decls) =
        compile_source_to_bytecode_with_externs(&all_source, "user")?;
    all_funcs.extend(funcs);
    all_funcs.push(main_func);

    // 3. AOT compile
    let func_map: HashMap<String, BytecodeFunc> = all_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new()?;
    for ext in stdlib_extern_c_decls.iter().chain(&extern_c_decls) {
        aot.register_extern_c(&ext.c_name, ext.arity);
    }
    for func in &all_funcs {
        let _ = aot.compile_all(std::slice::from_ref(func), &func_map);
    }
    aot.emit_entry_point()?;
    let obj_bytes = aot.finish();

    // 4. Write object file
    let obj_path = format!("{}.o", output_path);
    std::fs::write(&obj_path, &obj_bytes)
        .map_err(|e| format!("write {}: {}", obj_path, e))?;

    // 5. Find libraries and link
    // Only link libairl_runtime.a if the program uses builtins that need it
    // (run-bytecode, compile-to-executable). Normal programs only need libairl_rt.a.
    let needs_runtime = all_funcs.iter().any(|f| {
        f.constants.iter().any(|c| matches!(c,
            Value::Str(s) if s == "run-bytecode" || s == "compile-to-executable"))
    });

    let rt_lib = get_or_extract_rt_lib()?;
    let mut cmd = std::process::Command::new("cc");
    cmd.arg(&obj_path).arg("-o").arg(output_path);
    cmd.arg(&rt_lib);
    if needs_runtime {
        let runtime_lib = find_lib("airl_runtime");
        if !runtime_lib.is_empty() {
            cmd.arg(&runtime_lib);
            if !rt_lib.is_empty() { cmd.arg(&rt_lib); }
        }
    }
    #[cfg(target_os = "linux")]
    { cmd.arg("-lm").arg("-lpthread").arg("-ldl"); }
    #[cfg(target_os = "macos")]
    { cmd.arg("-lSystem"); }

    let status = cmd.status().map_err(|e| format!("linker: {}", e))?;
    let _ = std::fs::remove_file(&obj_path);
    // SEC-14: clean up temp runtime library after linking
    if rt_lib.starts_with(&std::env::temp_dir().to_string_lossy().to_string()) {
        let _ = std::fs::remove_file(&rt_lib);
    }

    if status.success() {
        Ok(())
    } else {
        Err(format!("linker failed: {:?}", status.code()))
    }
}

/// Compressed libairl_rt.a embedded at build time.
/// Empty if the library wasn't found during compilation (development builds).
const EMBEDDED_RT_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libairl_rt.a.gz"));

/// Generate a unique temp file path for the runtime library.
/// Uses PID and timestamp to avoid predictable paths (SEC-14: TOCTOU mitigation).
fn unique_rt_temp_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("libairl_rt_{}_{}.a", std::process::id(), nanos))
}

/// Extract the embedded compressed runtime to a temp file. Returns the path, or None if not embedded.
pub fn extract_embedded_rt() -> Option<String> {
    if EMBEDDED_RT_GZ.is_empty() { return None; }
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(EMBEDDED_RT_GZ);
    let mut data = Vec::new();
    decoder.read_to_end(&mut data).ok()?;
    let tmp = unique_rt_temp_path();
    std::fs::write(&tmp, &data).ok()?;
    Some(tmp.to_string_lossy().to_string())
}

/// Get the runtime library path: try embedded first, fall back to disk search.
pub fn get_or_extract_rt_lib() -> Result<String, String> {
    // If embedded runtime is available (non-empty), decompress to temp
    if !EMBEDDED_RT_GZ.is_empty() {
        use std::io::Read;
        let mut decoder = flate2::read::GzDecoder::new(EMBEDDED_RT_GZ);
        let mut data = Vec::new();
        decoder.read_to_end(&mut data)
            .map_err(|e| format!("decompress embedded runtime: {}", e))?;
        let tmp = unique_rt_temp_path();
        std::fs::write(&tmp, &data)
            .map_err(|e| format!("write runtime to {}: {}", tmp.display(), e))?;
        return Ok(tmp.to_string_lossy().to_string());
    }
    // Fall back to searching on disk (development builds)
    let path = find_lib("airl_rt");
    if path.is_empty() {
        Err("libairl_rt.a not found. Build with: cargo build --features aot -p airl-rt".into())
    } else {
        Ok(path)
    }
}

pub fn find_lib(name: &str) -> String {
    let candidates = [
        format!("target/release/lib{}.a", name),
        format!("target/debug/lib{}.a", name),
        format!("../target/release/lib{}.a", name),
        format!("../target/debug/lib{}.a", name),
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let lib = dir.join(format!("lib{}.a", name));
            if lib.exists() { return lib.to_string_lossy().to_string(); }
        }
    }
    String::new()
}

/// C-ABI entry point: compile source files to a native executable.
/// Takes (List[Str paths], Str output_path) → Nil on success, calls rt_error on failure.
#[no_mangle]
pub extern "C" fn airl_compile_to_executable(
    paths_val: *mut airl_rt::value::RtValue,
    output_val: *mut airl_rt::value::RtValue,
) -> *mut airl_rt::value::RtValue {
    use airl_rt::value::*;

    let paths: Vec<String> = unsafe {
        match &(*paths_val).data {
            RtData::List { .. } => airl_rt::list::list_items(&(*paths_val).data).iter().map(|p| {
                match &(**p).data {
                    RtData::Str(s) => s.clone(),
                    _ => String::new(),
                }
            }).collect(),
            _ => {
                crate::error::RuntimeError::TypeError(
                    "compile: expected list of paths".into());
                return rt_nil();
            }
        }
    };
    let output = unsafe {
        match &(*output_val).data {
            RtData::Str(s) => s.clone(),
            _ => "a.out".to_string(),
        }
    };

    match compile_to_executable_impl(&paths, &output) {
        Ok(()) => {
            eprintln!("Compiled to {}", output);
            rt_nil()
        }
        Err(e) => {
            use airl_rt::value::{rt_variant, rt_str};
            rt_variant("Err".into(), rt_str(format!("Compilation error: {}", e)))
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
        let aot = BytecodeAot::new();
        assert!(aot.is_ok(), "BytecodeAot::new() failed: {:?}", aot.err());
    }

    #[test]
    fn builtin_map_contains_expected_keys() {
        let aot = BytecodeAot::new().unwrap();
        for key in [
            "+", "-", "*", "/", "%",
            "=", "!=", "<", ">", "<=", ">=",
            "not", "and", "or", "xor",
            "head", "tail", "cons", "empty?", "length", "at", "append",
            "print", "type-of", "valid",
            "char-at", "substring", "chars", "split", "join",
            "replace",
            "map-new", "map-get", "map-set",
            "map-has", "map-remove", "map-keys",
        ] {
            assert!(aot.builtin_map.contains_key(key), "missing builtin: {}", key);
        }
    }

    #[test]
    fn intern_string_creates_data_section() {
        let mut aot = BytecodeAot::new().unwrap();
        let (data_id, len) = aot.intern_string("hello");
        assert_eq!(len, 5);
        assert_eq!(aot.stable_strings.len(), 1);
        assert_eq!(aot.stable_strings[0], (data_id, 5));
    }

    #[test]
    fn compile_simple_function() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function(
            "add",
            &["a".into(), "b".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]),
        );
        let all = HashMap::new();
        let mut aot = BytecodeAot::new().unwrap();
        let result = aot.compile_func(&func, &all);
        assert!(result.is_ok(), "compile_func failed: {:?}", result.err());
        assert!(aot.compiled_funcs.contains_key("add"));
    }

    #[test]
    fn compile_and_finish_produces_object_bytes() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function(
            "__main__",
            &[],
            &IRNode::Int(42),
        );
        let all: HashMap<String, BytecodeFunc> = HashMap::new();
        let mut aot = BytecodeAot::new().unwrap();
        aot.compile_func(&func, &all).unwrap();
        aot.emit_entry_point().unwrap();
        let bytes = aot.finish();
        assert!(bytes.len() > 4, "object file too small: {} bytes", bytes.len());
        if cfg!(target_os = "linux") {
            assert_eq!(&bytes[..4], b"\x7fELF", "not a valid ELF object file");
        }
    }

    // Unboxed compilation eligibility tests removed — unboxed path is disabled.

    #[test]
    fn unresolved_function_produces_warning() {
        // Calling a function that doesn't exist should produce a warning
        // (printed to stderr) but still compile — needed for G3 self-compilation
        // where bootstrap module functions are linked separately.
        let func = BytecodeFunc {
            name: "caller".into(),
            arity: 0,
            register_count: 4,
            capture_count: 0,
            instructions: vec![
                Instruction::new(Op::LoadConst, 0, 0, 0),  // r0 = 42
                Instruction::new(Op::Move, 2, 0, 0),       // r2 = r0 (arg slot)
                Instruction::new(Op::Call, 1, 1, 1),        // r1 = nonexistent-fn(r2); name_idx=1, argc=1
                Instruction::new(Op::Return, 0, 1, 0),
            ],
            constants: vec![Value::Int(42), Value::Str("nonexistent-function".into())],
        };
        let all: HashMap<String, BytecodeFunc> = HashMap::new();
        let mut aot = BytecodeAot::new().unwrap();
        // Should succeed (with a warning to stderr) rather than error
        let result = aot.compile_func(&func, &all);
        assert!(result.is_ok(), "Unresolved function should warn, not error: {:?}", result.err());
    }

    // --- SEC-14 tests: unique temp file paths ---

    #[test]
    fn unique_rt_temp_path_is_unpredictable() {
        let p1 = unique_rt_temp_path();
        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(2));
        let p2 = unique_rt_temp_path();
        assert_ne!(p1, p2, "Consecutive temp paths should differ");
        // Verify paths contain PID
        let pid = std::process::id().to_string();
        assert!(p1.to_string_lossy().contains(&pid),
            "Temp path should contain PID: {:?}", p1);
    }

    #[test]
    fn unique_rt_temp_path_not_predictable_fixed_name() {
        let p = unique_rt_temp_path();
        let name = p.file_name().unwrap().to_string_lossy();
        // Must NOT be the old predictable name
        assert_ne!(name, "libairl_rt.a",
            "Temp path should not use the fixed predictable name");
        assert!(name.starts_with("libairl_rt_"),
            "Temp path should start with libairl_rt_ prefix: {}", name);
    }

    #[test]
    fn unique_rt_temp_path_is_in_temp_dir() {
        let p = unique_rt_temp_path();
        let temp = std::env::temp_dir();
        assert!(p.starts_with(&temp),
            "Temp path should be under temp dir: {:?} vs {:?}", p, temp);
    }
}
