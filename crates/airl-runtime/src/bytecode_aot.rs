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

use std::collections::{BTreeSet, HashMap};

use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder, MemFlags, StackSlotData};
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

pub struct BytecodeAot {
    module: ObjectModule,
    rt: RuntimeImports,
    builtin_map: HashMap<String, FuncId>,
    /// String constants stored as (DataId, byte_length) in the object file's
    /// data section — replaces the JIT's heap-pointer approach.
    stable_strings: Vec<(cranelift_module::DataId, usize)>,
    /// Compiled function names → FuncId for `func_addr` in closures.
    compiled_funcs: HashMap<String, FuncId>,
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
            print, print_values, type_of, valid,
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
        let mut in_progress = std::collections::HashSet::new();
        for func in funcs {
            if !self.compiled_funcs.contains_key(&func.name) {
                self.compile_with_deps(func, all_functions, &mut in_progress)?;
            }
        }
        Ok(())
    }

    /// Compile a function and its dependencies (recursively).
    fn compile_with_deps(
        &mut self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
        in_progress: &mut std::collections::HashSet<String>,
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
                            self.compile_with_deps(&callee, all_functions, in_progress)?;
                        }
                    }
                }
            }
        }

        if std::env::var("AIRL_AOT_DEBUG").as_deref() == Ok("1") {
            eprintln!("[AOT] compiling {} ({} instrs)", name, func.instructions.len());
        }
        self.compile_func(func, all_functions)?;
        in_progress.remove(&name);
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Core Cranelift IR emitter
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
}
