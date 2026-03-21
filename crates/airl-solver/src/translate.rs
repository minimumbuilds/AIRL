use std::collections::HashMap;
use z3::ast::{self, Ast};
use z3::{Config, Context};
use airl_syntax::ast::{Expr, ExprKind};

/// Error during AIRL → Z3 translation.
#[derive(Debug, Clone)]
pub enum TranslateError {
    UnsupportedExpression(String),
    UnsupportedType(String),
    UndefinedVariable(String),
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranslateError::UnsupportedExpression(e) => write!(f, "unsupported: {}", e),
            TranslateError::UnsupportedType(t) => write!(f, "unsupported type: {}", t),
            TranslateError::UndefinedVariable(v) => write!(f, "undefined: {}", v),
        }
    }
}

/// Which Z3 sort a variable has.
#[derive(Debug, Clone, Copy)]
pub enum VarSort {
    Int,
    Bool,
}

/// Translates AIRL expressions to Z3 AST nodes.
pub struct Translator<'ctx> {
    ctx: &'ctx Context,
    int_vars: HashMap<String, ast::Int<'ctx>>,
    bool_vars: HashMap<String, ast::Bool<'ctx>>,
}

impl<'ctx> Translator<'ctx> {
    pub fn new(ctx: &'ctx Context) -> Self {
        Self {
            ctx,
            int_vars: HashMap::new(),
            bool_vars: HashMap::new(),
        }
    }

    /// Declare an integer variable (parameter or result).
    pub fn declare_int(&mut self, name: &str) {
        let var = ast::Int::new_const(self.ctx, name);
        self.int_vars.insert(name.to_string(), var);
    }

    /// Declare a boolean variable.
    pub fn declare_bool(&mut self, name: &str) {
        let var = ast::Bool::new_const(self.ctx, name);
        self.bool_vars.insert(name.to_string(), var);
    }

    /// Get a reference to an integer variable (for counterexample extraction).
    pub fn get_int_var(&self, name: &str) -> Option<&ast::Int<'ctx>> {
        self.int_vars.get(name)
    }

    /// Translate an AIRL expression to a Z3 Bool (for contracts).
    pub fn translate_bool(&self, expr: &Expr) -> Result<ast::Bool<'ctx>, TranslateError> {
        match &expr.kind {
            ExprKind::BoolLit(v) => Ok(ast::Bool::from_bool(self.ctx, *v)),

            ExprKind::SymbolRef(name) => {
                if let Some(var) = self.bool_vars.get(name) {
                    Ok(var.clone())
                } else {
                    Err(TranslateError::UndefinedVariable(name.clone()))
                }
            }

            ExprKind::FnCall(callee, args) => {
                if let ExprKind::SymbolRef(op) = &callee.kind {
                    match op.as_str() {
                        // Comparison operators → Bool result
                        "=" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs._eq(&rhs))
                        }
                        "!=" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs._eq(&rhs).not())
                        }
                        "<" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs.lt(&rhs))
                        }
                        ">" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs.gt(&rhs))
                        }
                        "<=" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs.le(&rhs))
                        }
                        ">=" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs.ge(&rhs))
                        }
                        // Boolean operators
                        "and" => {
                            let lhs = self.translate_bool(&args[0])?;
                            let rhs = self.translate_bool(&args[1])?;
                            Ok(ast::Bool::and(self.ctx, &[&lhs, &rhs]))
                        }
                        "or" => {
                            let lhs = self.translate_bool(&args[0])?;
                            let rhs = self.translate_bool(&args[1])?;
                            Ok(ast::Bool::or(self.ctx, &[&lhs, &rhs]))
                        }
                        "not" => {
                            let inner = self.translate_bool(&args[0])?;
                            Ok(inner.not())
                        }
                        "valid" => {
                            // valid(x) is always true in Z3 context
                            Ok(ast::Bool::from_bool(self.ctx, true))
                        }
                        _ => Err(TranslateError::UnsupportedExpression(
                            format!("boolean context: {}", op)
                        )),
                    }
                } else {
                    Err(TranslateError::UnsupportedExpression("non-symbol callee".into()))
                }
            }

            _ => Err(TranslateError::UnsupportedExpression(
                format!("{:?}", expr.kind)
            )),
        }
    }

    /// Translate an AIRL expression to a Z3 Int.
    pub fn translate_int(&self, expr: &Expr) -> Result<ast::Int<'ctx>, TranslateError> {
        match &expr.kind {
            ExprKind::IntLit(v) => Ok(ast::Int::from_i64(self.ctx, *v)),

            ExprKind::SymbolRef(name) => {
                if let Some(var) = self.int_vars.get(name) {
                    Ok(var.clone())
                } else {
                    Err(TranslateError::UndefinedVariable(name.clone()))
                }
            }

            ExprKind::FnCall(callee, args) => {
                if let ExprKind::SymbolRef(op) = &callee.kind {
                    match op.as_str() {
                        "+" => {
                            let operands: Result<Vec<_>, _> = args.iter()
                                .map(|a| self.translate_int(a))
                                .collect();
                            let operands = operands?;
                            let refs: Vec<&ast::Int> = operands.iter().collect();
                            Ok(ast::Int::add(self.ctx, &refs))
                        }
                        "-" => {
                            if args.len() == 2 {
                                let lhs = self.translate_int(&args[0])?;
                                let rhs = self.translate_int(&args[1])?;
                                Ok(ast::Int::sub(self.ctx, &[&lhs, &rhs]))
                            } else {
                                Err(TranslateError::UnsupportedExpression("unary minus".into()))
                            }
                        }
                        "*" => {
                            let operands: Result<Vec<_>, _> = args.iter()
                                .map(|a| self.translate_int(a))
                                .collect();
                            let operands = operands?;
                            let refs: Vec<&ast::Int> = operands.iter().collect();
                            Ok(ast::Int::mul(self.ctx, &refs))
                        }
                        "/" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs.div(&rhs))
                        }
                        "%" => {
                            let lhs = self.translate_int(&args[0])?;
                            let rhs = self.translate_int(&args[1])?;
                            Ok(lhs.modulo(&rhs))
                        }
                        _ => Err(TranslateError::UnsupportedExpression(
                            format!("int context: {}", op)
                        )),
                    }
                } else {
                    Err(TranslateError::UnsupportedExpression("non-symbol callee".into()))
                }
            }

            _ => Err(TranslateError::UnsupportedExpression(
                format!("int context: {:?}", expr.kind)
            )),
        }
    }

    /// Determine variable sort from an AIRL type name.
    pub fn sort_from_type_name(name: &str) -> Option<VarSort> {
        match name {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "Nat" => Some(VarSort::Int),
            "bool" => Some(VarSort::Bool),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> Context {
        let cfg = Config::new();
        Context::new(&cfg)
    }

    #[test]
    fn translate_int_literal() {
        let ctx = make_ctx();
        let t = Translator::new(&ctx);
        let expr = Expr { kind: ExprKind::IntLit(42), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok());
    }

    #[test]
    fn translate_bool_literal() {
        let ctx = make_ctx();
        let t = Translator::new(&ctx);
        let expr = Expr { kind: ExprKind::BoolLit(true), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_ok());
    }

    #[test]
    fn translate_int_variable() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        let expr = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok());
    }

    #[test]
    fn translate_undefined_variable() {
        let ctx = make_ctx();
        let t = Translator::new(&ctx);
        let expr = Expr { kind: ExprKind::SymbolRef("y".into()), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_err());
    }

    #[test]
    fn translate_addition() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("a");
        t.declare_int("b");
        // (+ a b)
        let callee = Expr { kind: ExprKind::SymbolRef("+".into()), span: airl_syntax::Span::dummy() };
        let a = Expr { kind: ExprKind::SymbolRef("a".into()), span: airl_syntax::Span::dummy() };
        let b = Expr { kind: ExprKind::SymbolRef("b".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![a, b]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok());
    }

    #[test]
    fn translate_equality() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("result");
        t.declare_int("a");
        // (= result a)
        let callee = Expr { kind: ExprKind::SymbolRef("=".into()), span: airl_syntax::Span::dummy() };
        let r = Expr { kind: ExprKind::SymbolRef("result".into()), span: airl_syntax::Span::dummy() };
        let a = Expr { kind: ExprKind::SymbolRef("a".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![r, a]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_ok());
    }

    #[test]
    fn translate_valid_is_true() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        let callee = Expr { kind: ExprKind::SymbolRef("valid".into()), span: airl_syntax::Span::dummy() };
        let x = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![x]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_ok());
    }

    #[test]
    fn translate_unsupported_string() {
        let ctx = make_ctx();
        let t = Translator::new(&ctx);
        let expr = Expr { kind: ExprKind::StrLit("hello".into()), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_err());
    }

    #[test]
    fn sort_from_type() {
        assert!(matches!(Translator::sort_from_type_name("i32"), Some(VarSort::Int)));
        assert!(matches!(Translator::sort_from_type_name("bool"), Some(VarSort::Bool)));
        assert!(Translator::sort_from_type_name("String").is_none());
    }
}
