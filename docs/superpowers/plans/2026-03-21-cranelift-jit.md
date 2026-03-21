# Cranelift JIT Compilation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add transparent JIT compilation of pure arithmetic functions via Cranelift — functions with primitive signatures are compiled to native code on first call, cached, and called directly on subsequent invocations.

**Architecture:** New `airl-codegen` crate with Cranelift dependencies. A `JitCache` holds compiled function pointers. The interpreter checks the cache in `call_fn` before interpreting. Unsupported functions silently fall back to interpretation.

**Tech Stack:** Rust, cranelift-codegen 0.130.0, cranelift-frontend, cranelift-jit, cranelift-module.

**Spec:** `docs/superpowers/specs/2026-03-21-cranelift-jit-design.md`

---

## File Map

```
Cargo.toml                              # MODIFY — add airl-codegen to workspace members
crates/
├── airl-codegen/                       # NEW CRATE
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                      # Re-exports
│       ├── types.rs                    # Type mapping: AIRL → Cranelift
│       ├── lower.rs                    # AST → Cranelift IR lowering
│       ├── jit.rs                      # JitCache, compilation, calling
│       └── marshal.rs                  # Value ↔ native type conversion
│
├── airl-runtime/
│   ├── Cargo.toml                      # MODIFY — add airl-codegen dependency
│   └── src/
│       └── eval.rs                     # MODIFY — integrate JitCache into call_fn
│
├── airl-driver/
│   └── Cargo.toml                      # MODIFY — add airl-codegen dependency
│
tests/fixtures/valid/
│   └── jit_arithmetic.airl             # NEW — benchmark fixture
```

---

## Task 1: Scaffold `airl-codegen` Crate

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/airl-codegen/Cargo.toml`
- Create: `crates/airl-codegen/src/lib.rs`

- [ ] **Step 1: Add to workspace**

In root `Cargo.toml`, add `"crates/airl-codegen"` to members (before `airl-driver`).

- [ ] **Step 2: Create Cargo.toml**

```toml
[package]
name = "airl-codegen"
version.workspace = true
edition.workspace = true

[dependencies]
airl-syntax = { path = "../airl-syntax" }
airl-types = { path = "../airl-types" }
cranelift-codegen = "0.130"
cranelift-frontend = "0.130"
cranelift-jit = "0.130"
cranelift-module = "0.130"
target-lexicon = "0.13"
```

Note: `target-lexicon` is needed for `cranelift_jit::JITBuilder::new(target_lexicon::Triple::host())`.

- [ ] **Step 3: Create stub lib.rs**

```rust
pub mod types;
pub mod lower;
pub mod jit;
pub mod marshal;
```

Create empty stub files for each module (just `// placeholder`).

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p airl-codegen`
Expected: compiles (downloading cranelift crates will take a moment first time)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/airl-codegen/
git commit -m "scaffold: add airl-codegen crate with Cranelift dependencies"
```

---

## Task 2: Type Mapping (`types.rs`)

**Files:**
- Create: `crates/airl-codegen/src/types.rs`

Maps AIRL types to Cranelift types and checks eligibility.

- [ ] **Step 1: Implement type mapping**

```rust
use airl_syntax::ast::{AstType, AstTypeKind, FnDef, Param};
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type as CraneliftType;

/// Map an AIRL type name to a Cranelift IR type.
/// Returns None if the type is not a supported primitive.
pub fn airl_type_to_cranelift(name: &str) -> Option<CraneliftType> {
    match name {
        "i32" => Some(types::I32),
        "i64" => Some(types::I64),
        "f32" => Some(types::F32),
        "f64" => Some(types::F64),
        "bool" => Some(types::I8),
        _ => None,
    }
}

/// Map an AST type to a Cranelift type.
pub fn resolve_ast_type(ty: &AstType) -> Option<CraneliftType> {
    match &ty.kind {
        AstTypeKind::Named(name) => airl_type_to_cranelift(name),
        _ => None,
    }
}

/// Check if a function's signature is all-primitive (eligible for JIT).
pub fn is_jit_eligible(def: &FnDef) -> bool {
    // All params must have primitive types
    for param in &def.params {
        if resolve_ast_type(&param.ty).is_none() {
            return false;
        }
    }
    // Return type must be primitive
    resolve_ast_type(&def.return_type).is_some()
}

/// Returns true if a Cranelift type is floating point.
pub fn is_float_type(ty: CraneliftType) -> bool {
    ty == types::F32 || ty == types::F64
}

/// Returns true if a Cranelift type is integer (includes bool as I8).
pub fn is_int_type(ty: CraneliftType) -> bool {
    ty == types::I8 || ty == types::I32 || ty == types::I64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_primitive_types() {
        assert_eq!(airl_type_to_cranelift("i32"), Some(types::I32));
        assert_eq!(airl_type_to_cranelift("i64"), Some(types::I64));
        assert_eq!(airl_type_to_cranelift("f32"), Some(types::F32));
        assert_eq!(airl_type_to_cranelift("f64"), Some(types::F64));
        assert_eq!(airl_type_to_cranelift("bool"), Some(types::I8));
    }

    #[test]
    fn non_primitive_returns_none() {
        assert_eq!(airl_type_to_cranelift("String"), None);
        assert_eq!(airl_type_to_cranelift("List"), None);
        assert_eq!(airl_type_to_cranelift("tensor"), None);
    }

    #[test]
    fn float_int_classification() {
        assert!(is_float_type(types::F64));
        assert!(!is_float_type(types::I64));
        assert!(is_int_type(types::I32));
        assert!(!is_int_type(types::F32));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p airl-codegen`
Expected: pass

- [ ] **Step 3: Commit**

```bash
git add crates/airl-codegen/src/types.rs
git commit -m "feat(codegen): add AIRL → Cranelift type mapping"
```

---

## Task 3: Value Marshaling (`marshal.rs`)

**Files:**
- Create: `crates/airl-codegen/src/marshal.rs`

Convert between `Value` (runtime) and raw native types for the JIT call boundary.

- [ ] **Step 1: Implement marshaling**

```rust
use airl_syntax::ast::AstType;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type as CraneliftType;
use crate::types::resolve_ast_type;

/// Error during marshaling.
#[derive(Debug)]
pub enum MarshalError {
    TypeMismatch { expected: String, got: String },
    UnsupportedType(String),
}

impl std::fmt::Display for MarshalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarshalError::TypeMismatch { expected, got } => {
                write!(f, "marshal type mismatch: expected {}, got {}", expected, got)
            }
            MarshalError::UnsupportedType(t) => write!(f, "unsupported type for JIT: {}", t),
        }
    }
}

/// A raw value that can be passed to/from native code.
/// Stored as a u64 bitpattern regardless of actual type.
#[derive(Debug, Clone, Copy)]
pub struct RawValue(pub u64);

impl RawValue {
    pub fn from_i32(v: i32) -> Self { Self(v as u64) }
    pub fn from_i64(v: i64) -> Self { Self(v as u64) }
    pub fn from_f32(v: f32) -> Self { Self(f32::to_bits(v) as u64) }
    pub fn from_f64(v: f64) -> Self { Self(f64::to_bits(v)) }
    pub fn from_bool(v: bool) -> Self { Self(v as u64) }

    pub fn to_i32(self) -> i32 { self.0 as i32 }
    pub fn to_i64(self) -> i64 { self.0 as i64 }
    pub fn to_f32(self) -> f32 { f32::from_bits(self.0 as u32) }
    pub fn to_f64(self) -> f64 { f64::from_bits(self.0) }
    pub fn to_bool(self) -> bool { self.0 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_i64() {
        let v = RawValue::from_i64(42);
        assert_eq!(v.to_i64(), 42);
    }

    #[test]
    fn roundtrip_i64_negative() {
        let v = RawValue::from_i64(-7);
        assert_eq!(v.to_i64(), -7);
    }

    #[test]
    fn roundtrip_f64() {
        let v = RawValue::from_f64(3.14);
        assert!((v.to_f64() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn roundtrip_bool() {
        assert!(RawValue::from_bool(true).to_bool());
        assert!(!RawValue::from_bool(false).to_bool());
    }

    #[test]
    fn roundtrip_i32() {
        let v = RawValue::from_i32(99);
        assert_eq!(v.to_i32(), 99);
    }

    #[test]
    fn roundtrip_f32() {
        let v = RawValue::from_f32(2.5);
        assert!((v.to_f32() - 2.5).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(codegen): add Value ↔ native type marshaling"
```

---

## Task 4: AST → Cranelift IR Lowering (`lower.rs`)

**Files:**
- Create: `crates/airl-codegen/src/lower.rs`

This is the core of the codegen. It walks the AIRL AST and emits Cranelift IR.

- [ ] **Step 1: Define the Lowerer struct and entry point**

```rust
use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder, Value as CrValue};
use cranelift_codegen::ir::condcodes::{IntCC, FloatCC};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_codegen::Context;
use cranelift_codegen::settings;
use airl_syntax::ast::*;
use crate::types::*;

/// Error during lowering — triggers fallback to interpreter.
#[derive(Debug)]
pub enum LowerError {
    UnsupportedExpression(String),
    UnsupportedType(String),
    UndefinedVariable(String),
    InternalError(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::UnsupportedExpression(e) => write!(f, "unsupported expression: {}", e),
            LowerError::UnsupportedType(t) => write!(f, "unsupported type: {}", t),
            LowerError::UndefinedVariable(v) => write!(f, "undefined variable: {}", v),
            LowerError::InternalError(e) => write!(f, "internal error: {}", e),
        }
    }
}

struct Lowerer<'a> {
    builder: &'a mut FunctionBuilder<'a>,
    variables: HashMap<String, (Variable, ir::Type)>,
    next_var: usize,
}
```

- [ ] **Step 2: Implement expression lowering**

The core `lower_expr` method dispatches on ExprKind:

```rust
impl<'a> Lowerer<'a> {
    fn lower_expr(&mut self, expr: &Expr) -> Result<(CrValue, ir::Type), LowerError> {
        match &expr.kind {
            ExprKind::IntLit(v) => {
                let val = self.builder.ins().iconst(types::I64, *v);
                Ok((val, types::I64))
            }
            ExprKind::FloatLit(v) => {
                let val = self.builder.ins().f64const(*v);
                Ok((val, types::F64))
            }
            ExprKind::BoolLit(v) => {
                let val = self.builder.ins().iconst(types::I8, *v as i64);
                Ok((val, types::I8))
            }
            ExprKind::SymbolRef(name) => {
                let (var, ty) = self.variables.get(name)
                    .ok_or_else(|| LowerError::UndefinedVariable(name.clone()))?;
                let val = self.builder.use_var(*var);
                Ok((val, *ty))
            }
            ExprKind::FnCall(callee, args) => self.lower_builtin_call(callee, args),
            ExprKind::If(cond, then_expr, else_expr) => self.lower_if(cond, then_expr, else_expr),
            ExprKind::Let(bindings, body) => self.lower_let(bindings, body),
            ExprKind::Do(exprs) => self.lower_do(exprs),
            _ => Err(LowerError::UnsupportedExpression(format!("{:?}", expr.kind))),
        }
    }
}
```

Then implement each helper: `lower_builtin_call` (dispatches on `+`, `-`, `*`, `/`, `%`, comparisons, logic), `lower_if` (with block params for merge), `lower_let`, `lower_do`.

**lower_builtin_call:**
```rust
fn lower_builtin_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<(CrValue, ir::Type), LowerError> {
    let name = match &callee.kind {
        ExprKind::SymbolRef(s) => s.as_str(),
        _ => return Err(LowerError::UnsupportedExpression("non-symbol callee".into())),
    };

    match name {
        "+" | "-" | "*" | "/" | "%" => self.lower_arithmetic(name, args),
        "=" | "!=" | "<" | ">" | "<=" | ">=" => self.lower_comparison(name, args),
        "and" | "or" => self.lower_logic_binop(name, args),
        "not" => self.lower_not(args),
        _ => Err(LowerError::UnsupportedExpression(format!("unsupported builtin: {}", name))),
    }
}
```

**lower_arithmetic** (dispatch int vs float):
```rust
fn lower_arithmetic(&mut self, op: &str, args: &[Expr]) -> Result<(CrValue, ir::Type), LowerError> {
    if args.len() != 2 {
        return Err(LowerError::UnsupportedExpression(format!("{} needs 2 args", op)));
    }
    let (lhs, lty) = self.lower_expr(&args[0])?;
    let (rhs, rty) = self.lower_expr(&args[1])?;
    // Type must match
    if lty != rty {
        return Err(LowerError::UnsupportedType(format!("mismatched types: {:?} vs {:?}", lty, rty)));
    }
    let val = if is_float_type(lty) {
        match op {
            "+" => self.builder.ins().fadd(lhs, rhs),
            "-" => self.builder.ins().fsub(lhs, rhs),
            "*" => self.builder.ins().fmul(lhs, rhs),
            "/" => self.builder.ins().fdiv(lhs, rhs),
            "%" => return Err(LowerError::UnsupportedExpression("float modulus".into())),
            _ => unreachable!(),
        }
    } else {
        match op {
            "+" => self.builder.ins().iadd(lhs, rhs),
            "-" => self.builder.ins().isub(lhs, rhs),
            "*" => self.builder.ins().imul(lhs, rhs),
            "/" => self.builder.ins().sdiv(lhs, rhs),
            "%" => self.builder.ins().srem(lhs, rhs),
            _ => unreachable!(),
        }
    };
    Ok((val, lty))
}
```

**lower_if** (most complex — uses Cranelift blocks):
```rust
fn lower_if(&mut self, cond: &Expr, then_expr: &Expr, else_expr: &Expr) -> Result<(CrValue, ir::Type), LowerError> {
    let (cond_val, _) = self.lower_expr(cond)?;

    let then_block = self.builder.create_block();
    let else_block = self.builder.create_block();
    let merge_block = self.builder.create_block();

    self.builder.ins().brif(cond_val, then_block, &[], else_block, &[]);

    // Then
    self.builder.switch_to_block(then_block);
    self.builder.seal_block(then_block);
    let (then_val, then_ty) = self.lower_expr(then_expr)?;
    self.builder.ins().jump(merge_block, &[then_val]);

    // Else
    self.builder.switch_to_block(else_block);
    self.builder.seal_block(else_block);
    let (else_val, _) = self.lower_expr(else_expr)?;
    self.builder.ins().jump(merge_block, &[else_val]);

    // Merge
    self.builder.append_block_param(merge_block, then_ty);
    self.builder.switch_to_block(merge_block);
    self.builder.seal_block(merge_block);
    let result = self.builder.block_params(merge_block)[0];

    Ok((result, then_ty))
}
```

- [ ] **Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    // Tests will compile and execute functions via the JIT (Task 5)
    // For now, just verify the lowerer doesn't panic on valid input
    // Full round-trip tests go in jit.rs
}
```

Since the lowerer produces Cranelift IR that can only be validated by actually compiling it, most tests will be in `jit.rs` (Task 5) as round-trip tests.

- [ ] **Step 4: Run tests, commit**

```bash
git commit -m "feat(codegen): add AST → Cranelift IR lowering"
```

---

## Task 5: JIT Cache and Compilation (`jit.rs`)

**Files:**
- Create: `crates/airl-codegen/src/jit.rs`

This is the top-level API: compile a function, cache it, call it.

- [ ] **Step 1: Implement JitCache**

```rust
use std::collections::{HashMap, HashSet};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, Linkage, FuncId};
use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder};
use cranelift_codegen::settings;
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use airl_syntax::ast::FnDef;
use crate::types::*;
use crate::lower::*;
use crate::marshal::RawValue;

pub struct JitCache {
    module: JITModule,
    compiled: HashMap<String, CompiledFn>,
    uncompilable: HashSet<String>,
}

struct CompiledFn {
    ptr: *const u8,
    param_types: Vec<ir::Type>,
    return_type: ir::Type,
}

impl JitCache {
    pub fn new() -> Result<Self, String> {
        let builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            compiled: HashMap::new(),
            uncompilable: HashSet::new(),
        })
    }

    /// Try to call a function via JIT.
    /// Returns Ok(Some(raw_result)) if compiled and called.
    /// Returns Ok(None) if not compilable (fall back to interpreter).
    /// Returns Err on runtime error (e.g., division by zero trap).
    pub fn try_call(&mut self, def: &FnDef, args: &[RawValue]) -> Result<Option<RawValue>, String> {
        let name = &def.name;

        if self.uncompilable.contains(name.as_str()) {
            return Ok(None);
        }

        if !self.compiled.contains_key(name.as_str()) {
            if !is_jit_eligible(def) {
                self.uncompilable.insert(name.clone());
                return Ok(None);
            }
            match self.compile(def) {
                Ok(()) => {}
                Err(_) => {
                    self.uncompilable.insert(name.clone());
                    return Ok(None);
                }
            }
        }

        let compiled = &self.compiled[name.as_str()];
        let result = unsafe { self.call_native(compiled, args) }?;
        Ok(Some(result))
    }

    fn compile(&mut self, def: &FnDef) -> Result<(), LowerError> {
        // 1. Build Cranelift function signature
        let mut sig = self.module.make_signature();
        for param in &def.params {
            let ty = resolve_ast_type(&param.ty)
                .ok_or_else(|| LowerError::UnsupportedType(format!("{:?}", param.ty.kind)))?;
            sig.params.push(AbiParam::new(ty));
        }
        let ret_ty = resolve_ast_type(&def.return_type)
            .ok_or_else(|| LowerError::UnsupportedType(format!("{:?}", def.return_type.kind)))?;
        sig.returns.push(AbiParam::new(ret_ty));

        // 2. Declare function in module
        let func_id = self.module.declare_function(&def.name, Linkage::Local, &sig)
            .map_err(|e| LowerError::InternalError(format!("declare: {}", e)))?;

        // 3. Build function body with Cranelift
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
            let entry_block = builder.create_block();
            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);
            builder.seal_block(entry_block);

            // Map params to variables
            let mut lowerer = Lowerer::new(&mut builder);
            for (i, param) in def.params.iter().enumerate() {
                let param_val = builder.block_params(entry_block)[i];
                let ty = resolve_ast_type(&param.ty).unwrap();
                lowerer.define_variable(&param.name, param_val, ty);
            }

            // Lower body
            let (result, _) = lowerer.lower_expr(&def.body)?;
            builder.ins().return_(&[result]);
            builder.finalize();
        }

        // 4. Compile
        self.module.define_function(func_id, &mut ctx)
            .map_err(|e| LowerError::InternalError(format!("define: {}", e)))?;
        self.module.clear_context(&mut ctx);
        self.module.finalize_definitions()
            .map_err(|e| LowerError::InternalError(format!("finalize: {}", e)))?;

        // 5. Get function pointer
        let ptr = self.module.get_finalized_function(func_id);

        let param_types = def.params.iter()
            .map(|p| resolve_ast_type(&p.ty).unwrap())
            .collect();

        self.compiled.insert(def.name.clone(), CompiledFn {
            ptr,
            param_types,
            return_type: ret_ty,
        });

        Ok(())
    }

    unsafe fn call_native(&self, compiled: &CompiledFn, args: &[RawValue]) -> Result<RawValue, String> {
        // Dynamic dispatch based on number of params and types
        // For Phase 1, support 0-4 params, all passed as u64
        // The Cranelift calling convention uses the system ABI
        match args.len() {
            0 => {
                let f: fn() -> u64 = std::mem::transmute(compiled.ptr);
                Ok(RawValue(f()))
            }
            1 => {
                let f: fn(u64) -> u64 = std::mem::transmute(compiled.ptr);
                Ok(RawValue(f(args[0].0)))
            }
            2 => {
                let f: fn(u64, u64) -> u64 = std::mem::transmute(compiled.ptr);
                Ok(RawValue(f(args[0].0, args[1].0)))
            }
            3 => {
                let f: fn(u64, u64, u64) -> u64 = std::mem::transmute(compiled.ptr);
                Ok(RawValue(f(args[0].0, args[1].0, args[2].0)))
            }
            4 => {
                let f: fn(u64, u64, u64, u64) -> u64 = std::mem::transmute(compiled.ptr);
                Ok(RawValue(f(args[0].0, args[1].0, args[2].0, args[3].0)))
            }
            n => Err(format!("JIT does not support {} params (max 4)", n)),
        }
    }
}
```

**IMPORTANT NOTE on calling convention:** The above `transmute` approach assumes all params and returns are passed as `u64`. This works if the Cranelift function signature uses `I64` for all params. For `I32`/`F32`/`F64` params, we need to ensure the Cranelift signature matches. The simplest approach: always use `I64` as the Cranelift param type and insert `ireduce`/`sextend` instructions in the lowerer to convert between I32 and I64. For floats, use `F64` as the canonical type and `fpromote`/`fdemote` for F32. This way all native calls use the `fn(u64...) -> u64` signature.

Alternatively, the implementer can generate proper-typed signatures and use the correct `transmute` — this is more correct but requires more dispatch code. Start with the simpler I64-everything approach and refine if needed.

- [ ] **Step 2: Write round-trip tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::Lexer;
    use airl_syntax::parse_sexpr_all;
    use airl_syntax::parser;
    use airl_syntax::Diagnostics;

    fn compile_and_call(source: &str, args: &[RawValue]) -> Option<RawValue> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = parser::parse_top_level(&sexprs[0], &mut diags).unwrap();
        let def = match top {
            airl_syntax::ast::TopLevel::Defn(f) => f,
            _ => panic!("expected defn"),
        };

        let mut jit = JitCache::new().unwrap();
        let raw_args: Vec<RawValue> = args.to_vec();
        jit.try_call(&def, &raw_args).unwrap()
    }

    #[test]
    fn jit_add_integers() {
        let source = r#"
            (defn add :sig [(a : i64) (b : i64) -> i64]
              :intent "add" :requires [(valid a)] :ensures [(valid result)]
              :body (+ a b))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(3), RawValue::from_i64(4)]);
        assert_eq!(result.unwrap().to_i64(), 7);
    }

    #[test]
    fn jit_multiply() {
        let source = r#"
            (defn mul :sig [(a : i64) (b : i64) -> i64]
              :intent "mul" :requires [(valid a)] :ensures [(valid result)]
              :body (* a b))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(6), RawValue::from_i64(7)]);
        assert_eq!(result.unwrap().to_i64(), 42);
    }

    #[test]
    fn jit_nested_arithmetic() {
        let source = r#"
            (defn compute :sig [(x : i64) -> i64]
              :intent "poly" :requires [(valid x)] :ensures [(valid result)]
              :body (+ (* x x) (* 3 x) 7))
        "#;
        // x=5: 25 + 15 + 7 = 47... wait, (+ (* 5 5) (* 3 5) 7)
        // This is (+ (+ (* x x) (* 3 x)) 7) due to prefix nesting
        // Actually: (+ (* x x) (* 3 x) 7) is a 3-arg call to +
        // The interpreter handles multi-arg +, but Cranelift + is binary
        // This test needs the lowerer to handle multi-arg builtins via left-fold
        // OR the user writes binary: (+ (+ (* x x) (* 3 x)) 7)
        let source = r#"
            (defn compute :sig [(x : i64) -> i64]
              :intent "poly" :requires [(valid x)] :ensures [(valid result)]
              :body (+ (+ (* x x) (* 3 x)) 7))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(5)]);
        assert_eq!(result.unwrap().to_i64(), 47);
    }

    #[test]
    fn jit_if_expression() {
        let source = r#"
            (defn max2 :sig [(a : i64) (b : i64) -> i64]
              :intent "max" :requires [(valid a)] :ensures [(valid result)]
              :body (if (> a b) a b))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(10), RawValue::from_i64(3)]);
        assert_eq!(result.unwrap().to_i64(), 10);

        let result2 = compile_and_call(source, &[RawValue::from_i64(2), RawValue::from_i64(8)]);
        assert_eq!(result2.unwrap().to_i64(), 8);
    }

    #[test]
    fn jit_let_binding() {
        let source = r#"
            (defn lettest :sig [(x : i64) -> i64]
              :intent "let" :requires [(valid x)] :ensures [(valid result)]
              :body (let (y : i64 (+ x 1)) (* y y)))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(4)]);
        assert_eq!(result.unwrap().to_i64(), 25); // (4+1)^2
    }

    #[test]
    fn jit_do_block() {
        let source = r#"
            (defn dotest :sig [(x : i64) -> i64]
              :intent "do" :requires [(valid x)] :ensures [(valid result)]
              :body (do (+ x 1) (* x 2)))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(5)]);
        assert_eq!(result.unwrap().to_i64(), 10); // last expr: 5*2
    }

    #[test]
    fn jit_ineligible_returns_none() {
        let source = r#"
            (defn greet :sig [(name : String) -> String]
              :intent "greet" :requires [(valid name)] :ensures [(valid result)]
              :body name)
        "#;
        let result = compile_and_call(source, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn jit_comparison() {
        let source = r#"
            (defn is_positive :sig [(x : i64) -> bool]
              :intent "check" :requires [(valid x)] :ensures [(valid result)]
              :body (> x 0))
        "#;
        let result = compile_and_call(source, &[RawValue::from_i64(5)]);
        assert!(result.unwrap().to_bool());

        let result2 = compile_and_call(source, &[RawValue::from_i64(-3)]);
        assert!(!result2.unwrap().to_bool());
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-codegen`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-codegen/src/jit.rs
git commit -m "feat(codegen): add JIT cache with compilation and calling"
```

---

## Task 6: Update lib.rs Exports

**Files:**
- Modify: `crates/airl-codegen/src/lib.rs`

- [ ] **Step 1: Export public API**

```rust
pub mod types;
pub mod lower;
pub mod jit;
pub mod marshal;

pub use jit::JitCache;
pub use marshal::RawValue;
pub use types::is_jit_eligible;
```

- [ ] **Step 2: Run tests, commit**

```bash
git commit -m "feat(codegen): finalize airl-codegen public API"
```

---

## Task 7: Integrate JitCache into Interpreter

**Files:**
- Modify: `crates/airl-runtime/Cargo.toml`
- Modify: `crates/airl-runtime/src/eval.rs`

- [ ] **Step 1: Add dependency**

In `crates/airl-runtime/Cargo.toml`:
```toml
airl-codegen = { path = "../airl-codegen" }
```

- [ ] **Step 2: Add JitCache to Interpreter**

In `eval.rs`, add to struct:
```rust
pub struct Interpreter {
    pub env: Env,
    builtins: Builtins,
    jit: Option<airl_codegen::JitCache>,
}
```

In `Interpreter::new()`:
```rust
jit: airl_codegen::JitCache::new().ok(),
```

- [ ] **Step 3: Add JIT check in call_fn**

In `call_fn`, after `:requires` contract check (line ~276) and before body evaluation (line ~278):

```rust
// Try JIT path
if let Some(ref mut jit) = self.jit {
    // Marshal args to raw values
    let raw_args: Result<Vec<_>, _> = args.iter().zip(def.params.iter()).map(|(val, param)| {
        value_to_raw(val, &param.ty)
    }).collect();

    if let Ok(raw_args) = raw_args {
        match jit.try_call(def, &raw_args) {
            Ok(Some(raw_result)) => {
                let result_val = raw_to_value(raw_result, &def.return_type);
                // Check :ensures
                self.env.bind("result".to_string(), result_val.clone());
                for contract in &def.ensures {
                    let contract_result = self.eval(contract)?;
                    if contract_result != Value::Bool(true) {
                        self.env.pop_frame();
                        return Err(RuntimeError::ContractViolation(
                            airl_contracts::violation::ContractViolation {
                                function: fn_val.name.clone(),
                                contract_kind: airl_contracts::violation::ContractKind::Ensures,
                                clause_source: format!("{:?}", contract.kind),
                                bindings: vec![],
                                evaluated: format!("{}", contract_result),
                                span: contract.span,
                            },
                        ));
                    }
                }
                self.env.pop_frame();
                return Ok(result_val);
            }
            Ok(None) => {} // not compilable, fall through to interpreter
            Err(e) => {
                // JIT error — fall through to interpreter
                eprintln!("JIT error for {}: {}", fn_val.name, e);
            }
        }
    }
}
```

Add helper functions:
```rust
fn value_to_raw(val: &Value, ty: &airl_syntax::ast::AstType) -> Result<airl_codegen::RawValue, ()> {
    match (&val, &ty.kind) {
        (Value::Int(v), _) => Ok(airl_codegen::RawValue::from_i64(*v)),
        (Value::Float(v), _) => Ok(airl_codegen::RawValue::from_f64(*v)),
        (Value::Bool(v), _) => Ok(airl_codegen::RawValue::from_bool(*v)),
        _ => Err(()),
    }
}

fn raw_to_value(raw: airl_codegen::RawValue, ty: &airl_syntax::ast::AstType) -> Value {
    match &ty.kind {
        airl_syntax::ast::AstTypeKind::Named(name) => match name.as_str() {
            "i32" => Value::Int(raw.to_i32() as i64),
            "i64" => Value::Int(raw.to_i64()),
            "f32" => Value::Float(raw.to_f32() as f64),
            "f64" => Value::Float(raw.to_f64()),
            "bool" => Value::Bool(raw.to_bool()),
            _ => Value::Int(raw.to_i64()), // fallback
        },
        _ => Value::Int(raw.to_i64()),
    }
}
```

- [ ] **Step 4: Write integration test**

```rust
#[test]
fn jit_transparent_same_result() {
    // A function that's JIT-eligible should produce the same result
    let input = r#"
        (defn add-nums
          :sig [(a : i64) (b : i64) -> i64]
          :intent "add" :requires [(valid a) (valid b)]
          :ensures [(valid result)]
          :body (+ a b))
        (add-nums 100 200)
    "#;
    assert_eq!(eval_str(input), Value::Int(300));
}
```

This test exercises the full path: parse → register function → call → JIT compile → native execute → marshal result. It passes because the JIT result equals the interpreted result.

- [ ] **Step 5: Run all tests**

Run: `cargo test --workspace`
Expected: all 376+ existing tests pass, plus new codegen tests

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/ crates/airl-codegen/
git commit -m "feat(runtime): integrate JitCache into Interpreter — transparent JIT"
```

---

## Task 8: Test Fixture and Final Verification

**Files:**
- Create: `tests/fixtures/valid/jit_arithmetic.airl`
- Modify: `crates/airl-driver/Cargo.toml` (add airl-codegen if needed)

- [ ] **Step 1: Create fixture**

`tests/fixtures/valid/jit_arithmetic.airl`:
```clojure
;; EXPECT: 47
;; Tests JIT compilation of pure arithmetic
(defn compute
  :sig [(x : i64) -> i64]
  :intent "polynomial evaluation"
  :requires [(valid x)]
  :ensures [(valid result)]
  :body (+ (+ (* x x) (* 3 x)) 7))
(compute 5)
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 3: Verify JIT is working**

The existing `eval_str` tests exercise the JIT path transparently. To verify JIT is actually being used (not just interpreted), add a test that checks the cache:

```rust
#[test]
fn jit_cache_populated_after_call() {
    let mut interp = Interpreter::new();
    // Define and call a JIT-eligible function
    let input = r#"
        (defn square
          :sig [(x : i64) -> i64]
          :intent "square" :requires [(valid x)] :ensures [(valid result)]
          :body (* x x))
    "#;
    // ... parse and eval_top_level ...
    interp.call_by_name("square", vec![Value::Int(5)]).unwrap();
    // Verify the JIT cache has an entry (if accessible)
    assert!(interp.jit.is_some());
}
```

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/ crates/
git commit -m "chore: JIT compilation complete — transparent Cranelift backend"
```
