use std::collections::HashMap;

use airl_syntax::ast::*;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{self, types, BlockArg, InstBuilder, Value as CrValue};
use cranelift_frontend::{FunctionBuilder, Variable};

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

/// Walks the AIRL AST and emits Cranelift IR via a FunctionBuilder.
pub struct Lowerer<'a, 'b: 'a> {
    builder: &'a mut FunctionBuilder<'b>,
    variables: HashMap<String, (Variable, ir::Type)>,
}

impl<'a, 'b: 'a> Lowerer<'a, 'b> {
    pub fn new(builder: &'a mut FunctionBuilder<'b>) -> Self {
        Self {
            builder,
            variables: HashMap::new(),
        }
    }

    /// Define a variable (e.g., a function parameter) in the lowerer's scope.
    pub fn define_variable(&mut self, name: &str, val: CrValue, ty: ir::Type) {
        let var = self.builder.declare_var(ty);
        self.builder.def_var(var, val);
        self.variables.insert(name.to_string(), (var, ty));
    }

    /// Lower an AST expression to Cranelift IR, returning the resulting value and its type.
    pub fn lower_expr(&mut self, expr: &Expr) -> Result<(CrValue, ir::Type), LowerError> {
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
                let (var, ty) = self
                    .variables
                    .get(name)
                    .ok_or_else(|| LowerError::UndefinedVariable(name.clone()))?;
                let val = self.builder.use_var(*var);
                Ok((val, *ty))
            }
            ExprKind::FnCall(callee, args) => self.lower_builtin_call(callee, args),
            ExprKind::If(cond, then_expr, else_expr) => {
                self.lower_if(cond, then_expr, else_expr)
            }
            ExprKind::Let(bindings, body) => self.lower_let(bindings, body),
            ExprKind::Do(exprs) => self.lower_do(exprs),
            _ => Err(LowerError::UnsupportedExpression(format!(
                "{:?}",
                expr.kind
            ))),
        }
    }

    fn lower_builtin_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
    ) -> Result<(CrValue, ir::Type), LowerError> {
        let name = match &callee.kind {
            ExprKind::SymbolRef(s) => s.as_str(),
            _ => {
                return Err(LowerError::UnsupportedExpression(
                    "non-symbol callee".into(),
                ))
            }
        };

        match name {
            "+" | "-" | "*" | "/" | "%" => self.lower_arithmetic(name, args),
            "=" | "!=" | "<" | ">" | "<=" | ">=" => self.lower_comparison(name, args),
            "and" | "or" => self.lower_logic_binop(name, args),
            "not" => self.lower_not(args),
            _ => Err(LowerError::UnsupportedExpression(format!(
                "unsupported builtin: {}",
                name
            ))),
        }
    }

    fn lower_arithmetic(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<(CrValue, ir::Type), LowerError> {
        if args.len() != 2 {
            return Err(LowerError::UnsupportedExpression(format!(
                "{} needs 2 args",
                op
            )));
        }
        let (lhs, lty) = self.lower_expr(&args[0])?;
        let (rhs, rty) = self.lower_expr(&args[1])?;
        if lty != rty {
            return Err(LowerError::UnsupportedType(format!(
                "mismatched types: {:?} vs {:?}",
                lty, rty
            )));
        }
        let val = if is_float_type(lty) {
            match op {
                "+" => self.builder.ins().fadd(lhs, rhs),
                "-" => self.builder.ins().fsub(lhs, rhs),
                "*" => self.builder.ins().fmul(lhs, rhs),
                "/" => self.builder.ins().fdiv(lhs, rhs),
                "%" => {
                    return Err(LowerError::UnsupportedExpression(
                        "float modulus".into(),
                    ))
                }
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

    fn lower_comparison(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<(CrValue, ir::Type), LowerError> {
        if args.len() != 2 {
            return Err(LowerError::UnsupportedExpression(format!(
                "{} needs 2 args",
                op
            )));
        }
        let (lhs, lty) = self.lower_expr(&args[0])?;
        let (rhs, rty) = self.lower_expr(&args[1])?;
        if lty != rty {
            return Err(LowerError::UnsupportedType(format!(
                "mismatched types: {:?} vs {:?}",
                lty, rty
            )));
        }

        let val = if is_float_type(lty) {
            let cc = match op {
                "=" => FloatCC::Equal,
                "!=" => FloatCC::NotEqual,
                "<" => FloatCC::LessThan,
                ">" => FloatCC::GreaterThan,
                "<=" => FloatCC::LessThanOrEqual,
                ">=" => FloatCC::GreaterThanOrEqual,
                _ => unreachable!(),
            };
            self.builder.ins().fcmp(cc, lhs, rhs)
        } else {
            let cc = match op {
                "=" => IntCC::Equal,
                "!=" => IntCC::NotEqual,
                "<" => IntCC::SignedLessThan,
                ">" => IntCC::SignedGreaterThan,
                "<=" => IntCC::SignedLessThanOrEqual,
                ">=" => IntCC::SignedGreaterThanOrEqual,
                _ => unreachable!(),
            };
            self.builder.ins().icmp(cc, lhs, rhs)
        };
        Ok((val, types::I8))
    }

    fn lower_logic_binop(
        &mut self,
        op: &str,
        args: &[Expr],
    ) -> Result<(CrValue, ir::Type), LowerError> {
        if args.len() != 2 {
            return Err(LowerError::UnsupportedExpression(format!(
                "{} needs 2 args",
                op
            )));
        }
        let (lhs, _) = self.lower_expr(&args[0])?;
        let (rhs, _) = self.lower_expr(&args[1])?;
        let val = match op {
            "and" => self.builder.ins().band(lhs, rhs),
            "or" => self.builder.ins().bor(lhs, rhs),
            _ => unreachable!(),
        };
        Ok((val, types::I8))
    }

    fn lower_not(&mut self, args: &[Expr]) -> Result<(CrValue, ir::Type), LowerError> {
        if args.len() != 1 {
            return Err(LowerError::UnsupportedExpression(
                "not needs 1 arg".into(),
            ));
        }
        let (val, _) = self.lower_expr(&args[0])?;
        let one = self.builder.ins().iconst(types::I8, 1);
        let result = self.builder.ins().bxor(val, one);
        Ok((result, types::I8))
    }

    fn lower_if(
        &mut self,
        cond: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
    ) -> Result<(CrValue, ir::Type), LowerError> {
        let (cond_val, _) = self.lower_expr(cond)?;

        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();

        self.builder.ins().brif(
            cond_val,
            then_block,
            &[] as &[BlockArg],
            else_block,
            &[] as &[BlockArg],
        );

        // Then branch
        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        let (then_val, then_ty) = self.lower_expr(then_expr)?;
        self.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(then_val)]);

        // Else branch
        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        let (else_val, _) = self.lower_expr(else_expr)?;
        self.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(else_val)]);

        // Merge block — block param carries the result
        self.builder.append_block_param(merge_block, then_ty);
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        let result = self.builder.block_params(merge_block)[0];

        Ok((result, then_ty))
    }

    fn lower_let(
        &mut self,
        bindings: &[LetBinding],
        body: &Expr,
    ) -> Result<(CrValue, ir::Type), LowerError> {
        for binding in bindings {
            let (val, ty) = self.lower_expr(&binding.value)?;
            self.define_variable(&binding.name, val, ty);
        }
        self.lower_expr(body)
    }

    fn lower_do(&mut self, exprs: &[Expr]) -> Result<(CrValue, ir::Type), LowerError> {
        if exprs.is_empty() {
            return Err(LowerError::UnsupportedExpression(
                "empty do block".into(),
            ));
        }
        let mut result = None;
        for expr in exprs {
            result = Some(self.lower_expr(expr)?);
        }
        Ok(result.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_error_display() {
        let err = LowerError::UnsupportedExpression("match".into());
        assert_eq!(err.to_string(), "unsupported expression: match");
    }

    #[test]
    fn lower_error_undefined_var() {
        let err = LowerError::UndefinedVariable("x".into());
        assert_eq!(err.to_string(), "undefined variable: x");
    }
}
