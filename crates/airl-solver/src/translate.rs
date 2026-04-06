use std::collections::HashMap;
use z3::ast::{self, Ast};
use z3::Context;
use airl_syntax::ast::{Expr, ExprKind, LetBinding, AstTypeKind};

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
    Real,
}

/// Translates AIRL expressions to Z3 AST nodes.
pub struct Translator<'ctx> {
    ctx: &'ctx Context,
    int_vars: HashMap<String, ast::Int<'ctx>>,
    bool_vars: HashMap<String, ast::Bool<'ctx>>,
    real_vars: HashMap<String, ast::Real<'ctx>>,
}

impl<'ctx> Translator<'ctx> {
    pub fn new(ctx: &'ctx Context) -> Self {
        Self {
            ctx,
            int_vars: HashMap::new(),
            bool_vars: HashMap::new(),
            real_vars: HashMap::new(),
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

    /// Declare a real variable (for f32/f64 parameters).
    pub fn declare_real(&mut self, name: &str) {
        let var = ast::Real::new_const(self.ctx, name);
        self.real_vars.insert(name.to_string(), var);
    }

    /// Get a reference to an integer variable (for counterexample extraction).
    pub fn get_int_var(&self, name: &str) -> Option<&ast::Int<'ctx>> {
        self.int_vars.get(name)
    }

    /// Get a reference to a real variable (for counterexample extraction).
    pub fn get_real_var(&self, name: &str) -> Option<&ast::Real<'ctx>> {
        self.real_vars.get(name)
    }

    /// Get a reference to a boolean variable.
    pub fn get_bool_var(&self, name: &str) -> Option<&ast::Bool<'ctx>> {
        self.bool_vars.get(name)
    }

    /// Push let bindings into the variable maps, saving prior state.
    /// Returns the saved state on success, or restores on error and returns Err.
    fn push_let_bindings(
        &mut self,
        bindings: &[LetBinding],
    ) -> Result<(
        Vec<(String, Option<ast::Int<'ctx>>)>,
        Vec<(String, Option<ast::Bool<'ctx>>)>,
        Vec<(String, Option<ast::Real<'ctx>>)>,
    ), TranslateError> {
        let mut saved_ints: Vec<(String, Option<ast::Int<'ctx>>)> = Vec::new();
        let mut saved_bools: Vec<(String, Option<ast::Bool<'ctx>>)> = Vec::new();
        let mut saved_reals: Vec<(String, Option<ast::Real<'ctx>>)> = Vec::new();

        for binding in bindings {
            let sort = if let AstTypeKind::Named(tn) = &binding.ty.kind {
                Self::sort_from_type_name(tn)
            } else {
                None
            };

            match sort {
                Some(VarSort::Int) => match self.translate_int(&binding.value) {
                    Ok(v) => {
                        let prev = self.int_vars.insert(binding.name.clone(), v);
                        saved_ints.push((binding.name.clone(), prev));
                    }
                    Err(e) => {
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals);
                        return Err(e);
                    }
                },
                Some(VarSort::Bool) => match self.translate_bool(&binding.value) {
                    Ok(v) => {
                        let prev = self.bool_vars.insert(binding.name.clone(), v);
                        saved_bools.push((binding.name.clone(), prev));
                    }
                    Err(e) => {
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals);
                        return Err(e);
                    }
                },
                Some(VarSort::Real) => match self.translate_real(&binding.value) {
                    Ok(v) => {
                        let prev = self.real_vars.insert(binding.name.clone(), v);
                        saved_reals.push((binding.name.clone(), prev));
                    }
                    Err(e) => {
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals);
                        return Err(e);
                    }
                },
                None => {
                    self.pop_let_bindings(saved_ints, saved_bools, saved_reals);
                    return Err(TranslateError::UnsupportedExpression(
                        format!("let binding '{}': unsupported type", binding.name)
                    ));
                }
            }
        }

        Ok((saved_ints, saved_bools, saved_reals))
    }

    /// Restore variable maps from state saved by `push_let_bindings`.
    fn pop_let_bindings(
        &mut self,
        saved_ints: Vec<(String, Option<ast::Int<'ctx>>)>,
        saved_bools: Vec<(String, Option<ast::Bool<'ctx>>)>,
        saved_reals: Vec<(String, Option<ast::Real<'ctx>>)>,
    ) {
        for (name, prev) in saved_ints.into_iter().rev() {
            match prev {
                Some(v) => { self.int_vars.insert(name, v); }
                None => { self.int_vars.remove(&name); }
            }
        }
        for (name, prev) in saved_bools.into_iter().rev() {
            match prev {
                Some(v) => { self.bool_vars.insert(name, v); }
                None => { self.bool_vars.remove(&name); }
            }
        }
        for (name, prev) in saved_reals.into_iter().rev() {
            match prev {
                Some(v) => { self.real_vars.insert(name, v); }
                None => { self.real_vars.remove(&name); }
            }
        }
    }

    /// Translate an AIRL expression to a Z3 Bool (for contracts).
    pub fn translate_bool(&mut self, expr: &Expr) -> Result<ast::Bool<'ctx>, TranslateError> {
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
                        // Try Int first; if that fails, try Real
                        "=" => self.translate_cmp_eq(&args[0], &args[1], false),
                        "!=" => self.translate_cmp_eq(&args[0], &args[1], true),
                        "<" => self.translate_cmp_ord(&args[0], &args[1], "lt"),
                        ">" => self.translate_cmp_ord(&args[0], &args[1], "gt"),
                        "<=" => self.translate_cmp_ord(&args[0], &args[1], "le"),
                        ">=" => self.translate_cmp_ord(&args[0], &args[1], "ge"),
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

            ExprKind::If(cond, then_e, else_e) => {
                let c = self.translate_bool(cond)?;
                let t = self.translate_bool(then_e)?;
                let e = self.translate_bool(else_e)?;
                Ok(c.ite(&t, &e))
            }

            ExprKind::Let(bindings, body) => {
                let saved = self.push_let_bindings(bindings)?;
                let result = self.translate_bool(body);
                self.pop_let_bindings(saved.0, saved.1, saved.2);
                result
            }

            ExprKind::Do(exprs) => exprs.last()
                .ok_or_else(|| TranslateError::UnsupportedExpression("empty do block".into()))
                .and_then(|e| self.translate_bool(e)),

            ExprKind::Forall(param, where_clause, body) => {
                self.translate_quantifier(param, where_clause.as_deref(), body, true)
            }

            ExprKind::Exists(param, where_clause, body) => {
                self.translate_quantifier(param, where_clause.as_deref(), body, false)
            }

            _ => Err(TranslateError::UnsupportedExpression(
                format!("{:?}", expr.kind)
            )),
        }
    }

    /// Translate an AIRL expression to a Z3 Int.
    pub fn translate_int(&mut self, expr: &Expr) -> Result<ast::Int<'ctx>, TranslateError> {
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
                            } else if args.len() == 1 {
                                let inner = self.translate_int(&args[0])?;
                                let neg_one = ast::Int::from_i64(self.ctx, -1);
                                Ok(ast::Int::mul(self.ctx, &[&neg_one, &inner]))
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

            ExprKind::If(cond, then_e, else_e) => {
                let c = self.translate_bool(cond)?;
                let t = self.translate_int(then_e)?;
                let e = self.translate_int(else_e)?;
                Ok(c.ite(&t, &e))
            }

            ExprKind::Let(bindings, body) => {
                let saved = self.push_let_bindings(bindings)?;
                let result = self.translate_int(body);
                self.pop_let_bindings(saved.0, saved.1, saved.2);
                result
            }

            ExprKind::Do(exprs) => exprs.last()
                .ok_or_else(|| TranslateError::UnsupportedExpression("empty do block".into()))
                .and_then(|e| self.translate_int(e)),

            _ => Err(TranslateError::UnsupportedExpression(
                format!("int context: {:?}", expr.kind)
            )),
        }
    }

    /// Translate an AIRL expression to a Z3 Real (for float arithmetic).
    pub fn translate_real(&mut self, expr: &Expr) -> Result<ast::Real<'ctx>, TranslateError> {
        match &expr.kind {
            ExprKind::FloatLit(v) => {
                // Approximate: convert f64 to rational num/den via scaled integer
                // For exact representation, we scale by 1_000_000 and use from_real
                let scaled = (*v * 1_000_000.0).round() as i32;
                Ok(ast::Real::from_real(self.ctx, scaled, 1_000_000))
            }

            ExprKind::IntLit(v) => {
                // Allow int literals in real context
                Ok(ast::Real::from_real(self.ctx, *v as i32, 1))
            }

            ExprKind::SymbolRef(name) => {
                if let Some(var) = self.real_vars.get(name) {
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
                                .map(|a| self.translate_real(a))
                                .collect();
                            let operands = operands?;
                            let refs: Vec<&ast::Real> = operands.iter().collect();
                            Ok(ast::Real::add(self.ctx, &refs))
                        }
                        "-" => {
                            if args.len() == 2 {
                                let lhs = self.translate_real(&args[0])?;
                                let rhs = self.translate_real(&args[1])?;
                                Ok(ast::Real::sub(self.ctx, &[&lhs, &rhs]))
                            } else {
                                Err(TranslateError::UnsupportedExpression("unary minus".into()))
                            }
                        }
                        "*" => {
                            let operands: Result<Vec<_>, _> = args.iter()
                                .map(|a| self.translate_real(a))
                                .collect();
                            let operands = operands?;
                            let refs: Vec<&ast::Real> = operands.iter().collect();
                            Ok(ast::Real::mul(self.ctx, &refs))
                        }
                        "/" => {
                            let lhs = self.translate_real(&args[0])?;
                            let rhs = self.translate_real(&args[1])?;
                            Ok(lhs.div(&rhs))
                        }
                        _ => Err(TranslateError::UnsupportedExpression(
                            format!("real context: {}", op)
                        )),
                    }
                } else {
                    Err(TranslateError::UnsupportedExpression("non-symbol callee".into()))
                }
            }

            ExprKind::If(cond, then_e, else_e) => {
                let c = self.translate_bool(cond)?;
                let t = self.translate_real(then_e)?;
                let e = self.translate_real(else_e)?;
                Ok(c.ite(&t, &e))
            }

            ExprKind::Let(bindings, body) => {
                let saved = self.push_let_bindings(bindings)?;
                let result = self.translate_real(body);
                self.pop_let_bindings(saved.0, saved.1, saved.2);
                result
            }

            ExprKind::Do(exprs) => exprs.last()
                .ok_or_else(|| TranslateError::UnsupportedExpression("empty do block".into()))
                .and_then(|e| self.translate_real(e)),

            _ => Err(TranslateError::UnsupportedExpression(
                format!("real context: {:?}", expr.kind)
            )),
        }
    }

    /// Translate an equality/inequality comparison, trying Int then Real.
    fn translate_cmp_eq(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        negate: bool,
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        // Try Int first
        if let (Ok(l), Ok(r)) = (self.translate_int(lhs), self.translate_int(rhs)) {
            let eq = l._eq(&r);
            return Ok(if negate { eq.not() } else { eq });
        }
        // Try Real
        if let (Ok(l), Ok(r)) = (self.translate_real(lhs), self.translate_real(rhs)) {
            let eq = l._eq(&r);
            return Ok(if negate { eq.not() } else { eq });
        }
        Err(TranslateError::UnsupportedExpression("cannot translate comparison operands".into()))
    }

    /// Translate an ordering comparison (lt/le/gt/ge), trying Int then Real.
    fn translate_cmp_ord(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        op: &str,
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        // Try Int first
        if let (Ok(l), Ok(r)) = (self.translate_int(lhs), self.translate_int(rhs)) {
            return Ok(match op {
                "lt" => l.lt(&r),
                "le" => l.le(&r),
                "gt" => l.gt(&r),
                "ge" => l.ge(&r),
                _ => unreachable!(),
            });
        }
        // Try Real
        if let (Ok(l), Ok(r)) = (self.translate_real(lhs), self.translate_real(rhs)) {
            return Ok(match op {
                "lt" => l.lt(&r),
                "le" => l.le(&r),
                "gt" => l.gt(&r),
                "ge" => l.ge(&r),
                _ => unreachable!(),
            });
        }
        Err(TranslateError::UnsupportedExpression(
            format!("cannot translate {} operands", op)
        ))
    }

    /// Translate a quantified expression (forall/exists) to Z3.
    /// Creates a fresh Z3 constant for the bound variable, temporarily adds it to
    /// the variable maps, builds the body formula, then removes it.
    fn translate_quantifier(
        &mut self,
        param: &airl_syntax::ast::Param,
        where_clause: Option<&Expr>,
        body: &Expr,
        is_forall: bool,
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        let var_name = &param.name;
        let type_name = match &param.ty.kind {
            airl_syntax::ast::AstTypeKind::Named(n) => n.as_str(),
            _ => return Err(TranslateError::UnsupportedType(format!("{:?}", param.ty))),
        };

        let sort = Self::sort_from_type_name(type_name)
            .ok_or_else(|| TranslateError::UnsupportedType(type_name.to_string()))?;

        // Create fresh Z3 constant for the bound variable
        match sort {
            VarSort::Int => {
                let bound_var = ast::Int::new_const(self.ctx, var_name.as_str());
                // Save and replace any existing variable
                let prev = self.int_vars.insert(var_name.clone(), bound_var.clone());

                // Build body formula
                let body_bool = self.translate_bool(body)?;

                // If there's a where clause, build (where => body) for forall,
                // or (where && body) for exists
                let formula = if let Some(guard) = where_clause {
                    let guard_bool = self.translate_bool(guard)?;
                    if is_forall {
                        // forall x. (guard(x) => body(x))
                        guard_bool.implies(&body_bool)
                    } else {
                        // exists x. (guard(x) && body(x))
                        ast::Bool::and(self.ctx, &[&guard_bool, &body_bool])
                    }
                } else {
                    body_bool
                };

                // Build quantified formula
                let result = if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                };

                // Restore previous variable binding
                if let Some(prev_var) = prev {
                    self.int_vars.insert(var_name.clone(), prev_var);
                } else {
                    self.int_vars.remove(var_name);
                }

                Ok(result)
            }
            VarSort::Bool => {
                let bound_var = ast::Bool::new_const(self.ctx, var_name.as_str());
                let prev = self.bool_vars.insert(var_name.clone(), bound_var.clone());

                let body_bool = self.translate_bool(body)?;
                let formula = if let Some(guard) = where_clause {
                    let guard_bool = self.translate_bool(guard)?;
                    if is_forall {
                        guard_bool.implies(&body_bool)
                    } else {
                        ast::Bool::and(self.ctx, &[&guard_bool, &body_bool])
                    }
                } else {
                    body_bool
                };

                let result = if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                };

                if let Some(prev_var) = prev {
                    self.bool_vars.insert(var_name.clone(), prev_var);
                } else {
                    self.bool_vars.remove(var_name);
                }

                Ok(result)
            }
            VarSort::Real => {
                let bound_var = ast::Real::new_const(self.ctx, var_name.as_str());
                let prev = self.real_vars.insert(var_name.clone(), bound_var.clone());

                let body_bool = self.translate_bool(body)?;
                let formula = if let Some(guard) = where_clause {
                    let guard_bool = self.translate_bool(guard)?;
                    if is_forall {
                        guard_bool.implies(&body_bool)
                    } else {
                        ast::Bool::and(self.ctx, &[&guard_bool, &body_bool])
                    }
                } else {
                    body_bool
                };

                let result = if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                };

                if let Some(prev_var) = prev {
                    self.real_vars.insert(var_name.clone(), prev_var);
                } else {
                    self.real_vars.remove(var_name);
                }

                Ok(result)
            }
        }
    }

    /// Determine variable sort from an AIRL type name.
    pub fn sort_from_type_name(name: &str) -> Option<VarSort> {
        match name {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "Nat" => Some(VarSort::Int),
            "bool" => Some(VarSort::Bool),
            "f16" | "f32" | "f64" | "bf16" => Some(VarSort::Real),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use z3::Config;

    fn make_ctx() -> Context {
        let cfg = Config::new();
        Context::new(&cfg)
    }

    #[test]
    fn translate_int_literal() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        let expr = Expr { kind: ExprKind::IntLit(42), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok());
    }

    #[test]
    fn translate_bool_literal() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
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
        let mut t = Translator::new(&ctx);
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
        let mut t = Translator::new(&ctx);
        let expr = Expr { kind: ExprKind::StrLit("hello".into()), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_err());
    }

    #[test]
    fn sort_from_type() {
        assert!(matches!(Translator::sort_from_type_name("i32"), Some(VarSort::Int)));
        assert!(matches!(Translator::sort_from_type_name("bool"), Some(VarSort::Bool)));
        assert!(matches!(Translator::sort_from_type_name("f32"), Some(VarSort::Real)));
        assert!(matches!(Translator::sort_from_type_name("f64"), Some(VarSort::Real)));
        assert!(Translator::sort_from_type_name("String").is_none());
    }
}
