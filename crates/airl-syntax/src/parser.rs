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
                    "deftype" => parse_deftype(&items[1..], *span, diags).map(TopLevel::DefType),
                    "module" => parse_module(&items[1..], *span, diags).map(TopLevel::Module),
                    "task" => parse_task(&items[1..], *span, diags).map(TopLevel::Task),
                    "use" => parse_use(&items[1..], *span, diags).map(TopLevel::UseDecl),
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

fn is_let_binding(sexpr: &SExpr) -> bool {
    // A binding is a list of 4+ elements where the second element is ":"
    if let SExpr::List(items, _) = sexpr {
        if items.len() >= 4 {
            if let Some(s) = items[1].as_symbol() {
                return s == ":";
            }
        }
    }
    false
}

fn parse_let_binding(sexpr: &SExpr, diags: &mut Diagnostics) -> Result<LetBinding, Diagnostic> {
    let span = sexpr.span();
    if let SExpr::List(items, _) = sexpr {
        // (name : Type value)
        if items.len() < 4 {
            return Err(Diagnostic::error("let binding requires (name : Type value)", span));
        }
        let name = expect_symbol(&items[0])?;
        // items[1] is ":"
        let ty = parse_type(&items[2], diags)?;
        let value = parse_expr(&items[3], diags)?;
        Ok(LetBinding { name, ty, value, span })
    } else {
        Err(Diagnostic::error("expected let binding list", span))
    }
}

fn parse_let_expr(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<Expr, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("let requires at least a body", span));
    }

    let mut bindings = Vec::new();
    let mut i = 0;
    while i < items.len() && is_let_binding(&items[i]) {
        bindings.push(parse_let_binding(&items[i], diags)?);
        i += 1;
    }

    if i >= items.len() {
        return Err(Diagnostic::error("let requires a body expression", span));
    }

    let body = parse_expr(&items[i], diags)?;
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

// ── defn parsing ───────────────────────────────────────

fn parse_defn(items: &[SExpr], span: Span, diags: &mut Diagnostics) -> Result<FnDef, Diagnostic> {
    if items.is_empty() {
        return Err(Diagnostic::error("defn requires a name", span));
    }

    let name = expect_symbol(&items[0])?;

    let mut params = Vec::new();
    let mut return_type = AstType { kind: AstTypeKind::Named("Unit".to_string()), span };
    let mut intent = None;
    let mut requires = Vec::new();
    let mut ensures = Vec::new();
    let mut invariants = Vec::new();
    let mut body = None;
    let mut execute_on = None;
    let mut priority = None;

    let mut i = 1;
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
                    // Unknown keyword, skip
                }
            }
        }
        i += 1;
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
        body,
        execute_on,
        priority,
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

    Ok(TypeDef { name, type_params, body, span })
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
                _ => {}
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

/// Parse a version like `0.1.0` which the lexer produces as Float(0.1) Symbol(".0")
/// or potentially as a symbol "0.1.0" depending on how the lexer handles it.
fn parse_version(items: &[SExpr], i: &mut usize) -> Result<Version, Diagnostic> {
    let span = items[*i].span();

    // The lexer produces 0.1.0 as Float(0.1) then Symbol(".0")
    // Or it could be a single symbol if the lexer treats it differently
    match &items[*i] {
        SExpr::Atom(Atom { kind: AtomKind::Float(f), .. }) => {
            // e.g., 0.1 — check if next item is like ".0"
            let major_minor = format!("{}", f);
            let parts: Vec<&str> = major_minor.split('.').collect();
            let major: u32 = parts[0].parse().unwrap_or(0);
            let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

            // Check for .patch
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
                _ => {}
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
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let tops: Vec<_> = sexprs.iter().map(|s| parse_top_level(s, &mut diags)).collect::<Result<_, _>>().unwrap();
        assert!(!diags.has_errors(), "unexpected errors: {:?}", diags);
        tops
    }

    fn parse_expr_str(input: &str) -> Expr {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
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

    #[test]
    fn parse_missing_contracts_is_error() {
        let input = r#"(defn bad :sig [(x : i32) -> i32] :body x)"#;
        let mut lexer = Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = parse_sexpr_all(&tokens).unwrap();
        let mut diags = Diagnostics::new();
        let _ = parse_top_level(&sexprs[0], &mut diags);
        assert!(diags.has_errors(), "expected error for missing contracts");
    }
}
