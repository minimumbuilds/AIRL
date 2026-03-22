// crates/airl-runtime/src/ir_vm.rs
use std::collections::HashMap;
use crate::ir::*;
use crate::value::Value;
use crate::builtins::Builtins;
use crate::error::RuntimeError;

pub struct IrVm {
    env: Vec<HashMap<String, Value>>,
    pub functions: HashMap<String, IRFunc>,
    builtins: Builtins,
    recursion_depth: usize,
}

impl IrVm {
    pub fn new() -> Self {
        IrVm {
            env: vec![HashMap::new()],
            functions: HashMap::new(),
            builtins: Builtins::new(),
            recursion_depth: 0,
        }
    }

    fn push_frame(&mut self) {
        self.env.push(HashMap::new());
    }

    fn pop_frame(&mut self) {
        self.env.pop();
    }

    fn env_bind(&mut self, name: &str, val: Value) {
        if let Some(frame) = self.env.last_mut() {
            frame.insert(name.to_string(), val);
        }
    }

    fn env_lookup(&self, name: &str) -> Result<Value, RuntimeError> {
        for frame in self.env.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Ok(val.clone());
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }

    pub fn exec(&mut self, node: &IRNode) -> Result<Value, RuntimeError> {
        match node {
            IRNode::Int(v) => Ok(Value::Int(*v)),
            IRNode::Float(v) => Ok(Value::Float(*v)),
            IRNode::Str(s) => Ok(Value::Str(s.clone())),
            IRNode::Bool(b) => Ok(Value::Bool(*b)),
            IRNode::Nil => Ok(Value::Nil),

            IRNode::Load(name) => self.env_lookup(name),

            IRNode::If(cond, then_, else_) => {
                let cond_val = self.exec(cond)?;
                match cond_val {
                    Value::Bool(true) => self.exec(then_),
                    Value::Bool(false) => self.exec(else_),
                    _ => Err(RuntimeError::TypeError(
                        "if: condition must be Bool".to_string(),
                    )),
                }
            }

            IRNode::Do(exprs) => {
                let mut result = Value::Nil;
                for expr in exprs {
                    result = self.exec(expr)?;
                }
                Ok(result)
            }

            IRNode::Let(bindings, body) => {
                self.push_frame();
                for binding in bindings {
                    let val = self.exec(&binding.expr)?;
                    self.env_bind(&binding.name, val);
                }
                let result = self.exec(body);
                self.pop_frame();
                result
            }

            IRNode::List(items) => {
                let vals: Vec<Value> = items
                    .iter()
                    .map(|item| self.exec(item))
                    .collect::<Result<_, _>>()?;
                Ok(Value::List(vals))
            }

            IRNode::Variant(tag, args) => {
                let vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.exec(a))
                    .collect::<Result<_, _>>()?;
                match vals.len() {
                    0 => Ok(Value::Variant(tag.clone(), Box::new(Value::Nil))),
                    1 => Ok(Value::Variant(tag.clone(), Box::new(vals.into_iter().next().unwrap()))),
                    _ => Ok(Value::Variant(tag.clone(), Box::new(Value::List(vals)))),
                }
            }

            IRNode::Try(expr) => {
                let val = self.exec(expr)?;
                match val {
                    Value::Variant(ref tag, ref inner) if tag == "Ok" => Ok(*inner.clone()),
                    Value::Variant(ref tag, ref inner) if tag == "Err" => {
                        Err(RuntimeError::Custom(format!("{}", inner)))
                    }
                    _ => Err(RuntimeError::TryOnNonResult(format!("{}", val))),
                }
            }

            // Stubs for function/match/lambda — implemented in Task 3
            _ => Err(RuntimeError::Custom(
                "IR VM: unimplemented node type".to_string(),
            )),
        }
    }
}

impl Default for IrVm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literals() {
        let mut vm = IrVm::new();
        assert_eq!(vm.exec(&IRNode::Int(42)).unwrap(), Value::Int(42));
        assert_eq!(vm.exec(&IRNode::Float(3.14)).unwrap(), Value::Float(3.14));
        assert_eq!(
            vm.exec(&IRNode::Str("hi".into())).unwrap(),
            Value::Str("hi".into())
        );
        assert_eq!(vm.exec(&IRNode::Bool(true)).unwrap(), Value::Bool(true));
        assert_eq!(vm.exec(&IRNode::Nil).unwrap(), Value::Nil);
    }

    #[test]
    fn test_if() {
        let mut vm = IrVm::new();
        let node = IRNode::If(
            Box::new(IRNode::Bool(true)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(1));
    }

    #[test]
    fn test_if_false_branch() {
        let mut vm = IrVm::new();
        let node = IRNode::If(
            Box::new(IRNode::Bool(false)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(2));
    }

    #[test]
    fn test_let() {
        let mut vm = IrVm::new();
        let node = IRNode::Let(
            vec![IRBinding {
                name: "x".into(),
                expr: IRNode::Int(42),
            }],
            Box::new(IRNode::Load("x".into())),
        );
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(42));
    }

    #[test]
    fn test_do() {
        let mut vm = IrVm::new();
        let node = IRNode::Do(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]);
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(3));
    }

    #[test]
    fn test_do_empty() {
        let mut vm = IrVm::new();
        let node = IRNode::Do(vec![]);
        assert_eq!(vm.exec(&node).unwrap(), Value::Nil);
    }

    #[test]
    fn test_list() {
        let mut vm = IrVm::new();
        let node = IRNode::List(vec![IRNode::Int(1), IRNode::Int(2)]);
        match vm.exec(&node).unwrap() {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_variant() {
        let mut vm = IrVm::new();
        let node = IRNode::Variant("Ok".into(), vec![IRNode::Int(42)]);
        match vm.exec(&node).unwrap() {
            Value::Variant(tag, inner) => {
                assert_eq!(tag, "Ok");
                assert_eq!(*inner, Value::Int(42));
            }
            _ => panic!("expected variant"),
        }
    }

    #[test]
    fn test_variant_no_args() {
        let mut vm = IrVm::new();
        let node = IRNode::Variant("None".into(), vec![]);
        match vm.exec(&node).unwrap() {
            Value::Variant(tag, inner) => {
                assert_eq!(tag, "None");
                assert_eq!(*inner, Value::Nil);
            }
            _ => panic!("expected variant"),
        }
    }

    #[test]
    fn test_try_ok() {
        let mut vm = IrVm::new();
        let node = IRNode::Try(Box::new(IRNode::Variant(
            "Ok".into(),
            vec![IRNode::Int(7)],
        )));
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(7));
    }

    #[test]
    fn test_try_err() {
        let mut vm = IrVm::new();
        let node = IRNode::Try(Box::new(IRNode::Variant(
            "Err".into(),
            vec![IRNode::Str("bad".into())],
        )));
        assert!(vm.exec(&node).is_err());
    }

    #[test]
    fn test_load_undefined() {
        let mut vm = IrVm::new();
        let result = vm.exec(&IRNode::Load("no_such_var".into()));
        assert!(matches!(result, Err(RuntimeError::UndefinedSymbol(_))));
    }

    #[test]
    fn test_let_scope_cleanup() {
        let mut vm = IrVm::new();
        // After the let, `x` should not be visible
        let node = IRNode::Let(
            vec![IRBinding {
                name: "x".into(),
                expr: IRNode::Int(10),
            }],
            Box::new(IRNode::Load("x".into())),
        );
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(10));
        // Now x should be gone
        assert!(vm.exec(&IRNode::Load("x".into())).is_err());
    }

    #[test]
    fn test_if_non_bool_error() {
        let mut vm = IrVm::new();
        let node = IRNode::If(
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
            Box::new(IRNode::Int(3)),
        );
        assert!(matches!(vm.exec(&node), Err(RuntimeError::TypeError(_))));
    }
}
