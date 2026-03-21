use melior::dialect::{arith, func, memref, scf};
use melior::ir::attribute::{Attribute, FloatAttribute, IntegerAttribute, StringAttribute, TypeAttribute};
use melior::ir::r#type::{FunctionType, MemRefType};
use melior::ir::ValueLike;
use melior::ir::{Block, Location, Module, Operation, Region, Type, Value};
use melior::Context;

/// Build a `math.exp` operation (not in melior's convenience API).
fn math_exp<'c>(input: Value<'c, '_>, loc: Location<'c>) -> Operation<'c> {
    melior::ir::operation::OperationBuilder::new("math.exp", loc)
        .add_operands(&[input])
        .add_results(&[input.r#type()])
        .build()
        .expect("valid math.exp operation")
}

/// Which element-wise operation to lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementwiseOp {
    Add,
    Mul,
}

/// Lower an element-wise operation (add or mul) on 1-D memrefs of f64.
///
/// Generates MLIR equivalent to:
/// ```mlir
/// func.func @tensor_add(%a: memref<?xf64>, %b: memref<?xf64>,
///                        %out: memref<?xf64>, %len: index) {
///   scf.for %i = 0 to %len step 1 {
///     %va = memref.load %a[%i] : memref<?xf64>
///     %vb = memref.load %b[%i] : memref<?xf64>
///     %r  = arith.addf %va, %vb : f64
///     memref.store %r, %out[%i] : memref<?xf64>
///   }
///   return
/// }
/// ```
pub fn lower_elementwise<'c>(
    ctx: &'c Context,
    module: &Module<'c>,
    op: ElementwiseOp,
    fn_name: &str,
) -> Result<(), String> {
    let loc = Location::unknown(ctx);
    let f64_type = Type::float64(ctx);
    let index_type = Type::index(ctx);
    let memref_type: Type =
        MemRefType::new(f64_type, &[i64::MIN], None, None).into();

    let fn_type = FunctionType::new(
        ctx,
        &[memref_type, memref_type, memref_type, index_type],
        &[],
    );

    let fn_region = Region::new();
    let fn_block = Block::new(&[
        (memref_type, loc),
        (memref_type, loc),
        (memref_type, loc),
        (index_type, loc),
    ]);

    let a = fn_block.argument(0).map_err(|e| e.to_string())?.into();
    let b = fn_block.argument(1).map_err(|e| e.to_string())?.into();
    let out = fn_block.argument(2).map_err(|e| e.to_string())?.into();
    let len = fn_block.argument(3).map_err(|e| e.to_string())?.into();

    let c0 = fn_block.append_operation(arith::constant(
        ctx,
        IntegerAttribute::new(index_type, 0).into(),
        loc,
    ));
    let c1 = fn_block.append_operation(arith::constant(
        ctx,
        IntegerAttribute::new(index_type, 1).into(),
        loc,
    ));
    let zero = c0.result(0).map_err(|e| e.to_string())?.into();
    let one = c1.result(0).map_err(|e| e.to_string())?.into();

    // Loop body: load a[i], load b[i], compute, store to out[i]
    let loop_region = Region::new();
    let loop_block = Block::new(&[(index_type, loc)]);
    let iv = loop_block.argument(0).map_err(|e| e.to_string())?.into();

    let load_a = loop_block.append_operation(memref::load(a, &[iv], loc));
    let va = load_a.result(0).map_err(|e| e.to_string())?.into();

    let load_b = loop_block.append_operation(memref::load(b, &[iv], loc));
    let vb = load_b.result(0).map_err(|e| e.to_string())?.into();

    let compute = match op {
        ElementwiseOp::Add => loop_block.append_operation(arith::addf(va, vb, loc)),
        ElementwiseOp::Mul => loop_block.append_operation(arith::mulf(va, vb, loc)),
    };
    let result = compute.result(0).map_err(|e| e.to_string())?.into();

    loop_block.append_operation(memref::store(result, out, &[iv], loc));
    loop_block.append_operation(scf::r#yield(&[], loc));
    loop_region.append_block(loop_block);

    fn_block.append_operation(scf::r#for(zero, len, one, loop_region, loc));
    fn_block.append_operation(func::r#return(&[], loc));
    fn_region.append_block(fn_block);

    let func_op = func::func(
        ctx,
        StringAttribute::new(ctx, fn_name),
        TypeAttribute::new(fn_type.into()),
        fn_region,
        &[(
            melior::ir::Identifier::new(ctx, "llvm.emit_c_interface"),
            Attribute::unit(ctx),
        )],
        loc,
    );

    module.body().append_operation(func_op);
    Ok(())
}

/// Lower a matrix multiplication on 2-D memrefs of f64.
///
/// Generates a triple-nested scf.for loop:
///   for i in 0..m:
///     for j in 0..n:
///       for p in 0..k:
///         out[i,j] += a[i,p] * b[p,j]
///
/// Values from outer regions (a, b, out, bounds, IVs) are implicitly captured
/// by inner scf.for regions — this is standard MLIR SSA dominance.
pub fn lower_matmul<'c>(
    ctx: &'c Context,
    module: &Module<'c>,
    fn_name: &str,
) -> Result<(), String> {
    let loc = Location::unknown(ctx);
    let f64_type = Type::float64(ctx);
    let index_type = Type::index(ctx);
    let memref_2d: Type =
        MemRefType::new(f64_type, &[i64::MIN, i64::MIN], None, None).into();

    let fn_type = FunctionType::new(
        ctx,
        &[
            memref_2d, memref_2d, memref_2d,
            index_type, index_type, index_type,
        ],
        &[],
    );

    let fn_region = Region::new();
    let fn_block = Block::new(&[
        (memref_2d, loc),
        (memref_2d, loc),
        (memref_2d, loc),
        (index_type, loc),
        (index_type, loc),
        (index_type, loc),
    ]);

    let a = fn_block.argument(0).map_err(|e| e.to_string())?.into();
    let b = fn_block.argument(1).map_err(|e| e.to_string())?.into();
    let out = fn_block.argument(2).map_err(|e| e.to_string())?.into();
    let m = fn_block.argument(3).map_err(|e| e.to_string())?.into();
    let k = fn_block.argument(4).map_err(|e| e.to_string())?.into();
    let n = fn_block.argument(5).map_err(|e| e.to_string())?.into();

    let c0 = fn_block.append_operation(arith::constant(
        ctx,
        IntegerAttribute::new(index_type, 0).into(),
        loc,
    ));
    let c1 = fn_block.append_operation(arith::constant(
        ctx,
        IntegerAttribute::new(index_type, 1).into(),
        loc,
    ));
    let zero = c0.result(0).map_err(|e| e.to_string())?.into();
    let one = c1.result(0).map_err(|e| e.to_string())?.into();

    // Build inside-out: innermost body first, then wrap in loops.
    // MLIR SSA: values from outer regions are captured implicitly.

    // Innermost: k-loop body (accumulate a[i,p] * b[p,j] into out[i,j])
    let k_region = Region::new();
    let k_block = Block::new(&[(index_type, loc)]); // p: index
    let _p: Value = k_block.argument(0).map_err(|e| e.to_string())?.into();
    // We need i and j here, but they come from outer loop blocks.
    // In MLIR, scf.for block args are only the IV. Outer values are captured.
    // So we must build the i-loop block, get its IV, build the j-loop block
    // inside it, get its IV, and build the k-loop body inside that.
    // This means we can't build inside-out; we must build outside-in.
    k_block.append_operation(scf::r#yield(&[], loc));
    k_region.append_block(k_block);

    // Because melior builds IR by appending ops to blocks, and scf.for takes
    // a Region argument, we need to construct the complete inner region before
    // passing it to the outer scf.for. But the inner region needs the outer
    // IV, which is only available as a block argument of the outer region.
    //
    // The solution: build regions outside-in. The i-loop body block defines
    // `i` as its argument, builds the j-loop (whose body defines `j` and
    // builds the k-loop). Each inner region captures outer IVs implicitly.

    drop(k_region); // discard the placeholder

    // Build from outermost to innermost:
    // i-loop region
    let i_region = Region::new();
    let i_block = Block::new(&[(index_type, loc)]);
    let i: Value = i_block.argument(0).map_err(|e| e.to_string())?.into();

    // j-loop region (inside i-loop)
    let j_region = Region::new();
    let j_block = Block::new(&[(index_type, loc)]);
    let j: Value = j_block.argument(0).map_err(|e| e.to_string())?.into();

    // k-loop region (inside j-loop) — the innermost body
    let inner_region = Region::new();
    let inner_block = Block::new(&[(index_type, loc)]);
    let p: Value = inner_block.argument(0).map_err(|e| e.to_string())?.into();

    // out[i,j] += a[i,p] * b[p,j]
    let load_a = inner_block.append_operation(memref::load(a, &[i, p], loc));
    let va = load_a.result(0).map_err(|e| e.to_string())?.into();

    let load_b = inner_block.append_operation(memref::load(b, &[p, j], loc));
    let vb = load_b.result(0).map_err(|e| e.to_string())?.into();

    let prod_op = inner_block.append_operation(arith::mulf(va, vb, loc));
    let prod = prod_op.result(0).map_err(|e| e.to_string())?.into();

    let load_prev = inner_block.append_operation(memref::load(out, &[i, j], loc));
    let prev = load_prev.result(0).map_err(|e| e.to_string())?.into();

    let sum_op = inner_block.append_operation(arith::addf(prev, prod, loc));
    let sum = sum_op.result(0).map_err(|e| e.to_string())?.into();

    inner_block.append_operation(memref::store(sum, out, &[i, j], loc));
    inner_block.append_operation(scf::r#yield(&[], loc));
    inner_region.append_block(inner_block);

    // j-loop body: scf.for p = 0 to k step 1 { inner }
    j_block.append_operation(scf::r#for(zero, k, one, inner_region, loc));
    j_block.append_operation(scf::r#yield(&[], loc));
    j_region.append_block(j_block);

    // i-loop body: scf.for j = 0 to n step 1 { j_body }
    i_block.append_operation(scf::r#for(zero, n, one, j_region, loc));
    i_block.append_operation(scf::r#yield(&[], loc));
    i_region.append_block(i_block);

    // Function body: scf.for i = 0 to m step 1 { i_body }
    fn_block.append_operation(scf::r#for(zero, m, one, i_region, loc));
    fn_block.append_operation(func::r#return(&[], loc));
    fn_region.append_block(fn_block);

    let func_op = func::func(
        ctx,
        StringAttribute::new(ctx, fn_name),
        TypeAttribute::new(fn_type.into()),
        fn_region,
        &[(
            melior::ir::Identifier::new(ctx, "llvm.emit_c_interface"),
            Attribute::unit(ctx),
        )],
        loc,
    );

    module.body().append_operation(func_op);
    Ok(())
}

/// Build the common `llvm.emit_c_interface` attribute list for func ops.
fn emit_c_interface_attrs<'c>(ctx: &'c Context) -> Vec<(melior::ir::Identifier<'c>, Attribute<'c>)> {
    vec![(
        melior::ir::Identifier::new(ctx, "llvm.emit_c_interface"),
        Attribute::unit(ctx),
    )]
}

/// Lower a 2-D transpose on memrefs of f64.
///
/// Generates MLIR equivalent to:
/// ```mlir
/// func.func @tensor_transpose(%in: memref<?x?xf64>, %out: memref<?x?xf64>,
///                              %rows: index, %cols: index) {
///   scf.for %i = 0 to %rows step 1 {
///     scf.for %j = 0 to %cols step 1 {
///       %v = memref.load %in[%i, %j]
///       memref.store %v, %out[%j, %i]
///     }
///   }
///   return
/// }
/// ```
pub fn lower_transpose<'c>(
    ctx: &'c Context,
    module: &Module<'c>,
    fn_name: &str,
) -> Result<(), String> {
    let loc = Location::unknown(ctx);
    let f64_type = Type::float64(ctx);
    let index_type = Type::index(ctx);
    let memref_2d: Type =
        MemRefType::new(f64_type, &[i64::MIN, i64::MIN], None, None).into();

    // (memref<?x?xf64>, memref<?x?xf64>, index, index) -> ()
    let fn_type = FunctionType::new(
        ctx,
        &[memref_2d, memref_2d, index_type, index_type],
        &[],
    );

    let fn_region = Region::new();
    let fn_block = Block::new(&[
        (memref_2d, loc), (memref_2d, loc),
        (index_type, loc), (index_type, loc),
    ]);

    let input = fn_block.argument(0).map_err(|e| e.to_string())?.into();
    let output = fn_block.argument(1).map_err(|e| e.to_string())?.into();
    let rows = fn_block.argument(2).map_err(|e| e.to_string())?.into();
    let cols = fn_block.argument(3).map_err(|e| e.to_string())?.into();

    let c0 = fn_block.append_operation(arith::constant(ctx, IntegerAttribute::new(index_type, 0).into(), loc));
    let c1 = fn_block.append_operation(arith::constant(ctx, IntegerAttribute::new(index_type, 1).into(), loc));
    let zero = c0.result(0).map_err(|e| e.to_string())?.into();
    let one = c1.result(0).map_err(|e| e.to_string())?.into();

    // Outer loop: for i in 0..rows
    let i_region = Region::new();
    let i_block = Block::new(&[(index_type, loc)]);
    let i: Value = i_block.argument(0).map_err(|e| e.to_string())?.into();

    // Inner loop: for j in 0..cols
    let j_region = Region::new();
    let j_block = Block::new(&[(index_type, loc)]);
    let j: Value = j_block.argument(0).map_err(|e| e.to_string())?.into();

    // out[j, i] = in[i, j]
    let load_op = j_block.append_operation(memref::load(input, &[i, j], loc));
    let v = load_op.result(0).map_err(|e| e.to_string())?.into();
    j_block.append_operation(memref::store(v, output, &[j, i], loc));
    j_block.append_operation(scf::r#yield(&[], loc));
    j_region.append_block(j_block);

    i_block.append_operation(scf::r#for(zero, cols, one, j_region, loc));
    i_block.append_operation(scf::r#yield(&[], loc));
    i_region.append_block(i_block);

    fn_block.append_operation(scf::r#for(zero, rows, one, i_region, loc));
    fn_block.append_operation(func::r#return(&[], loc));
    fn_region.append_block(fn_block);

    let attrs = emit_c_interface_attrs(ctx);
    let func_op = func::func(ctx, StringAttribute::new(ctx, fn_name),
        TypeAttribute::new(fn_type.into()), fn_region, &attrs, loc);
    module.body().append_operation(func_op);
    Ok(())
}

/// Lower a numerically-stable softmax on a 1-D memref of f64.
///
/// Three-pass algorithm using stack-allocated accumulators:
///   1. Find max(input) for numerical stability
///   2. Compute exp(input[i] - max) and accumulate sum
///   3. Divide each element by sum → store to output
///
/// Signature: (memref<?xf64>, memref<?xf64>, index) -> ()
pub fn lower_softmax<'c>(
    ctx: &'c Context,
    module: &Module<'c>,
    fn_name: &str,
) -> Result<(), String> {
    let loc = Location::unknown(ctx);
    let f64_type = Type::float64(ctx);
    let index_type = Type::index(ctx);
    let memref_1d: Type =
        MemRefType::new(f64_type, &[i64::MIN], None, None).into();
    let scalar_memref = MemRefType::new(f64_type, &[], None, None);

    let fn_type = FunctionType::new(ctx, &[memref_1d, memref_1d, index_type], &[]);

    let fn_region = Region::new();
    let fn_block = Block::new(&[(memref_1d, loc), (memref_1d, loc), (index_type, loc)]);

    let input = fn_block.argument(0).map_err(|e| e.to_string())?.into();
    let output = fn_block.argument(1).map_err(|e| e.to_string())?.into();
    let len = fn_block.argument(2).map_err(|e| e.to_string())?.into();

    let c0 = fn_block.append_operation(arith::constant(ctx, IntegerAttribute::new(index_type, 0).into(), loc));
    let c1 = fn_block.append_operation(arith::constant(ctx, IntegerAttribute::new(index_type, 1).into(), loc));
    let zero = c0.result(0).map_err(|e| e.to_string())?.into();
    let one = c1.result(0).map_err(|e| e.to_string())?.into();

    let neg_inf_op = fn_block.append_operation(arith::constant(
        ctx, FloatAttribute::new(ctx, f64_type, f64::NEG_INFINITY).into(), loc));
    let neg_inf = neg_inf_op.result(0).map_err(|e| e.to_string())?.into();

    let zero_f64_op = fn_block.append_operation(arith::constant(
        ctx, FloatAttribute::new(ctx, f64_type, 0.0).into(), loc));
    let zero_f64 = zero_f64_op.result(0).map_err(|e| e.to_string())?.into();

    // Allocate scalar accumulators on the stack (memref<f64> with 0 dims = scalar)
    let alloca_max = fn_block.append_operation(memref::alloca(ctx, scalar_memref, &[], &[], None, loc));
    let max_ref = alloca_max.result(0).map_err(|e| e.to_string())?.into();
    fn_block.append_operation(memref::store(neg_inf, max_ref, &[], loc));

    let alloca_sum = fn_block.append_operation(memref::alloca(ctx, scalar_memref, &[], &[], None, loc));
    let sum_ref = alloca_sum.result(0).map_err(|e| e.to_string())?.into();
    fn_block.append_operation(memref::store(zero_f64, sum_ref, &[], loc));

    // --- Pass 1: find max ---
    let max_region = Region::new();
    let max_block = Block::new(&[(index_type, loc)]);
    let max_iv = max_block.argument(0).map_err(|e| e.to_string())?.into();
    let ld_v = max_block.append_operation(memref::load(input, &[max_iv], loc));
    let v = ld_v.result(0).map_err(|e| e.to_string())?.into();
    let ld_cur = max_block.append_operation(memref::load(max_ref, &[], loc));
    let cur = ld_cur.result(0).map_err(|e| e.to_string())?.into();
    // arith.maxf may not be registered in LLVM 19; use cmpf + select instead
    let cmp_op = max_block.append_operation(arith::cmpf(
        ctx, arith::CmpfPredicate::Ogt, cur, v, loc));
    let cmp_val = cmp_op.result(0).map_err(|e| e.to_string())?.into();
    let new_max_op = max_block.append_operation(arith::select(cmp_val, cur, v, loc));
    let new_max = new_max_op.result(0).map_err(|e| e.to_string())?.into();
    max_block.append_operation(memref::store(new_max, max_ref, &[], loc));
    max_block.append_operation(scf::r#yield(&[], loc));
    max_region.append_block(max_block);
    fn_block.append_operation(scf::r#for(zero, len, one, max_region, loc));

    // --- Pass 2: compute exp(x[i] - max) and accumulate sum ---
    let exp_region = Region::new();
    let exp_block = Block::new(&[(index_type, loc)]);
    let exp_iv = exp_block.argument(0).map_err(|e| e.to_string())?.into();
    let ld_xi = exp_block.append_operation(memref::load(input, &[exp_iv], loc));
    let xi = ld_xi.result(0).map_err(|e| e.to_string())?.into();
    let ld_max = exp_block.append_operation(memref::load(max_ref, &[], loc));
    let max_val = ld_max.result(0).map_err(|e| e.to_string())?.into();
    let sub_op = exp_block.append_operation(arith::subf(xi, max_val, loc));
    let shifted = sub_op.result(0).map_err(|e| e.to_string())?.into();
    let exp_op = exp_block.append_operation(math_exp(shifted, loc));
    let exp_val = exp_op.result(0).map_err(|e| e.to_string())?.into();
    exp_block.append_operation(memref::store(exp_val, output, &[exp_iv], loc));
    let ld_sum = exp_block.append_operation(memref::load(sum_ref, &[], loc));
    let cur_sum = ld_sum.result(0).map_err(|e| e.to_string())?.into();
    let new_sum_op = exp_block.append_operation(arith::addf(cur_sum, exp_val, loc));
    let new_sum = new_sum_op.result(0).map_err(|e| e.to_string())?.into();
    exp_block.append_operation(memref::store(new_sum, sum_ref, &[], loc));
    exp_block.append_operation(scf::r#yield(&[], loc));
    exp_region.append_block(exp_block);
    fn_block.append_operation(scf::r#for(zero, len, one, exp_region, loc));

    // --- Pass 3: divide each exp value by sum ---
    let div_region = Region::new();
    let div_block = Block::new(&[(index_type, loc)]);
    let div_iv = div_block.argument(0).map_err(|e| e.to_string())?.into();
    let ld_exp = div_block.append_operation(memref::load(output, &[div_iv], loc));
    let exp_i = ld_exp.result(0).map_err(|e| e.to_string())?.into();
    let ld_total = div_block.append_operation(memref::load(sum_ref, &[], loc));
    let total = ld_total.result(0).map_err(|e| e.to_string())?.into();
    let div_op = div_block.append_operation(arith::divf(exp_i, total, loc));
    let result = div_op.result(0).map_err(|e| e.to_string())?.into();
    div_block.append_operation(memref::store(result, output, &[div_iv], loc));
    div_block.append_operation(scf::r#yield(&[], loc));
    div_region.append_block(div_block);
    fn_block.append_operation(scf::r#for(zero, len, one, div_region, loc));

    fn_block.append_operation(func::r#return(&[], loc));
    fn_region.append_block(fn_block);

    let attrs = emit_c_interface_attrs(ctx);
    let func_op = func::func(ctx, StringAttribute::new(ctx, fn_name),
        TypeAttribute::new(fn_type.into()), fn_region, &attrs, loc);
    module.body().append_operation(func_op);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::MlirContext;

    #[test]
    fn test_lower_elementwise_add() {
        let mlir_ctx = MlirContext::new().unwrap();
        let module = mlir_ctx.new_module();
        lower_elementwise(mlir_ctx.context(), &module, ElementwiseOp::Add, "tensor_add")
            .unwrap();
        assert!(
            module.as_operation().verify(),
            "Generated MLIR for elementwise add should verify"
        );
    }

    #[test]
    fn test_lower_elementwise_mul() {
        let mlir_ctx = MlirContext::new().unwrap();
        let module = mlir_ctx.new_module();
        lower_elementwise(mlir_ctx.context(), &module, ElementwiseOp::Mul, "tensor_mul")
            .unwrap();
        assert!(
            module.as_operation().verify(),
            "Generated MLIR for elementwise mul should verify"
        );
    }

    #[test]
    fn test_lower_matmul() {
        let mlir_ctx = MlirContext::new().unwrap();
        let module = mlir_ctx.new_module();
        lower_matmul(mlir_ctx.context(), &module, "tensor_matmul").unwrap();
        assert!(
            module.as_operation().verify(),
            "Generated MLIR for matmul should verify"
        );
    }

    #[test]
    fn test_lower_transpose() {
        let mlir_ctx = MlirContext::new().unwrap();
        let module = mlir_ctx.new_module();
        lower_transpose(mlir_ctx.context(), &module, "tensor_transpose").unwrap();
        assert!(
            module.as_operation().verify(),
            "Generated MLIR for transpose should verify"
        );
    }

    #[test]
    fn test_lower_softmax() {
        let mlir_ctx = MlirContext::new().unwrap();
        let module = mlir_ctx.new_module();
        lower_softmax(mlir_ctx.context(), &module, "tensor_softmax").unwrap();
        assert!(
            module.as_operation().verify(),
            "Generated MLIR for softmax should verify"
        );
    }
}
