// crates/airl-runtime/src/ast_to_ir.rs
//! AST → IR lowering.  Converts `airl_syntax` AST nodes into the
//! `IRNode` representation consumed by `BytecodeCompiler`.

use airl_syntax::ast::*;
use crate::ir::*;
use crate::value::Value;

pub fn compile_expr(expr: &Expr) -> IRNode {
    match &expr.kind {
        ExprKind::IntLit(v) => IRNode::Int(*v),
        ExprKind::FloatLit(v) => IRNode::Float(*v),
        ExprKind::StrLit(s) => IRNode::Str(s.clone()),
        ExprKind::BoolLit(b) => IRNode::Bool(*b),
        ExprKind::NilLit => IRNode::Nil,
        ExprKind::SymbolRef(name) => IRNode::Load(name.clone()),
        ExprKind::KeywordLit(k) => IRNode::Str(format!(":{}", k)),

        ExprKind::If(cond, then_, else_) => IRNode::If(
            Box::new(compile_expr(cond)),
            Box::new(compile_expr(then_)),
            Box::new(compile_expr(else_)),
        ),

        ExprKind::Let(bindings, body) => {
            let ir_bindings: Vec<IRBinding> = bindings.iter().map(|b| {
                IRBinding { name: b.name.clone(), expr: compile_expr(&b.value) }
            }).collect();
            IRNode::Let(ir_bindings, Box::new(compile_expr(body)))
        }

        ExprKind::Do(exprs) => IRNode::Do(exprs.iter().map(compile_expr).collect()),

        ExprKind::FnCall(callee, args) => {
            let ir_args: Vec<IRNode> = args.iter().map(compile_expr).collect();
            if let ExprKind::SymbolRef(name) = &callee.kind {
                IRNode::Call(name.clone(), ir_args)
            } else {
                IRNode::CallExpr(Box::new(compile_expr(callee)), ir_args)
            }
        }

        ExprKind::Lambda(params, body) => {
            let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            IRNode::Lambda(param_names, Box::new(compile_expr(body)))
        }

        ExprKind::Match(scrutinee, arms) => {
            let ir_arms: Vec<IRArm> = arms.iter().map(|arm| {
                IRArm { pattern: compile_pattern(&arm.pattern), body: compile_expr(&arm.body) }
            }).collect();
            IRNode::Match(Box::new(compile_expr(scrutinee)), ir_arms)
        }

        ExprKind::ListLit(items) => IRNode::List(items.iter().map(compile_expr).collect()),

        ExprKind::VariantCtor(name, args) => {
            IRNode::Variant(name.clone(), args.iter().map(compile_expr).collect())
        }

        ExprKind::Try(inner) => IRNode::Try(Box::new(compile_expr(inner))),

        ExprKind::StructLit(name, fields) => {
            let mut items = Vec::new();
            for (key, val) in fields {
                items.push(IRNode::List(vec![IRNode::Str(key.clone()), compile_expr(val)]));
            }
            IRNode::Variant(name.clone(), vec![IRNode::List(items)])
        }

        ExprKind::Forall(..) | ExprKind::Exists(..) => {
            let is_forall = matches!(&expr.kind, ExprKind::Forall(..));
            let (param, where_clause, body) = match &expr.kind {
                ExprKind::Forall(p, w, b) | ExprKind::Exists(p, w, b) => (p, w, b),
                _ => unreachable!(),
            };
            let var_name = param.name.clone();
            let acc_name = "__quant_acc".to_string();
            let upper_bound = match where_clause {
                Some(w) => extract_upper_bound(w, &var_name).unwrap_or_else(|| compile_expr(w)),
                None => IRNode::Int(10000),
            };
            let compiled_body = compile_expr(body);
            let fold_body = if is_forall {
                IRNode::If(
                    Box::new(IRNode::Call("not".to_string(), vec![IRNode::Load(acc_name.clone())])),
                    Box::new(IRNode::Bool(false)),
                    Box::new(compiled_body),
                )
            } else {
                IRNode::If(
                    Box::new(IRNode::Load(acc_name.clone())),
                    Box::new(IRNode::Bool(true)),
                    Box::new(compiled_body),
                )
            };
            let callback = IRNode::Lambda(vec![acc_name, var_name], Box::new(fold_body));
            let init = if is_forall { IRNode::Bool(true) } else { IRNode::Bool(false) };
            let range_expr = IRNode::Call("range".to_string(), vec![IRNode::Int(0), upper_bound]);
            IRNode::Call("fold".to_string(), vec![callback, init, range_expr])
        }
    }
}

pub fn compile_pattern(pat: &Pattern) -> IRPattern {
    match &pat.kind {
        PatternKind::Wildcard => IRPattern::Wild,
        PatternKind::Binding(name) => IRPattern::Bind(name.clone()),
        PatternKind::Literal(lit) => {
            let val = match lit {
                LitPattern::Int(v) => Value::Int(*v),
                LitPattern::Float(v) => Value::Float(*v),
                LitPattern::Str(s) => Value::Str(s.clone()),
                LitPattern::Bool(b) => Value::Bool(*b),
                LitPattern::Nil => Value::Nil,
            };
            IRPattern::Lit(val)
        }
        PatternKind::Variant(name, sub_pats) => {
            IRPattern::Variant(name.clone(), sub_pats.iter().map(compile_pattern).collect())
        }
    }
}

pub fn compile_top_level(top: &TopLevel) -> IRNode {
    match top {
        TopLevel::Defn(f) => {
            let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
            IRNode::Func(f.name.clone(), param_names, Box::new(compile_expr(&f.body)))
        }
        TopLevel::Expr(e) => compile_expr(e),
        TopLevel::ExternC(_) => IRNode::Nil, // no body to compile — handled by AOT
        _ => IRNode::Nil,
    }
}

fn extract_upper_bound(where_expr: &Expr, var_name: &str) -> Option<IRNode> {
    if let ExprKind::FnCall(callee, args) = &where_expr.kind {
        if let ExprKind::SymbolRef(op) = &callee.kind {
            if args.len() == 2 {
                if let ExprKind::SymbolRef(ref name) = &args[0].kind {
                    if name == var_name {
                        if op == "<" {
                            return Some(compile_expr(&args[1]));
                        } else if op == "<=" {
                            return Some(IRNode::Call("+".to_string(), vec![
                                compile_expr(&args[1]), IRNode::Int(1),
                            ]));
                        }
                    }
                }
            }
        }
    }
    None
}
