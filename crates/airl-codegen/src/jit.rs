use std::collections::{HashMap, HashSet};

use airl_syntax::ast::FnDef;
use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};

use crate::lower::{LowerError, Lowerer};
use crate::marshal::RawValue;
use crate::types::*;

/// A compiled native function with its metadata.
#[allow(dead_code)]
struct CompiledFn {
    ptr: *const u8,
    param_types: Vec<ir::Type>,
    return_type: ir::Type,
}

/// JIT compilation cache. Compiles eligible AIRL functions to native code
/// via Cranelift and caches the results.
pub struct JitCache {
    module: JITModule,
    compiled: HashMap<String, CompiledFn>,
    uncompilable: HashSet<String>,
}

impl JitCache {
    /// Create a new JIT cache with a fresh Cranelift JIT module.
    pub fn new() -> Result<Self, String> {
        let builder = cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
            .map_err(|e| format!("JIT builder error: {}", e))?;
        let module = JITModule::new(builder);
        Ok(Self {
            module,
            compiled: HashMap::new(),
            uncompilable: HashSet::new(),
        })
    }

    /// Try to call a function via JIT.
    /// Returns `Ok(Some(result))` if the function was compiled and called successfully.
    /// Returns `Ok(None)` if the function is not eligible for JIT (fall back to interpreter).
    /// Returns `Err` on compilation or runtime error.
    pub fn try_call(
        &mut self,
        def: &FnDef,
        args: &[RawValue],
    ) -> Result<Option<RawValue>, String> {
        let name = &def.name;

        // Skip functions we already know are uncompilable.
        if self.uncompilable.contains(name.as_str()) {
            return Ok(None);
        }

        // Compile on first call if not cached.
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
        let result = unsafe { Self::call_native(compiled, args) }?;
        Ok(Some(result))
    }

    /// Compile a function definition to native code via Cranelift.
    fn compile(&mut self, def: &FnDef) -> Result<(), LowerError> {
        // 1. Build Cranelift function signature.
        //    We use I64 for all params and return value so that the native
        //    calling convention always passes values in integer registers.
        //    The lowerer handles the actual typed operations internally.
        let mut sig = self.module.make_signature();
        let param_types: Vec<ir::Type> = def
            .params
            .iter()
            .map(|p| {
                resolve_ast_type(&p.ty)
                    .ok_or_else(|| LowerError::UnsupportedType(format!("{:?}", p.ty.kind)))
            })
            .collect::<Result<_, _>>()?;

        // Use I64 for all ABI params to keep the calling convention uniform.
        for _ in &param_types {
            sig.params.push(AbiParam::new(types::I64));
        }

        let ret_ty = resolve_ast_type(&def.return_type)
            .ok_or_else(|| LowerError::UnsupportedType(format!("{:?}", def.return_type.kind)))?;
        // Return as I64 too (bool I8 will be zero-extended).
        sig.returns.push(AbiParam::new(types::I64));

        // 2. Declare function in module.
        let func_id = self
            .module
            .declare_function(&def.name, Linkage::Local, &sig)
            .map_err(|e| LowerError::InternalError(format!("declare: {}", e)))?;

        // 3. Build function body with Cranelift.
        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
            let entry_block = builder.create_block();
            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);
            builder.seal_block(entry_block);

            // Convert ABI params (all I64) to their logical types.
            // We collect them first to avoid borrow conflicts with the Lowerer.
            let mut converted_params: Vec<(String, ir::Value, ir::Type)> = Vec::new();
            for (i, param) in def.params.iter().enumerate() {
                let abi_val = builder.block_params(entry_block)[i];
                let logical_ty = param_types[i];

                let param_val = if logical_ty == types::I8 {
                    builder.ins().ireduce(types::I8, abi_val)
                } else if logical_ty == types::I32 {
                    builder.ins().ireduce(types::I32, abi_val)
                } else if is_float_type(logical_ty) {
                    let f64_val =
                        builder.ins().bitcast(types::F64, ir::MemFlags::new(), abi_val);
                    if logical_ty == types::F32 {
                        builder.ins().fdemote(types::F32, f64_val)
                    } else {
                        f64_val
                    }
                } else {
                    abi_val
                };

                converted_params.push((param.name.clone(), param_val, logical_ty));
            }

            // Create lowerer, define variables, and lower the body.
            let (result, result_ty) = {
                let mut lowerer = Lowerer::new(&mut builder);
                for (name, val, ty) in &converted_params {
                    lowerer.define_variable(name, *val, *ty);
                }
                lowerer.lower_expr(&def.body)?
            };

            // Convert result back to I64 for the ABI return.
            let abi_result = if result_ty == types::I8 {
                builder.ins().uextend(types::I64, result)
            } else if result_ty == types::I32 {
                builder.ins().sextend(types::I64, result)
            } else if is_float_type(result_ty) {
                let f64_val = if result_ty == types::F32 {
                    builder.ins().fpromote(types::F64, result)
                } else {
                    result
                };
                builder
                    .ins()
                    .bitcast(types::I64, ir::MemFlags::new(), f64_val)
            } else {
                result
            };

            builder.ins().return_(&[abi_result]);
            builder.finalize();
        }

        // 4. Compile the function.
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| LowerError::InternalError(format!("define: {}", e)))?;
        self.module.clear_context(&mut ctx);
        self.module
            .finalize_definitions()
            .map_err(|e| LowerError::InternalError(format!("finalize: {}", e)))?;

        // 5. Get function pointer and store in cache.
        let ptr = self.module.get_finalized_function(func_id);

        self.compiled.insert(
            def.name.clone(),
            CompiledFn {
                ptr,
                param_types,
                return_type: ret_ty,
            },
        );

        Ok(())
    }

    /// Call a compiled native function with the given arguments.
    /// All values are passed/returned as u64 (the I64 ABI convention).
    ///
    /// # Safety
    /// The function pointer must be valid and the argument count must match.
    unsafe fn call_native(compiled: &CompiledFn, args: &[RawValue]) -> Result<RawValue, String> {
        let raw_result: u64 = match args.len() {
            0 => {
                let f: fn() -> u64 = std::mem::transmute(compiled.ptr);
                f()
            }
            1 => {
                let f: fn(u64) -> u64 = std::mem::transmute(compiled.ptr);
                f(args[0].0)
            }
            2 => {
                let f: fn(u64, u64) -> u64 = std::mem::transmute(compiled.ptr);
                f(args[0].0, args[1].0)
            }
            3 => {
                let f: fn(u64, u64, u64) -> u64 = std::mem::transmute(compiled.ptr);
                f(args[0].0, args[1].0, args[2].0)
            }
            4 => {
                let f: fn(u64, u64, u64, u64) -> u64 = std::mem::transmute(compiled.ptr);
                f(args[0].0, args[1].0, args[2].0, args[3].0)
            }
            n => return Err(format!("JIT does not support {} params (max 4)", n)),
        };

        Ok(RawValue(raw_result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airl_syntax::ast::TopLevel;
    use airl_syntax::{Diagnostics, Lexer};

    /// Parse an AIRL source string into a FnDef.
    fn parse_fn(source: &str) -> FnDef {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all().expect("lex failed");
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).expect("sexpr parse failed");
        let mut diags = Diagnostics::new();
        let top =
            airl_syntax::parse_top_level(&sexprs[0], &mut diags).expect("top-level parse failed");
        match top {
            TopLevel::Defn(f) => f,
            other => panic!("expected defn, got {:?}", other),
        }
    }

    /// Compile and call a function, returning the result (or None if ineligible).
    fn compile_and_call(source: &str, args: &[RawValue]) -> Option<RawValue> {
        let def = parse_fn(source);
        let mut jit = JitCache::new().unwrap();
        jit.try_call(&def, args).unwrap()
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
              :body (+ (+ (* x x) (* 3 x)) 7))
        "#;
        // x=5: 25 + 15 + 7 = 47
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
        let result =
            compile_and_call(source, &[RawValue::from_i64(10), RawValue::from_i64(3)]);
        assert_eq!(result.unwrap().to_i64(), 10);

        let result2 =
            compile_and_call(source, &[RawValue::from_i64(2), RawValue::from_i64(8)]);
        assert_eq!(result2.unwrap().to_i64(), 8);
    }

    #[test]
    fn jit_let_binding() {
        let source = r#"
            (defn lettest :sig [(x : i64) -> i64]
              :intent "let" :requires [(valid x)] :ensures [(valid result)]
              :body (let (y : i64 (+ x 1)) (* y y)))
        "#;
        // (4+1)^2 = 25
        let result = compile_and_call(source, &[RawValue::from_i64(4)]);
        assert_eq!(result.unwrap().to_i64(), 25);
    }

    #[test]
    fn jit_do_block() {
        let source = r#"
            (defn dotest :sig [(x : i64) -> i64]
              :intent "do" :requires [(valid x)] :ensures [(valid result)]
              :body (do (+ x 1) (* x 2)))
        "#;
        // Last expression: 5*2 = 10
        let result = compile_and_call(source, &[RawValue::from_i64(5)]);
        assert_eq!(result.unwrap().to_i64(), 10);
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
