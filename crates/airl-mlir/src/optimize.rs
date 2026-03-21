use melior::ir::Module;
use melior::pass::{self, PassManager};
use melior::Context;

/// Run the MLIR pass pipeline that lowers from scf/arith/memref to the LLVM dialect.
///
/// Pass ordering:
/// 1. convert-scf-to-cf         — structured control flow → branch-based control flow
/// 2. convert-arith-to-llvm     — arithmetic ops → LLVM dialect equivalents
/// 3. convert-memref-to-llvm    — memref operations → LLVM pointer operations
/// 4. convert-func-to-llvm      — func.func/return → llvm.func/return
/// 5. reconcile-unrealized-casts — clean up leftover type casts
///
/// After this pipeline, the module contains only LLVM dialect ops and is ready
/// for `ExecutionEngine` JIT compilation.
pub fn run_lowering_pipeline(ctx: &Context, module: &mut Module) -> Result<(), String> {
    let pm = PassManager::new(ctx);

    // Lower structured control flow (scf.for → cf.br/cf.cond_br)
    pm.add_pass(pass::conversion::create_scf_to_control_flow());

    // Lower math dialect to LLVM (exp, etc.)
    pm.add_pass(pass::conversion::create_math_to_llvm());

    // Lower arithmetic to LLVM
    pm.add_pass(pass::conversion::create_arith_to_llvm());

    // Finalize memref to LLVM (handles memref descriptors → LLVM structs)
    pm.add_pass(pass::conversion::create_finalize_mem_ref_to_llvm());

    // Lower func dialect to LLVM
    pm.add_pass(pass::conversion::create_func_to_llvm());

    // Clean up unrealized casts left by partial conversions
    pm.add_pass(pass::conversion::create_reconcile_unrealized_casts());

    pm.run(module)
        .map_err(|_| "MLIR lowering pass pipeline failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::MlirContext;
    use crate::lower::{lower_elementwise, ElementwiseOp};

    #[test]
    fn test_lowering_pipeline_add() {
        let mlir_ctx = MlirContext::new().unwrap();
        let mut module = mlir_ctx.new_module();
        lower_elementwise(
            mlir_ctx.context(),
            &module,
            ElementwiseOp::Add,
            "tensor_add",
        )
        .unwrap();

        assert!(module.as_operation().verify(), "Pre-lowering verify failed");

        run_lowering_pipeline(mlir_ctx.context(), &mut module).unwrap();

        assert!(
            module.as_operation().verify(),
            "Post-lowering verify failed"
        );
    }
}
