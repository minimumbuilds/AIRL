use std::collections::HashMap;
use z3::ast::{self, Ast};
use z3::Context;
use z3_sys::{self, Z3_context, Z3_func_decl};
use airl_syntax::ast::{Expr, ExprKind, LetBinding, AstTypeKind, MatchArm, Pattern, PatternKind, LitPattern};

/// Extract the raw Z3_context pointer from a z3::Context.
///
/// The z3 crate 0.12 does not expose a public accessor for the underlying
/// Z3_context, but the struct has a single field (`z3_ctx: Z3_context`).
/// We read it via pointer cast — this is sound because:
///   1. Context is a non-repr(C) single-field struct (guaranteed same layout).
///   2. Z3_context is a pointer type (no padding/alignment issues).
///   3. We only read; we never modify or take ownership.
///
/// SAFETY: Only valid for z3 crate version 0.12.x where Context = { z3_ctx: Z3_context }.
/// If the crate is upgraded, this must be re-verified.
unsafe fn raw_z3_ctx(ctx: &Context) -> Z3_context {
    *(ctx as *const Context as *const Z3_context)
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarSort {
    Int,
    Bool,
    Real,
    Str,
    /// Z3 Seq(Int) — used for AIRL lists in contracts.
    /// AIRL lists are heterogeneous at runtime, but contracts that use `length`,
    /// `list-contains?`, etc. on lists typically operate on homogeneous integer lists.
    /// We model them as Seq(Int) for Z3 purposes; heterogeneous list contracts
    /// return TranslationError.
    Seq,
}

/// Translates AIRL expressions to Z3 AST nodes.
pub struct Translator<'ctx> {
    ctx: &'ctx Context,
    int_vars: HashMap<String, ast::Int<'ctx>>,
    bool_vars: HashMap<String, ast::Bool<'ctx>>,
    real_vars: HashMap<String, ast::Real<'ctx>>,
    string_vars: HashMap<String, ast::String<'ctx>>,
    /// Seq(Int) variables for AIRL list parameters in contracts.
    /// Stored as Dynamic because the z3 crate has no typed Seq wrapper.
    seq_vars: HashMap<String, ast::Dynamic<'ctx>>,
    /// Counter for generating unique quantifier-bound variable names when shadowing occurs.
    quant_counter: usize,
    /// Cache of uninterpreted function declarations (name -> Z3_func_decl).
    /// When a contract references a user-defined function that cannot be inlined,
    /// we declare it as an uninterpreted function so Z3 can reason about call
    /// relationships (e.g., f(x) = f(x) is provable by function congruence).
    func_decls: HashMap<String, Z3_func_decl>,
}

impl<'ctx> Translator<'ctx> {
    pub fn new(ctx: &'ctx Context) -> Self {
        Self {
            ctx,
            int_vars: HashMap::new(),
            bool_vars: HashMap::new(),
            real_vars: HashMap::new(),
            string_vars: HashMap::new(),
            seq_vars: HashMap::new(),
            quant_counter: 0,
            func_decls: HashMap::new(),
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

    /// Seed an integer variable with a pre-computed Z3 expression.
    /// Used by `Z3Prover::inductive_verify` to populate a call-site translator
    /// with argument values computed in the parent context.
    pub fn seed_int(&mut self, name: &str, val: ast::Int<'ctx>) {
        self.int_vars.insert(name.to_string(), val);
    }

    /// Seed a boolean variable with a pre-computed Z3 expression.
    pub fn seed_bool(&mut self, name: &str, val: ast::Bool<'ctx>) {
        self.bool_vars.insert(name.to_string(), val);
    }

    /// Seed a real variable with a pre-computed Z3 expression.
    pub fn seed_real(&mut self, name: &str, val: ast::Real<'ctx>) {
        self.real_vars.insert(name.to_string(), val);
    }

    /// Declare a string variable.
    pub fn declare_string(&mut self, name: &str) {
        let var = ast::String::new_const(self.ctx, name);
        self.string_vars.insert(name.to_string(), var);
    }

    /// Get a reference to a string variable (for counterexample extraction).
    pub fn get_string_var(&self, name: &str) -> Option<&ast::String<'ctx>> {
        self.string_vars.get(name)
    }

    /// Declare a Seq(Int) variable for an AIRL list parameter.
    /// Z3 Seq sort is parameterized; we use Seq(Int) since most list contracts
    /// deal with integer lists. Stored as Dynamic (no typed Seq in the z3 crate).
    pub fn declare_seq(&mut self, name: &str) {
        let z3_ctx = unsafe { raw_z3_ctx(self.ctx) };
        let int_sort = unsafe { z3_sys::Z3_mk_int_sort(z3_ctx) };
        let seq_sort = unsafe { z3_sys::Z3_mk_seq_sort(z3_ctx, int_sort) };
        let c_name = std::ffi::CString::new(name)
            .expect("AIRL identifiers cannot contain null bytes");
        let name_sym = unsafe {
            z3_sys::Z3_mk_string_symbol(z3_ctx, c_name.as_ptr())
        };
        let var_ast = unsafe { z3_sys::Z3_mk_const(z3_ctx, name_sym, seq_sort) };
        let var = unsafe { ast::Dynamic::wrap(self.ctx, var_ast) };
        self.seq_vars.insert(name.to_string(), var);
    }

    /// Get a reference to a seq variable (for counterexample extraction).
    pub fn get_seq_var(&self, name: &str) -> Option<&ast::Dynamic<'ctx>> {
        self.seq_vars.get(name)
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
        Vec<(String, Option<ast::String<'ctx>>)>,
        Vec<(String, Option<ast::Dynamic<'ctx>>)>,
    ), TranslateError> {
        let mut saved_ints: Vec<(String, Option<ast::Int<'ctx>>)> = Vec::new();
        let mut saved_bools: Vec<(String, Option<ast::Bool<'ctx>>)> = Vec::new();
        let mut saved_reals: Vec<(String, Option<ast::Real<'ctx>>)> = Vec::new();
        let mut saved_strings: Vec<(String, Option<ast::String<'ctx>>)> = Vec::new();
        let mut saved_seqs: Vec<(String, Option<ast::Dynamic<'ctx>>)> = Vec::new();

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
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs);
                        return Err(e);
                    }
                },
                Some(VarSort::Bool) => match self.translate_bool(&binding.value) {
                    Ok(v) => {
                        let prev = self.bool_vars.insert(binding.name.clone(), v);
                        saved_bools.push((binding.name.clone(), prev));
                    }
                    Err(e) => {
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs);
                        return Err(e);
                    }
                },
                Some(VarSort::Real) => match self.translate_real(&binding.value) {
                    Ok(v) => {
                        let prev = self.real_vars.insert(binding.name.clone(), v);
                        saved_reals.push((binding.name.clone(), prev));
                    }
                    Err(e) => {
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs);
                        return Err(e);
                    }
                },
                Some(VarSort::Str) => match self.translate_string(&binding.value) {
                    Ok(v) => {
                        let prev = self.string_vars.insert(binding.name.clone(), v);
                        saved_strings.push((binding.name.clone(), prev));
                    }
                    Err(e) => {
                        self.pop_let_bindings(saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs);
                        return Err(e);
                    }
                },
                Some(VarSort::Seq) => {
                    // Seq let bindings are not translatable (no seq expression translation yet)
                    self.pop_let_bindings(saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs);
                    return Err(TranslateError::UnsupportedExpression(
                        format!("let binding '{}': Seq values cannot be constructed in contracts", binding.name)
                    ));
                },
                None => {
                    self.pop_let_bindings(saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs);
                    return Err(TranslateError::UnsupportedExpression(
                        format!("let binding '{}': unsupported type", binding.name)
                    ));
                }
            }
        }

        Ok((saved_ints, saved_bools, saved_reals, saved_strings, saved_seqs))
    }

    /// Restore variable maps from state saved by `push_let_bindings`.
    fn pop_let_bindings(
        &mut self,
        saved_ints: Vec<(String, Option<ast::Int<'ctx>>)>,
        saved_bools: Vec<(String, Option<ast::Bool<'ctx>>)>,
        saved_reals: Vec<(String, Option<ast::Real<'ctx>>)>,
        saved_strings: Vec<(String, Option<ast::String<'ctx>>)>,
        saved_seqs: Vec<(String, Option<ast::Dynamic<'ctx>>)>,
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
        for (name, prev) in saved_strings.into_iter().rev() {
            match prev {
                Some(v) => { self.string_vars.insert(name, v); }
                None => { self.string_vars.remove(&name); }
            }
        }
        for (name, prev) in saved_seqs.into_iter().rev() {
            match prev {
                Some(v) => { self.seq_vars.insert(name, v); }
                None => { self.seq_vars.remove(&name); }
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
                        // String predicates
                        "string-contains?" => {
                            let s = self.translate_string(&args[0])?;
                            let sub = self.translate_string(&args[1])?;
                            Ok(s.contains(&sub))
                        }
                        "string-prefix?" => {
                            let prefix = self.translate_string(&args[0])?;
                            let s = self.translate_string(&args[1])?;
                            Ok(prefix.prefix(&s))
                        }
                        "string-suffix?" => {
                            let suffix = self.translate_string(&args[0])?;
                            let s = self.translate_string(&args[1])?;
                            Ok(suffix.suffix(&s))
                        }
                        "list-contains?" => {
                            // (list-contains? xs x) — xs is Seq(Int), x is Int
                            // Z3_mk_seq_contains expects both args to be sequences,
                            // so we wrap the element as a unit sequence.
                            if let ExprKind::SymbolRef(list_name) = &args[0].kind {
                                if let Some(seq_var) = self.seq_vars.get(list_name).cloned() {
                                    let elem = self.translate_int(&args[1])?;
                                    let z3_ctx = unsafe { raw_z3_ctx(self.ctx) };
                                    let unit_seq = unsafe {
                                        z3_sys::Z3_mk_seq_unit(z3_ctx, elem.get_z3_ast())
                                    };
                                    let contains_ast = unsafe {
                                        z3_sys::Z3_mk_seq_contains(z3_ctx, seq_var.get_z3_ast(), unit_seq)
                                    };
                                    return Ok(unsafe { ast::Bool::wrap(self.ctx, contains_ast) });
                                }
                            }
                            Err(TranslateError::UnsupportedExpression(
                                "list-contains? requires a declared list/seq variable".into()
                            ))
                        }
                        "valid" => {
                            // valid() cannot be encoded as a Z3 predicate — ownership
                            // semantics require a separate checker, not SMT encoding.
                            return Err(TranslateError::UnsupportedExpression(
                                "valid() predicate is not yet encoded — contracts using valid() are unverified".into()
                            ));
                        }
                        _ => {
                            // Unknown function — declare as uninterpreted with Bool return
                            let app_ast = self.apply_uninterpreted_fn(op, args, VarSort::Bool)?;
                            Ok(unsafe { ast::Bool::wrap(self.ctx, app_ast) })
                        }
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
                self.pop_let_bindings(saved.0, saved.1, saved.2, saved.3, saved.4);
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
                        "string-length" => {
                            let s = self.translate_string(&args[0])?;
                            // z3 crate 0.12 does not expose a len() method on ast::String,
                            // so we call Z3_mk_seq_length via z3-sys directly.
                            let z3_ctx = unsafe { raw_z3_ctx(self.ctx) };
                            let len_ast = unsafe {
                                z3_sys::Z3_mk_seq_length(z3_ctx, s.get_z3_ast())
                            };
                            Ok(unsafe { ast::Int::wrap(self.ctx, len_ast) })
                        }
                        "length" => {
                            // (length xs) on a Seq(Int) variable → Z3_mk_seq_length
                            if let ExprKind::SymbolRef(name) = &args[0].kind {
                                if let Some(seq_var) = self.seq_vars.get(name) {
                                    let z3_ctx = unsafe { raw_z3_ctx(self.ctx) };
                                    let len_ast = unsafe {
                                        z3_sys::Z3_mk_seq_length(z3_ctx, seq_var.get_z3_ast())
                                    };
                                    return Ok(unsafe { ast::Int::wrap(self.ctx, len_ast) });
                                }
                            }
                            Err(TranslateError::UnsupportedExpression(
                                "length requires a declared list/seq variable".into()
                            ))
                        }
                        // ── Bitwise operations via Z3 BV64 ──
                        // Bit-precise semantics: translate args to 64-bit
                        // bitvectors, apply the bitwise op in BV theory, convert
                        // result back to Int for the surrounding arithmetic
                        // context. Supersedes the a03e980 fresh-var axiom
                        // approach (which lives at HEAD~ if needed for
                        // archaeology). See
                        // artifacts/spec-airl-z3-bv64-bitwise.md.
                        "bitwise-and" | "bitwise-or" | "bitwise-xor"
                        | "bitwise-shl" | "bitwise-shr" => {
                            if args.len() != 2 {
                                return Err(TranslateError::UnsupportedExpression(
                                    format!("{} requires exactly 2 arguments", op)));
                            }
                            let a = self.translate_int(&args[0])?;
                            let b = self.translate_int(&args[1])?;
                            let bv_a = ast::BV::from_int(&a, 64);
                            let bv_b = ast::BV::from_int(&b, 64);
                            let bv_res = match op.as_str() {
                                "bitwise-and" => bv_a.bvand(&bv_b),
                                "bitwise-or"  => bv_a.bvor(&bv_b),
                                "bitwise-xor" => bv_a.bvxor(&bv_b),
                                "bitwise-shl" => bv_a.bvshl(&bv_b),
                                // Logical (unsigned) shift right. AIRL's
                                // `bitwise-shr` matches C's `>>` on unsigned,
                                // which is zero-fill. If we later want signed
                                // arithmetic shift, add a separate `bitwise-sar`
                                // builtin and translate via bvashr.
                                "bitwise-shr" => bv_a.bvlshr(&bv_b),
                                _ => unreachable!(),
                            };
                            // Signed interpretation: AIRL integers are i64.
                            Ok(ast::Int::from_bv(&bv_res, true))
                        }
                        "bitwise-not" => {
                            if args.len() != 1 {
                                return Err(TranslateError::UnsupportedExpression(
                                    "bitwise-not requires exactly 1 argument".into()));
                            }
                            let a = self.translate_int(&args[0])?;
                            let bv_a = ast::BV::from_int(&a, 64);
                            let bv_res = bv_a.bvnot();
                            Ok(ast::Int::from_bv(&bv_res, true))
                        }
                        _ => {
                            // Unknown function — declare as uninterpreted with Int return
                            let app_ast = self.apply_uninterpreted_fn(op, args, VarSort::Int)?;
                            Ok(unsafe { ast::Int::wrap(self.ctx, app_ast) })
                        }
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
                self.pop_let_bindings(saved.0, saved.1, saved.2, saved.3, saved.4);
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
                self.pop_let_bindings(saved.0, saved.1, saved.2, saved.3, saved.4);
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

    /// Translate an AIRL expression to a Z3 String.
    pub fn translate_string(&mut self, expr: &Expr) -> Result<ast::String<'ctx>, TranslateError> {
        match &expr.kind {
            ExprKind::StrLit(s) => {
                ast::String::from_str(self.ctx, s).map_err(|e| {
                    TranslateError::UnsupportedExpression(
                        format!("string literal contains null byte: {}", e)
                    )
                })
            }

            ExprKind::SymbolRef(name) => {
                if let Some(var) = self.string_vars.get(name) {
                    Ok(var.clone())
                } else {
                    Err(TranslateError::UndefinedVariable(name.clone()))
                }
            }

            ExprKind::FnCall(callee, args) => {
                if let ExprKind::SymbolRef(op) = &callee.kind {
                    match op.as_str() {
                        "string-concat" | "str-concat" => {
                            let operands: Result<Vec<_>, _> = args.iter()
                                .map(|a| self.translate_string(a))
                                .collect();
                            let operands = operands?;
                            let refs: Vec<&ast::String> = operands.iter().collect();
                            Ok(ast::String::concat(self.ctx, &refs))
                        }
                        _ => Err(TranslateError::UnsupportedExpression(
                            format!("string context: {}", op)
                        )),
                    }
                } else {
                    Err(TranslateError::UnsupportedExpression("non-symbol callee".into()))
                }
            }

            ExprKind::If(cond, then_e, else_e) => {
                let c = self.translate_bool(cond)?;
                let t = self.translate_string(then_e)?;
                let e = self.translate_string(else_e)?;
                Ok(c.ite(&t, &e))
            }

            ExprKind::Let(bindings, body) => {
                let saved = self.push_let_bindings(bindings)?;
                let result = self.translate_string(body);
                self.pop_let_bindings(saved.0, saved.1, saved.2, saved.3, saved.4);
                result
            }

            ExprKind::Do(exprs) => exprs.last()
                .ok_or_else(|| TranslateError::UnsupportedExpression("empty do block".into()))
                .and_then(|e| self.translate_string(e)),

            _ => Err(TranslateError::UnsupportedExpression(
                format!("string context: {:?}", expr.kind)
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
        // Both String
        if let (Ok(l), Ok(r)) = (self.translate_string(lhs), self.translate_string(rhs)) {
            let eq = l._eq(&r);
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
            VarSort::Str => {
                let z3_name = if self.string_vars.contains_key(var_name) {
                    let n = self.quant_counter;
                    self.quant_counter += 1;
                    format!("{}_q{}", var_name, n)
                } else {
                    var_name.clone()
                };
                let bound_var = ast::String::new_const(self.ctx, z3_name.as_str());
                let saved_prev = self.string_vars.insert(var_name.clone(), bound_var.clone());

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

                match saved_prev {
                    Some(v) => { self.string_vars.insert(var_name.clone(), v); }
                    None => { self.string_vars.remove(var_name); }
                }

                let formula = formula_result?;
                Ok(if is_forall {
                    ast::forall_const(self.ctx, &[&bound_var], &[], &formula)
                } else {
                    ast::exists_const(self.ctx, &[&bound_var], &[], &formula)
                })
            }
            VarSort::Seq => {
                Err(TranslateError::UnsupportedExpression(
                    "quantifiers over List/Seq variables are not supported".into()
                ))
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

        // Handle binding pattern in last arm: bind the variable before translating body
        let saved_last = if let PatternKind::Binding(name) = &last_arm.pattern.kind {
            let scrutinee_val = self.translate_bool(scrutinee)?;
            let prev = self.bool_vars.insert(name.clone(), scrutinee_val);
            Some((name.clone(), prev))
        } else {
            None
        };

        let mut result = self.translate_bool(&last_arm.body)?;

        // Restore the previous binding after translating last arm
        if let Some((name, prev)) = saved_last {
            match prev {
                Some(v) => { self.bool_vars.insert(name, v); }
                None => { self.bool_vars.remove(&name); }
            }
        }

        // For each preceding arm (in reverse order), build an ITE around the result.
        for arm in arms[0..arms.len() - 1].iter().rev() {
            let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;

            // Handle binding pattern: bind the variable before translating body
            let saved = if let PatternKind::Binding(name) = &arm.pattern.kind {
                let scrutinee_val = self.translate_bool(scrutinee)?;
                let prev = self.bool_vars.insert(name.clone(), scrutinee_val);
                Some((name.clone(), prev))
            } else {
                None
            };

            let body = self.translate_bool(&arm.body)?;

            // Restore the previous binding after translating body
            if let Some((name, prev)) = saved {
                match prev {
                    Some(v) => { self.bool_vars.insert(name, v); }
                    None => { self.bool_vars.remove(&name); }
                }
            }

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

        // Handle binding pattern in last arm: bind the variable before translating body
        let saved_last = if let PatternKind::Binding(name) = &last_arm.pattern.kind {
            let scrutinee_val = self.translate_int(scrutinee)?;
            let prev = self.int_vars.insert(name.clone(), scrutinee_val);
            Some((name.clone(), prev))
        } else {
            None
        };

        let mut result = self.translate_int(&last_arm.body)?;

        // Restore the previous binding after translating last arm
        if let Some((name, prev)) = saved_last {
            match prev {
                Some(v) => { self.int_vars.insert(name, v); }
                None => { self.int_vars.remove(&name); }
            }
        }

        for arm in arms[0..arms.len() - 1].iter().rev() {
            let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;

            // Handle binding pattern: bind the variable before translating body
            let saved = if let PatternKind::Binding(name) = &arm.pattern.kind {
                let scrutinee_val = self.translate_int(scrutinee)?;
                let prev = self.int_vars.insert(name.clone(), scrutinee_val);
                Some((name.clone(), prev))
            } else {
                None
            };

            let body = self.translate_int(&arm.body)?;

            // Restore the previous binding after translating body
            if let Some((name, prev)) = saved {
                match prev {
                    Some(v) => { self.int_vars.insert(name, v); }
                    None => { self.int_vars.remove(&name); }
                }
            }

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

        // Handle binding pattern in last arm: bind the variable before translating body
        let saved_last = if let PatternKind::Binding(name) = &last_arm.pattern.kind {
            let scrutinee_val = self.translate_real(scrutinee)?;
            let prev = self.real_vars.insert(name.clone(), scrutinee_val);
            Some((name.clone(), prev))
        } else {
            None
        };

        let mut result = self.translate_real(&last_arm.body)?;

        // Restore the previous binding after translating last arm
        if let Some((name, prev)) = saved_last {
            match prev {
                Some(v) => { self.real_vars.insert(name, v); }
                None => { self.real_vars.remove(&name); }
            }
        }

        for arm in arms[0..arms.len() - 1].iter().rev() {
            let condition = self.translate_pattern_condition(scrutinee, &arm.pattern)?;

            // Handle binding pattern: bind the variable before translating body
            let saved = if let PatternKind::Binding(name) = &arm.pattern.kind {
                let scrutinee_val = self.translate_real(scrutinee)?;
                let prev = self.real_vars.insert(name.clone(), scrutinee_val);
                Some((name.clone(), prev))
            } else {
                None
            };

            let body = self.translate_real(&arm.body)?;

            // Restore the previous binding after translating body
            if let Some((name, prev)) = saved {
                match prev {
                    Some(v) => { self.real_vars.insert(name, v); }
                    None => { self.real_vars.remove(&name); }
                }
            }

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

    /// Declare (or retrieve from cache) an uninterpreted function and apply it to arguments.
    ///
    /// Uninterpreted functions let Z3 reason about function call relationships without
    /// knowing the implementation. Key property: Z3 knows uninterpreted functions are
    /// deterministic, so `f(x) = f(x)` is provable, while `f(x) = f(y)` requires x = y.
    ///
    /// All parameters default to Int sort and the return sort is specified by the caller.
    /// The function declaration is cached so repeated calls to the same function reuse
    /// the same Z3 FuncDecl.
    fn apply_uninterpreted_fn(
        &mut self,
        name: &str,
        args: &[Expr],
        return_sort_kind: VarSort,
    ) -> Result<z3_sys::Z3_ast, TranslateError> {
        let z3_ctx = unsafe { raw_z3_ctx(self.ctx) };
        let arity = args.len();

        // Build a cache key that includes name + arity + return sort to avoid
        // collisions between overloads (same name, different arity/return).
        let cache_key = format!("{}/${}/{:?}", name, arity, return_sort_kind);

        let func_decl = if let Some(&cached) = self.func_decls.get(&cache_key) {
            cached
        } else {
            // Build domain sorts — default all params to Int
            let int_sort = unsafe { z3_sys::Z3_mk_int_sort(z3_ctx) };
            let domain: Vec<z3_sys::Z3_sort> = (0..arity).map(|_| int_sort).collect();

            let range = match return_sort_kind {
                VarSort::Int => unsafe { z3_sys::Z3_mk_int_sort(z3_ctx) },
                VarSort::Bool => unsafe { z3_sys::Z3_mk_bool_sort(z3_ctx) },
                VarSort::Real => unsafe { z3_sys::Z3_mk_real_sort(z3_ctx) },
                VarSort::Str => unsafe { z3_sys::Z3_mk_string_sort(z3_ctx) },
                VarSort::Seq => {
                    return Err(TranslateError::UnsupportedExpression(
                        format!("uninterpreted function '{}': Seq return sort not supported", name),
                    ));
                }
            };

            let c_name = std::ffi::CString::new(name)
                .expect("AIRL identifiers cannot contain null bytes");
            let sym = unsafe { z3_sys::Z3_mk_string_symbol(z3_ctx, c_name.as_ptr()) };
            let decl = unsafe {
                z3_sys::Z3_mk_func_decl(
                    z3_ctx,
                    sym,
                    arity as u32,
                    if domain.is_empty() { std::ptr::null() } else { domain.as_ptr() },
                    range,
                )
            };
            // Increment the Z3 reference count so the declaration is not freed
            // by Z3's garbage collector while this Translator (or a sibling
            // Translator sharing the same context) is still alive.
            // SAFETY: decl is a valid Z3_func_decl just returned by Z3_mk_func_decl.
            unsafe { z3_sys::Z3_inc_ref(z3_ctx, z3_sys::Z3_func_decl_to_ast(z3_ctx, decl)) };
            self.func_decls.insert(cache_key, decl);
            decl
        };

        // Translate arguments — all assumed to be Int
        let mut z3_args: Vec<z3_sys::Z3_ast> = Vec::with_capacity(arity);
        for arg in args {
            let z3_int = self.translate_int(arg)?;
            z3_args.push(z3_int.get_z3_ast());
        }

        let app_ast = unsafe {
            z3_sys::Z3_mk_app(
                z3_ctx,
                func_decl,
                arity as u32,
                if z3_args.is_empty() { std::ptr::null() } else { z3_args.as_ptr() },
            )
        };

        Ok(app_ast)
    }


    /// Build seed values for inductive hypothesis injection.
    ///
    /// For each recursive call site f(a0, a1, ...) in a function body:
    ///   - Translate each argument expression to a typed Z3 value.
    ///   - Build the uninterpreted function application f_interp(a0, a1, ...) using the
    ///     same Z3 func_decl that body translation will use (sharing the cache ensures
    ///     symbol identity — Z3 can relate the IH axiom to the body sub-term).
    ///   - Return (Vec<SeedVal> for params, SeedVal for result) so the caller can seed
    ///     a call-site Translator and translate the :ensures clause.
    ///
    /// Returns None if any argument cannot be translated or the return type is unsupported.
    pub fn apply_recursive_fn_as_result(
        &mut self,
        fn_name: &str,
        call_args: &[Expr],
        return_type: &airl_syntax::ast::AstType,
        params: &[airl_syntax::ast::Param],
    ) -> Option<(Vec<SeedVal<'ctx>>, SeedVal<'ctx>)> {
        // Translate each argument expression.
        let mut arg_seeds: Vec<SeedVal<'ctx>> = Vec::with_capacity(params.len());
        for (param, arg_expr) in params.iter().zip(call_args.iter()) {
            if let AstTypeKind::Named(type_name) = &param.ty.kind {
                match Self::sort_from_type_name(type_name) {
                    Some(VarSort::Int) => match self.translate_int(arg_expr) {
                        Ok(v) => arg_seeds.push(SeedVal::Int(v)),
                        Err(_) => return None,
                    },
                    Some(VarSort::Bool) => match self.translate_bool(arg_expr) {
                        Ok(v) => arg_seeds.push(SeedVal::Bool(v)),
                        Err(_) => return None,
                    },
                    Some(VarSort::Real) => match self.translate_real(arg_expr) {
                        Ok(v) => arg_seeds.push(SeedVal::Real(v)),
                        Err(_) => return None,
                    },
                    _ => return None,
                }
            } else {
                return None;
            }
        }

        // Determine return sort and build the uninterpreted function application.
        let return_sort = if let AstTypeKind::Named(type_name) = &return_type.kind {
            Self::sort_from_type_name(type_name)?
        } else {
            return None;
        };

        // Build the app AST using the SAME uninterpreted func_decl as body translation.
        let app_ast = self.apply_uninterpreted_fn(fn_name, call_args, return_sort).ok()?;

        // Wrap the raw Z3_ast as the appropriate typed AST.
        let result_seed = match return_sort {
            VarSort::Int  => SeedVal::Int(unsafe { ast::Int::wrap(self.ctx, app_ast) }),
            VarSort::Bool => SeedVal::Bool(unsafe { ast::Bool::wrap(self.ctx, app_ast) }),
            VarSort::Real => SeedVal::Real(unsafe { ast::Real::wrap(self.ctx, app_ast) }),
            _ => return None,
        };

        Some((arg_seeds, result_seed))
    }

    /// Determine variable sort from an AIRL type name.
    pub fn sort_from_type_name(name: &str) -> Option<VarSort> {
        match name {
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "Nat" => Some(VarSort::Int),
            "bool" => Some(VarSort::Bool),
            "f16" | "f32" | "f64" | "bf16" => Some(VarSort::Real),
            "String" | "str" => Some(VarSort::Str),
            "List" => Some(VarSort::Seq),
            _ => None,
        }
    }
}

impl<'ctx> Drop for Translator<'ctx> {
    /// Decrement Z3 reference counts for all cached uninterpreted function declarations.
    ///
    /// `apply_uninterpreted_fn` calls `Z3_inc_ref` on each new declaration to prevent
    /// Z3's garbage collector from freeing the pointer while the Translator is alive.
    /// This Drop impl releases those references so Z3 can reclaim the memory.
    fn drop(&mut self) {
        if self.func_decls.is_empty() {
            return;
        }
        let z3_ctx = unsafe { raw_z3_ctx(self.ctx) };
        for &decl in self.func_decls.values() {
            unsafe {
                z3_sys::Z3_dec_ref(z3_ctx, z3_sys::Z3_func_decl_to_ast(z3_ctx, decl));
            }
        }
    }
}

// ── Inductive verification support ───────────────────────────────────────────

/// A typed Z3 expression used to seed a call-site Translator for inductive
/// hypothesis injection.  Created by `Translator::apply_recursive_fn_as_result`.
pub enum SeedVal<'ctx> {
    Int(ast::Int<'ctx>),
    Bool(ast::Bool<'ctx>),
    Real(ast::Real<'ctx>),
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
        assert!(matches!(Translator::sort_from_type_name("String"), Some(VarSort::Str)));
        assert!(matches!(Translator::sort_from_type_name("str"), Some(VarSort::Str)));
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

    #[test]
    fn translate_match_binding_uses_scrutinee() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");

        // (match x (y (+ y 1)))
        let scrutinee = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let arm = MatchArm {
            pattern: Pattern { kind: PatternKind::Binding("y".into()), span: airl_syntax::Span::dummy() },
            body: Expr {
                kind: ExprKind::FnCall(
                    Box::new(Expr { kind: ExprKind::SymbolRef("+".into()), span: airl_syntax::Span::dummy() }),
                    vec![
                        Expr { kind: ExprKind::SymbolRef("y".into()), span: airl_syntax::Span::dummy() },
                        Expr { kind: ExprKind::IntLit(1), span: airl_syntax::Span::dummy() },
                    ],
                ),
                span: airl_syntax::Span::dummy(),
            },
            span: airl_syntax::Span::dummy(),
        };
        let expr = Expr { kind: ExprKind::Match(Box::new(scrutinee), vec![arm]), span: airl_syntax::Span::dummy() };
        let result = t.translate_int(&expr);
        assert!(result.is_ok(), "match with binding pattern should succeed: {:?}", result.err());
    }

    #[test]
    fn translate_match_binding_shadow_restores() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        t.declare_int("y"); // outer y

        // (match x (y y))
        let scrutinee = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let arm = MatchArm {
            pattern: Pattern { kind: PatternKind::Binding("y".into()), span: airl_syntax::Span::dummy() },
            body: Expr { kind: ExprKind::SymbolRef("y".into()), span: airl_syntax::Span::dummy() },
            span: airl_syntax::Span::dummy(),
        };
        let expr = Expr { kind: ExprKind::Match(Box::new(scrutinee), vec![arm]), span: airl_syntax::Span::dummy() };
        let result = t.translate_int(&expr);
        assert!(result.is_ok(), "match binding shadow should work");

        // After the match, y should be restored to the original declared value
        assert!(t.get_int_var("y").is_some(), "outer y should still exist after match");
    }

    #[test]
    fn sort_from_type_list() {
        assert!(matches!(Translator::sort_from_type_name("List"), Some(VarSort::Seq)));
    }

    #[test]
    fn declare_seq_variable() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_seq("xs");
        assert!(t.get_seq_var("xs").is_some());
    }

    #[test]
    fn translate_length_of_seq() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_seq("xs");
        // (length xs)
        let callee = Expr { kind: ExprKind::SymbolRef("length".into()), span: airl_syntax::Span::dummy() };
        let xs = Expr { kind: ExprKind::SymbolRef("xs".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![xs]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok(), "length on seq variable should translate to Z3 Int");
    }

    #[test]
    fn translate_length_of_non_seq_fails() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("n");
        // (length n) — n is not a seq variable
        let callee = Expr { kind: ExprKind::SymbolRef("length".into()), span: airl_syntax::Span::dummy() };
        let n = Expr { kind: ExprKind::SymbolRef("n".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![n]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_err(), "length on non-seq should fail");
    }

    #[test]
    fn translate_list_contains() {
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_seq("xs");
        t.declare_int("x");
        // (list-contains? xs x)
        let callee = Expr { kind: ExprKind::SymbolRef("list-contains?".into()), span: airl_syntax::Span::dummy() };
        let xs = Expr { kind: ExprKind::SymbolRef("xs".into()), span: airl_syntax::Span::dummy() };
        let x = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![xs, x]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_ok(), "list-contains? should translate to Z3 Bool");
    }

    // --- Uninterpreted function tests ---

    #[test]
    fn uninterpreted_fn_call_translates_in_int_context() {
        // (f x) where f is unknown — should succeed as uninterpreted function
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        let callee = Expr { kind: ExprKind::SymbolRef("f".into()), span: airl_syntax::Span::dummy() };
        let x = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![x]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_int(&expr).is_ok(), "uninterpreted fn call should translate in int context");
    }

    #[test]
    fn uninterpreted_fn_call_translates_in_bool_context() {
        // (p x) where p is unknown — should succeed as uninterpreted predicate
        let ctx = make_ctx();
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        let callee = Expr { kind: ExprKind::SymbolRef("p".into()), span: airl_syntax::Span::dummy() };
        let x = Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() };
        let expr = Expr { kind: ExprKind::FnCall(Box::new(callee), vec![x]), span: airl_syntax::Span::dummy() };
        assert!(t.translate_bool(&expr).is_ok(), "uninterpreted fn call should translate in bool context");
    }

    #[test]
    fn uninterpreted_fn_determinism_provable() {
        // (= (f x) (f x)) should be provable — uninterpreted functions are deterministic
        use z3::{Solver, SatResult};
        let ctx = make_ctx();
        let solver = Solver::new(&ctx);
        let mut t = Translator::new(&ctx);
        t.declare_int("x");

        // Build (= (f x) (f x))
        let fx1 = Expr {
            kind: ExprKind::FnCall(
                Box::new(Expr { kind: ExprKind::SymbolRef("f".into()), span: airl_syntax::Span::dummy() }),
                vec![Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() }],
            ),
            span: airl_syntax::Span::dummy(),
        };
        let fx2 = Expr {
            kind: ExprKind::FnCall(
                Box::new(Expr { kind: ExprKind::SymbolRef("f".into()), span: airl_syntax::Span::dummy() }),
                vec![Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() }],
            ),
            span: airl_syntax::Span::dummy(),
        };
        let eq_expr = Expr {
            kind: ExprKind::FnCall(
                Box::new(Expr { kind: ExprKind::SymbolRef("=".into()), span: airl_syntax::Span::dummy() }),
                vec![fx1, fx2],
            ),
            span: airl_syntax::Span::dummy(),
        };

        let z3_bool = t.translate_bool(&eq_expr).expect("should translate");
        // Negate and check: if UNSAT, the original is proven
        solver.assert(&z3_bool.not());
        assert_eq!(solver.check(), SatResult::Unsat, "(= (f x) (f x)) should be provable");
    }

    #[test]
    fn uninterpreted_fn_different_args_not_provable() {
        // (= (f x) (f y)) should NOT be provable — different args may yield different results
        use z3::{Solver, SatResult};
        let ctx = make_ctx();
        let solver = Solver::new(&ctx);
        let mut t = Translator::new(&ctx);
        t.declare_int("x");
        t.declare_int("y");

        // Build (= (f x) (f y))
        let fx = Expr {
            kind: ExprKind::FnCall(
                Box::new(Expr { kind: ExprKind::SymbolRef("f".into()), span: airl_syntax::Span::dummy() }),
                vec![Expr { kind: ExprKind::SymbolRef("x".into()), span: airl_syntax::Span::dummy() }],
            ),
            span: airl_syntax::Span::dummy(),
        };
        let fy = Expr {
            kind: ExprKind::FnCall(
                Box::new(Expr { kind: ExprKind::SymbolRef("f".into()), span: airl_syntax::Span::dummy() }),
                vec![Expr { kind: ExprKind::SymbolRef("y".into()), span: airl_syntax::Span::dummy() }],
            ),
            span: airl_syntax::Span::dummy(),
        };
        let eq_expr = Expr {
            kind: ExprKind::FnCall(
                Box::new(Expr { kind: ExprKind::SymbolRef("=".into()), span: airl_syntax::Span::dummy() }),
                vec![fx, fy],
            ),
            span: airl_syntax::Span::dummy(),
        };

        let z3_bool = t.translate_bool(&eq_expr).expect("should translate");
        // Negate and check: if SAT, the original is NOT provable (counterexample exists)
        solver.assert(&z3_bool.not());
        assert_eq!(solver.check(), SatResult::Sat, "(= (f x) (f y)) should NOT be provable");
    }
}
