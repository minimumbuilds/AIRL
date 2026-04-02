use airl_syntax::{ast, Span, Diagnostic, Diagnostics};
use crate::ty::*;
use crate::env::TypeEnv;
use crate::unify::{DimSubst, unify_dim};

/// Type checker for the AIRL language.
///
/// Resolves AST types to internal `Ty`, checks expressions, functions,
/// and top-level forms. Supports dependent dimension unification for
/// tensor types.
pub struct TypeChecker {
    pub env: TypeEnv,
    pub dim_subst: DimSubst,
    diags: Diagnostics,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self {
            env: TypeEnv::new(),
            dim_subst: DimSubst::new(),
            diags: Diagnostics::new(),
        };
        tc.register_builtins();
        tc
    }

    /// Register built-in arithmetic and comparison operators.
    fn register_builtins(&mut self) {
        // Arithmetic: (+ a b), (- a b), (* a b), (/ a b)
        for op in &["+", "-", "*", "/", "%"] {
            self.env.bind(
                op.to_string(),
                Ty::Func {
                    params: vec![Ty::Prim(PrimTy::I64), Ty::Prim(PrimTy::I64)],
                    ret: Box::new(Ty::Prim(PrimTy::I64)),
                },
            );
        }
        // Comparison: (< a b), (> a b), (<= a b), (>= a b), (== a b), (!= a b)
        for op in &["<", ">", "<=", ">=", "==", "!="] {
            self.env.bind(
                op.to_string(),
                Ty::Func {
                    params: vec![Ty::Prim(PrimTy::I64), Ty::Prim(PrimTy::I64)],
                    ret: Box::new(Ty::Prim(PrimTy::Bool)),
                },
            );
        }
        // Boolean ops
        for op in &["and", "or"] {
            self.env.bind(
                op.to_string(),
                Ty::Func {
                    params: vec![Ty::Prim(PrimTy::Bool), Ty::Prim(PrimTy::Bool)],
                    ret: Box::new(Ty::Prim(PrimTy::Bool)),
                },
            );
        }
        self.env.bind(
            "not".to_string(),
            Ty::Func {
                params: vec![Ty::Prim(PrimTy::Bool)],
                ret: Box::new(Ty::Prim(PrimTy::Bool)),
            },
        );

        // Collection builtins — registered as TypeVar for polymorphic dispatch
        for name in &[
            "length", "at", "append", "head", "tail", "empty?", "cons",
            "print", "type-of", "shape", "valid",
            "tensor.zeros", "tensor.ones", "tensor.rand", "tensor.identity",
            "tensor.add", "tensor.mul", "tensor.matmul", "tensor.reshape",
            "tensor.transpose", "tensor.softmax", "tensor.sum", "tensor.max",
            "tensor.slice",
            "spawn-agent", "send",
            "char-at", "substring", "split", "join", "contains",
            "starts-with", "ends-with", "trim", "to-upper", "to-lower",
            "replace", "index-of", "chars",
            "map-new", "map-from", "map-get", "map-get-or", "map-set",
            "map-has", "map-remove", "map-keys", "map-values", "map-size",
            // Agent builtins
            "await", "parallel", "broadcast", "retry", "escalate", "any-agent",
            "send-async",
            // Stdlib: collections (prelude.airl)
            "map", "filter", "fold", "reverse", "concat", "zip", "flatten",
            "range", "take", "drop", "any", "all", "find", "sort", "merge",
            // Stdlib: math (math.airl)
            "abs", "min", "max", "clamp", "sign", "even?", "odd?",
            "pow", "gcd", "lcm", "sum-list", "product-list",
            // Stdlib: result (result.airl)
            "is-ok?", "is-err?", "unwrap-or", "map-ok", "map-err",
            "and-then", "or-else", "ok-or",
            // Stdlib: string (string.airl)
            "words", "unwords", "lines", "unlines", "repeat-str",
            "pad-left", "pad-right", "is-empty-str", "reverse-str",
            "count-occurrences",
            // Stdlib: map (map.airl)
            "map-entries", "map-from-entries", "map-merge", "map-map-values",
            "map-filter", "map-update", "map-update-or", "map-count",
            // File I/O
            "read-file", "write-file", "file-exists?", "get-args",
            "append-file", "delete-file", "delete-dir", "rename-file",
            "read-dir", "create-dir", "file-size", "is-dir?",
            "at-or", "set-at", "list-contains?",
            // System builtins
            "str", "int-to-string", "float-to-string", "string-to-int", "string-to-float",
            "char-code", "char-from-code",
            // Float math
            "sqrt", "sin", "cos", "tan", "log", "exp",
            "floor", "ceil", "round", "float-to-int", "int-to-float",
            "infinity", "nan", "is-nan?", "is-infinite?",
            "panic", "assert",
            "json-parse", "json-stringify",
            "shell-exec", "cpu-count", "time-now", "sleep", "format-time", "getenv",
            "run-bytecode", "compile-to-executable", "compile-bytecode-to-executable",
            "compile-bytecode-to-executable-with-target",
            // Byte encoding
            "bytes-new", "bytes-from-int8", "bytes-from-int16", "bytes-from-int32", "bytes-from-int64",
            "bytes-to-int16", "bytes-to-int32", "bytes-to-int64",
            "bytes-from-string", "bytes-to-string", "bytes-concat", "bytes-concat-all", "bytes-slice", "crc32c",
            // TCP sockets
            "tcp-connect", "tcp-close", "tcp-send", "tcp-recv", "tcp-recv-exact", "tcp-set-timeout",
            "tcp-listen", "tcp-accept", "tcp-accept-tls",
            // Threading and channels
            "thread-spawn", "thread-join", "thread-set-affinity",
            "channel-new", "channel-send", "channel-recv", "channel-recv-timeout", "channel-drain", "channel-close",
        ] {
            self.env.bind(name.to_string(), Ty::TypeVar("builtin".to_string()));
        }
    }

    // ── Type resolution ──────────────────────────────────

    /// Resolve an AST type name to an internal Ty.
    pub fn resolve_type_name(&self, name: &str) -> Result<Ty, ()> {
        if let Some(prim) = PrimTy::from_name(name) {
            return Ok(Ty::Prim(prim));
        }
        match name {
            "Unit" => Ok(Ty::Unit),
            "Never" => Ok(Ty::Never),
            _ => {
                if let Some(reg) = self.env.lookup_type(name) {
                    Ok(reg.ty.clone())
                } else {
                    Err(())
                }
            }
        }
    }

    /// Resolve a full AST type node to internal Ty.
    pub fn resolve_type(&mut self, ast_ty: &ast::AstType) -> Result<Ty, ()> {
        match &ast_ty.kind {
            ast::AstTypeKind::Named(name) => {
                // "_" is an inferred type placeholder — we return a special marker
                if name == "_" {
                    return Ok(Ty::TypeVar("_".to_string()));
                }
                self.resolve_type_name(name)
            }
            ast::AstTypeKind::App(name, args) => {
                if name == "tensor" {
                    // tensor[ElemType Dim1 Dim2 ...]
                    if args.is_empty() {
                        return Err(());
                    }
                    let elem = self.resolve_type(&args[0])?;
                    let shape = args[1..]
                        .iter()
                        .map(|a| self.resolve_dim(a))
                        .collect::<Result<_, _>>()?;
                    Ok(Ty::Tensor {
                        elem: Box::new(elem),
                        shape,
                    })
                } else {
                    // Named type application: Result[i32, DivError]
                    let resolved_args = args
                        .iter()
                        .map(|a| self.resolve_type(a).map(TyArg::Type))
                        .collect::<Result<_, _>>()?;
                    Ok(Ty::Named {
                        name: name.clone(),
                        args: resolved_args,
                    })
                }
            }
            ast::AstTypeKind::Func(params, ret) => {
                let param_tys = params
                    .iter()
                    .map(|p| self.resolve_type(p))
                    .collect::<Result<_, _>>()?;
                let ret_ty = self.resolve_type(ret)?;
                Ok(Ty::Func {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                })
            }
            ast::AstTypeKind::Nat(nat) => Ok(Ty::Nat(Self::ast_nat_to_dim(nat))),
        }
    }

    /// Resolve an AST type node used in a dimension position to a DimExpr.
    fn resolve_dim(&mut self, ast_ty: &ast::AstType) -> Result<DimExpr, ()> {
        match &ast_ty.kind {
            ast::AstTypeKind::Nat(nat) => Ok(Self::ast_nat_to_dim(nat)),
            ast::AstTypeKind::Named(name) => {
                // Could be a dimension variable or a literal
                if let Ok(n) = name.parse::<u64>() {
                    Ok(DimExpr::Lit(n))
                } else {
                    Ok(DimExpr::Var(name.clone()))
                }
            }
            _ => Err(()),
        }
    }

    /// Convert an AST NatExpr to a DimExpr.
    fn ast_nat_to_dim(nat: &ast::NatExpr) -> DimExpr {
        match nat {
            ast::NatExpr::Lit(v) => DimExpr::Lit(*v),
            ast::NatExpr::Var(s) => DimExpr::Var(s.clone()),
            ast::NatExpr::BinOp(op, l, r) => {
                let dim_op = match op {
                    ast::NatOp::Add => DimOp::Add,
                    ast::NatOp::Sub => DimOp::Sub,
                    ast::NatOp::Mul => DimOp::Mul,
                };
                DimExpr::BinOp(
                    dim_op,
                    Box::new(Self::ast_nat_to_dim(l)),
                    Box::new(Self::ast_nat_to_dim(r)),
                )
            }
        }
    }

    // ── Expression checking ──────────────────────────────

    /// Check an expression and return its type.
    pub fn check_expr(&mut self, expr: &ast::Expr) -> Result<Ty, ()> {
        match &expr.kind {
            ast::ExprKind::IntLit(_) => Ok(Ty::Prim(PrimTy::I64)),
            ast::ExprKind::FloatLit(_) => Ok(Ty::Prim(PrimTy::F64)),
            ast::ExprKind::BoolLit(_) => Ok(Ty::Prim(PrimTy::Bool)),
            ast::ExprKind::StrLit(_) => Ok(Ty::Prim(PrimTy::Str)),
            ast::ExprKind::NilLit => Ok(Ty::Unit),
            ast::ExprKind::KeywordLit(_) => Ok(Ty::Prim(PrimTy::Str)),

            ast::ExprKind::SymbolRef(name) => {
                self.env.lookup(name).cloned().ok_or_else(|| {
                    self.diags.add(Diagnostic::error(
                        format!("undefined symbol: `{}`", name),
                        expr.span,
                    ));
                })
            }

            ast::ExprKind::If(cond, then_branch, else_branch) => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Ty::Prim(PrimTy::Bool) {
                    self.diags.add(Diagnostic::error(
                        "if condition must be bool",
                        cond.span,
                    ));
                    return Err(());
                }
                let then_ty = self.check_expr(then_branch)?;
                let else_ty = self.check_expr(else_branch)?;
                if then_ty != else_ty {
                    self.diags.add(Diagnostic::error(
                        format!(
                            "if branches have different types: {:?} vs {:?}",
                            then_ty, else_ty
                        ),
                        expr.span,
                    ));
                    return Err(());
                }
                Ok(then_ty)
            }

            ast::ExprKind::Let(bindings, body) => {
                self.env.push_scope();
                for b in bindings {
                    let actual = self.check_expr(&b.value)?;
                    let declared = self.resolve_type(&b.ty)?;
                    // If declared type is inferred placeholder, use actual
                    let bound_ty = if declared == Ty::TypeVar("_".to_string()) {
                        actual
                    } else {
                        // Check that actual is assignable to declared
                        if !self.types_compatible(&actual, &declared) {
                            self.diags.add(Diagnostic::error(
                                format!(
                                    "let binding type mismatch: expected {:?}, got {:?}",
                                    declared, actual
                                ),
                                b.span,
                            ));
                            self.env.pop_scope();
                            return Err(());
                        }
                        declared
                    };
                    self.env.bind(b.name.clone(), bound_ty);
                }
                let body_ty = self.check_expr(body)?;
                self.env.pop_scope();
                Ok(body_ty)
            }

            ast::ExprKind::Do(exprs) => {
                let mut ty = Ty::Unit;
                for e in exprs {
                    ty = self.check_expr(e)?;
                }
                Ok(ty)
            }

            ast::ExprKind::FnCall(callee, args) => {
                let callee_ty = self.check_expr(callee)?;
                match callee_ty {
                    Ty::Func { params, ret } => {
                        if args.len() != params.len() {
                            self.diags.add(Diagnostic::error(
                                format!(
                                    "function expects {} arguments, got {}",
                                    params.len(),
                                    args.len()
                                ),
                                expr.span,
                            ));
                            return Err(());
                        }
                        for (arg, param_ty) in args.iter().zip(params.iter()) {
                            let arg_ty = self.check_expr(arg)?;
                            self.check_assignable(&arg_ty, param_ty, arg.span)?;
                        }
                        Ok(*ret)
                    }
                    Ty::TypeVar(_) => {
                        // Polymorphic builtin — check args but return wildcard type
                        for arg in args {
                            let _ = self.check_expr(arg);
                        }
                        Ok(Ty::TypeVar("_".to_string()))
                    }
                    _ => {
                        self.diags.add(Diagnostic::error(
                            format!("expected function type, got {:?}", callee_ty),
                            expr.span,
                        ));
                        Err(())
                    }
                }
            }

            ast::ExprKind::Match(scrutinee, arms) => {
                let scrut_ty = self.check_expr(scrutinee)?;
                let mut result_ty: Option<Ty> = None;
                for arm in arms {
                    self.env.push_scope();
                    self.check_pattern(&arm.pattern, &scrut_ty)?;
                    let arm_ty = self.check_expr(&arm.body)?;
                    self.env.pop_scope();
                    if let Some(ref prev) = result_ty {
                        if arm_ty != *prev {
                            self.diags.add(Diagnostic::error(
                                format!(
                                    "match arms have different types: {:?} vs {:?}",
                                    prev, arm_ty
                                ),
                                arm.span,
                            ));
                            return Err(());
                        }
                    } else {
                        result_ty = Some(arm_ty);
                    }
                }
                result_ty.ok_or_else(|| {
                    self.diags.add(Diagnostic::error(
                        "match requires at least one arm",
                        expr.span,
                    ));
                })
            }

            ast::ExprKind::Lambda(params, body) => {
                self.env.push_scope();
                let mut param_tys = Vec::new();
                for p in params {
                    let ty = self.resolve_type(&p.ty)?;
                    // For untyped lambda params (ty = "_"), default to inferred
                    let bound_ty = if ty == Ty::TypeVar("_".to_string()) {
                        // Without full inference, we cannot determine the type,
                        // so we keep it as a type variable
                        Ty::TypeVar(p.name.clone())
                    } else {
                        ty
                    };
                    self.env.bind(p.name.clone(), bound_ty.clone());
                    param_tys.push(bound_ty);
                }
                let body_ty = self.check_expr(body)?;
                self.env.pop_scope();
                Ok(Ty::Func {
                    params: param_tys,
                    ret: Box::new(body_ty),
                })
            }

            ast::ExprKind::Try(inner) => {
                let inner_ty = self.check_expr(inner)?;
                // If inner returns Named("Result", [T, E]), result type is T
                match inner_ty {
                    Ty::Named { ref name, ref args } if name == "Result" && args.len() == 2 => {
                        if let TyArg::Type(ref t) = args[0] {
                            Ok(t.clone())
                        } else {
                            Err(())
                        }
                    }
                    _ => {
                        // try on a non-Result type just passes through
                        Ok(inner_ty)
                    }
                }
            }

            ast::ExprKind::VariantCtor(name, args) => {
                // Look up the variant constructor type if registered
                if let Some(ty) = self.env.lookup(name).cloned() {
                    match ty {
                        Ty::Func { params, ret } => {
                            if args.len() != params.len() {
                                self.diags.add(Diagnostic::error(
                                    format!(
                                        "variant {} expects {} arguments, got {}",
                                        name,
                                        params.len(),
                                        args.len()
                                    ),
                                    expr.span,
                                ));
                                return Err(());
                            }
                            for (arg, param_ty) in args.iter().zip(params.iter()) {
                                let arg_ty = self.check_expr(arg)?;
                                self.check_assignable(&arg_ty, param_ty, arg.span)?;
                            }
                            Ok(*ret)
                        }
                        _ => Ok(ty),
                    }
                } else {
                    // Unknown variant — check args and return a placeholder Named type
                    let mut arg_tys = Vec::new();
                    for arg in args {
                        arg_tys.push(TyArg::Type(self.check_expr(arg)?));
                    }
                    Ok(Ty::Named {
                        name: name.clone(),
                        args: arg_tys,
                    })
                }
            }

            ast::ExprKind::StructLit(_name, fields) => {
                let mut field_tys = Vec::new();
                for (fname, fexpr) in fields {
                    let ty = self.check_expr(fexpr)?;
                    field_tys.push(TyField {
                        name: fname.clone(),
                        ty,
                    });
                }
                Ok(Ty::Product(field_tys))
            }

            ast::ExprKind::ListLit(items) => {
                // All items must have the same type
                if items.is_empty() {
                    return Ok(Ty::Unit);
                }
                let first_ty = self.check_expr(&items[0])?;
                for item in &items[1..] {
                    let item_ty = self.check_expr(item)?;
                    if item_ty != first_ty {
                        self.diags.add(Diagnostic::error(
                            format!(
                                "list elements have different types: {:?} vs {:?}",
                                first_ty, item_ty
                            ),
                            item.span,
                        ));
                        return Err(());
                    }
                }
                Ok(first_ty)
            }

            ast::ExprKind::Forall(_, _, _) | ast::ExprKind::Exists(_, _, _) => {
                Ok(Ty::Prim(PrimTy::Bool))
            }
        }
    }

    // ── Pattern checking ─────────────────────────────────

    /// Check a pattern against a scrutinee type, binding pattern variables.
    fn check_pattern(&mut self, pattern: &ast::Pattern, scrut_ty: &Ty) -> Result<(), ()> {
        match &pattern.kind {
            ast::PatternKind::Wildcard => Ok(()),
            ast::PatternKind::Binding(name) => {
                self.env.bind(name.clone(), scrut_ty.clone());
                Ok(())
            }
            ast::PatternKind::Literal(_) => {
                // We don't deeply check literal patterns against the scrutinee
                // for now — just accept them.
                Ok(())
            }
            ast::PatternKind::Variant(_name, sub_pats) => {
                // For variant patterns, bind sub-patterns.
                // In a full checker we'd look up the variant fields,
                // but for now we bind each sub-pattern to the scrutinee type.
                for sub in sub_pats {
                    // Bind sub-pattern variables generically
                    self.check_pattern_binding(sub)?;
                }
                Ok(())
            }
        }
    }

    /// Bind variables in a sub-pattern without type information.
    fn check_pattern_binding(&mut self, pattern: &ast::Pattern) -> Result<(), ()> {
        match &pattern.kind {
            ast::PatternKind::Wildcard => Ok(()),
            ast::PatternKind::Binding(name) => {
                // Without full variant type info, bind as a type variable
                self.env.bind(name.clone(), Ty::TypeVar(name.clone()));
                Ok(())
            }
            ast::PatternKind::Literal(_) => Ok(()),
            ast::PatternKind::Variant(_, sub_pats) => {
                for sub in sub_pats {
                    self.check_pattern_binding(sub)?;
                }
                Ok(())
            }
        }
    }

    // ── Top-level checking ───────────────────────────────

    /// Check a top-level form.
    pub fn check_top_level(&mut self, top: &ast::TopLevel) -> Result<(), ()> {
        match top {
            ast::TopLevel::Defn(f) => {
                self.check_fn(f)?;
                Ok(())
            }
            ast::TopLevel::DefType(td) => {
                self.register_type_def(td)?;
                Ok(())
            }
            ast::TopLevel::Module(m) => {
                for item in &m.body {
                    self.check_top_level(item)?;
                }
                Ok(())
            }
            ast::TopLevel::Expr(e) => {
                self.check_expr(e)?;
                Ok(())
            }
            ast::TopLevel::Define(_) => Ok(()), // No type checking for define
            ast::TopLevel::Task(_) => Ok(()),
            ast::TopLevel::UseDecl(_) => Ok(()),
            ast::TopLevel::ExternC(decl) => {
                let mut param_tys = Vec::new();
                for p in &decl.params {
                    let ty = self.resolve_type(&p.ty)?;
                    param_tys.push(ty);
                }
                let ret_ty = self.resolve_type(&decl.return_type)?;
                let fn_ty = Ty::Func {
                    params: param_tys,
                    ret: Box::new(ret_ty),
                };
                self.env.bind(decl.c_name.clone(), fn_ty);
                Ok(())
            }
            ast::TopLevel::Import { .. } => Ok(()),
        }
    }

    /// Check a function definition.
    pub fn check_fn(&mut self, f: &ast::FnDef) -> Result<Ty, ()> {
        self.env.push_scope();
        let mut param_tys = Vec::new();
        for p in &f.params {
            let ty = self.resolve_type(&p.ty)?;
            self.env.bind(p.name.clone(), ty.clone());
            param_tys.push(ty);
        }
        let declared_ret = self.resolve_type(&f.return_type)?;
        let body_ty = self.check_expr(&f.body)?;
        // Check body_ty is assignable to declared_ret
        self.check_assignable(&body_ty, &declared_ret, f.body.span)?;
        self.env.pop_scope();
        let fn_ty = Ty::Func {
            params: param_tys,
            ret: Box::new(declared_ret),
        };
        self.env.bind(f.name.clone(), fn_ty.clone());
        Ok(fn_ty)
    }

    /// Register a type definition in the environment.
    fn register_type_def(&mut self, td: &ast::TypeDef) -> Result<(), ()> {
        let param_names: Vec<String> = td.type_params.iter().map(|p| p.name.clone()).collect();
        let ty = match &td.body {
            ast::TypeDefBody::Sum(variants) => {
                let ty_variants: Vec<TyVariant> = variants
                    .iter()
                    .map(|v| {
                        let fields = v
                            .fields
                            .iter()
                            .map(|f| self.resolve_type(f).unwrap_or(Ty::TypeVar("?".into())))
                            .collect();
                        TyVariant {
                            name: v.name.clone(),
                            fields,
                        }
                    })
                    .collect();
                Ty::Sum(ty_variants)
            }
            ast::TypeDefBody::Product(fields) => {
                let ty_fields: Vec<TyField> = fields
                    .iter()
                    .map(|f| TyField {
                        name: f.name.clone(),
                        ty: self.resolve_type(&f.ty).unwrap_or(Ty::TypeVar("?".into())),
                    })
                    .collect();
                Ty::Product(ty_fields)
            }
            ast::TypeDefBody::Alias(ast_ty) => self.resolve_type(ast_ty)?,
        };
        self.env.register_type(td.name.clone(), param_names, ty);
        Ok(())
    }

    // ── Type compatibility ───────────────────────────────

    /// Check that `actual` is assignable to `expected`.
    fn check_assignable(&mut self, actual: &Ty, expected: &Ty, span: Span) -> Result<(), ()> {
        if self.types_compatible(actual, expected) {
            Ok(())
        } else {
            self.diags.add(Diagnostic::error(
                format!("type mismatch: expected {:?}, got {:?}", expected, actual),
                span,
            ));
            Err(())
        }
    }

    /// Check if two types are compatible, including dimension unification.
    fn types_compatible(&mut self, actual: &Ty, expected: &Ty) -> bool {
        // TypeVar("_") is compatible with anything (inference placeholder)
        if matches!(actual, Ty::TypeVar(n) if n == "_")
            || matches!(expected, Ty::TypeVar(n) if n == "_")
        {
            return true;
        }
        // Never is compatible with anything (bottom type)
        if matches!(actual, Ty::Never) || matches!(expected, Ty::Never) {
            return true;
        }
        match (actual, expected) {
            (Ty::Prim(a), Ty::Prim(b)) => {
                if a == b {
                    return true;
                }
                // Allow numeric coercion: any integer literal type is compatible
                // with any other integer type, same for floats
                (a.is_integer() && b.is_integer()) || (a.is_float() && b.is_float())
            }
            (Ty::Unit, Ty::Unit) => true,
            (Ty::Func { params: ap, ret: ar }, Ty::Func { params: bp, ret: br }) => {
                ap.len() == bp.len()
                    && ap.iter().zip(bp.iter()).all(|(a, b)| self.types_compatible(a, b))
                    && self.types_compatible(ar, br)
            }
            (
                Ty::Tensor { elem: ae, shape: as_ },
                Ty::Tensor { elem: be, shape: bs },
            ) => {
                if !self.types_compatible(ae, be) {
                    return false;
                }
                if as_.len() != bs.len() {
                    return false;
                }
                for (a, b) in as_.iter().zip(bs.iter()) {
                    if unify_dim(a, b, &mut self.dim_subst).is_err() {
                        return false;
                    }
                }
                true
            }
            (
                Ty::Named { name: an, args: aa },
                Ty::Named { name: bn, args: ba },
            ) => {
                an == bn
                    && aa.len() == ba.len()
                    && aa.iter().zip(ba.iter()).all(|(a, b)| match (a, b) {
                        (TyArg::Type(at), TyArg::Type(bt)) => self.types_compatible(at, bt),
                        (TyArg::Nat(ad), TyArg::Nat(bd)) => {
                            unify_dim(ad, bd, &mut self.dim_subst).is_ok()
                        }
                        _ => false,
                    })
            }
            (Ty::TypeVar(a), Ty::TypeVar(b)) => a == b,
            _ => actual == expected,
        }
    }

    // ── Diagnostics ──────────────────────────────────────

    pub fn into_diagnostics(self) -> Diagnostics {
        self.diags
    }

    /// Drain diagnostics without consuming the checker.
    /// Useful for REPL where the checker persists across inputs.
    pub fn drain_diagnostics(&mut self) -> Diagnostics {
        std::mem::replace(&mut self.diags, Diagnostics::new())
    }

    pub fn has_errors(&self) -> bool {
        self.diags.has_errors()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helper ──────────────────────────────────────

    fn parse_and_check(input: &str) -> Result<Ty, String> {
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().map_err(|d| d.message)?;
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).map_err(|d| d.message)?;
        if sexprs.is_empty() {
            return Err("no expressions parsed".to_string());
        }
        let mut diags = Diagnostics::new();
        let expr = airl_syntax::parser::parse_expr(&sexprs[0], &mut diags)
            .map_err(|d| d.message)?;
        let mut checker = TypeChecker::new();
        checker.check_expr(&expr).map_err(|_| "type error".to_string())
    }

    // ── Task 11a: Type resolution and basic expression checking ──

    #[test]
    fn resolve_primitive_types() {
        let checker = TypeChecker::new();
        assert_eq!(checker.resolve_type_name("i32"), Ok(Ty::Prim(PrimTy::I32)));
        assert_eq!(checker.resolve_type_name("bool"), Ok(Ty::Prim(PrimTy::Bool)));
        assert_eq!(checker.resolve_type_name("f64"), Ok(Ty::Prim(PrimTy::F64)));
        assert_eq!(checker.resolve_type_name("String"), Ok(Ty::Prim(PrimTy::Str)));
        assert_eq!(checker.resolve_type_name("Unit"), Ok(Ty::Unit));
        assert_eq!(checker.resolve_type_name("Never"), Ok(Ty::Never));
    }

    #[test]
    fn resolve_unknown_type_fails() {
        let checker = TypeChecker::new();
        assert!(checker.resolve_type_name("Nonexistent").is_err());
    }

    #[test]
    fn check_int_literal() {
        assert_eq!(parse_and_check("42"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_float_literal() {
        assert_eq!(parse_and_check("3.14"), Ok(Ty::Prim(PrimTy::F64)));
    }

    #[test]
    fn check_bool_literal() {
        assert_eq!(parse_and_check("true"), Ok(Ty::Prim(PrimTy::Bool)));
        assert_eq!(parse_and_check("false"), Ok(Ty::Prim(PrimTy::Bool)));
    }

    #[test]
    fn check_string_literal() {
        assert_eq!(parse_and_check(r#""hello""#), Ok(Ty::Prim(PrimTy::Str)));
    }

    #[test]
    fn check_nil_literal() {
        assert_eq!(parse_and_check("nil"), Ok(Ty::Unit));
    }

    #[test]
    fn check_arithmetic_same_type() {
        assert_eq!(parse_and_check("(+ 1 2)"), Ok(Ty::Prim(PrimTy::I64)));
        assert_eq!(parse_and_check("(- 10 3)"), Ok(Ty::Prim(PrimTy::I64)));
        assert_eq!(parse_and_check("(* 4 5)"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_arithmetic_type_mismatch() {
        assert!(parse_and_check(r#"(+ 1 "hello")"#).is_err());
    }

    #[test]
    fn check_comparison_returns_bool() {
        assert_eq!(parse_and_check("(< 1 2)"), Ok(Ty::Prim(PrimTy::Bool)));
        assert_eq!(parse_and_check("(== 3 4)"), Ok(Ty::Prim(PrimTy::Bool)));
    }

    #[test]
    fn check_let_binding_type() {
        assert_eq!(
            parse_and_check("(let (x : i32 42) x)"),
            Ok(Ty::Prim(PrimTy::I32))
        );
    }

    #[test]
    fn check_let_binding_type_mismatch() {
        // Binding declares i32 but value is a string
        assert!(parse_and_check(r#"(let (x : i32 "hello") x)"#).is_err());
    }

    #[test]
    fn check_if_branches_same_type() {
        assert_eq!(
            parse_and_check("(if true 1 2)"),
            Ok(Ty::Prim(PrimTy::I64))
        );
    }

    #[test]
    fn check_if_branches_different_type() {
        assert!(parse_and_check(r#"(if true 1 "hello")"#).is_err());
    }

    #[test]
    fn check_if_condition_must_be_bool() {
        assert!(parse_and_check("(if 42 1 2)").is_err());
    }

    // ── Task 11b: FnCall, Match, Lambda, Do, Try ─────────

    #[test]
    fn check_fn_definition_and_call() {
        // Define add function using AIRL keyword syntax, then call it
        let mut checker = TypeChecker::new();
        let input = r#"(defn add :sig [(a : i32) (b : i32) -> i32] :requires [(>= a 0)] :body (+ a b))"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let top = airl_syntax::parse_top_level(&sexprs[0], &mut diags).unwrap();
        checker.check_top_level(&top).unwrap();

        // Now check a call: (add 1 2) should return i32
        let call_input = "(add 1 2)";
        let mut lexer2 = airl_syntax::Lexer::new(call_input);
        let tokens2 = lexer2.lex_all().unwrap();
        let sexprs2 = airl_syntax::parse_sexpr_all(&tokens2).unwrap();
        let call_expr = airl_syntax::parser::parse_expr(&sexprs2[0], &mut diags).unwrap();
        let result = checker.check_expr(&call_expr).unwrap();
        assert_eq!(result, Ty::Prim(PrimTy::I32));
    }

    #[test]
    fn check_fn_call_wrong_arg_count() {
        let mut checker = TypeChecker::new();
        // Register a function manually
        checker.env.bind(
            "foo".to_string(),
            Ty::Func {
                params: vec![Ty::Prim(PrimTy::I32)],
                ret: Box::new(Ty::Prim(PrimTy::I32)),
            },
        );
        // Call with wrong number of args
        let input = "(foo 1 2)";
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let expr = airl_syntax::parser::parse_expr(&sexprs[0], &mut diags).unwrap();
        assert!(checker.check_expr(&expr).is_err());
    }

    #[test]
    fn check_fn_call_wrong_arg_type() {
        let mut checker = TypeChecker::new();
        checker.env.bind(
            "foo".to_string(),
            Ty::Func {
                params: vec![Ty::Prim(PrimTy::I32)],
                ret: Box::new(Ty::Prim(PrimTy::I32)),
            },
        );
        let input = r#"(foo "hello")"#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let expr = airl_syntax::parser::parse_expr(&sexprs[0], &mut diags).unwrap();
        assert!(checker.check_expr(&expr).is_err());
    }

    #[test]
    fn check_do_block() {
        // (do 1 2 3) returns type of last expression
        assert_eq!(parse_and_check("(do 1 2 3)"), Ok(Ty::Prim(PrimTy::I64)));
    }

    #[test]
    fn check_do_block_returns_last_type() {
        // (do 1 true) returns bool
        assert_eq!(
            parse_and_check("(do 1 true)"),
            Ok(Ty::Prim(PrimTy::Bool))
        );
    }

    #[test]
    fn check_do_empty() {
        assert_eq!(parse_and_check("(do)"), Ok(Ty::Unit));
    }

    #[test]
    fn check_match_result() {
        // match on a value with wildcard arms returning same type
        assert_eq!(
            parse_and_check("(match 42 x 1 _ 2)"),
            Ok(Ty::Prim(PrimTy::I64))
        );
    }

    #[test]
    fn check_match_arms_must_agree() {
        assert!(parse_and_check(r#"(match 42 x 1 _ "hello")"#).is_err());
    }

    #[test]
    fn check_lambda_typed_params() {
        // Lambda with typed params: (fn [(x : i64)] (+ x 1))
        assert_eq!(
            parse_and_check("(fn [(x : i64)] (+ x 1))"),
            Ok(Ty::Func {
                params: vec![Ty::Prim(PrimTy::I64)],
                ret: Box::new(Ty::Prim(PrimTy::I64)),
            })
        );
    }

    #[test]
    fn check_nested_let() {
        // (let (x : i64 1) (let (y : i64 2) (+ x y)))
        assert_eq!(
            parse_and_check("(let (x : i64 1) (let (y : i64 2) (+ x y)))"),
            Ok(Ty::Prim(PrimTy::I64))
        );
    }

    #[test]
    fn check_undefined_symbol() {
        assert!(parse_and_check("undefined_var").is_err());
    }
}
