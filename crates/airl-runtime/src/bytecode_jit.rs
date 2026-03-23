// crates/airl-runtime/src/bytecode_jit.rs
//! Bytecode→Cranelift JIT compiler.
//!
//! Compiles eligible BytecodeFunc instructions to native x86-64 via Cranelift.
//! Eligible = primitive-typed functions with no list/variant/closure/builtin opcodes.

use std::collections::{BTreeSet, HashMap, HashSet};

use cranelift_codegen::ir::{self, condcodes::{FloatCC, IntCC}, types, AbiParam, InstBuilder, MemFlags};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

use crate::bytecode::*;
use crate::value::Value;
use crate::error::RuntimeError;

/// Type hint for marshaling native results back to Value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeHint {
    Int,
    Float,
    Bool,
}

pub struct BytecodeJit {
    module: JITModule,
    compiled: HashMap<String, (*const u8, TypeHint)>,
    ineligible: HashSet<String>,
}

impl BytecodeJit {
    pub fn new() -> Result<Self, String> {
        let builder = cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            compiled: HashMap::new(),
            ineligible: HashSet::new(),
        })
    }

    /// Check if a BytecodeFunc is eligible for JIT compilation.
    /// Returns false if any instruction uses non-primitive operations.
    fn is_eligible(func: &BytecodeFunc, all_functions: &HashMap<String, BytecodeFunc>, compiled: &HashMap<String, (*const u8, TypeHint)>, ineligible: &HashSet<String>) -> bool {
        for instr in &func.instructions {
            match instr.op {
                // Disqualifying opcodes — require non-primitive Value types
                Op::MakeList | Op::MakeVariant | Op::MakeVariant0 |
                Op::MakeClosure | Op::MatchTag | Op::JumpIfNoMatch |
                Op::MatchWild | Op::TryUnwrap | Op::CallBuiltin | Op::CallReg => {
                    return false;
                }
                Op::Call => {
                    // Check if the call target is JIT-eligible
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s,
                        _ => return false,
                    };
                    // Self-calls are fine (handled as recursion)
                    if name == &func.name {
                        continue;
                    }
                    // Target must be already compiled or at least not ineligible
                    if ineligible.contains(name) {
                        return false;
                    }
                    if !compiled.contains_key(name) {
                        // Target not yet compiled — check if it exists and is eligible
                        if let Some(target) = all_functions.get(name) {
                            if !Self::is_eligible(target, all_functions, compiled, ineligible) {
                                return false;
                            }
                        } else {
                            // Unknown function (builtin like "print") — ineligible
                            return false;
                        }
                    }
                }
                Op::TailCall => {
                    // Verify it's a self-call
                    let name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s,
                        _ => return false,
                    };
                    if name != &func.name {
                        return false; // Cross-function tail call — not supported
                    }
                }
                // All other opcodes are fine for primitives
                _ => {}
            }
        }
        // Check arity limit
        if func.arity > 8 {
            return false;
        }
        true
    }

    /// Try to JIT-compile a function. On success, stores the native pointer in `self.compiled`.
    /// On ineligibility or error, marks as ineligible. Optionally prints debug info.
    pub fn try_compile(
        &mut self,
        func: &BytecodeFunc,
        all_functions: &HashMap<String, BytecodeFunc>,
    ) {
        let name = func.name.clone();

        // Already compiled or already marked ineligible — skip.
        if self.compiled.contains_key(&name) || self.ineligible.contains(&name) {
            return;
        }

        if !Self::is_eligible(func, all_functions, &self.compiled, &self.ineligible) {
            if std::env::var("AIRL_JIT_DEBUG").as_deref() == Ok("1") {
                eprintln!("[JIT] {} ineligible (opcodes)", name);
            }
            self.ineligible.insert(name);
            return;
        }

        match self.compile_func(func) {
            Ok((ptr, hint)) => {
                if std::env::var("AIRL_JIT_DEBUG").as_deref() == Ok("1") {
                    eprintln!("[JIT] compiled {} → {:?}", name, hint);
                }
                self.compiled.insert(name, (ptr, hint));
            }
            Err(e) => {
                if std::env::var("AIRL_JIT_DEBUG").as_deref() == Ok("1") {
                    eprintln!("[JIT] {} compile error: {}", name, e);
                }
                self.ineligible.insert(name);
            }
        }
    }

    /// Core Cranelift IR emitter. Translates a BytecodeFunc to native code.
    /// Returns the function pointer and a TypeHint for unmarshaling the result.
    fn compile_func(&mut self, func: &BytecodeFunc) -> Result<(*const u8, TypeHint), String> {
        // ── 1. Build Cranelift signature (all I64) ──────────────────────────
        let mut sig = self.module.make_signature();
        for _ in 0..func.arity {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));

        // ── 2. Declare function in JIT module ──────────────────────────────
        let func_id = self
            .module
            .declare_function(&func.name, Linkage::Local, &sig)
            .map_err(|e| format!("declare: {}", e))?;

        // ── 3. Build function body ─────────────────────────────────────────
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();

        // Pre-declare function references for Call targets (before builder scope to avoid borrow conflicts)
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
                        let callee_id = self.module
                            .declare_function(callee_name, Linkage::Import, &call_sig)
                            .map_err(|e| format!("call declare: {}", e))?;
                        call_targets.insert(callee_name.clone(), callee_id);
                    }
                }
            }
        }

        // Type hints per register — used to decide int vs float ops.
        let reg_count = func.register_count as usize;
        let mut type_hints: Vec<TypeHint> = vec![TypeHint::Int; reg_count];

        // Track the overall return type hint (updated at every Return instruction).
        let mut return_hint = TypeHint::Int;

        // ── Pass 1: Scan instructions to find basic block boundaries ───────
        //
        // A new block starts at:
        //   • index 0 (entry)
        //   • the target of a Jump / JumpIfFalse / JumpIfTrue
        //   • the instruction immediately after a conditional jump (fallthrough)
        let instrs = &func.instructions;
        let mut block_starts: BTreeSet<usize> = BTreeSet::new();
        block_starts.insert(0); // entry block always starts at 0

        for (i, instr) in instrs.iter().enumerate() {
            match instr.op {
                Op::Jump => {
                    // a encodes a signed i16 offset
                    let offset = instr.a as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                }
                Op::JumpIfFalse | Op::JumpIfTrue => {
                    // b encodes a signed i16 offset; i+1 is the fallthrough
                    let offset = instr.b as i16 as isize;
                    let target = (i as isize + 1 + offset) as usize;
                    block_starts.insert(target);
                    block_starts.insert(i + 1); // fallthrough block
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

        // ── Declare Cranelift Variables for every bytecode register ────────
        let mut vars: Vec<Variable> = Vec::with_capacity(reg_count);
        for _r in 0..reg_count {
            let var = builder.declare_var(types::I64);
            vars.push(var);
        }

        // Bind function params to the first `arity` variables.
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

        // ── Create loop_block for TailCall back-edges ─────────────────────
        // entry_block has function-parameter block params. TailCall cannot
        // jump back to entry_block with args (Cranelift verifier rejects
        // empty-arg jumps to parameterized blocks). Instead, we create a
        // param-free loop_block: entry_block falls through to it, and
        // TailCall jumps back to it. Variables carry values across the edge
        // via Cranelift's phi-insertion (def_var/use_var mechanism).
        //
        // We remap instruction index 0 → loop_block in index_to_block so that
        // any jump targeting instruction 0 (i.e. a loop-back) goes to loop_block,
        // not entry_block.
        let loop_block = builder.create_block();
        index_to_block.insert(0, loop_block); // remap: instr 0 → loop_block
        builder.ins().jump(loop_block, &[]);
        builder.switch_to_block(loop_block);
        // entry_block just terminated with a jump; mark true so the loop
        // at i=0 (which sees index_to_block[0]=loop_block) doesn't emit
        // a redundant jump-to-self before switching to loop_block.
        let mut last_was_terminator = true;

        for (i, instr) in instrs.iter().enumerate() {
            // When crossing a block boundary, emit a fallthrough jump from
            // the previous block (if it didn't already end with a terminator).
            if let Some(&blk) = index_to_block.get(&i) {
                if !last_was_terminator {
                    builder.ins().jump(blk, &[]);
                }
                builder.switch_to_block(blk);
                last_was_terminator = false;
            }

            match instr.op {
                // ── Literals ─────────────────────────────────────────────
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
                        _ => return Err(format!("LoadConst: unsupported constant type")),
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

                // ── Arithmetic ───────────────────────────────────────────
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

                // ── Comparisons ───────────────────────────────────────────
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

                // ── Logic ─────────────────────────────────────────────────
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

                // ── Control flow ──────────────────────────────────────────
                Op::Jump => {
                    let offset = instr.a as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let target_blk = index_to_block[&target_idx];
                    builder.ins().jump(target_blk, &[]);
                    last_was_terminator = true;
                }
                Op::JumpIfFalse => {
                    // Branch to target when cond == 0 (false), else fallthrough.
                    let cond_reg = instr.a as usize;
                    let offset = instr.b as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];
                    let cond = builder.use_var(vars[cond_reg]);
                    // brif: branches to first block if nonzero, second if zero.
                    // JumpIfFalse wants to jump when zero → target is second arg.
                    builder.ins().brif(cond, fallthrough_blk, &[], target_blk, &[]);
                    last_was_terminator = true;
                }
                Op::JumpIfTrue => {
                    // Branch to target when cond != 0 (true), else fallthrough.
                    let cond_reg = instr.a as usize;
                    let offset = instr.b as i16 as isize;
                    let target_idx = (i as isize + 1 + offset) as usize;
                    let fallthrough_idx = i + 1;
                    let target_blk = index_to_block[&target_idx];
                    let fallthrough_blk = index_to_block[&fallthrough_idx];
                    let cond = builder.use_var(vars[cond_reg]);
                    // brif: branches to first block if nonzero → target is first.
                    builder.ins().brif(cond, target_blk, &[], fallthrough_blk, &[]);
                    last_was_terminator = true;
                }
                Op::Return => {
                    let src = instr.a as usize;
                    return_hint = type_hints[src];
                    let v = builder.use_var(vars[src]);
                    builder.ins().return_(&[v]);
                    last_was_terminator = true;
                }

                // ── Calls ────────────────────────────────────────────────
                Op::Call => {
                    let callee_name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("call: func name must be string".into()),
                    };
                    let argc = instr.b as usize;
                    let dst = instr.dst as usize;

                    // Get the FuncRef — self-call uses func_id, cross-call uses pre-declared target
                    let callee_func_id = if callee_name == func.name {
                        func_id
                    } else if let Some(&id) = call_targets.get(&callee_name) {
                        id
                    } else {
                        return Err(format!("call target '{}' not declared", callee_name));
                    };
                    let func_ref = self.module.declare_func_in_func(callee_func_id, builder.func);

                    let mut call_args = Vec::new();
                    for i in 0..argc {
                        let arg = builder.use_var(vars[dst + 1 + i]);
                        call_args.push(arg);
                    }
                    let call = builder.ins().call(func_ref, &call_args);
                    let result = builder.inst_results(call)[0];
                    builder.def_var(vars[dst], result);
                    last_was_terminator = false;
                }
                Op::TailCall => {
                    // Self-recursive tail call — jump back to entry block.
                    // The bytecode compiler emits Move instructions before TailCall to place
                    // new arg values into r0..rN. Those Moves are already compiled by earlier
                    // iterations, so parameter variables already hold the correct values.
                    let callee_name = match &func.constants[instr.a as usize] {
                        Value::Str(s) => s.clone(),
                        _ => return Err("tailcall: func name must be string".into()),
                    };
                    if callee_name != func.name {
                        return Err(format!("cross-function TailCall to '{}' not supported", callee_name));
                    }
                    // Jump back to loop_block (not entry_block, which has function params
                    // as block params and cannot be re-entered with an empty-arg jump).
                    builder.ins().jump(loop_block, &[]);
                    last_was_terminator = true;
                }

                // Any other opcode should have been caught by is_eligible.
                op => {
                    return Err(format!("unhandled opcode {:?} in JIT", op));
                }
            }
        }

        // If the last instruction didn't terminate the block, add an implicit return nil.
        if !last_was_terminator {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().return_(&[zero]);
        }

        // ── Seal all blocks (after all predecessors are defined) ──────────
        builder.seal_all_blocks();
        builder.finalize();

        // ── Define function, finalize, extract pointer ────────────────────
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| format!("define: {}", e))?;
        self.module
            .finalize_definitions()
            .map_err(|e| format!("finalize: {}", e))?;

        let ptr = self.module.get_finalized_function(func_id);
        Ok((ptr, return_hint))
    }

    pub fn try_call_native(&self, name: &str, args: &[Value]) -> Option<Result<Value, RuntimeError>> {
        let (ptr, return_hint) = self.compiled.get(name)?;

        // Marshal args — bail if any is non-primitive
        let raw_args: Vec<u64> = args.iter()
            .map(marshal_arg)
            .collect::<Option<Vec<_>>>()?;

        let raw_result: u64 = unsafe {
            match raw_args.len() {
                0 => {
                    let f: fn() -> u64 = std::mem::transmute(*ptr);
                    f()
                }
                1 => {
                    let f: fn(u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0])
                }
                2 => {
                    let f: fn(u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1])
                }
                3 => {
                    let f: fn(u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2])
                }
                4 => {
                    let f: fn(u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3])
                }
                5 => {
                    let f: fn(u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4])
                }
                6 => {
                    let f: fn(u64, u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4], raw_args[5])
                }
                7 => {
                    let f: fn(u64, u64, u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4], raw_args[5], raw_args[6])
                }
                8 => {
                    let f: fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64 = std::mem::transmute(*ptr);
                    f(raw_args[0], raw_args[1], raw_args[2], raw_args[3], raw_args[4], raw_args[5], raw_args[6], raw_args[7])
                }
                _ => return None, // >8 args — fall back to bytecode
            }
        };

        Some(Ok(unmarshal_result(raw_result, *return_hint)))
    }
}

fn marshal_arg(val: &Value) -> Option<u64> {
    match val {
        Value::Int(n) => Some(*n as u64),
        Value::Float(f) => Some(f.to_bits()),
        Value::Bool(b) => Some(*b as u64),
        _ => None,
    }
}

fn unmarshal_result(raw: u64, hint: TypeHint) -> Value {
    match hint {
        TypeHint::Int => Value::Int(raw as i64),
        TypeHint::Float => Value::Float(f64::from_bits(raw)),
        TypeHint::Bool => Value::Bool(raw != 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::{IRBinding, IRNode};

    fn make_func(name: &str, arity: u16, ops: Vec<Op>) -> BytecodeFunc {
        BytecodeFunc {
            name: name.to_string(),
            arity,
            register_count: arity + 2,
            capture_count: 0,
            instructions: ops.into_iter().map(|op| Instruction::new(op, 0, 0, 0)).collect(),
            constants: vec![],
        }
    }

    #[test]
    fn test_eligible_arithmetic() {
        let func = make_func("add", 2, vec![Op::Add, Op::Return]);
        let all = HashMap::new();
        assert!(BytecodeJit::is_eligible(&func, &all, &HashMap::new(), &HashSet::new()));
    }

    #[test]
    fn test_ineligible_make_list() {
        let func = make_func("f", 1, vec![Op::MakeList, Op::Return]);
        let all = HashMap::new();
        assert!(!BytecodeJit::is_eligible(&func, &all, &HashMap::new(), &HashSet::new()));
    }

    #[test]
    fn test_ineligible_call_reg() {
        let func = make_func("f", 1, vec![Op::CallReg, Op::Return]);
        let all = HashMap::new();
        assert!(!BytecodeJit::is_eligible(&func, &all, &HashMap::new(), &HashSet::new()));
    }

    #[test]
    fn test_marshal_roundtrip_int() {
        let val = Value::Int(42);
        let raw = marshal_arg(&val).unwrap();
        let back = unmarshal_result(raw, TypeHint::Int);
        assert_eq!(back, Value::Int(42));
    }

    #[test]
    fn test_marshal_roundtrip_float() {
        let val = Value::Float(3.14);
        let raw = marshal_arg(&val).unwrap();
        let back = unmarshal_result(raw, TypeHint::Float);
        assert_eq!(back, Value::Float(3.14));
    }

    #[test]
    fn test_marshal_roundtrip_bool() {
        let val = Value::Bool(true);
        let raw = marshal_arg(&val).unwrap();
        let back = unmarshal_result(raw, TypeHint::Bool);
        assert_eq!(back, Value::Bool(true));
    }

    #[test]
    fn test_marshal_rejects_string() {
        assert!(marshal_arg(&Value::Str("hello".into())).is_none());
    }

    #[test]
    fn test_marshal_rejects_list() {
        assert!(marshal_arg(&Value::List(vec![])).is_none());
    }

    // ── JIT compilation tests ─────────────────────────────────────────────

    /// Compile `(defn add [a b] (+ a b))` and call with Int(3)+Int(4) → Int(7).
    #[test]
    fn test_jit_add_ints() {
        let body = IRNode::Call("+".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]);
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("add", &["a".into(), "b".into()], &body);

        let mut jit = BytecodeJit::new().expect("JIT init failed");
        let all: HashMap<String, BytecodeFunc> = HashMap::new();
        jit.try_compile(&func, &all);

        assert!(jit.compiled.contains_key("add"), "function should be compiled");

        let result = jit.try_call_native("add", &[Value::Int(3), Value::Int(4)]);
        let val = result.expect("should return Some").expect("should be Ok");
        assert_eq!(val, Value::Int(7));
    }

    /// Compile `(defn max2 [a b] (if (> a b) a b))` and test both branches.
    #[test]
    fn test_jit_if_expression() {
        let cond = IRNode::Call(">".into(), vec![IRNode::Load("a".into()), IRNode::Load("b".into())]);
        let body = IRNode::If(Box::new(cond), Box::new(IRNode::Load("a".into())), Box::new(IRNode::Load("b".into())));
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("max2", &["a".into(), "b".into()], &body);

        let mut jit = BytecodeJit::new().expect("JIT init failed");
        let all: HashMap<String, BytecodeFunc> = HashMap::new();
        jit.try_compile(&func, &all);

        assert!(jit.compiled.contains_key("max2"), "function should be compiled");

        // a > b: should return a
        let r1 = jit.try_call_native("max2", &[Value::Int(10), Value::Int(3)])
            .expect("Some").expect("Ok");
        assert_eq!(r1, Value::Int(10));

        // b > a: should return b
        let r2 = jit.try_call_native("max2", &[Value::Int(2), Value::Int(7)])
            .expect("Some").expect("Ok");
        assert_eq!(r2, Value::Int(7));
    }

    /// Compile `(defn sq_plus1 [x] (let (y (+ x 1)) (* y y)))` and test with Int(4) → Int(25).
    #[test]
    fn test_jit_let_binding() {
        let binding_expr = IRNode::Call("+".into(), vec![IRNode::Load("x".into()), IRNode::Int(1)]);
        let body_expr = IRNode::Call("*".into(), vec![IRNode::Load("y".into()), IRNode::Load("y".into())]);
        let body = IRNode::Let(
            vec![IRBinding { name: "y".into(), expr: binding_expr }],
            Box::new(body_expr),
        );
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("sq_plus1", &["x".into()], &body);

        let mut jit = BytecodeJit::new().expect("JIT init failed");
        let all: HashMap<String, BytecodeFunc> = HashMap::new();
        jit.try_compile(&func, &all);

        assert!(jit.compiled.contains_key("sq_plus1"), "function should be compiled");

        // (4 + 1) * (4 + 1) = 5 * 5 = 25
        let result = jit.try_call_native("sq_plus1", &[Value::Int(4)])
            .expect("Some").expect("Ok");
        assert_eq!(result, Value::Int(25));
    }

    #[test]
    fn test_jit_factorial_recursive() {
        use crate::ir::*;

        // (defn fact [n] (if (<= n 1) 1 (* n (fact (- n 1)))))
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

        let all_funcs = HashMap::new();
        let mut jit = BytecodeJit::new().unwrap();
        jit.try_compile(&func, &all_funcs);
        assert!(jit.compiled.contains_key("fact"), "factorial should be JIT-eligible");

        let r = jit.try_call_native("fact", &[Value::Int(5)]).unwrap().unwrap();
        assert_eq!(r, Value::Int(120));
    }

    #[test]
    fn test_jit_tailcall_no_overflow() {
        use crate::ir::*;

        // (defn countdown [n] (if (= n 0) 0 (countdown (- n 1))))
        let body = IRNode::If(
            Box::new(IRNode::Call("=".into(), vec![IRNode::Load("n".into()), IRNode::Int(0)])),
            Box::new(IRNode::Int(0)),
            Box::new(IRNode::Call("countdown".into(), vec![
                IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
            ])),
        );

        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("countdown", &["n".into()], &body);

        let all_funcs = HashMap::new();
        let mut jit = BytecodeJit::new().unwrap();
        jit.try_compile(&func, &all_funcs);
        assert!(jit.compiled.contains_key("countdown"));

        // 100K iterations — would overflow stack without TailCall→loop
        let r = jit.try_call_native("countdown", &[Value::Int(100_000)]).unwrap().unwrap();
        assert_eq!(r, Value::Int(0));
    }

    /// Compile a function with MakeList and verify it's marked ineligible (not compiled).
    #[test]
    fn test_jit_ineligible_skipped() {
        let body = IRNode::List(vec![IRNode::Int(1), IRNode::Int(2)]);
        let mut compiler = BytecodeCompiler::new();
        let func = compiler.compile_function("make_pair", &["a".into()], &body);

        let mut jit = BytecodeJit::new().expect("JIT init failed");
        let all: HashMap<String, BytecodeFunc> = HashMap::new();
        jit.try_compile(&func, &all);

        assert!(!jit.compiled.contains_key("make_pair"), "ineligible function should not be compiled");
        assert!(jit.ineligible.contains("make_pair"), "should be marked ineligible");
    }
}
