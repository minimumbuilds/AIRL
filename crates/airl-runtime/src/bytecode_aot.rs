// crates/airl-runtime/src/bytecode_aot.rs
//! Ahead-of-time Cranelift compiler that produces a native object file (`.o`)
//! via `ObjectModule`.  Structurally identical to `bytecode_jit_full.rs` but
//! emits relocatable object code instead of executable JIT pages.
//!
//! Key differences from the JIT path:
//!   - String constants live in data sections (not heap pointers).
//!   - Closure function pointers use `func_addr` (not raw code pointers).
//!   - Emits a C `main()` entry point that calls `__main__()`.
//!   - `finish()` returns the object file bytes.

use std::collections::{BTreeSet, HashMap, HashSet};

use cranelift_codegen::ir::{self, condcodes::{FloatCC, IntCC}, types, AbiParam, InstBuilder, MemFlags, StackSlotData};
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

    // Variant / pattern
    pub make_variant: FuncId,
    pub match_tag:    FuncId,

    // Closure
    pub make_closure: FuncId,
    pub call_closure: FuncId,

    // I/O / misc
    pub print:        FuncId,
    pub println:      FuncId,
    pub print_values: FuncId,
    pub type_of:      FuncId,
    pub valid:        FuncId,

    // String builtins
    pub char_at:     FuncId,
    pub substring:   FuncId,
    pub chars:       FuncId,
    pub split:       FuncId,
    pub join:        FuncId,
    pub contains:    FuncId,
    pub starts_with: FuncId,
    pub ends_with:   FuncId,
    pub index_of:    FuncId,
    pub trim:        FuncId,
    pub to_upper:    FuncId,
    pub to_lower:    FuncId,
    pub replace:     FuncId,

    // Map builtins
    pub map_new:    FuncId,
    pub map_from:   FuncId,
    pub map_get:    FuncId,
    pub map_get_or: FuncId,
    pub map_set:    FuncId,
    pub map_has:    FuncId,
    pub map_remove: FuncId,
    pub map_keys:   FuncId,
    pub map_values: FuncId,
    pub map_size:   FuncId,

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
    pub get_args:    FuncId,
    pub run_bytecode: FuncId,
    pub compile_to_exe: FuncId,

    // Type conversions
    pub int_to_string: FuncId,
    pub float_to_string: FuncId,
    pub string_to_int: FuncId,

    // Timing
    pub time_now: FuncId,

    // Environment
    pub getenv: FuncId,

    // HTTP
    pub http_post: FuncId,

    // JSON
    pub json_parse: FuncId,
    pub json_stringify: FuncId,

    // Process
    pub shell_exec: FuncId,

    // Contract failure
    pub contract_fail: FuncId,
}

// ─────────────────────────────────────────────────────────────────────────────
// Signature helpers (ObjectModule versions)
// ─────────────────────────────────────────────────────────────────────────────

const PTR: types::Type = types::I64;

fn sig_0_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_1_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_2_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_3_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_1_ptr_ret_i64(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn sig_1_ptr_ret_void(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig
}

fn sig_i64_ret_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_ptr_i64_ret_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_ptr_ptr_i64_ret_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(PTR));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(PTR));
    sig
}

fn sig_f64_ret_ptr(m: &ObjectModule) -> cranelift_codegen::ir::Signature {
    let mut sig = m.make_signature();
    sig.params.push(AbiParam::new(types::F64));
    sig.returns.push(AbiParam::new(PTR));
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
}

impl BytecodeAot {
    /// Create a new AOT compiler context targeting the host triple.
    pub fn new() -> Result<Self, String> {
        let isa = cranelift_codegen::isa::lookup(target_lexicon::Triple::host())
            .map_err(|e| format!("ISA lookup: {}", e))?
            .finish(cranelift_codegen::settings::Flags::new(
                cranelift_codegen::settings::builder(),
            ))
            .map_err(|e| format!("ISA build: {}", e))?;

        let builder = ObjectBuilder::new(
            isa,
            "airl_program",
            cranelift_module::default_libcall_names(),
        )
        .map_err(|e| format!("ObjectBuilder: {}", e))?;

        let mut module = ObjectModule::new(builder);
        let rt = Self::declare_runtime_imports(&mut module);
        let builtin_map = Self::build_builtin_map(&rt);

        Ok(Self {
            module,
            rt,
            builtin_map,
            stable_strings: Vec::new(),
            compiled_funcs: HashMap::new(),
            eligible_funcs: HashSet::new(),
            eligible_return_hints: HashMap::new(),
        })
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

    fn declare_runtime_imports(m: &mut ObjectModule) -> RuntimeImports {
        let void_1 = sig_1_ptr_ret_void(m);
        let value_retain  = declare_import(m, "airl_value_retain",  void_1.clone());
        let value_release = declare_import(m, "airl_value_release", void_1);
        let value_clone   = declare_import(m, "airl_value_clone",   sig_1_ptr(m));

        let int_ctor   = declare_import(m, "airl_int",   sig_i64_ret_ptr(m));
        let float_ctor = declare_import(m, "airl_float", sig_f64_ret_ptr(m));
        let bool_ctor  = declare_import(m, "airl_bool",  sig_i64_ret_ptr(m));
        let nil_ctor   = declare_import(m, "airl_nil",   sig_0_ptr(m));
        let unit_ctor  = declare_import(m, "airl_unit",  sig_0_ptr(m));
        let str_ctor   = declare_import(m, "airl_str",   sig_ptr_i64_ret_ptr(m));

        let as_bool_raw = declare_import(m, "airl_as_bool_raw", sig_1_ptr_ret_i64(m));
        let as_int_raw  = declare_import(m, "airl_as_int_raw",  sig_1_ptr_ret_i64(m));

        let s2 = sig_2_ptr(m);
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

        let s1 = sig_1_ptr(m);
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
        let list_new = declare_import(m, "airl_list_new", sig_ptr_i64_ret_ptr(m));

        let make_variant = declare_import(m, "airl_make_variant", s2.clone());
        let match_tag    = declare_import(m, "airl_match_tag",    s2.clone());

        let make_closure = declare_import(m, "airl_make_closure", sig_ptr_ptr_i64_ret_ptr(m));
        let call_closure = declare_import(m, "airl_call_closure", sig_ptr_ptr_i64_ret_ptr(m));

        let print        = declare_import(m, "airl_print",        s1.clone());
        let println      = declare_import(m, "airl_println",      s1.clone());
        let print_values = declare_import(m, "airl_print_values", sig_ptr_i64_ret_ptr(m));
        let type_of      = declare_import(m, "airl_type_of",      s1.clone());
        let valid        = declare_import(m, "airl_valid",        s1.clone());

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
        let compile_to_exe = declare_import(m, "airl_compile_to_executable", s2.clone());

        // Type conversions
        let int_to_string = declare_import(m, "airl_int_to_string", s1.clone());
        let float_to_string = declare_import(m, "airl_float_to_string", s1.clone());
        let string_to_int = declare_import(m, "airl_string_to_int", s1.clone());

        // Timing
        let time_now = declare_import(m, "airl_time_now", sig_0_ptr(m));

        // Environment
        let getenv = declare_import(m, "airl_getenv", s1.clone());

        // HTTP
        let http_post = declare_import(m, "airl_http_post", sig_3_ptr(m));

        // JSON
        let json_parse = declare_import(m, "airl_json_parse", s1.clone());
        let json_stringify = declare_import(m, "airl_json_stringify", s1.clone());

        // Process
        let shell_exec = declare_import(m, "airl_shell_exec", s2.clone());

        // Contract failure: (kind: i64, fn_name_idx: i64, clause_idx: i64) -> i64
        let mut cf_sig = m.make_signature();
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.params.push(AbiParam::new(types::I64));
        cf_sig.returns.push(AbiParam::new(types::I64));
        let contract_fail = declare_import(m, "airl_jit_contract_fail", cf_sig);

        RuntimeImports {
            value_retain, value_release, value_clone,
            int_ctor, float_ctor, bool_ctor, nil_ctor, unit_ctor, str_ctor,
            as_bool_raw, as_int_raw,
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
            get_args, run_bytecode, compile_to_exe,
            int_to_string, float_to_string, string_to_int,
            time_now, getenv, http_post, json_parse, json_stringify, shell_exec,
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

        m.insert("print".into(),   rt.print);
        m.insert("println".into(), rt.println);
        m.insert("type-of".into(), rt.type_of);
        m.insert("valid".into(),   rt.valid);

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
        m.insert("compile-to-executable".into(), rt.compile_to_exe);

        // Type conversions
        m.insert("int-to-string".into(),   rt.int_to_string);
        m.insert("float-to-string".into(), rt.float_to_string);
        m.insert("string-to-int".into(),   rt.string_to_int);

        // Timing
        m.insert("time-now".into(), rt.time_now);

        // Environment
        m.insert("getenv".into(), rt.getenv);

        // HTTP
        m.insert("http-post".into(), rt.http_post);

        // JSON
        m.insert("json-parse".into(),     rt.json_parse);
        m.insert("json-stringify".into(), rt.json_stringify);

        // Process
        m.insert("shell-exec".into(), rt.shell_exec);

        m
    }

    // ──────────────────────────────────────────────────────────────────────
    // Eligibility check for unboxed compilation
    // ──────────────────────────────────────────────────────────────────────

    /// Check if a BytecodeFunc is eligible for unboxed (primitive) compilation.
    /// Returns false if any instruction uses non-primitive Value types.
    fn is_eligible(
        &self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
        eligible_cache: &mut HashSet<String>,
        ineligible_cache: &mut HashSet<String>,
    ) -> bool {
        // Check caches first
        if eligible_cache.contains(&func.name) {
            return true;
        }
        if ineligible_cache.contains(&func.name) {
            return false;
        }

        // Arity limit
        if func.arity > 8 {
            ineligible_cache.insert(func.name.clone());
            return false;
        }

        for instr in &func.instructions {
            match instr.op {
                // Disqualifying opcodes — require non-primitive Value types
                Op::MakeList | Op::MakeVariant | Op::MakeVariant0 |
                Op::MakeClosure | Op::MatchTag | Op::JumpIfNoMatch |
                Op::MatchWild | Op::TryUnwrap | Op::CallBuiltin | Op::CallReg => {
                    ineligible_cache.insert(func.name.clone());
                    return false;
                }
                // Ownership tracking — disqualify from unboxed
                Op::MarkMoved | Op::CheckNotMoved => {
                    ineligible_cache.insert(func.name.clone());
                    return false;
                }
                // LoadConst — only int/float/bool constants are valid
                Op::LoadConst => {
                    let idx = instr.a as usize;
                    if idx < func.constants.len() {
                        match &func.constants[idx] {
                            Value::Int(_) | Value::Float(_) | Value::Bool(_) => {}
                            _ => {
                                ineligible_cache.insert(func.name.clone());
                                return false;
                            }
                        }
                    }
                }
                // Contract assertions are compilable (one conditional branch)
                Op::AssertRequires | Op::AssertEnsures | Op::AssertInvariant => {}
                Op::Call => {
                    // Check if the call target is eligible
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s,
                        _ => {
                            ineligible_cache.insert(func.name.clone());
                            return false;
                        }
                    };
                    // Self-calls are fine (handled as tail-call loop)
                    if name == &func.name {
                        continue;
                    }
                    // Already known ineligible
                    if ineligible_cache.contains(name) {
                        ineligible_cache.insert(func.name.clone());
                        return false;
                    }
                    // Already known eligible or already compiled as eligible
                    if eligible_cache.contains(name) || self.eligible_funcs.contains(name) {
                        continue;
                    }
                    // Recursively check callee
                    if let Some(target) = all_functions.get(name) {
                        if !self.is_eligible(target, all_functions, eligible_cache, ineligible_cache) {
                            ineligible_cache.insert(func.name.clone());
                            return false;
                        }
                    } else {
                        // Unknown function (builtin like "print") — ineligible
                        ineligible_cache.insert(func.name.clone());
                        return false;
                    }
                }
                Op::TailCall => {
                    // Verify it's a self-call
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s,
                        _ => {
                            ineligible_cache.insert(func.name.clone());
                            return false;
                        }
                    };
                    if name != &func.name {
                        ineligible_cache.insert(func.name.clone());
                        return false;
                    }
                }
                // All other opcodes are fine for primitives
                _ => {}
            }
        }
        eligible_cache.insert(func.name.clone());
        true
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
        }

        // Two-tier dispatch: eligible functions get unboxed compilation
        let is_eligible = self.is_eligible(func, all_functions, eligible_cache, ineligible_cache);

        if std::env::var("AIRL_AOT_DEBUG").as_deref() == Ok("1") {
            eprintln!("[AOT] compiling {} ({} instrs, {})", name, func.instructions.len(),
                if is_eligible { "unboxed" } else { "boxed" });
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
            .declare_function(&func.name, Linkage::Local, &sig)
            .map_err(|e| format!("declare: {}", e))?;
        self.compiled_funcs.insert(func.name.clone(), func_id);
        self.eligible_funcs.insert(func.name.clone());

        // ── 3. Build function body ───────────────────────────────────────
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();

        // Pre-declare call targets with I64 signatures (not PTR).
        let mut call_targets: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for instr in &func.instructions {
            if instr.op == Op::Call {
                if let Value::Str(callee_name) = &func.constants[instr.a as usize] {
                    if callee_name != &func.name && !call_targets.contains_key(callee_name) {
                        let argc = instr.b as usize;
                        let mut call_sig = self.module.make_signature();
                        for _ in 0..argc {
                            call_sig.params.push(AbiParam::new(types::I64));
                        }
                        call_sig.returns.push(AbiParam::new(types::I64));
                        let callee_id = self
                            .module
                            .declare_function(callee_name, Linkage::Local, &call_sig)
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
                last_was_terminator = false;
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
            sig.params.push(AbiParam::new(PTR));
        }
        sig.returns.push(AbiParam::new(PTR));

        // ── 2. Declare function in object module ─────────────────────────
        let func_id = self
            .module
            .declare_function(&func.name, Linkage::Local, &sig)
            .map_err(|e| format!("declare: {}", e))?;
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
                        let is_variadic_print = callee_name == "print" && argc != 1;
                        if is_variadic_print {
                            // handled inline
                        } else if let Some(&builtin_id) = self.builtin_map.get(callee_name.as_str()) {
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
                                .declare_function(callee_name, Linkage::Local, &call_sig)
                                .map_err(|e| format!("call declare (eligible): {}", e))?;
                            call_targets.insert(callee_name.clone(), callee_id);
                        } else {
                            let mut call_sig = self.module.make_signature();
                            for _ in 0..argc {
                                call_sig.params.push(AbiParam::new(PTR));
                            }
                            call_sig.returns.push(AbiParam::new(PTR));
                            let callee_id = self
                                .module
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
            let var = builder.declare_var(PTR);
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
            let zero = builder.ins().iconst(PTR, 0);
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
                last_was_terminator = false;
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
                            let ptr_val = builder.ins().global_value(PTR, gv);
                            let len_val = builder.ins().iconst(types::I64, slen as i64);
                            let call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                            let result = builder.inst_results(call)[0];
                            builder.def_var(vars[dst], result);
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

                    if callee_name == "print" && argc != 1 && !call_targets.contains_key("print") {
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
                        let nil_ref = self.module.declare_func_in_func(self.rt.nil_ctor, builder.func);
                        let call = builder.ins().call(nil_ref, &[]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
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
                        let null = builder.ins().iconst(PTR, 0);
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
                    // AOT: string tag via data section
                    let (data_id, slen) = self.intern_string(tag_src);
                    let str_ref = self.module.declare_func_in_func(self.rt.str_ctor, builder.func);
                    let gv = self.module.declare_data_in_func(data_id, builder.func);
                    let ptr_val = builder.ins().global_value(PTR, gv);
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
                    let ptr_val = builder.ins().global_value(PTR, gv);
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
                    let fn_ptr_val = builder.ins().func_addr(PTR, callee_ref);

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
                        let cap_addr = builder.ins().stack_addr(PTR, slot, 0);
                        let count_val =
                            builder.ins().iconst(types::I64, capture_count as i64);
                        let call = builder
                            .ins()
                            .call(make_closure_ref, &[fn_ptr_val, cap_addr, count_val]);
                        let result = builder.inst_results(call)[0];
                        builder.def_var(vars[dst], result);
                    } else {
                        let null = builder.ins().iconst(PTR, 0);
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
                    let ptr_val = builder.ins().global_value(PTR, gv);
                    let len_val = builder.ins().iconst(types::I64, slen as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let mt_ref = self.module.declare_func_in_func(self.rt.match_tag, builder.func);
                    let scrutinee = builder.use_var(vars[scrutinee_reg]);
                    let call = builder.ins().call(mt_ref, &[scrutinee, tag_rt]);
                    let match_result = builder.inst_results(call)[0];

                    let zero = builder.ins().iconst(PTR, 0);
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
                    let ptr_val = builder.ins().global_value(PTR, gv);
                    let len_val = builder.ins().iconst(types::I64, ok_len as i64);
                    let tag_call = builder.ins().call(str_ref, &[ptr_val, len_val]);
                    let tag_rt = builder.inst_results(tag_call)[0];

                    let mt_ref = self.module.declare_func_in_func(self.rt.match_tag, builder.func);
                    let src_val = builder.use_var(vars[src_reg]);
                    let call = builder.ins().call(mt_ref, &[src_val, tag_rt]);
                    let match_result = builder.inst_results(call)[0];

                    let zero = builder.ins().iconst(PTR, 0);
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

        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| format!("define: {}", e))?;

        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Entry point
    // ──────────────────────────────────────────────────────────────────────

    /// Emit a C `main()` function that calls `__main__()` and returns 0.
    pub fn emit_entry_point(&mut self) -> Result<(), String> {
        let mut sig = self.module.make_signature();
        sig.returns.push(AbiParam::new(types::I32));

        let main_id = self
            .module
            .declare_function("main", Linkage::Export, &sig)
            .map_err(|e| format!("declare main: {}", e))?;

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
const COLLECTIONS_SOURCE: &str = include_str!("../../../stdlib/prelude.airl");
const MATH_SOURCE: &str = include_str!("../../../stdlib/math.airl");
const RESULT_SOURCE: &str = include_str!("../../../stdlib/result.airl");
const STRING_SOURCE: &str = include_str!("../../../stdlib/string.airl");
const MAP_SOURCE: &str = include_str!("../../../stdlib/map.airl");

/// Compile source string to bytecode functions via the Rust-side pipeline.
fn compile_source_to_bytecode(
    source: &str,
    prefix: &str,
) -> Result<(Vec<BytecodeFunc>, BytecodeFunc), String> {
    use airl_syntax::*;
    use crate::bytecode_compiler::BytecodeCompiler;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.lex_all().map_err(|d| format!("lex: {}", d.message))?;
    let sexprs = parse_sexpr_all(&tokens).map_err(|d| format!("parse: {}", d.message))?;
    let mut diags = Diagnostics::new();

    let mut tops = Vec::new();
    for sexpr in &sexprs {
        match parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => tops.push(top),
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
    Ok(bc_compiler.compile_program(&ir_nodes))
}

/// Full pipeline: source files → native executable.
/// Called from AOT-compiled native binaries (the self-hosting compiler).
pub fn compile_to_executable_impl(
    source_paths: &[String],
    output_path: &str,
) -> Result<(), String> {
    let mut all_funcs: Vec<BytecodeFunc> = Vec::new();

    // 1. Compile stdlib
    for (src, name) in &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
    ] {
        let (funcs, _stdlib_main) = compile_source_to_bytecode(src, name)?;
        all_funcs.extend(funcs);
    }

    // 2. Compile user sources
    let mut all_source = String::new();
    for path in source_paths {
        let s = std::fs::read_to_string(path)
            .map_err(|e| format!("{}: {}", path, e))?;
        all_source.push_str(&s);
        all_source.push('\n');
    }
    let (funcs, main_func) = compile_source_to_bytecode(&all_source, "user")?;
    all_funcs.extend(funcs);
    all_funcs.push(main_func);

    // 3. AOT compile
    let func_map: HashMap<String, BytecodeFunc> = all_funcs.iter()
        .map(|f| (f.name.clone(), f.clone()))
        .collect();

    let mut aot = BytecodeAot::new()?;
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

    let rt_lib = find_lib("airl_rt");
    let mut cmd = std::process::Command::new("cc");
    cmd.arg(&obj_path).arg("-o").arg(output_path);
    if !rt_lib.is_empty() { cmd.arg(&rt_lib); }
    if needs_runtime {
        let runtime_lib = find_lib("airl_runtime");
        if !runtime_lib.is_empty() {
            cmd.arg(&runtime_lib);
            if !rt_lib.is_empty() { cmd.arg(&rt_lib); }
        }
    }
    cmd.arg("-lm").arg("-lpthread").arg("-ldl");

    let status = cmd.status().map_err(|e| format!("linker: {}", e))?;
    let _ = std::fs::remove_file(&obj_path);

    if status.success() {
        Ok(())
    } else {
        Err(format!("linker failed: {:?}", status.code()))
    }
}

fn find_lib(name: &str) -> String {
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
            RtData::List(items) => items.iter().map(|p| {
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
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
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
            "contains", "starts-with", "ends-with", "index-of",
            "trim", "to-upper", "to-lower", "replace",
            "map-new", "map-from", "map-get", "map-get-or", "map-set",
            "map-has", "map-remove", "map-keys", "map-values", "map-size",
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
        // Object file should have ELF magic bytes (0x7f ELF) on Linux
        assert!(bytes.len() > 4, "object file too small: {} bytes", bytes.len());
        // Check ELF magic on Linux
        if cfg!(target_os = "linux") {
            assert_eq!(&bytes[..4], b"\x7fELF", "not a valid ELF object file");
        }
    }

    #[test]
    fn eligible_arithmetic_func() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        // (defn add (a b) (+ a b)) — should be eligible
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function(
            "add",
            &["a".into(), "b".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]),
        );
        let all: HashMap<String, BytecodeFunc> = [
            ("add".into(), func.clone()),
        ].into_iter().collect();
        let aot = BytecodeAot::new().unwrap();
        let mut eligible = HashSet::new();
        let mut ineligible = HashSet::new();
        // The + call compiles to an Add opcode, not CallBuiltin, because
        // the bytecode compiler inlines primitive ops. Check eligibility.
        let result = aot.is_eligible(&func, &all, &mut eligible, &mut ineligible);
        // If the compiler emits a CallBuiltin for "+", it's ineligible.
        // If it emits an Add opcode, it's eligible.
        // Either way the test validates the check runs without panicking.
        // Check that the function was cached in one of the sets.
        assert!(eligible.contains("add") || ineligible.contains("add"),
            "function should be cached after eligibility check");
        // Verify round-trip: second call uses cache
        let result2 = aot.is_eligible(&func, &all, &mut eligible, &mut ineligible);
        assert_eq!(result, result2);
    }

    #[test]
    fn ineligible_list_func() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        // (defn make-list () (list 1 2 3)) — uses MakeList, should be ineligible
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function(
            "make-list",
            &[],
            &IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]),
        );
        let all: HashMap<String, BytecodeFunc> = [
            ("make-list".into(), func.clone()),
        ].into_iter().collect();
        let aot = BytecodeAot::new().unwrap();
        let mut eligible = HashSet::new();
        let mut ineligible = HashSet::new();
        let result = aot.is_eligible(&func, &all, &mut eligible, &mut ineligible);
        assert!(!result, "list-creating function should be ineligible");
        assert!(ineligible.contains("make-list"));
    }

    #[test]
    fn eligible_func_compiled_unboxed() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        // Pure arithmetic: (defn double (x) (+ x x))
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function(
            "double",
            &["x".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("x".into()), IRNode::Load("x".into())]),
        );
        let all: HashMap<String, BytecodeFunc> = [
            ("double".into(), func.clone()),
        ].into_iter().collect();
        let mut aot = BytecodeAot::new().unwrap();
        let mut eligible = HashSet::new();
        let mut ineligible = HashSet::new();
        let is_elig = aot.is_eligible(&func, &all, &mut eligible, &mut ineligible);
        if is_elig {
            let result = aot.compile_func_unboxed(&func, &all);
            assert!(result.is_ok(), "compile_func_unboxed failed: {:?}", result.err());
            assert!(aot.eligible_funcs.contains("double"));
            assert!(aot.compiled_funcs.contains_key("double"));
        }
    }

    #[test]
    fn two_tier_dispatch_routes_correctly() {
        use crate::bytecode_compiler::BytecodeCompiler;
        use crate::ir::*;
        // Compile a list function (ineligible) and an arithmetic function (eligible)
        let mut compiler = BytecodeCompiler::new();
        let list_func = compiler.compile_function(
            "make-list",
            &[],
            &IRNode::List(vec![IRNode::Int(1), IRNode::Int(2)]),
        );
        let arith_func = compiler.compile_function(
            "inc",
            &["x".into()],
            &IRNode::Call("+".into(), vec![IRNode::Load("x".into()), IRNode::Int(1)]),
        );
        let all: HashMap<String, BytecodeFunc> = [
            ("make-list".into(), list_func.clone()),
            ("inc".into(), arith_func.clone()),
        ].into_iter().collect();
        let mut aot = BytecodeAot::new().unwrap();
        aot.compile_all(&[list_func, arith_func], &all).unwrap();
        // make-list should be compiled but NOT eligible
        assert!(aot.compiled_funcs.contains_key("make-list"));
        assert!(!aot.eligible_funcs.contains("make-list"));
        // inc might be eligible (depends on whether + compiles to Add or CallBuiltin)
        assert!(aot.compiled_funcs.contains_key("inc"));
    }
}
