use std::collections::HashMap;
use z3::ast::{self, Ast};
use z3::Context;
use airl_syntax::ast::{Expr, ExprKind, LetBinding, AstTypeKind, MatchArm, Pattern, PatternKind, LitPattern};

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
    /// Counter for generating unique quantifier-bound variable names when shadowing occurs.
    quant_counter: usize,
}

impl<'ctx> Translator<'ctx> {
    pub fn new(ctx: &'ctx Context) -> Self {
        Self {
            ctx,
            int_vars: HashMap::new(),
            bool_vars: HashMap::new(),
            real_vars: HashMap::new(),
            quant_counter: 0,
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
                            // valid() cannot be encoded as a Z3 predicate — ownership
                            // semantics require a separate checker, not SMT encoding.
                            return Err(TranslateError::UnsupportedExpression(
                                "valid() predicate is not yet encoded — contracts using valid() are unverified".into()
                            ));
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

            ExprKind::Lambda(_, _) => Err(TranslateError::UnsupportedExpression(
                "lambda expressions cannot appear in Z3 contracts (must be immediately applied)".into()
            )),

            ExprKind::Match(scrutinee, arms) => {
                self.translate_match_bool(scrutinee, arms)
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

            ExprKind::Lambda(_, _) => Err(TranslateError::UnsupportedExpression(
                "lambda expressions cannot appear in Z3 contracts (must be immediately applied)".into()
            )),

            ExprKind::Match(scrutinee, arms) => {
                self.translate_match_int(scrutinee, arms)
            }

            _ => Err(TranslateError::UnsupportedExpression(
                format!("int context: {:?}", expr.kind)
            )),
        }
    }

    /// Translate an AIRL expression to a Z3 Real (for float arithmetic).
    pub fn translate_real(&mut self, expr: &Expr) -> Result<ast::Real<'ctx>, TranslateError> {
        match &expr.kind {
            ExprKind::FloatLit(v) => {
                let v = *v;
                if v.is_nan() {
                    return Err(TranslateError::UnsupportedExpression(
                        "NaN cannot be represented as a Z3 Real".into(),
                    ));
                }
                if v.is_infinite() {
                    return Err(TranslateError::UnsupportedExpression(
                        "Infinity cannot be represented as a Z3 Real".into(),
                    ));
                }
                // Scale by 1_000_000 for 6 decimal places of precision.
                // Use i64 to detect overflow before casting to i32 (i32 overflows at ~2147).
                const SCALE: i64 = 1_000_000;
                let scaled = (v * SCALE as f64).round() as i64;
                if scaled >= i32::MIN as i64 && scaled <= i32::MAX as i64 {
                    Ok(ast::Real::from_real(self.ctx, scaled as i32, SCALE as i32))
                } else {
                    // Value exceeds 6-decimal range; fall back to integer approximation.
                    let approx = v.round() as i64;
                    if approx >= i32::MIN as i64 && approx <= i32::MAX as i64 {
                        Ok(ast::Real::from_real(self.ctx, approx as i32, 1))
                    } else {
                        Err(TranslateError::UnsupportedExpression(format!(
                            "float literal {} is too large to represent as Z3 Real",
                            v
                        )))
                    }
                }
            }

            ExprKind::IntLit(v) => {
                // Allow int literals in real context.
                let v = *v;
                if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
                    Ok(ast::Real::from_real(self.ctx, v as i32, 1))
                } else {
                    Err(TranslateError::UnsupportedExpression(format!(
                        "integer literal {} is too large for Z3 Real context",
                        v
                    )))
                }
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

            ExprKind::Lambda(_, _) => Err(TranslateError::UnsupportedExpression(
                "lambda expressions cannot appear in Z3 contracts (must be immediately applied)".into()
            )),

            ExprKind::Match(scrutinee, arms) => {
                self.translate_match_real(scrutinee, arms)
            }

            _ => Err(TranslateError::UnsupportedExpression(
                format!("real context: {:?}", expr.kind)
            )),
        }
    }

    /// Translate an equality/inequality comparison, trying Bool, Int, then Real.
    /// If one side is Int and the other is Real, coerce the Int side with `to_real()`.
    fn translate_cmp_eq(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        negate: bool,
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        // Both Bool (e.g. (= b true) where b is a bool param)
        if let (Ok(l), Ok(r)) = (self.translate_bool(lhs), self.translate_bool(rhs)) {
            // Avoid infinite recursion: BoolLit and SymbolRef(bool) are fine here,
            // but FnCall comparisons could recurse. We check that neither operand is a
            // comparison operator call before proceeding.
            let is_cmp_call = |e: &Expr| matches!(&e.kind,
                ExprKind::FnCall(callee, _) if matches!(&callee.kind,
                    ExprKind::SymbolRef(op) if matches!(op.as_str(), "=" | "!=" | "<" | ">" | "<=" | ">=")
                )
            );
            if !is_cmp_call(lhs) && !is_cmp_call(rhs) {
                let eq = l._eq(&r);
                return Ok(if negate { eq.not() } else { eq });
            }
        }
        // Both Int
        if let (Ok(l), Ok(r)) = (self.translate_int(lhs), self.translate_int(rhs)) {
            let eq = l._eq(&r);
            return Ok(if negate { eq.not() } else { eq });
        }
        // Both Real
        if let (Ok(l), Ok(r)) = (self.translate_real(lhs), self.translate_real(rhs)) {
            let eq = l._eq(&r);
            return Ok(if negate { eq.not() } else { eq });
        }
        // Mixed: Int lhs + Real rhs → coerce lhs to Real
        if let (Ok(l_int), Ok(r_real)) = (self.translate_int(lhs), self.translate_real(rhs)) {
            let l_real = l_int.to_real();
            let eq = l_real._eq(&r_real);
            return Ok(if negate { eq.not() } else { eq });
        }
        // Mixed: Real lhs + Int rhs → coerce rhs to Real
        if let (Ok(l_real), Ok(r_int)) = (self.translate_real(lhs), self.translate_int(rhs)) {
            let r_real = r_int.to_real();
            let eq = l_real._eq(&r_real);
            return Ok(if negate { eq.not() } else { eq });
        }
        Err(TranslateError::UnsupportedExpression("cannot translate comparison operands".into()))
    }

    /// Translate an ordering comparison (lt/le/gt/ge), trying Int then Real.
    /// If one side is Int and the other is Real, coerce the Int side with `to_real()`.
    fn translate_cmp_ord(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        op: &str,
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        let apply_ord_real = |l: ast::Real<'ctx>, r: ast::Real<'ctx>| -> Result<ast::Bool<'ctx>, TranslateError> {
            Ok(match op {
                "lt" => l.lt(&r),
                "le" => l.le(&r),
                "gt" => l.gt(&r),
                "ge" => l.ge(&r),
                _ => return Err(TranslateError::UnsupportedExpression(
                    format!("unknown comparison operator: {}", op)
                )),
            })
        };
        let apply_ord_int = |l: ast::Int<'ctx>, r: ast::Int<'ctx>| -> Result<ast::Bool<'ctx>, TranslateError> {
            Ok(match op {
                "lt" => l.lt(&r),
                "le" => l.le(&r),
                "gt" => l.gt(&r),
                "ge" => l.ge(&r),
                _ => return Err(TranslateError::UnsupportedExpression(
                    format!("unknown comparison operator: {}", op)
                )),
            })
        };

        // Both Int
        if let (Ok(l), Ok(r)) = (self.translate_int(lhs), self.translate_int(rhs)) {
            return apply_ord_int(l, r);
        }
        // Both Real
        if let (Ok(l), Ok(r)) = (self.translate_real(lhs), self.translate_real(rhs)) {
            return apply_ord_real(l, r);
        }
        // Mixed: Int lhs + Real rhs
        if let (Ok(l_int), Ok(r_real)) = (self.translate_int(lhs), self.translate_real(rhs)) {
            return apply_ord_real(l_int.to_real(), r_real);
        }
        // Mixed: Real lhs + Int rhs
        if let (Ok(l_real), Ok(r_int)) = (self.translate_real(lhs), self.translate_int(rhs)) {
            return apply_ord_real(l_real, r_int.to_real());
        }
        Err(TranslateError::UnsupportedExpression(
            format!("cannot translate {} operands", op)
        ))
    }

    /// Translate a quantified expression (forall/exists) to Z3.
    /// Creates a fresh Z3 constant for the bound variable, temporarily adds it to
    /// the variable maps, builds the body formula, then removes it.
    ///
    /// If the bound variable name shadows an outer binding (i.e. a variable with
    /// the same name already exists in the relevant sort map), a unique Z3 name
    /// `{name}_q{n}` is used to avoid Z3 treating the bound variable as the same
    /// constant as the outer free variable.
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
                // Use a fresh Z3 name if the variable shadows an outer binding.
                let z3_name = if self.int_vars.contains_key(var_name) {
                    let n = self.quant_counter;
                    self.quant_counter += 1;
                    format!("{}_q{}", var_name, n)
                } else {
                    var_name.clone()
                };
                let bound_var = ast::Int::new_const(self.ctx, z3_name.as_str());
                // Save and replace any existing variable with the same name
                let saved_prev = self.int_vars.insert(var_name.clone(), bound_var.clone());

                // Build body formula — restore on any error so outer scope is not corrupted
                let formula_result = (|| -> Result<ast::Bool<'ctx>, TranslateError> {
                    let body_bool = self.translate_bool(body)?;
                    if let Some(guard) = where_clause {
                        let guard_bool = self.translate_bool(guard)?;
                        if is_forall {
                            // forall x. (guard(x) => body(x))
                            Ok(guard_bool.implies(&body_bool))
                        } else {
                            // exists x. (guard(x) && body(x))
                            Ok(ast::Bool::and(self.ctx, &[&guard_bool, &body_bool]))
                        }
                    } else {
                        Ok(body_bool)
                    }
                })();

                // Always restore previous variable binding (even on error)
                match saved_prev {
                    Some(v) => { self.int_vars.insert(var_name.clone(), v); }
                    None => { self.int_vars.remove(var_name); }
                }

                let formula = formula_result?;
                Ok(if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                })
            }
            VarSort::Bool => {
                let z3_name = if self.bool_vars.contains_key(var_name) {
                    let n = self.quant_counter;
                    self.quant_counter += 1;
                    format!("{}_q{}", var_name, n)
                } else {
                    var_name.clone()
                };
                let bound_var = ast::Bool::new_const(self.ctx, z3_name.as_str());
                let saved_prev = self.bool_vars.insert(var_name.clone(), bound_var.clone());

                let formula_result = (|| -> Result<ast::Bool<'ctx>, TranslateError> {
                    let body_bool = self.translate_bool(body)?;
                    if let Some(guard) = where_clause {
                        let guard_bool = self.translate_bool(guard)?;
                        if is_forall {
                            Ok(guard_bool.implies(&body_bool))
                        } else {
                            Ok(ast::Bool::and(self.ctx, &[&guard_bool, &body_bool]))
                        }
                    } else {
                        Ok(body_bool)
                    }
                })();

                // Always restore previous variable binding (even on error)
                match saved_prev {
                    Some(v) => { self.bool_vars.insert(var_name.clone(), v); }
                    None => { self.bool_vars.remove(var_name); }
                }

                let formula = formula_result?;
                Ok(if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                })
            }
            VarSort::Real => {
                let z3_name = if self.real_vars.contains_key(var_name) {
                    let n = self.quant_counter;
                    self.quant_counter += 1;
                    format!("{}_q{}", var_name, n)
                } else {
                    var_name.clone()
                };
                let bound_var = ast::Real::new_const(self.ctx, z3_name.as_str());
                let saved_prev = self.real_vars.insert(var_name.clone(), bound_var.clone());

                let formula_result = (|| -> Result<ast::Bool<'ctx>, TranslateError> {
                    let body_bool = self.translate_bool(body)?;
                    if let Some(guard) = where_clause {
                        let guard_bool = self.translate_bool(guard)?;
                        if is_forall {
                            Ok(guard_bool.implies(&body_bool))
                        } else {
                            Ok(ast::Bool::and(self.ctx, &[&guard_bool, &body_bool]))
                        }
                    } else {
                        Ok(body_bool)
                    }
                })();

                // Always restore previous variable binding (even on error)
                match saved_prev {
                    Some(v) => { self.real_vars.insert(var_name.clone(), v); }
                    None => { self.real_vars.remove(var_name); }
                }

                let formula = formula_result?;
                Ok(if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                })
            }
        }
    }

    /// Translate a match expression to a Z3 Bool via ITE chain.
    /// Each arm is checked in order; the pattern condition determines whether the arm fires.
    /// The last arm's condition (if wildcard) becomes the final else branch.
    fn translate_match_bool(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        if arms.is_empty() {
            return Err(TranslateError::UnsupportedExpression(
                "match with no arms".into()
            ));
        }

        // Build ITE chain from right to left: innermost (last arm) first.
        // Start with the last arm as the base case.
        let last_arm = &arms[arms.len() - 1];
        let mut result = self.translate_bool(&last_arm.body)?;

        // For each preceding arm (in reverse order), build an ITE around the result.
        for arm in arms[0..arms.len() - 1].iter().rev() {
            let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;
            let body = self.translate_bool(&arm.body)?;
            result = condition.ite(&body, &result);
        }

        Ok(result)
    }

    /// Translate a match expression to a Z3 Int via ITE chain.
    fn translate_match_int(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
    ) -> Result<ast::Int<'ctx>, TranslateError> {
        if arms.is_empty() {
            return Err(TranslateError::UnsupportedExpression(
                "match with no arms".into()
            ));
        }

        // Build ITE chain from right to left.
        let last_arm = &arms[arms.len() - 1];
        let mut result = self.translate_int(&last_arm.body)?;

        for arm in arms[0..arms.len() - 1].iter().rev() {
            let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;
            let body = self.translate_int(&arm.body)?;
            result = condition.ite(&body, &result);
        }

        Ok(result)
    }

    /// Translate a match expression to a Z3 Real via ITE chain.
    fn translate_match_real(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
    ) -> Result<ast::Real<'ctx>, TranslateError> {
        if arms.is_empty() {
            return Err(TranslateError::UnsupportedExpression(
                "match with no arms".into()
            ));
        }

        // Build ITE chain from right to left.
        let last_arm = &arms[arms.len() - 1];
        let mut result = self.translate_real(&last_arm.body)?;

        for arm in arms[0..arms.len() - 1].iter().rev() {
            let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;
            let body = self.translate_real(&arm.body)?;
            result = condition.ite(&body, &result);
        }

        Ok(result)
    }

    /// Generate a Bool condition for a pattern match against a scrutinee.
    /// - Wildcard (_) → always true
    /// - Binding (x) → always true (we bind x = scrutinee in a context where needed)
    /// - Literal (42, true, "x", nil) → (= scrutinee literal)
    /// - Variant (Ok x) → more complex; for now, raise error (no variant decomposition info)
    fn translate_pattern_condition(
        &mut self,
        scrutinee: &Expr,
        pattern: &Pattern,
    ) -> Result<ast::Bool<'ctx>, TranslateError> {
        match &pattern.kind {
            PatternKind::Wildcard => {
                // Wildcard always matches
                Ok(ast::Bool::from_bool(self.ctx, true))
            }
            PatternKind::Binding(_) => {
                // Bare binding always matches (and we'd bind the name in the body context)
                Ok(ast::Bool::from_bool(self.ctx, true))
            }
            PatternKind::Literal(lit) => {
                match lit {
                    LitPattern::Int(n) => {
                        let scrutinee_int = self.translate_int(scrutinee)?;
                        let lit_val = ast::Int::from_i64(self.ctx, *n);
                        Ok(scrutinee_int._eq(&lit_val))
                    }
                    LitPattern::Bool(b) => {
                        let scrutinee_bool = self.translate_bool(scrutinee)?;
                        let lit_val = ast::Bool::from_bool(self.ctx, *b);
                        Ok(scrutinee_bool._eq(&lit_val))
                    }
                    LitPattern::Float(f) => {
                        let scrutinee_real = self.translate_real(scrutinee)?;
                        // Translate float literal to Z3 Real.
                        let v = *f;
                        if v.is_nan() || v.is_infinite() {
                            return Err(TranslateError::UnsupportedExpression(
                                format!("float literal {} in pattern is NaN or Infinity", f)
                            ));
                        }
                        const SCALE: i64 = 1_000_000;
                        let scaled = (v * SCALE as f64).round() as i64;
                        let lit_val = if scaled >= i32::MIN as i64 && scaled <= i32::MAX as i64 {
                            ast::Real::from_real(self.ctx, scaled as i32, SCALE as i32)
                        } else {
                            let approx = v.round() as i64;
                            if approx >= i32::MIN as i64 && approx <= i32::MAX as i64 {
                                ast::Real::from_real(self.ctx, approx as i32, 1)
                            } else {
                                return Err(TranslateError::UnsupportedExpression(
                                    format!("float literal {} is too large for Z3 Real", f)
                                ));
                            }
                        };
                        Ok(scrutinee_real._eq(&lit_val))
                    }
                    LitPattern::Str(_) => {
                        Err(TranslateError::UnsupportedExpression(
                            "string literals in match patterns are not yet supported".into()
                        ))
                    }
                    LitPattern::Nil => {
                        Err(TranslateError::UnsupportedExpression(
                            "nil pattern matching is not yet supported".into()
                        ))
                    }
                }
            }
            PatternKind::Variant(_, _) => {
                Err(TranslateError::UnsupportedExpression(
                    "variant pattern matching requires type information and constructor awareness not yet integrated".into()
                ))
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
    fn translate_valid_returns_error() {
        // valid() is not encodable as Z3 — must return an error, not vacuous true.
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        let callee = Expr { kind: ExprKind::SymbolRef("valid".into()), span: airl_syntax::Span::dummy() };
        let x = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![x]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_err(), "valid() should return an error");
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

    #[test]
    fn translate_real_nan_and_inf_return_err() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        let nan = Expr { kind: ExprKind::FloatLit(f64::NAN), span: airl_syntax::Span::dummy() };
        assert!(t.translate_real(&nan).is_err(), "NaN should return Err");
        let inf = Expr { kind: ExprKind::FloatLit(f64::INFINITY), span: airl_syntax::Span::dummy() };
        assert!(t.translate_real(&inf).is_err(), "Infinity should return Err");
    }

    #[test]
    fn translate_real_large_float_no_overflow() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        // 3000.5 exceeds the i32-scaled range at 1e6 precision; falls back to integer approx
        let expr = Expr { kind: ExprKind::FloatLit(3000.5), span: airl_syntax::Span::dummy() };
        assert!(t.translate_real(&expr).is_ok(), "large float should succeed via integer fallback");
    }

    #[test]
    fn translate_cmp_mixed_int_real() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("n");
        t.declare_real("x");
        // (= n x) — one side Int, other Real; should coerce and succeed
        let callee = Expr { kind: ExprKind::SymbolRef("=".into()), span: airl_syntax::Span::dummy() };
        let n = Expr { kind: ExprKind::SymbolRef("n".into()), span: airl_syntax::Span::dummy() };
        let x = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![n, x]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_ok(), "mixed Int/Real comparison should succeed via coercion");
    }

    #[test]
    fn translate_match_int_literal() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");

        // (match x
        //   ((42) (1))
        //   ((0) (2)))
        let scrutinee = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let arm1 = MatchArm {
            pattern: Pattern { kind: PatternKind::Literal(LitPattern::Int(42)), span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::IntLit(1), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let arm2 = MatchArm {
            pattern: Pattern { kind: PatternKind::Literal(LitPattern::Int(0)), span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::IntLit(2), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let expr = Expr { kind: ExprKind::Match(Box::new(scrutinee), vec![arm1, arm2]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok(), "match on int literals should succeed");
    }

    #[test]
    fn translate_match_bool_literal() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_bool("b");

        // (match b
        //   ((true) (1))
        //   ((false) (0)))
        let scrutinee = Expr { kind: ExprKind::SymbolRef("b".into()), span: airl_syntax::Span::dummy() };
        let arm1 = MatchArm {
            pattern: Pattern { kind: PatternKind::Literal(LitPattern::Bool(true)), span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::IntLit(1), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let arm2 = MatchArm {
            pattern: Pattern { kind: PatternKind::Literal(LitPattern::Bool(false)), span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::IntLit(0), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let expr = Expr { kind: ExprKind::Match(Box::new(scrutinee), vec![arm1, arm2]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok(), "match on bool literals should succeed");
    }

    #[test]
    fn translate_match_with_wildcard() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");

        // (match x
        //   ((42) (1))
        //   ((_ ) (99)))
        let scrutinee = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let arm1 = MatchArm {
            pattern: Pattern { kind: PatternKind::Literal(LitPattern::Int(42)), span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::IntLit(1), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let arm2 = MatchArm {
            pattern: Pattern { kind: PatternKind::Wildcard, span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::IntLit(99), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let expr = Expr { kind: ExprKind::Match(Box::new(scrutinee), vec![arm1, arm2]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok(), "match with wildcard should succeed");
    }

    #[test]
    fn translate_match_empty_error() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");

        // (match x) — no arms
        let scrutinee = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::Match(Box::new(scrutinee), vec![]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_err(), "match with no arms should error");
    }
}
