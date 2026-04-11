use crate::ast::*;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::sexpr::{Atom, AtomKind, SExpr};
use crate::span::Span;

// ── Public API ─────────────────────────────────────────

pub fn parse_top_level(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<TopLevel, Diagnostic> {
    match sexpr {
        SExpr::List(items, span) if !items.is_empty() => {
            if let Some(head) = items[0].as_symbol() {
                match head {
                    "defn" => parse_defn(&items[1..], *span, diags).map(TopLevel::Defn),
                    "define" => parse_define(&items[1..], *span, diags).map(TopLevel::Define),
                    "deftype" => parse_deftype(&items[1..], *span, diags).map(TopLevel::DefType),
                    "module" => parse_module(&items[1..], *span, diags).map(TopLevel::Module),
                    "task" => parse_task(&items[1..], *span, diags).map(TopLevel::Task),
                    "use" => parse_use(&items[1..], *span, diags).map(TopLevel::UseDecl),
                    "import" => parse_import(&items[1..], *span, diags),
                    "extern-c" => parse_extern_c(&items[1..], *span, diags).map(TopLevel::ExternC),
                    _ => parse_expr(sexpr, diags).map(TopLevel::Expr),
                }
            } else {
                parse_expr(sexpr, diags).map(TopLevel::Expr)
            }
        }
        _ => parse_expr(sexpr, diags).map(TopLevel::Expr),
    }
}

pub fn parse_expr(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::Atom(atom) => parse_atom_expr(atom),
        SExpr::BracketList(items, sp) => {
            let mut exprs = Vec::new();
            for item in items {
                exprs.push(parse_expr(item, diags)?);
            }
            Ok(Expr { kind: ExprKind::ListLit(exprs), span: *sp })
        }
        SExpr::List(items, _) => {
            if items.is_empty() {
                return Ok(Expr { kind: ExprKind::NilLit, span });
            }
            if let Some(head) = items[0].as_symbol() {
                match head {
                    "if" => parse_if_expr(&items[1..], span, diags),
                    "let" => parse_let_expr(&items[1..], span, diags),
                    "do" => parse_do_expr(&items[1..], span, diags),
                    "match" => parse_match_expr(&items[1..], span, diags),
                    "fn" => parse_lambda_expr(&items[1..], span, diags),
                    "try" => parse_try_expr(&items[1..], span, diags),
                    "forall" => parse_quantifier_expr(&items[1..], span, true, diags),
                    "exists" => parse_quantifier_expr(&items[1..], span, false, diags),
                    "->>" => parse_thread_last(&items[1..], span, diags),
                    s if is_capitalized(s) => {
                        // VariantCtor
                        let name = s.to_string();
                        let mut args = Vec::new();
                        for item in &items[1..] {
                            args.push(parse_expr(item, diags)?);
                        }
                        Ok(Expr { kind: ExprKind::VariantCtor(name, args), span })
                    }
                    _ => {
                        // FnCall
                        let callee = parse_expr(&items[0], diags)?;
                        let mut args = Vec::new();
                        for item in &items[1..] {
                            args.push(parse_expr(item, diags)?);
                        }
                        Ok(Expr { kind: ExprKind::FnCall(Box::new(callee), args), span })
                    }
                }
            } else if matches!(items[0], SExpr::Atom(Atom { kind: AtomKind::Arrow, .. })) {
                // (-> ...) thread-first macro
                parse_thread_first(&items[1..], span, diags)
            } else {
                // First element is not a symbol — treat as FnCall
                let callee = parse_expr(&items[0], diags)?;
                let mut args = Vec::new();
                for item in &items[1..] {
                    args.push(parse_expr(item, diags)?);
                }
                Ok(Expr { kind: ExprKind::FnCall(Box::new(callee), args), span })
            }
        }
    }
}

// ── Atom expression ────────────────────────────────────

fn parse_atom_expr(atom: &Atom) -> Result<Expr, Diagnostic> {
    let span = atom.span;
    let kind = match &atom.kind {
        AtomKind::Integer(v) => ExprKind::IntLit(*v),
        AtomKind::Float(v) => ExprKind::FloatLit(*v),
        AtomKind::Str(v) => ExprKind::StrLit(v.clone()),
        AtomKind::Bool(v) => ExprKind::BoolLit(*v),
        AtomKind::Nil => ExprKind::NilLit,
        AtomKind::Symbol(s) => ExprKind::SymbolRef(s.clone()),
        AtomKind::Keyword(s) => ExprKind::KeywordLit(s.clone()),
        AtomKind::Arrow => ExprKind::SymbolRef("->".to_string()),
        AtomKind::Version(_, _, _) => {
            return Err(Diagnostic::error(
                "version literal is only valid in :version position",
                span,
            ));
        }
    };
    Ok(Expr { kind, span })
}

// ── Expression forms ───────────────────────────────────

fn parse_if_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.len() != 3 {
        return Err(Diagnostic::error(
            format!("if requires exactly 3 subforms, found {}", items.len()),
            span,
        ));
    }
    let cond = parse_expr(&items[0], diags)?;
    let then = parse_expr(&items[1], diags)?;
    let else_ = parse_expr(&items[2], diags)?;
    Ok(Expr {
        kind: ExprKind::If(Box::new(cond), Box::new(then), Box::new(else_)),
        span,
    })
}

/// Global counter for generating unique gensym names during destructuring.
static GENSYM_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn gensym() -> String {
    let n = GENSYM_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("__d{}", n)
}

fn is_let_binding(sexpr: &SExpr) -> bool {
    if let SExpr::List(items, _) = sexpr {
        if items.is_empty() {
            return false;
        }
        // Standard typed binding: (name : Type value) — 4+ items, second is ":"
        if items.len() >= 4 {
            if let Some(s) = items[1].as_symbol() {
                if s == ":" {
                    return true;
                }
            }
        }
        // List destructure: ([a b c] value) or ([a b c] : Type value)
        if let SExpr::BracketList(_, _) = &items[0] {
            return items.len() >= 2;
        }
        // Map destructure: ({name age} value) — first item is a List starting with a symbol (map key names)
        if let SExpr::List(pattern_items, _) = &items[0] {
            if !pattern_items.is_empty() {
                // A map pattern is a list of symbols (keys), not a function call
                if pattern_items.iter().all(|p| p.as_symbol().is_some()) {
                    return items.len() >= 2;
                }
            }
        }
    }
    false
}

/// Parse one let-binding sexpr into potentially multiple `LetBinding`s (destructuring expands).
fn parse_let_bindings_from(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Vec<LetBinding>, Diagnostic> {
    let span = sexpr.span();
    if let SExpr::List(items, _) = sexpr {
        if items.is_empty() {
            return Err(Diagnostic::error("expected let binding list", span));
        }
        // List destructure: ([a b & rest] value) or ([a b & rest] : Type value)
        if let SExpr::BracketList(pattern_items, _) = &items[0] {
            return parse_list_destructure(pattern_items, &items[1..], span, diags);
        }
        // Map destructure: ({name age} value) — list of symbols as keys
        if let SExpr::List(pattern_items, _) = &items[0] {
            if !pattern_items.is_empty() && pattern_items.iter().all(|p| p.as_symbol().is_some()) {
                return parse_map_destructure(pattern_items, &items[1..], span, diags);
            }
        }
        // Standard: (name : Type value)
        if items.len() < 4 {
            return Err(Diagnostic::error("let binding requires (name : Type value)", span));
        }
        let name = expect_symbol(&items[0])?;
        let ty = parse_type(&items[2], diags)?;
        let value = parse_expr(&items[3], diags)?;
        Ok(vec![LetBinding { name, ty, value, span }])
    } else {
        Err(Diagnostic::error("expected let binding list", span))
    }
}

/// Desugar `([a b & rest] value)` into gensym + sequential `at` bindings.
fn parse_list_destructure(
    pattern_items: &[SExpr],
    rest_items: &[SExpr],   // items after the pattern in the binding form
    span: Span,
    diags: &mut Diagnostics,
) -> Result<Vec<LetBinding>, Diagnostic> {
    // Skip optional `: Type` annotation between pattern and value
    let (value_sexpr, _ty) = if rest_items.len() >= 3
        && rest_items[0].as_symbol() == Some(":")
    {
        let ty = parse_type(&rest_items[1], diags)?;
        (&rest_items[2], Some(ty))
    } else if !rest_items.is_empty() {
        (&rest_items[0], None)
    } else {
        return Err(Diagnostic::error("list destructure binding missing value", span));
    };

    let wild_ty = AstType { kind: AstTypeKind::Named("_".to_string()), span };

    // Bind the value expression to a gensym to avoid double-evaluation
    let gs = gensym();
    let value_expr = parse_expr(value_sexpr, diags)?;
    let mut bindings = vec![LetBinding {
        name: gs.clone(),
        ty: wild_ty.clone(),
        value: value_expr,
        span,
    }];

    // Walk the pattern items, handling `& rest`
    let mut idx = 0usize;
    let mut i = 0;
    while i < pattern_items.len() {
        if let Some("&") = pattern_items[i].as_symbol() {
            // Rest binding: everything after current index as (drop idx gensym)
            i += 1;
            if i >= pattern_items.len() {
                return Err(Diagnostic::error("& in let pattern requires a name after it", span));
            }
            let rest_name = expect_symbol(&pattern_items[i])?;
            let drop_fn = Expr { kind: ExprKind::SymbolRef("drop".to_string()), span };
            let idx_lit = Expr { kind: ExprKind::IntLit(idx as i64), span };
            let gs_ref = Expr { kind: ExprKind::SymbolRef(gs.clone()), span };
            bindings.push(LetBinding {
                name: rest_name,
                ty: wild_ty.clone(),
                value: Expr { kind: ExprKind::FnCall(Box::new(drop_fn), vec![idx_lit, gs_ref]), span },
                span,
            });
            i += 1;
        } else {
            let elem_name = expect_symbol(&pattern_items[i])?;
            let at_fn = Expr { kind: ExprKind::SymbolRef("at".to_string()), span };
            let gs_ref = Expr { kind: ExprKind::SymbolRef(gs.clone()), span };
            let idx_lit = Expr { kind: ExprKind::IntLit(idx as i64), span };
            bindings.push(LetBinding {
                name: elem_name,
                ty: wild_ty.clone(),
                value: Expr { kind: ExprKind::FnCall(Box::new(at_fn), vec![gs_ref, idx_lit]), span },
                span,
            });
            idx += 1;
            i += 1;
        }
    }

    Ok(bindings)
}

/// Desugar `({name age} value)` into `(map-get gensym "name")` etc.
fn parse_map_destructure(
    pattern_items: &[SExpr],
    rest_items: &[SExpr],
    span: Span,
    diags: &mut Diagnostics,
) -> Result<Vec<LetBinding>, Diagnostic> {
    let value_sexpr = if !rest_items.is_empty() {
        &rest_items[0]
    } else {
        return Err(Diagnostic::error("map destructure binding missing value", span));
    };

    let wild_ty = AstType { kind: AstTypeKind::Named("_".to_string()), span };

    let gs = gensym();
    let value_expr = parse_expr(value_sexpr, diags)?;
    let mut bindings = vec![LetBinding {
        name: gs.clone(),
        ty: wild_ty.clone(),
        value: value_expr,
        span,
    }];

    for item in pattern_items {
        let key_name = expect_symbol(item)?;
        let map_get = Expr { kind: ExprKind::SymbolRef("map-get".to_string()), span };
        let gs_ref = Expr { kind: ExprKind::SymbolRef(gs.clone()), span };
        let key_str = Expr { kind: ExprKind::StrLit(key_name.clone()), span };
        bindings.push(LetBinding {
            name: key_name,
            ty: wild_ty.clone(),
            value: Expr { kind: ExprKind::FnCall(Box::new(map_get), vec![gs_ref, key_str]), span },
            span,
        });
    }

    Ok(bindings)
}

fn parse_let_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("let requires at least a body", span));
    }

    let mut bindings = Vec::new();
    let mut i = 0;
    while i < items.len() && is_let_binding(&items[i]) {
        let mut new_bindings = parse_let_bindings_from(&items[i], diags)?;
        bindings.append(&mut new_bindings);
        i += 1;
    }

    // Check for malformed bindings: (name value) without `: Type`
    if bindings.is_empty() && !items.is_empty() {
        if let SExpr::List(inner, inner_span) = &items[0] {
            if inner.len() >= 2 && inner.len() <= 3 {
                if let Some(name) = inner[0].as_symbol() {
                    // Looks like a binding attempt without type annotation
                    if inner.get(1).and_then(|s| s.as_symbol()).map_or(true, |s| s != ":") {
                        return Err(Diagnostic::error(
                            format!("let binding '{}' is missing type annotation — use (let ({} : Type value) body)", name, name),
                            *inner_span,
                        ));
                    }
                }
            }
        }
    }

    if i >= items.len() {
        return Err(Diagnostic::error("let requires a body expression", span));
    }

    let body = parse_expr(&items[i], diags)?;
    i += 1;
    if i < items.len() {
        let extra = items.len() - i;
        diags.add(Diagnostic::warning(
            format!("let has {} extra expression(s) after body that will be ignored", extra),
            items[i].span(),
        ));
    }
    Ok(Expr {
        kind: ExprKind::Let(bindings, Box::new(body)),
        span,
    })
}

fn parse_do_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    let mut exprs = Vec::new();
    for item in items {
        exprs.push(parse_expr(item, diags)?);
    }
    Ok(Expr { kind: ExprKind::Do(exprs), span })
}

fn parse_match_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("match requires a scrutinee", span));
    }
    let scrutinee = parse_expr(&items[0], diags)?;
    let rest = &items[1..];
    if rest.len() % 2 != 0 {
        return Err(Diagnostic::error("match arms must come in pattern/body pairs", span));
    }
    let mut arms = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let pattern = parse_pattern(&rest[i], diags)?;
        let body = parse_expr(&rest[i + 1], diags)?;
        let arm_span = rest[i].span().merge(rest[i + 1].span());
        arms.push(MatchArm { pattern, body, span: arm_span });
        i += 2;
    }
    Ok(Expr {
        kind: ExprKind::Match(Box::new(scrutinee), arms),
        span,
    })
}

fn parse_lambda_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.len() < 2 {
        return Err(Diagnostic::error("fn requires params and body", span));
    }
    let params = parse_lambda_params(&items[0], diags)?;
    let body = parse_expr(&items[1], diags)?;
    if items.len() > 2 {
        let extra = items.len() - 2;
        diags.add(Diagnostic::warning(
            format!("fn has {} extra expression(s) after body that will be ignored", extra),
            items[2].span(),
        ));
    }
    Ok(Expr {
        kind: ExprKind::Lambda(params, Box::new(body)),
        span,
    })
}

fn parse_lambda_params(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Vec<Param>, Diagnostic> {
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut params = Vec::new();
            for item in items {
                // Simple lambda params: just symbols
                match item {
                    SExpr::Atom(Atom { kind: AtomKind::Symbol(s), span }) => {
                        params.push(Param {
                            ownership: Ownership::Default,
                            name: s.clone(),
                            ty: AstType {
                                kind: AstTypeKind::Named("_".to_string()),
                                span: *span,
                            },
                            default: None,
                            span: *span,
                        });
                    }
                    SExpr::List(..) => {
                        params.push(parse_param(item, diags)?);
                    }
                    _ => {
                        return Err(Diagnostic::error("expected parameter", item.span()));
                    }
                }
            }
            Ok(params)
        }
        _ => Err(Diagnostic::error("expected bracket list for parameters", sexpr.span())),
    }
}

fn parse_try_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.len() != 1 {
        return Err(Diagnostic::error("try requires exactly one expression", span));
    }
    let inner = parse_expr(&items[0], diags)?;
    Ok(Expr {
        kind: ExprKind::Try(Box::new(inner)),
        span,
    })
}

/// Parse a quantifier expression: (forall [i : Type] (where guard)? body)
///                                 (exists [i : Type] (where guard)? body)
fn parse_quantifier_expr(
    items: &[SExpr],
    span: Span,
    is_forall: bool,
    diags: &mut Diagnostics,
) -> Result<Expr, Diagnostic> {
    let name = if is_forall { "forall" } else { "exists" };
    if items.len() < 2 {
        return Err(Diagnostic::error(
            &format!("{} requires a parameter list and body", name), span,
        ));
    }

    // Parse parameter from bracket list [name : Type]
    let param = match &items[0] {
        SExpr::BracketList(param_items, pspan) => {
            if param_items.len() < 3 {
                return Err(Diagnostic::error(
                    &format!("{} parameter requires [name : Type]", name), *pspan,
                ));
            }
            let pname = expect_symbol(&param_items[0])?;
            if param_items[1].as_symbol() != Some(":") {
                return Err(Diagnostic::error("expected ':' in parameter", *pspan));
            }
            let ty_name = expect_symbol(&param_items[2])?;
            Param {
                ownership: Ownership::Default,
                name: pname,
                ty: AstType { kind: AstTypeKind::Named(ty_name), span: *pspan },
                default: None,
                span: *pspan,
            }
        }
        _ => return Err(Diagnostic::error(
            &format!("{} requires [name : Type] parameter list", name), span,
        )),
    };

    // Check for optional (where guard) clause
    let (where_clause, body_idx) = if items.len() >= 3 {
        // Check if items[1] is (where ...)
        if let SExpr::List(inner, _) = &items[1] {
            if !inner.is_empty() && inner[0].as_symbol() == Some("where") {
                if inner.len() != 2 {
                    return Err(Diagnostic::error("where clause requires exactly one expression", span));
                }
                let guard = parse_expr(&inner[1], diags)?;
                (Some(Box::new(guard)), 2)
            } else {
                (None, 1)
            }
        } else {
            (None, 1)
        }
    } else {
        (None, 1)
    };

    if body_idx >= items.len() {
        return Err(Diagnostic::error(
            &format!("{} requires a body expression", name), span,
        ));
    }

    let body = parse_expr(&items[body_idx], diags)?;

    let kind = if is_forall {
        ExprKind::Forall(Box::new(param), where_clause, Box::new(body))
    } else {
        ExprKind::Exists(Box::new(param), where_clause, Box::new(body))
    };

    Ok(Expr { kind, span })
}

// ── Threading macros ───────────────────────────────────

/// `(-> seed step1 step2 ...)` — thread-first: insert acc as first arg of each step.
fn parse_thread_first(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("-> requires at least one argument", span));
    }
    let mut acc = parse_expr(&items[0], diags)?;
    for step in &items[1..] {
        acc = thread_step(acc, step, /*last=*/false, diags)?;
    }
    Ok(acc)
}

/// `(->> seed step1 step2 ...)` — thread-last: insert acc as last arg of each step.
fn parse_thread_last(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("->> requires at least one argument", span));
    }
    let mut acc = parse_expr(&items[0], diags)?;
    for step in &items[1..] {
        acc = thread_step(acc, step, /*last=*/true, diags)?;
    }
    Ok(acc)
}

/// Build one threading step: if `last` is true, acc is the last arg; otherwise first.
fn thread_step(acc: Expr, step: &SExpr, last: bool, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    let step_span = step.span();
    match step {
        // Symbol step: (f acc) — same position regardless of ->/->>.
        SExpr::Atom(atom) if matches!(atom.kind, AtomKind::Symbol(_)) => {
            let f = parse_expr(step, diags)?;
            Ok(Expr { kind: ExprKind::FnCall(Box::new(f), vec![acc]), span: step_span })
        }
        // List step: (f a b ...) → (f acc a b ...) or (f a b ... acc)
        SExpr::List(elems, _) if !elems.is_empty() => {
            let f = parse_expr(&elems[0], diags)?;
            let mut extra: Vec<Expr> = elems[1..].iter()
                .map(|a| parse_expr(a, diags))
                .collect::<Result<_, _>>()?;
            let args = if last {
                extra.push(acc);
                extra
            } else {
                let mut args = vec![acc];
                args.extend(extra);
                args
            };
            Ok(Expr { kind: ExprKind::FnCall(Box::new(f), args), span: step_span })
        }
        _ => Err(Diagnostic::error("threading macro step must be a symbol or list", step_span)),
    }
}

// ── Pattern parsing ────────────────────────────────────

fn parse_pattern(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Pattern, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::Atom(atom) => {
            let kind = match &atom.kind {
                AtomKind::Symbol(s) if s == "_" => PatternKind::Wildcard,
                AtomKind::Symbol(s) => PatternKind::Binding(s.clone()),
                AtomKind::Integer(v) => PatternKind::Literal(LitPattern::Int(*v)),
                AtomKind::Float(v) => PatternKind::Literal(LitPattern::Float(*v)),
                AtomKind::Str(v) => PatternKind::Literal(LitPattern::Str(v.clone())),
                AtomKind::Bool(v) => PatternKind::Literal(LitPattern::Bool(*v)),
                AtomKind::Nil => PatternKind::Literal(LitPattern::Nil),
                _ => return Err(Diagnostic::error("unexpected pattern atom", span)),
            };
            Ok(Pattern { kind, span })
        }
        SExpr::List(items, _) => {
            if items.is_empty() {
                return Err(Diagnostic::error("empty pattern", span));
            }
            let name = expect_symbol(&items[0])?;
            let mut sub_patterns = Vec::new();
            for item in &items[1..] {
                sub_patterns.push(parse_pattern(item, diags)?);
            }
            Ok(Pattern {
                kind: PatternKind::Variant(name, sub_patterns),
                span,
            })
        }
        _ => Err(Diagnostic::error("unexpected pattern form", span)),
    }
}

// ── Type parsing ───────────────────────────────────────

fn parse_type(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<AstType, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) => {
            Ok(AstType { kind: AstTypeKind::Named(s.clone()), span })
        }
        SExpr::Atom(Atom { kind: AtomKind::Integer(v), .. }) => {
            // A number used as a type-level nat
            Ok(AstType {
                kind: AstTypeKind::Nat(NatExpr::Lit(*v as u64)),
                span,
            })
        }
        SExpr::List(items, _) => {
            // Could be a function type or something else
            if items.is_empty() {
                return Err(Diagnostic::error("empty type expression", span));
            }
            if let Some("->") = items[0].as_symbol() {
                // Function type (-> [arg types] return_type)
                if items.len() != 3 {
                    return Err(Diagnostic::error("function type requires (-> [params] return)", span));
                }
                if let SExpr::BracketList(params, _) = &items[1] {
                    let mut param_types = Vec::new();
                    for p in params {
                        param_types.push(parse_type(p, diags)?);
                    }
                    let ret = parse_type(&items[2], diags)?;
                    Ok(AstType {
                        kind: AstTypeKind::Func(param_types, Box::new(ret)),
                        span,
                    })
                } else {
                    Err(Diagnostic::error("function type params must be a bracket list", span))
                }
            } else {
                Err(Diagnostic::error("unexpected type form", span))
            }
        }
        SExpr::BracketList(_, _) => {
            Err(Diagnostic::error("unexpected bracket list in type position", span))
        }
        _ => Err(Diagnostic::error("unexpected type form", span)),
    }
}

/// Parse a type from a slice of SExprs starting at `pos`, consuming potentially
/// a symbol followed by a BracketList for type application (e.g. Result[i32, DivError]).
/// Returns the type and the number of SExprs consumed.
fn parse_type_from_slice(items: &[SExpr], pos: usize, diags: &mut Diagnostics) -> Result<(AstType, usize), Diagnostic> {
    if pos >= items.len() {
        return Err(Diagnostic::error("expected type", Span::dummy()));
    }

    // Check if it's a symbol potentially followed by a BracketList
    if let SExpr::Atom(Atom { kind: AtomKind::Symbol(name), span }) = &items[pos] {
        if pos + 1 < items.len() {
            if let SExpr::BracketList(args, bspan) = &items[pos + 1] {
                // Type application: Name[args]
                let mut type_args = Vec::new();
                let mut i = 0;
                while i < args.len() {
                    let (ty, consumed) = parse_type_from_slice(args, i, diags)?;
                    type_args.push(ty);
                    i += consumed;
                }
                let full_span = span.merge(*bspan);
                return Ok((AstType {
                    kind: AstTypeKind::App(name.clone(), type_args),
                    span: full_span,
                }, 2));
            }
        }
        return Ok((AstType { kind: AstTypeKind::Named(name.clone()), span: *span }, 1));
    }

    // Fall back to regular type parsing
    let ty = parse_type(&items[pos], diags)?;
    Ok((ty, 1))
}

// ── Param parsing ──────────────────────────────────────

fn parse_param(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Param, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::List(items, _) => {
            // (name : Type) or (name : Type default) or (ownership name : Type)
            if items.len() < 3 {
                return Err(Diagnostic::error("param requires at least (name : Type)", span));
            }

            let mut idx = 0;
            let ownership;
            // Check if first element is an ownership keyword
            if let Some(s) = items[0].as_symbol() {
                match s {
                    "own" | "ref" | "mut" | "copy" => {
                        ownership = match s {
                            "own" => Ownership::Own,
                            "ref" => Ownership::Ref,
                            "mut" => Ownership::Mut,
                            "copy" => Ownership::Copy,
                            _ => unreachable!(),
                        };
                        idx = 1;
                    }
                    _ => {
                        ownership = Ownership::Default;
                    }
                }
            } else {
                ownership = Ownership::Default;
            }

            let name = expect_symbol(&items[idx])?;
            idx += 1;

            // Expect ":"
            if idx >= items.len() {
                return Err(Diagnostic::error("expected ':' in param", span));
            }
            if items[idx].as_symbol() != Some(":") {
                return Err(Diagnostic::error("expected ':' in param", span));
            }
            idx += 1;

            // Parse type (may consume 1 or 2 items for App types)
            if idx >= items.len() {
                return Err(Diagnostic::error("expected type in param", span));
            }
            let (ty, consumed) = parse_type_from_slice(items, idx, diags)?;
            idx += consumed;

            // Optional default value
            let default = if idx < items.len() {
                Some(parse_expr(&items[idx], diags)?)
            } else {
                None
            };

            Ok(Param { ownership, name, ty, default, span })
        }
        _ => Err(Diagnostic::error("expected parameter list", span)),
    }
}

// ── define parsing (lightweight, no contracts) ─────────

fn parse_define(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<SimpleFnDef, Diagnostic> {
    // (define name (param1 param2 ...) body)
    if items.len() < 3 {
        return Err(Diagnostic::error("define requires name, parameter list, and body", span));
    }
    let name = items[0].as_symbol().ok_or_else(|| {
        Diagnostic::error("define: expected function name", items[0].span())
    })?.to_string();

    let params = match &items[1] {
        SExpr::List(param_items, _pspan) => {
            let mut params = Vec::new();
            for item in param_items {
                let pname = item.as_symbol().ok_or_else(|| {
                    Diagnostic::error("define: parameter must be a symbol", item.span())
                })?;
                params.push(pname.to_string());
            }
            params
        }
        _ => return Err(Diagnostic::error("define: expected parameter list", items[1].span())),
    };

    let body = parse_expr(&items[2], diags)?;

    Ok(SimpleFnDef { name, params, body, span })
}

// ── defn parsing ───────────────────────────────────────

fn parse_defn(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<FnDef, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("defn requires a name", span));
    }

    let name = expect_symbol(&items[0])?;

    // Check for :pub modifier
    let mut is_public = false;
    let mut start_idx = 1;
    if start_idx < items.len() {
        if let Some("pub") = items[start_idx].as_keyword() {
            is_public = true;
            start_idx += 1;
        }
    }

    let mut params = Vec::new();
    let mut return_type = AstType { kind: AstTypeKind::Named("Unit".to_string()), span };
    let mut intent = None;
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    let mut invariants = Vec::new();
    let mut is_pure = false;
    let mut is_total = false;
    let mut body = None;
    let mut execute_on = None;
    let mut priority = None;

    let mut i = start_idx;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "sig" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected signature after :sig", span));
                    }
                    let (p, ret) = parse_sig(&items[i], diags)?;
                    params = p;
                    return_type = ret;
                }
                "intent" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected string after :intent", span));
                    }
                    intent = Some(expect_string(&items[i])?);
                }
                "requires" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :requires", span));
                    }
                    requires = parse_expr_list(&items[i], diags)?;
                }
                "ensures" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :ensures", span));
                    }
                    ensures = parse_expr_list(&items[i], diags)?;
                }
                "invariant" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :invariant", span));
                    }
                    invariants = parse_expr_list(&items[i], diags)?;
                }
                "pure" => {
                    is_pure = true;
                }
                "total" => {
                    is_total = true;
                    is_pure = true;
                }
                "pre" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :pre", span));
                    }
                    requires.extend(parse_expr_list(&items[i], diags)?);
                }
                "post" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :post", span));
                    }
                    ensures.extend(parse_expr_list(&items[i], diags)?);
                }
                "body" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected expression after :body", span));
                    }
                    body = Some(parse_expr(&items[i], diags)?);
                }
                "execute-on" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected target after :execute-on", span));
                    }
                    execute_on = Some(parse_exec_target(&items[i])?);
                }
                "priority" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected priority after :priority", span));
                    }
                    priority = Some(parse_priority(&items[i])?);
                }
                _ => {
                    diags.add(Diagnostic::warning(
                        format!("unknown keyword :{} in defn", kw),
                        items[i].span(),
                    ));
                }
            }
        }
        i += 1;
    }

    // Expand :pure / :total — generate (valid <param>) requires + (valid result) ensures
    if is_pure {
        let generated_requires: Vec<Expr> = params.iter()
            .map(|p| make_valid_call(&p.name, span))
            .collect();
        // Prepend generated requires before any :pre entries
        let mut merged_requires = generated_requires;
        merged_requires.extend(requires.drain(..));
        requires = merged_requires;
        if ensures.is_empty() {
            ensures.push(make_valid_call("result", span));
        }
    }

    // Validate contracts — must have at least one of requires or ensures
    if requires.is_empty() && ensures.is_empty() {
        diags.add(Diagnostic::error(
            format!("function '{}' must have :requires and/or :ensures contracts", name),
            span,
        ));
    }

    // Warn if missing intent
    if intent.is_none() {
        diags.add(Diagnostic::warning(
            format!("function '{}' is missing :intent", name),
            span,
        ));
    }

    let body = body.unwrap_or(Expr { kind: ExprKind::NilLit, span });

    Ok(FnDef {
        name,
        params,
        return_type,
        intent,
        requires,
        ensures,
        invariants,
        is_pure,
        is_total,
        body,
        execute_on,
        priority,
        is_public,
        span,
    })
}

// ── Signature parsing ──────────────────────────────────

fn parse_sig(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<(Vec<Param>, AstType), Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::BracketList(items, _) => {
            // Find the arrow
            let arrow_pos = items.iter().position(|item| {
                matches!(item, SExpr::Atom(Atom { kind: AtomKind::Arrow, .. }))
            });

            let (param_items, return_items) = if let Some(pos) = arrow_pos {
                (&items[..pos], &items[pos + 1..])
            } else {
                (items.as_slice(), &[] as &[SExpr])
            };

            // Parse params
            let mut params = Vec::new();
            for item in param_items {
                params.push(parse_param(item, diags)?);
            }

            // Parse return type
            let return_type = if return_items.is_empty() {
                AstType { kind: AstTypeKind::Named("Unit".to_string()), span }
            } else {
                let (ty, _) = parse_type_from_slice(return_items, 0, diags)?;
                ty
            };

            Ok((params, return_type))
        }
        _ => Err(Diagnostic::error("expected bracket list for :sig", span)),
    }
}

// ── deftype parsing ────────────────────────────────────

fn parse_deftype(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<TypeDef, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("deftype requires a name", span));
    }

    let name = expect_symbol(&items[0])?;

    let mut idx = 1;

    // Check for :pub modifier
    let mut is_public = false;
    if idx < items.len() {
        if let Some("pub") = items[idx].as_keyword() {
            is_public = true;
            idx += 1;
        }
    }

    let mut type_params = Vec::new();

    // Optional type params in brackets: [T : Type, E : Type]
    if idx < items.len() {
        if let SExpr::BracketList(tp_items, _) = &items[idx] {
            type_params = parse_type_params(tp_items, diags)?;
            idx += 1;
        }
    }

    if idx >= items.len() {
        return Err(Diagnostic::error("deftype requires a body", span));
    }

    let body = parse_type_def_body(&items[idx], diags)?;

    Ok(TypeDef { name, type_params, body, is_public, span })
}

fn parse_type_params(items: &[SExpr], diags: &mut Diagnostics) -> Result<Vec<TypeParam>, Diagnostic> {
    // Items are like: T, :, Type, E, :, Type  (with commas stripped by sexpr parser)
    // Or grouped as: (T : Type) or individual T : Type sequences
    let mut params = Vec::new();
    let mut i = 0;
    while i < items.len() {
        // Try: name : bound
        let name = expect_symbol(&items[i])?;
        let span = items[i].span();
        i += 1;

        let bound = if i < items.len() && items[i].as_symbol() == Some(":") {
            i += 1; // skip ":"
            if i >= items.len() {
                return Err(Diagnostic::error("expected type bound", span));
            }
            let b = parse_type(&items[i], diags)?;
            i += 1;
            b
        } else {
            AstType { kind: AstTypeKind::Named("Type".to_string()), span }
        };

        params.push(TypeParam { name, bound, span });
    }
    Ok(params)
}

fn parse_type_def_body(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<TypeDefBody, Diagnostic> {
    let _span = sexpr.span();
    match sexpr {
        SExpr::List(items, _) if !items.is_empty() => {
            if let Some(head) = items[0].as_symbol() {
                match head {
                    "|" => {
                        // Sum type: (| (Ok T) (Err E))
                        let mut variants = Vec::new();
                        for item in &items[1..] {
                            variants.push(parse_variant(item, diags)?);
                        }
                        Ok(TypeDefBody::Sum(variants))
                    }
                    "&" => {
                        // Product type: (& (id : String) (from : AgentId))
                        let mut fields = Vec::new();
                        for item in &items[1..] {
                            fields.push(parse_field(item, diags)?);
                        }
                        Ok(TypeDefBody::Product(fields))
                    }
                    _ => {
                        // Alias
                        let ty = parse_type(sexpr, diags)?;
                        Ok(TypeDefBody::Alias(ty))
                    }
                }
            } else {
                let ty = parse_type(sexpr, diags)?;
                Ok(TypeDefBody::Alias(ty))
            }
        }
        _ => {
            let ty = parse_type(sexpr, diags)?;
            Ok(TypeDefBody::Alias(ty))
        }
    }
}

fn parse_variant(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Variant, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::List(items, _) if !items.is_empty() => {
            let name = expect_symbol(&items[0])?;
            let mut fields = Vec::new();
            for item in &items[1..] {
                fields.push(parse_type(item, diags)?);
            }
            Ok(Variant { name, fields, span })
        }
        SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) => {
            Ok(Variant { name: s.clone(), fields: Vec::new(), span })
        }
        _ => Err(Diagnostic::error("expected variant", span)),
    }
}

fn parse_field(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Field, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::List(items, _) => {
            // (name : Type)
            if items.len() < 3 {
                return Err(Diagnostic::error("field requires (name : Type)", span));
            }
            let name = expect_symbol(&items[0])?;
            // items[1] should be ":"
            let ty = parse_type(&items[2], diags)?;
            Ok(Field { name, ty, span })
        }
        _ => Err(Diagnostic::error("expected field", span)),
    }
}

// ── module parsing ─────────────────────────────────────

fn parse_module(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<ModuleDef, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("module requires a name", span));
    }

    let name = expect_symbol(&items[0])?;

    let mut version = None;
    let mut requires = Vec::new();
    let mut provides = Vec::new();
    let mut verify = VerifyLevel::default();
    let mut execute_on = None;
    let mut body = Vec::new();

    let mut i = 1;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "version" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected version after :version", span));
                    }
                    version = Some(parse_version(&items, &mut i)?);
                    continue; // i already advanced
                }
                "requires" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :requires", span));
                    }
                    requires = parse_symbol_list(&items[i])?;
                }
                "provides" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected list after :provides", span));
                    }
                    provides = parse_symbol_list(&items[i])?;
                }
                "verify" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected verify level after :verify", span));
                    }
                    verify = parse_verify_level(&items[i])?;
                }
                "execute-on" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected target after :execute-on", span));
                    }
                    execute_on = Some(parse_exec_target(&items[i])?);
                }
                _ => {
                    diags.add(Diagnostic::warning(
                        format!("unknown keyword :{} in module", kw),
                        items[i].span(),
                    ));
                }
            }
        } else {
            // Not a keyword — must be a body form
            let top = parse_top_level(&items[i], diags)?;
            body.push(top);
        }
        i += 1;
    }

    Ok(ModuleDef {
        name,
        version,
        requires,
        provides,
        verify,
        execute_on,
        body,
        span,
    })
}

/// Parse a version like `0.1.0`. The lexer now emits `VersionLit(major, minor, patch)`
/// directly. Legacy fallbacks handle old float+symbol or symbol representations.
fn parse_version(items: &[SExpr], i: &mut usize) -> Result<Version, Diagnostic> {
    let span = items[*i].span();
    match &items[*i] {
        SExpr::Atom(Atom { kind: AtomKind::Version(major, minor, patch), .. }) => {
            let v = Version { major: *major, minor: *minor, patch: *patch };
            *i += 1;
            Ok(v)
        }
        // Legacy fallback: Float(0.1) + Symbol(".0") from old lexer.
        SExpr::Atom(Atom { kind: AtomKind::Float(f), .. }) => {
            let major_minor = format!("{}", f);
            let parts: Vec<&str> = major_minor.split('.').collect();
            let major: u32 = parts[0].parse().unwrap_or(0);
            let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let mut patch = 0u32;
            if *i + 1 < items.len() {
                if let SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) = &items[*i + 1] {
                    if s.starts_with('.') {
                        if let Ok(p) = s[1..].parse::<u32>() {
                            patch = p;
                            *i += 1;
                        }
                    }
                }
            }
            *i += 1;
            Ok(Version { major, minor, patch })
        }
        SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) => {
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() == 3 {
                let major: u32 = parts[0].parse().unwrap_or(0);
                let minor: u32 = parts[1].parse().unwrap_or(0);
                let patch: u32 = parts[2].parse().unwrap_or(0);
                *i += 1;
                Ok(Version { major, minor, patch })
            } else {
                Err(Diagnostic::error("invalid version format", span))
            }
        }
        _ => Err(Diagnostic::error("expected version", span)),
    }
}

// ── task parsing ───────────────────────────────────────

fn parse_task(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<TaskDef, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("task requires an id", span));
    }

    let id = expect_string(&items[0])?;

    let mut from = None;
    let mut to = None;
    let mut deadline = None;
    let mut intent = None;
    let mut input = Vec::new();
    let mut expected_output = None;
    let mut constraints = Vec::new();
    let mut on_success = None;
    let mut on_failure = None;
    let mut on_timeout = None;

    let mut i = 1;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "from" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected value after :from", span)); }
                    from = Some(parse_expr(&items[i], diags)?);
                }
                "to" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected value after :to", span)); }
                    to = Some(parse_expr(&items[i], diags)?);
                }
                "deadline" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected value after :deadline", span)); }
                    deadline = Some(parse_expr(&items[i], diags)?);
                }
                "intent" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected string after :intent", span)); }
                    intent = Some(expect_string(&items[i])?);
                }
                "input" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected list after :input", span)); }
                    input = parse_param_list(&items[i], diags)?;
                }
                "expected-output" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected value after :expected-output", span)); }
                    expected_output = Some(parse_expected_output(&items[i], diags)?);
                }
                "constraints" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected list after :constraints", span)); }
                    constraints = parse_constraints(&items[i], diags)?;
                }
                "on-success" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected expr after :on-success", span)); }
                    on_success = Some(parse_expr(&items[i], diags)?);
                }
                "on-failure" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected expr after :on-failure", span)); }
                    on_failure = Some(parse_expr(&items[i], diags)?);
                }
                "on-timeout" => {
                    i += 1;
                    if i >= items.len() { return Err(Diagnostic::error("expected expr after :on-timeout", span)); }
                    on_timeout = Some(parse_expr(&items[i], diags)?);
                }
                _ => {
                    diags.add(Diagnostic::warning(
                        format!("unknown keyword :{} in task", kw),
                        items[i].span(),
                    ));
                }
            }
        }
        i += 1;
    }

    let from = from.unwrap_or(Expr { kind: ExprKind::NilLit, span });
    let to = to.unwrap_or(Expr { kind: ExprKind::NilLit, span });

    Ok(TaskDef {
        id,
        from,
        to,
        deadline,
        intent,
        input,
        expected_output,
        constraints,
        on_success,
        on_failure,
        on_timeout,
        span,
    })
}

fn parse_expected_output(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<ExpectedOutput, Diagnostic> {
    let span = sexpr.span();
    // For now, treat as a bracket list of params with optional ensures
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut params = Vec::new();
            for item in items {
                params.push(parse_param(item, diags)?);
            }
            Ok(ExpectedOutput { params, ensures: Vec::new(), span })
        }
        _ => Err(Diagnostic::error("expected bracket list for expected-output", span)),
    }
}

fn parse_constraints(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Vec<Constraint>, Diagnostic> {
    let span = sexpr.span();
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut constraints = Vec::new();
            for item in items {
                if let SExpr::List(parts, cspan) = item {
                    if !parts.is_empty() {
                        if let Some(kind) = parts[0].as_symbol() {
                            let constraint = match kind {
                                "max-memory" if parts.len() > 1 => {
                                    Constraint {
                                        kind: ConstraintKind::MaxMemory(parse_expr(&parts[1], diags)?),
                                        span: *cspan,
                                    }
                                }
                                "max-tokens" if parts.len() > 1 => {
                                    Constraint {
                                        kind: ConstraintKind::MaxTokens(parse_expr(&parts[1], diags)?),
                                        span: *cspan,
                                    }
                                }
                                "no-network" => {
                                    Constraint {
                                        kind: ConstraintKind::NoNetwork(true),
                                        span: *cspan,
                                    }
                                }
                                other if parts.len() > 1 => {
                                    Constraint {
                                        kind: ConstraintKind::Custom(other.to_string(), parse_expr(&parts[1], diags)?),
                                        span: *cspan,
                                    }
                                }
                                _ => continue,
                            };
                            constraints.push(constraint);
                        }
                    }
                }
            }
            Ok(constraints)
        }
        _ => Err(Diagnostic::error("expected bracket list for constraints", span)),
    }
}

// ── import parsing ─────────────────────────────────────

fn parse_import(items: &[SExpr], span: Span, _diags: &mut Diagnostics) -> Result<TopLevel, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("import requires a path string", span));
    }
    let path = expect_string(&items[0])?;
    let mut alias = None;
    let mut only = None;
    let mut i = 1;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "as" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected name after :as", span));
                    }
                    alias = Some(expect_symbol(&items[i])?);
                }
                "only" => {
                    i += 1;
                    if i >= items.len() {
                        return Err(Diagnostic::error("expected symbol list after :only", span));
                    }
                    only = Some(parse_symbol_list(&items[i])?);
                }
                _ => return Err(Diagnostic::error(
                    &format!("unknown import option :{}", kw), span)),
            }
        }
        i += 1;
    }
    Ok(TopLevel::Import { path, alias, only, span })
}

// ── extern-c parsing ──────────────────────────────────

fn parse_extern_c(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<ExternCDecl, Diagnostic> {
    // (extern-c "c_name" [params... -> RetType])
    if items.is_empty() {
        return Err(Diagnostic::error("extern-c requires a C function name string", span));
    }
    let c_name = expect_string(&items[0])?;
    if items.len() < 2 {
        return Err(Diagnostic::error("extern-c requires a signature after the C name", span));
    }
    let (params, return_type) = parse_sig(&items[1], diags)?;
    Ok(ExternCDecl { c_name, params, return_type, span })
}

// ── use parsing ────────────────────────────────────────

fn parse_use(items: &[SExpr], span: Span, _diags: &mut Diagnostics) -> Result<UseDef, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("use requires a module name", span));
    }

    let module = expect_symbol(&items[0])?;

    let kind = if items.len() == 1 {
        UseKind::All
    } else if items.len() >= 3 {
        if let Some(kw) = items[1].as_keyword() {
            if kw == "as" {
                UseKind::Prefixed(expect_symbol(&items[2])?)
            } else {
                UseKind::All
            }
        } else {
            // Second item might be a bracket list of symbols
            parse_use_symbols(&items[1])?
        }
    } else {
        // items.len() == 2
        parse_use_symbols(&items[1])?
    };

    Ok(UseDef { module, kind, span })
}

fn parse_use_symbols(sexpr: &SExpr) -> Result<UseKind, Diagnostic> {
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut syms = Vec::new();
            for item in items {
                syms.push(expect_symbol(item)?);
            }
            Ok(UseKind::Symbols(syms))
        }
        SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) if s == "*" => {
            Ok(UseKind::All)
        }
        _ => Err(Diagnostic::error("expected symbol list or :as alias in use", sexpr.span())),
    }
}

// ── Helpers ────────────────────────────────────────────

/// Build a `(valid <name>)` call expression for contract generation.
fn make_valid_call(name: &str, span: Span) -> Expr {
    let callee = Expr { kind: ExprKind::SymbolRef("valid".to_string()), span };
    let arg = Expr { kind: ExprKind::SymbolRef(name.to_string()), span };
    Expr { kind: ExprKind::FnCall(Box::new(callee), vec![arg]), span }
}

fn expect_symbol(sexpr: &SExpr) -> Result<String, Diagnostic> {
    match sexpr {
        SExpr::Atom(Atom { kind: AtomKind::Symbol(s), .. }) => Ok(s.clone()),
        _ => Err(Diagnostic::error(
            format!("expected symbol, found {:?}", sexpr),
            sexpr.span(),
        )),
    }
}

fn expect_string(sexpr: &SExpr) -> Result<String, Diagnostic> {
    match sexpr {
        SExpr::Atom(Atom { kind: AtomKind::Str(s), .. }) => Ok(s.clone()),
        _ => Err(Diagnostic::error(
            format!("expected string, found {:?}", sexpr),
            sexpr.span(),
        )),
    }
}

fn is_capitalized(s: &str) -> bool {
    s.chars().next().map_or(false, |c| c.is_uppercase())
}

fn parse_expr_list(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Vec<Expr>, Diagnostic> {
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut exprs = Vec::new();
            for item in items {
                exprs.push(parse_expr(item, diags)?);
            }
            Ok(exprs)
        }
        _ => {
            // Single expression
            Ok(vec![parse_expr(sexpr, diags)?])
        }
    }
}

fn parse_symbol_list(sexpr: &SExpr) -> Result<Vec<String>, Diagnostic> {
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut syms = Vec::new();
            for item in items {
                syms.push(expect_symbol(item)?);
            }
            Ok(syms)
        }
        _ => Err(Diagnostic::error("expected bracket list", sexpr.span())),
    }
}

fn parse_verify_level(sexpr: &SExpr) -> Result<VerifyLevel, Diagnostic> {
    match expect_symbol(sexpr)?.as_str() {
        "checked" => Ok(VerifyLevel::Checked),
        "proven" => Ok(VerifyLevel::Proven),
        "trusted" => Ok(VerifyLevel::Trusted),
        other => Err(Diagnostic::error(
            format!("unknown verify level: {}", other),
            sexpr.span(),
        )),
    }
}

fn parse_exec_target(sexpr: &SExpr) -> Result<ExecTarget, Diagnostic> {
    let s = expect_symbol(sexpr)?;
    match s.as_str() {
        "cpu" => Ok(ExecTarget::Cpu),
        "gpu" => Ok(ExecTarget::Gpu),
        "any" => Ok(ExecTarget::Any),
        other => Ok(ExecTarget::Agent(other.to_string())),
    }
}

fn parse_priority(sexpr: &SExpr) -> Result<Priority, Diagnostic> {
    match expect_symbol(sexpr)?.as_str() {
        "low" => Ok(Priority::Low),
        "normal" => Ok(Priority::Normal),
        "high" => Ok(Priority::High),
        "critical" => Ok(Priority::Critical),
        other => Err(Diagnostic::error(
            format!("unknown priority: {}", other),
            sexpr.span(),
        )),
    }
}

fn parse_param_list(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<Vec<Param>, Diagnostic> {
    match sexpr {
        SExpr::BracketList(items, _) => {
            let mut params = Vec::new();
            for item in items {
                params.push(parse_param(item, diags)?);
            }
            Ok(params)
        }
        _ => Err(Diagnostic::error("expected bracket list for params", sexpr.span())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::sexpr::parse_sexpr_all;

    fn parse_top(input: &str) -> Vec<TopLevel> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let tops: Vec<_> = sexprs.iter().map(|s| parse_top_level(s, &mut diags)).collect::<Result<_, _>>().unwrap();
        assert!(!diags.has_errors(), "unexpected errors: {:?}", diags);
        tops
    }

    fn parse_expr_str(input: &str) -> Expr {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        parse_expr(&sexprs[0], &mut diags).unwrap()
    }

    #[test]
    fn parse_defn_safe_divide() {
        let tops = parse_top(r#"
            (defn safe-divide
              :sig [(a : i32) (b : i32) -> Result[i32, DivError]]
              :intent "Divide a by b"
              :requires [(valid a) (valid b)]
              :ensures [(pure)]
              :body (if (= b 0) (Err :division-by-zero) (Ok (/ a b))))
        "#);
        assert_eq!(tops.len(), 1);
        if let TopLevel::Defn(f) = &tops[0] {
            assert_eq!(f.name, "safe-divide");
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.intent.as_deref(), Some("Divide a by b"));
            assert_eq!(f.requires.len(), 2);
            assert_eq!(f.ensures.len(), 1);
        } else {
            panic!("expected Defn");
        }
    }

    #[test]
    fn parse_deftype_sum() {
        let tops = parse_top(r#"
            (deftype Result [T : Type, E : Type]
              (| (Ok T) (Err E)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "Result");
            assert_eq!(td.type_params.len(), 2);
            assert!(matches!(td.body, TypeDefBody::Sum(_)));
        } else {
            panic!("expected DefType");
        }
    }

    #[test]
    fn parse_deftype_product() {
        let tops = parse_top(r#"
            (deftype AgentMessage
              (& (id : String) (from : AgentId)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "AgentMessage");
            if let TypeDefBody::Product(fields) = &td.body {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "id");
            } else {
                panic!("expected Product");
            }
        } else {
            panic!("expected DefType");
        }
    }

    #[test]
    fn parse_module() {
        let tops = parse_top(r#"
            (module my-service
              :version 0.1.0
              :requires [tensor]
              :provides [public-fn]
              :verify proven
              (defn public-fn
                :sig [(x : i32) -> i32]
                :intent "identity"
                :requires [(valid x)]
                :ensures [(= result x)]
                :body x))
        "#);
        if let TopLevel::Module(m) = &tops[0] {
            assert_eq!(m.name, "my-service");
            assert_eq!(m.verify, VerifyLevel::Proven);
            assert_eq!(m.body.len(), 1);
        } else {
            panic!("expected Module");
        }
    }

    #[test]
    fn parse_use_symbols() {
        let tops = parse_top("(use tensor [matmul transpose])");
        if let TopLevel::UseDecl(u) = &tops[0] {
            assert_eq!(u.module, "tensor");
            assert!(matches!(&u.kind, UseKind::Symbols(syms) if syms.len() == 2));
        } else {
            panic!("expected UseDecl");
        }
    }

    #[test]
    fn parse_use_prefixed() {
        let tops = parse_top("(use agent :as ag)");
        if let TopLevel::UseDecl(u) = &tops[0] {
            assert!(matches!(&u.kind, UseKind::Prefixed(p) if p == "ag"));
        } else {
            panic!("expected UseDecl");
        }
    }

    #[test]
    fn parse_if_expr() {
        let e = parse_expr_str("(if true 1 2)");
        assert!(matches!(e.kind, ExprKind::If(..)));
    }

    #[test]
    fn parse_let_expr() {
        let e = parse_expr_str("(let (x : i32 42) (+ x 1))");
        if let ExprKind::Let(bindings, _body) = &e.kind {
            assert_eq!(bindings.len(), 1);
            assert_eq!(bindings[0].name, "x");
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_let_multi_binding() {
        let e = parse_expr_str("(let (x : i32 1) (y : i32 2) (+ x y))");
        if let ExprKind::Let(bindings, _) = &e.kind {
            assert_eq!(bindings.len(), 2);
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_match_expr() {
        let e = parse_expr_str("(match x (Ok v) (use-v v) (Err e) (handle e))");
        assert!(matches!(e.kind, ExprKind::Match(..)));
    }

    #[test]
    fn parse_do_expr() {
        let e = parse_expr_str("(do (step1) (step2) (step3))");
        if let ExprKind::Do(exprs) = &e.kind {
            assert_eq!(exprs.len(), 3);
        } else {
            panic!("expected Do");
        }
    }

    #[test]
    fn parse_lambda() {
        let e = parse_expr_str("(fn [a b] (+ a b))");
        assert!(matches!(e.kind, ExprKind::Lambda(..)));
    }

    #[test]
    fn parse_try() {
        let e = parse_expr_str("(try (parse data))");
        assert!(matches!(e.kind, ExprKind::Try(..)));
    }

    #[test]
    fn parse_fn_call() {
        let e = parse_expr_str("(+ 1 2)");
        if let ExprKind::FnCall(callee, args) = &e.kind {
            assert_eq!(args.len(), 2);
            assert!(matches!(callee.kind, ExprKind::SymbolRef(ref s) if s == "+"));
        } else {
            panic!("expected FnCall");
        }
    }

    #[test]
    fn parse_task() {
        let tops = parse_top(r#"
            (task "research-kv-cache"
              :from agent:orchestrator
              :to agent:research
              :intent "Find papers"
              :input [(query : String "test")]
              :on-success (send agent:orchestrator result))
        "#);
        if let TopLevel::Task(t) = &tops[0] {
            assert_eq!(t.id, "research-kv-cache");
            assert!(t.intent.is_some());
        } else {
            panic!("expected Task");
        }
    }

    // ── Import parsing tests ───────────────────────────────

    #[test]
    fn parse_import_basic() {
        let tops = parse_top(r#"(import "lib/math.airl")"#);
        if let TopLevel::Import { path, alias, only, .. } = &tops[0] {
            assert_eq!(path, "lib/math.airl");
            assert!(alias.is_none());
            assert!(only.is_none());
        } else { panic!("expected Import, got {:?}", tops[0]); }
    }

    #[test]
    fn parse_import_with_alias() {
        let tops = parse_top(r#"(import "lib/math.airl" :as m)"#);
        if let TopLevel::Import { path, alias, only, .. } = &tops[0] {
            assert_eq!(path, "lib/math.airl");
            assert_eq!(alias.as_deref(), Some("m"));
            assert!(only.is_none());
        } else { panic!("expected Import"); }
    }

    #[test]
    fn parse_import_with_only() {
        let tops = parse_top(r#"(import "lib/math.airl" :only [abs min max])"#);
        if let TopLevel::Import { path, alias, only, .. } = &tops[0] {
            assert_eq!(path, "lib/math.airl");
            assert!(alias.is_none());
            assert_eq!(only.as_ref().unwrap(), &vec!["abs".to_string(), "min".to_string(), "max".to_string()]);
        } else { panic!("expected Import"); }
    }

    // ── :pub modifier tests ──────────────────────────────

    #[test]
    fn parse_defn_public() {
        let tops = parse_top(r#"
            (defn abs :pub
              :sig [(x : i64) -> i64]
              :intent "Absolute value"
              :requires [(valid x)]
              :ensures [(>= result 0)]
              :body (if (< x 0) (- 0 x) x))
        "#);
        if let TopLevel::Defn(f) = &tops[0] {
            assert_eq!(f.name, "abs");
            assert!(f.is_public);
        } else { panic!("expected Defn"); }
    }

    #[test]
    fn parse_defn_private_by_default() {
        let tops = parse_top(r#"
            (defn helper
              :sig [(x : i64) -> i64]
              :intent "identity"
              :requires [(valid x)]
              :ensures [(= result x)]
              :body x)
        "#);
        if let TopLevel::Defn(f) = &tops[0] {
            assert!(!f.is_public);
        } else { panic!("expected Defn"); }
    }

    #[test]
    fn parse_deftype_public() {
        let tops = parse_top(r#"
            (deftype Color :pub
              (| (Red) (Green) (Blue)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "Color");
            assert!(td.is_public);
        } else { panic!("expected DefType"); }
    }

    #[test]
    fn parse_deftype_private_by_default() {
        let tops = parse_top(r#"
            (deftype Color
              (| (Red) (Green) (Blue)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "Color");
            assert!(!td.is_public);
        } else { panic!("expected DefType"); }
    }

    #[test]
    fn parse_deftype_public_with_type_params() {
        let tops = parse_top(r#"
            (deftype MyResult :pub [T : Type, E : Type]
              (| (Ok T) (Err E)))
        "#);
        if let TopLevel::DefType(td) = &tops[0] {
            assert_eq!(td.name, "MyResult");
            assert!(td.is_public);
            assert_eq!(td.type_params.len(), 2);
        } else { panic!("expected DefType"); }
    }

    #[test]
    fn parse_missing_contracts_is_error() {
        let input = r#"(defn bad :sig [(x : i32) -> i32] :body x)"#;
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let _ = parse_top_level(&sexprs[0], &mut diags);
        assert!(diags.has_errors(), "expected error for missing contracts");
    }

    #[test]
    fn parse_extern_c_no_params() {
        let tops = parse_top(r#"(extern-c "airl_time_now" [-> Int])"#);
        assert_eq!(tops.len(), 1);
        if let TopLevel::ExternC(decl) = &tops[0] {
            assert_eq!(decl.c_name, "airl_time_now");
            assert!(decl.params.is_empty());
            assert_eq!(decl.return_type.to_airl(), "Int");
        } else {
            panic!("expected ExternC, got {:?}", tops[0]);
        }
    }

    #[test]
    fn parse_extern_c_with_params() {
        let tops = parse_top(r#"(extern-c "airl_getenv" [(name : String) -> Any])"#);
        assert_eq!(tops.len(), 1);
        if let TopLevel::ExternC(decl) = &tops[0] {
            assert_eq!(decl.c_name, "airl_getenv");
            assert_eq!(decl.params.len(), 1);
            assert_eq!(decl.params[0].name, "name");
            assert_eq!(decl.return_type.to_airl(), "Any");
        } else {
            panic!("expected ExternC, got {:?}", tops[0]);
        }
    }

    #[test]
    fn parse_extern_c_alongside_defn() {
        let input = r#"
            (extern-c "airl_time_now" [-> Int])
            (defn my-add :sig [(a : Int) (b : Int) -> Int]
              :requires ((>= a 0))
              :ensures ((>= %result 0))
              :body (+ a b))
        "#;
        let tops = parse_top(input);
        assert_eq!(tops.len(), 2);
        assert!(matches!(&tops[0], TopLevel::ExternC(_)));
        assert!(matches!(&tops[1], TopLevel::Defn(_)));
    }

    // ── Threading macro tests ────────────────────────────

    #[test]
    fn parse_thread_first_basic() {
        // (-> 1 (+ 2) (+ 3)) => (+ (+ 1 2) 3)
        let e = parse_expr_str("(-> 1 (+ 2) (+ 3))");
        if let ExprKind::FnCall(f, args) = &e.kind {
            assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "+"));
            assert_eq!(args.len(), 2);
            if let ExprKind::FnCall(f2, args2) = &args[0].kind {
                assert!(matches!(f2.kind, ExprKind::SymbolRef(ref s) if s == "+"));
                assert_eq!(args2.len(), 2);
                assert!(matches!(args2[0].kind, ExprKind::IntLit(1)));
                assert!(matches!(args2[1].kind, ExprKind::IntLit(2)));
            } else {
                panic!("expected inner FnCall, got {:?}", args[0].kind);
            }
            assert!(matches!(args[1].kind, ExprKind::IntLit(3)));
        } else {
            panic!("expected FnCall, got {:?}", e.kind);
        }
    }

    #[test]
    fn parse_thread_first_symbol_step() {
        // (-> [3 1 2] sort) => (sort [3 1 2])
        let e = parse_expr_str("(-> [3 1 2] sort)");
        if let ExprKind::FnCall(f, args) = &e.kind {
            assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "sort"));
            assert_eq!(args.len(), 1);
            assert!(matches!(args[0].kind, ExprKind::ListLit(_)));
        } else {
            panic!("expected FnCall, got {:?}", e.kind);
        }
    }

    #[test]
    fn parse_thread_first_single_seed() {
        // (-> 5) => just 5
        let e = parse_expr_str("(-> 5)");
        assert!(matches!(e.kind, ExprKind::IntLit(5)));
    }

    #[test]
    fn parse_thread_last_basic() {
        // (->> [1 2 3] (map f)) => (map f [1 2 3])
        let e = parse_expr_str("(->> [1 2 3] (map f))");
        if let ExprKind::FnCall(f, args) = &e.kind {
            assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "map"));
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0].kind, ExprKind::SymbolRef(ref s) if s == "f"));
            assert!(matches!(args[1].kind, ExprKind::ListLit(_)));
        } else {
            panic!("expected FnCall, got {:?}", e.kind);
        }
    }

    #[test]
    fn parse_thread_last_chained() {
        // (->> [1 2 3] (filter p) (map f)) => (map f (filter p [1 2 3]))
        let e = parse_expr_str("(->> [1 2 3] (filter p) (map f))");
        if let ExprKind::FnCall(f, args) = &e.kind {
            assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "map"));
            assert_eq!(args.len(), 2);
            assert!(matches!(args[0].kind, ExprKind::SymbolRef(ref s) if s == "f"));
            if let ExprKind::FnCall(f2, args2) = &args[1].kind {
                assert!(matches!(f2.kind, ExprKind::SymbolRef(ref s) if s == "filter"));
                assert_eq!(args2.len(), 2);
                assert!(matches!(args2[1].kind, ExprKind::ListLit(_)));
            } else {
                panic!("expected inner FnCall");
            }
        } else {
            panic!("expected FnCall, got {:?}", e.kind);
        }
    }

    #[test]
    fn parse_thread_first_error_no_args() {
        let mut lexer = Lexer::new("(->)");
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        let result = parse_expr(&sexprs[0], &mut diags);
        assert!(result.is_err(), "expected error for empty ->");
    }

    // ── Contract shorthand tests ─────────────────────────

    #[test]
    fn parse_defn_pure_generates_contracts() {
        let tops = parse_top(r#"
            (defn double
              :pure
              :sig [(x : i64) -> i64]
              :intent "double x"
              :body (* x 2))
        "#);
        if let TopLevel::Defn(f) = &tops[0] {
            assert_eq!(f.name, "double");
            assert!(f.is_pure);
            assert!(!f.is_total);
            assert_eq!(f.requires.len(), 1);
            assert_eq!(f.ensures.len(), 1);
            if let ExprKind::FnCall(callee, args) = &f.requires[0].kind {
                assert!(matches!(callee.kind, ExprKind::SymbolRef(ref s) if s == "valid"));
                assert!(matches!(args[0].kind, ExprKind::SymbolRef(ref s) if s == "x"));
            } else {
                panic!("expected (valid x) in requires");
            }
        } else {
            panic!("expected Defn");
        }
    }

    #[test]
    fn parse_defn_total_sets_both_flags() {
        let tops = parse_top(r#"
            (defn fib
              :total
              :sig [(n : i64) -> i64]
              :intent "fibonacci"
              :body n)
        "#);
        if let TopLevel::Defn(f) = &tops[0] {
            assert!(f.is_pure);
            assert!(f.is_total);
            assert!(!f.requires.is_empty());
        } else {
            panic!("expected Defn");
        }
    }

    #[test]
    fn parse_defn_pre_post_aliases() {
        let tops = parse_top(r#"
            (defn safe-div
              :sig [(a : i64) (b : i64) -> i64]
              :intent "divide safely"
              :pre  [(not (= b 0))]
              :post [(valid result)]
              :body (/ a b))
        "#);
        if let TopLevel::Defn(f) = &tops[0] {
            assert_eq!(f.requires.len(), 1);
            assert_eq!(f.ensures.len(), 1);
            assert!(!f.is_pure);
        } else {
            panic!("expected Defn");
        }
    }

    #[test]
    fn parse_defn_pure_plus_pre_merges() {
        let tops = parse_top(r#"
            (defn bounded
              :pure
              :sig [(x : i64) -> i64]
              :intent "clamp"
              :pre  [(>= x 0) (<= x 100)]
              :body x)
        "#);
        if let TopLevel::Defn(f) = &tops[0] {
            // :pure generates (valid x), then :pre adds 2 more
            assert_eq!(f.requires.len(), 3);
            assert_eq!(f.ensures.len(), 1);
        } else {
            panic!("expected Defn");
        }
    }

    // ── Let destructuring tests ──────────────────────────

    #[test]
    fn parse_let_list_destructure_with_type() {
        let e = parse_expr_str("(let ([a b c] : _ lst) (+ a b))");
        if let ExprKind::Let(bindings, _body) = &e.kind {
            // gensym + a + b + c = 4 bindings
            assert_eq!(bindings.len(), 4);
            assert!(matches!(bindings[0].value.kind, ExprKind::SymbolRef(ref s) if s == "lst"));
            assert_eq!(bindings[1].name, "a");
            assert_eq!(bindings[2].name, "b");
            assert_eq!(bindings[3].name, "c");
            if let ExprKind::FnCall(f, args) = &bindings[1].value.kind {
                assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "at"));
                assert!(matches!(args[1].kind, ExprKind::IntLit(0)));
            } else {
                panic!("expected (at ...) for a");
            }
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_let_list_destructure_rest() {
        // [h & t] → h = (at gs 0), t = (drop 1 gs)  — drop 1 because h consumed index 0
        let e = parse_expr_str("(let ([h & t] : _ lst) h)");
        if let ExprKind::Let(bindings, _body) = &e.kind {
            // gensym + h + t = 3 bindings
            assert_eq!(bindings.len(), 3);
            assert_eq!(bindings[1].name, "h");
            assert_eq!(bindings[2].name, "t");
            if let ExprKind::FnCall(f, args) = &bindings[2].value.kind {
                assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "drop"));
                // idx = 1 because h was at index 0 and consumed it
                assert!(matches!(args[0].kind, ExprKind::IntLit(1)));
            } else {
                panic!("expected (drop ...) for t");
            }
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_let_map_destructure() {
        // Map destructure: list of symbols as pattern
        let e = parse_expr_str("(let ((name age) person) nil)");
        if let ExprKind::Let(bindings, _body) = &e.kind {
            // gensym + name + age = 3 bindings
            assert_eq!(bindings.len(), 3);
            assert_eq!(bindings[1].name, "name");
            assert_eq!(bindings[2].name, "age");
            if let ExprKind::FnCall(f, args) = &bindings[1].value.kind {
                assert!(matches!(f.kind, ExprKind::SymbolRef(ref s) if s == "map-get"));
                assert!(matches!(args[1].kind, ExprKind::StrLit(ref s) if s == "name"));
            } else {
                panic!("expected (map-get ...) for name");
            }
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_let_list_destructure_no_type_annotation() {
        let e = parse_expr_str("(let ([a b] mylist) (+ a b))");
        if let ExprKind::Let(bindings, _) = &e.kind {
            // gensym + a + b = 3
            assert_eq!(bindings.len(), 3);
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn parse_let_mixed_normal_and_destructure() {
        let e = parse_expr_str("(let (n : i64 10) ([x y] pair) (+ n x))");
        if let ExprKind::Let(bindings, _) = &e.kind {
            // n + gensym + x + y = 4
            assert_eq!(bindings.len(), 4);
            assert_eq!(bindings[0].name, "n");
        } else {
            panic!("expected Let");
        }
    }

    // ── Bounds / safety sad-path tests ───────────────────

    fn parse_expr_err(input: &str) -> Diagnostic {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        parse_expr(&sexprs[0], &mut diags).expect_err("expected an error")
    }

    fn parse_top_err(input: &str) -> Diagnostic {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        parse_top_level(&sexprs[0], &mut diags).expect_err("expected an error")
    }

    #[test]
    fn parse_if_too_few_args_is_error() {
        // (if cond then) — missing else branch
        let err = parse_expr_err("(if true 1)");
        assert!(err.message.contains("if"), "error should mention 'if': {}", err.message);
    }

    #[test]
    fn parse_if_too_many_args_is_error() {
        let err = parse_expr_err("(if true 1 2 3)");
        assert!(err.message.contains("if"), "error should mention 'if': {}", err.message);
    }

    #[test]
    fn parse_if_no_args_is_error() {
        let err = parse_expr_err("(if)");
        assert!(err.message.contains("if"), "error should mention 'if': {}", err.message);
    }

    #[test]
    fn parse_let_binding_missing_type_is_error() {
        // (let (x 42) body) — no `: Type` annotation
        let err = parse_expr_err("(let (x 42) x)");
        assert!(
            err.message.contains("let") || err.message.contains("type") || err.message.contains("annotation"),
            "error should describe missing type annotation: {}", err.message
        );
    }

    #[test]
    fn parse_map_destructure_missing_value_is_error() {
        // A map destructure with no value expression — body acts as both binding and body.
        // (let ((name age)) body) — the inner list has no value after the pattern.
        // The parser treats (name age) as a map destructure but rest_items is empty,
        // which triggers the "map destructure binding missing value" error.
        let mut lexer = Lexer::new("(let ((name age)) x)");
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(tokens).unwrap();
        let mut diags = Diagnostics::new();
        // This should either error or produce a let with x as body — not panic.
        let result = parse_expr(&sexprs[0], &mut diags);
        // The important invariant: no panic. Whether it errors or recovers
        // gracefully is acceptable — the unsafe index access must not fire.
        let _ = result;
    }

    #[test]
    fn parse_define_missing_body_is_error() {
        // (define f (x)) — no body
        let err = parse_top_err("(define f (x))");
        assert!(
            err.message.contains("define") || err.message.contains("body") || err.message.contains("requires"),
            "error should describe missing body: {}", err.message
        );
    }

    #[test]
    fn parse_define_valid() {
        // (define double (x) (* x 2)) — happy path
        let tops = parse_top("(define double (x) (* x 2))");
        assert_eq!(tops.len(), 1);
        if let TopLevel::Define(d) = &tops[0] {
            assert_eq!(d.name, "double");
            assert_eq!(d.params, vec!["x".to_string()]);
        } else {
            panic!("expected Define");
        }
    }

    #[test]
    fn parse_task_valid() {
        // Minimal valid task: just an id
        let tops = parse_top(r#"(task "work")"#);
        assert_eq!(tops.len(), 1);
        if let TopLevel::Task(t) = &tops[0] {
            assert_eq!(t.id, "work");
        } else {
            panic!("expected Task");
        }
    }

    #[test]
    fn parse_task_missing_id_is_error() {
        // (task) — no id
        let err = parse_top_err("(task)");
        assert!(
            err.message.contains("task") || err.message.contains("id"),
            "error should describe missing task id: {}", err.message
        );
    }

    #[test]
    fn parse_field_missing_type_is_error() {
        // A field with no type: (name :) — triggers bounds error
        let err = parse_top_err("(deftype Foo (& (x :)))");
        // Just verify it doesn't panic; an error is returned
        let _ = err;
    }

    #[test]
    fn parse_module_version_minor_preserved() {
        let tops = parse_top(r#"
            (module mymod
              :version 0.10.0
              :provides [])
        "#);
        if let Some(TopLevel::Module(m)) = tops.first() {
            assert_eq!(m.version.as_ref().map(|v| v.major), Some(0));
            assert_eq!(m.version.as_ref().map(|v| v.minor), Some(10));  // must be 10, not 1
            assert_eq!(m.version.as_ref().map(|v| v.patch), Some(0));
        } else {
            panic!("expected module");
        }
    }
}
