// crates/airl-runtime/src/bytecode_jit.rs
//! Bytecode→Cranelift JIT compiler.
//!
//! Compiles eligible BytecodeFunc instructions to native x86-64 via Cranelift.
//! Eligible = primitive-typed functions with no list/variant/closure/builtin opcodes.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
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
}
