// crates/airl-runtime/src/ir_marshal.rs
use crate::ir::*;
use crate::value::Value;
use crate::error::RuntimeError;

/// Convert an AIRL Value (produced by the self-hosted compiler) into an IRNode.
pub fn value_to_ir(val: &Value) -> Result<IRNode, RuntimeError> {
    match val {
        Value::Variant(tag, inner) => match tag.as_str() {
            "IRInt" => match inner.as_ref() {
                Value::Int(v) => Ok(IRNode::Int(*v)),
                _ => Err(type_err("IRInt expects Int")),
            },
            "IRFloat" => match inner.as_ref() {
                Value::Float(v) => Ok(IRNode::Float(*v)),
                _ => Err(type_err("IRFloat expects Float")),
            },
            "IRStr" => match inner.as_ref() {
                Value::Str(s) => Ok(IRNode::Str(s.clone())),
                _ => Err(type_err("IRStr expects Str")),
            },
            "IRBool" => match inner.as_ref() {
                Value::Bool(b) => Ok(IRNode::Bool(*b)),
                _ => Err(type_err("IRBool expects Bool")),
            },
            "IRNil" => Ok(IRNode::Nil),
            "IRLoad" => match inner.as_ref() {
                Value::Str(s) => Ok(IRNode::Load(s.clone())),
                _ => Err(type_err("IRLoad expects Str")),
            },
            "IRIf" => {
                let items = expect_list(inner, "IRIf", 3)?;
                Ok(IRNode::If(
                    Box::new(value_to_ir(&items[0])?),
                    Box::new(value_to_ir(&items[1])?),
                    Box::new(value_to_ir(&items[2])?),
                ))
            }
            "IRDo" => {
                let items = expect_inner_list(inner, "IRDo")?;
                let nodes: Vec<IRNode> = items.iter().map(value_to_ir).collect::<Result<_, _>>()?;
                Ok(IRNode::Do(nodes))
            }
            "IRLet" => {
                let items = expect_list(inner, "IRLet", 2)?;
                let bindings = expect_inner_list(&items[0], "IRLet bindings")?;
                let ir_bindings: Vec<IRBinding> = bindings.iter().map(value_to_binding).collect::<Result<_, _>>()?;
                let body = value_to_ir(&items[1])?;
                Ok(IRNode::Let(ir_bindings, Box::new(body)))
            }
            "IRFunc" => {
                let items = expect_list(inner, "IRFunc", 3)?;
                let name = expect_str(&items[0], "IRFunc name")?;
                let params = expect_str_list(&items[1], "IRFunc params")?;
                let body = value_to_ir(&items[2])?;
                Ok(IRNode::Func(name, params, Box::new(body)))
            }
            "IRLambda" => {
                let items = expect_list(inner, "IRLambda", 2)?;
                let params = expect_str_list(&items[0], "IRLambda params")?;
                let body = value_to_ir(&items[1])?;
                Ok(IRNode::Lambda(params, Box::new(body)))
            }
            "IRCall" => {
                let items = expect_list(inner, "IRCall", 2)?;
                let name = expect_str(&items[0], "IRCall name")?;
                let args = expect_inner_list(&items[1], "IRCall args")?;
                let ir_args: Vec<IRNode> = args.iter().map(value_to_ir).collect::<Result<_, _>>()?;
                Ok(IRNode::Call(name, ir_args))
            }
            "IRCallExpr" => {
                let items = expect_list(inner, "IRCallExpr", 2)?;
                let callee = value_to_ir(&items[0])?;
                let args = expect_inner_list(&items[1], "IRCallExpr args")?;
                let ir_args: Vec<IRNode> = args.iter().map(value_to_ir).collect::<Result<_, _>>()?;
                Ok(IRNode::CallExpr(Box::new(callee), ir_args))
            }
            "IRList" => {
                let items = expect_inner_list(inner, "IRList")?;
                let nodes: Vec<IRNode> = items.iter().map(value_to_ir).collect::<Result<_, _>>()?;
                Ok(IRNode::List(nodes))
            }
            "IRVariant" => {
                let items = expect_list(inner, "IRVariant", 2)?;
                let tag = expect_str(&items[0], "IRVariant tag")?;
                let args = expect_inner_list(&items[1], "IRVariant args")?;
                let ir_args: Vec<IRNode> = args.iter().map(value_to_ir).collect::<Result<_, _>>()?;
                Ok(IRNode::Variant(tag, ir_args))
            }
            "IRMatch" => {
                let items = expect_list(inner, "IRMatch", 2)?;
                let scrutinee = value_to_ir(&items[0])?;
                let arms = expect_inner_list(&items[1], "IRMatch arms")?;
                let ir_arms: Vec<IRArm> = arms.iter().map(value_to_arm).collect::<Result<_, _>>()?;
                Ok(IRNode::Match(Box::new(scrutinee), ir_arms))
            }
            "IRTry" => {
                let expr = value_to_ir(inner)?;
                Ok(IRNode::Try(Box::new(expr)))
            }
            "IRBinding" => Err(type_err("IRBinding is not a standalone node")),
            _ => Err(type_err(&format!("unknown IR node tag: {}", tag))),
        },
        _ => Err(type_err(&format!("expected IR variant, got: {}", val))),
    }
}

fn value_to_binding(val: &Value) -> Result<IRBinding, RuntimeError> {
    match val {
        Value::Variant(tag, inner) if tag == "IRBinding" => {
            let items = expect_list(inner, "IRBinding", 2)?;
            let name = expect_str(&items[0], "IRBinding name")?;
            let expr = value_to_ir(&items[1])?;
            Ok(IRBinding { name, expr })
        }
        _ => Err(type_err("expected IRBinding variant")),
    }
}

fn value_to_arm(val: &Value) -> Result<IRArm, RuntimeError> {
    match val {
        Value::Variant(tag, inner) if tag == "IRArm" => {
            let items = expect_list(inner, "IRArm", 2)?;
            let pattern = value_to_pattern(&items[0])?;
            let body = value_to_ir(&items[1])?;
            Ok(IRArm { pattern, body })
        }
        _ => Err(type_err("expected IRArm variant")),
    }
}

pub fn value_to_pattern(val: &Value) -> Result<IRPattern, RuntimeError> {
    match val {
        Value::Variant(tag, inner) => match tag.as_str() {
            "IRPatWild" => Ok(IRPattern::Wild),
            "IRPatBind" => Ok(IRPattern::Bind(expect_str(inner, "IRPatBind")?)),
            "IRPatLit" => Ok(IRPattern::Lit(*inner.clone())),
            "IRPatVariant" => {
                let items = expect_list(inner, "IRPatVariant", 2)?;
                let tag = expect_str(&items[0], "IRPatVariant tag")?;
                let sub_pats = expect_inner_list(&items[1], "IRPatVariant sub-pats")?;
                let pats: Vec<IRPattern> = sub_pats.iter().map(value_to_pattern).collect::<Result<_, _>>()?;
                Ok(IRPattern::Variant(tag, pats))
            }
            _ => Err(type_err(&format!("unknown pattern tag: {}", tag))),
        },
        _ => Err(type_err("expected pattern variant")),
    }
}

// --- helpers ---

fn type_err(msg: &str) -> RuntimeError {
    RuntimeError::TypeError(format!("IR marshal: {}", msg))
}

fn expect_list(val: &Value, ctx: &str, expected_len: usize) -> Result<Vec<Value>, RuntimeError> {
    match val {
        Value::List(items) => {
            if items.len() >= expected_len {
                Ok(items.clone())
            } else {
                Err(type_err(&format!("{}: expected {} items, got {}", ctx, expected_len, items.len())))
            }
        }
        _ => Err(type_err(&format!("{}: expected list", ctx))),
    }
}

fn expect_inner_list(val: &Value, ctx: &str) -> Result<Vec<Value>, RuntimeError> {
    match val {
        Value::List(items) => Ok(items.clone()),
        _ => Err(type_err(&format!("{}: expected list", ctx))),
    }
}

fn expect_str(val: &Value, ctx: &str) -> Result<String, RuntimeError> {
    match val {
        Value::Str(s) => Ok(s.clone()),
        _ => Err(type_err(&format!("{}: expected string", ctx))),
    }
}

fn expect_str_list(val: &Value, ctx: &str) -> Result<Vec<String>, RuntimeError> {
    match val {
        Value::List(items) => items.iter().map(|v| expect_str(v, ctx)).collect(),
        _ => Err(type_err(&format!("{}: expected list of strings", ctx))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_marshal_int() {
        let val = Value::Variant("IRInt".into(), Box::new(Value::Int(42)));
        let node = value_to_ir(&val).unwrap();
        match node {
            IRNode::Int(42) => {}
            _ => panic!("expected IRNode::Int(42)"),
        }
    }

    #[test]
    fn test_marshal_if() {
        let val = Value::Variant(
            "IRIf".into(),
            Box::new(Value::List(vec![
                Value::Variant("IRBool".into(), Box::new(Value::Bool(true))),
                Value::Variant("IRInt".into(), Box::new(Value::Int(1))),
                Value::Variant("IRInt".into(), Box::new(Value::Int(2))),
            ])),
        );
        let node = value_to_ir(&val).unwrap();
        let mut vm = crate::ir_vm::IrVm::new();
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(1));
    }

    #[test]
    fn test_marshal_call() {
        let val = Value::Variant(
            "IRCall".into(),
            Box::new(Value::List(vec![
                Value::Str("+".into()),
                Value::List(vec![
                    Value::Variant("IRInt".into(), Box::new(Value::Int(3))),
                    Value::Variant("IRInt".into(), Box::new(Value::Int(4))),
                ]),
            ])),
        );
        let node = value_to_ir(&val).unwrap();
        let mut vm = crate::ir_vm::IrVm::new();
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(7));
    }

    #[test]
    fn test_marshal_pattern() {
        let val = Value::Variant("IRPatBind".into(), Box::new(Value::Str("x".into())));
        let pat = value_to_pattern(&val).unwrap();
        match pat {
            IRPattern::Bind(name) => assert_eq!(name, "x"),
            _ => panic!("expected Bind"),
        }
    }
}
