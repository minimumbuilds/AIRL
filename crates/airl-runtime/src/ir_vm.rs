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

enum TcoResult {
    Value(Value),
    TailCall(Vec<Value>),
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

            IRNode::Load(name) => {
                match self.env_lookup(name) {
                    Ok(val) => Ok(val),
                    Err(_) => {
                        // Fall back: if name is a known function, return a reference
                        if self.functions.contains_key(name.as_str()) {
                            Ok(Value::IRFuncRef(name.clone()))
                        } else if self.builtins.get(name).is_some() {
                            Ok(Value::BuiltinFn(name.clone()))
                        } else {
                            Err(RuntimeError::UndefinedSymbol(name.clone()))
                        }
                    }
                }
            }

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

            IRNode::Func(name, params, body) => {
                let func = IRFunc {
                    name: name.clone(),
                    params: params.clone(),
                    body: *body.clone(),
                };
                self.functions.insert(name.clone(), func);
                Ok(Value::Nil)
            }

            IRNode::Lambda(params, body) => {
                let mut captured = vec![];
                for frame in self.env.iter().rev() {
                    for (k, v) in frame {
                        captured.push((k.clone(), v.clone()));
                    }
                }
                Ok(Value::IRClosure(IRClosureValue {
                    params: params.clone(),
                    body: body.clone(),
                    captured_env: captured,
                }))
            }

            IRNode::Call(name, args) => {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.exec(a))
                    .collect::<Result<_, _>>()?;
                self.call_function(name, arg_vals)
            }

            IRNode::CallExpr(callee_expr, args) => {
                let callee = self.exec(callee_expr)?;
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.exec(a))
                    .collect::<Result<_, _>>()?;
                match callee {
                    Value::IRClosure(closure) => {
                        self.push_frame();
                        for (name, val) in &closure.captured_env {
                            self.env_bind(name, val.clone());
                        }
                        for (param, val) in closure.params.iter().zip(arg_vals) {
                            self.env_bind(&param, val);
                        }
                        let result = self.exec(&closure.body);
                        self.pop_frame();
                        result
                    }
                    Value::IRFuncRef(name) => self.call_function(&name, arg_vals),
                    Value::BuiltinFn(name) => {
                        if let Some(func) = self.builtins.get(&name) {
                            func(&arg_vals)
                        } else {
                            Err(RuntimeError::UndefinedSymbol(name))
                        }
                    }
                    _ => Err(RuntimeError::NotCallable(format!("{}", callee))),
                }
            }

            IRNode::Match(scrutinee, arms) => {
                let scr_val = self.exec(scrutinee)?;
                for arm in arms {
                    if let Some(bindings) = self.try_match_pattern(&arm.pattern, &scr_val) {
                        self.push_frame();
                        for (name, val) in bindings {
                            self.env_bind(&name, val);
                        }
                        let result = self.exec(&arm.body);
                        self.pop_frame();
                        return result;
                    }
                }
                Err(RuntimeError::NonExhaustiveMatch {
                    value: format!("{}", scr_val),
                })
            }
        }
    }

    pub fn exec_program(&mut self, nodes: &[IRNode]) -> Result<Value, RuntimeError> {
        let mut result = Value::Nil;
        for node in nodes {
            match node {
                IRNode::Func(name, params, body) => {
                    let func = IRFunc {
                        name: name.clone(),
                        params: params.clone(),
                        body: *body.clone(),
                    };
                    self.functions.insert(name.clone(), func);
                }
                _ => {
                    result = self.exec(node)?;
                }
            }
        }
        Ok(result)
    }

    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        // Try builtin first
        if let Some(func) = self.builtins.get(name) {
            return func(&args);
        }
        // User-defined function with self-TCO
        if let Some(func) = self.functions.get(name).cloned() {
            self.recursion_depth += 1;
            if self.recursion_depth > 50_000 {
                self.recursion_depth -= 1;
                return Err(RuntimeError::Custom("stack overflow".into()));
            }
            let mut current_args = args;
            let result = loop {
                self.push_frame();
                for (param, val) in func.params.iter().zip(current_args.iter()) {
                    self.env_bind(param, val.clone());
                }
                self.env_bind(name, Value::IRFuncRef(name.to_string()));
                match self.exec_tco(&func.body, name)? {
                    TcoResult::Value(v) => {
                        self.pop_frame();
                        break Ok(v);
                    }
                    TcoResult::TailCall(new_args) => {
                        self.pop_frame();
                        current_args = new_args;
                    }
                }
            };
            self.recursion_depth -= 1;
            return result;
        }
        // Check environment for closures / function refs bound via let
        if let Ok(val) = self.env_lookup(name) {
            match val {
                Value::IRClosure(closure) => {
                    self.push_frame();
                    for (k, v) in &closure.captured_env {
                        self.env_bind(k, v.clone());
                    }
                    for (param, arg) in closure.params.iter().zip(args) {
                        self.env_bind(param, arg);
                    }
                    let result = self.exec(&closure.body);
                    self.pop_frame();
                    return result;
                }
                Value::IRFuncRef(ref_name) => {
                    return self.call_function(&ref_name, args);
                }
                Value::BuiltinFn(ref bname) => {
                    if let Some(func) = self.builtins.get(bname) {
                        return func(&args);
                    }
                }
                _ => {}
            }
        }
        Err(RuntimeError::UndefinedSymbol(name.to_string()))
    }

    fn exec_tco(&mut self, node: &IRNode, fn_name: &str) -> Result<TcoResult, RuntimeError> {
        match node {
            IRNode::If(cond, then_, else_) => {
                let cond_val = self.exec(cond)?;
                match cond_val {
                    Value::Bool(true) => self.exec_tco(then_, fn_name),
                    Value::Bool(false) => self.exec_tco(else_, fn_name),
                    _ => Err(RuntimeError::TypeError("if: condition must be Bool".into())),
                }
            }
            IRNode::Do(exprs) => {
                if exprs.is_empty() {
                    return Ok(TcoResult::Value(Value::Nil));
                }
                for expr in &exprs[..exprs.len() - 1] {
                    self.exec(expr)?;
                }
                self.exec_tco(exprs.last().unwrap(), fn_name)
            }
            IRNode::Let(bindings, body) => {
                self.push_frame();
                for binding in bindings {
                    let val = self.exec(&binding.expr)?;
                    self.env_bind(&binding.name, val);
                }
                let result = self.exec_tco(body, fn_name);
                self.pop_frame();
                result
            }
            IRNode::Match(scrutinee, arms) => {
                let scr_val = self.exec(scrutinee)?;
                for arm in arms {
                    if let Some(bindings) = self.try_match_pattern(&arm.pattern, &scr_val) {
                        self.push_frame();
                        for (name, val) in bindings {
                            self.env_bind(&name, val);
                        }
                        let result = self.exec_tco(&arm.body, fn_name);
                        self.pop_frame();
                        return result;
                    }
                }
                Err(RuntimeError::NonExhaustiveMatch {
                    value: format!("{}", scr_val),
                })
            }
            IRNode::Call(callee_name, args) if callee_name == fn_name => {
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.exec(a))
                    .collect::<Result<_, _>>()?;
                Ok(TcoResult::TailCall(arg_vals))
            }
            _ => Ok(TcoResult::Value(self.exec(node)?)),
        }
    }

    fn try_match_pattern(&self, pattern: &IRPattern, value: &Value) -> Option<Vec<(String, Value)>> {
        match pattern {
            IRPattern::Wild => Some(vec![]),
            IRPattern::Bind(name) => Some(vec![(name.clone(), value.clone())]),
            IRPattern::Lit(lit_val) => {
                if value == lit_val {
                    Some(vec![])
                } else {
                    None
                }
            }
            IRPattern::Variant(tag, sub_pats) => match value {
                Value::Variant(vtag, inner) if vtag == tag => {
                    if sub_pats.is_empty() {
                        Some(vec![])
                    } else if sub_pats.len() == 1 {
                        self.try_match_pattern(&sub_pats[0], inner)
                    } else {
                        match inner.as_ref() {
                            Value::List(items) if items.len() == sub_pats.len() => {
                                let mut bindings = vec![];
                                for (pat, val) in sub_pats.iter().zip(items) {
                                    match self.try_match_pattern(pat, val) {
                                        Some(bs) => bindings.extend(bs),
                                        None => return None,
                                    }
                                }
                                Some(bindings)
                            }
                            _ => None,
                        }
                    }
                }
                _ => None,
            },
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

    #[test]
    fn test_function_call() {
        let mut vm = IrVm::new();
        let prog = vec![
            IRNode::Func(
                "double".into(),
                vec!["x".into()],
                Box::new(IRNode::Call(
                    "*".into(),
                    vec![IRNode::Load("x".into()), IRNode::Int(2)],
                )),
            ),
        ];
        vm.exec_program(&prog).unwrap();
        let result = vm.exec(&IRNode::Call("double".into(), vec![IRNode::Int(21)])).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn test_recursion() {
        let mut vm = IrVm::new();
        let fact_body = IRNode::If(
            Box::new(IRNode::Call("<=".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)])),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Call("*".into(), vec![
                IRNode::Load("n".into()),
                IRNode::Call("fact".into(), vec![
                    IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
                ]),
            ])),
        );
        vm.exec_program(&[IRNode::Func("fact".into(), vec!["n".into()], Box::new(fact_body))]).unwrap();
        let result = vm.exec(&IRNode::Call("fact".into(), vec![IRNode::Int(5)])).unwrap();
        assert_eq!(result, Value::Int(120));
    }

    #[test]
    fn test_match() {
        let mut vm = IrVm::new();
        let node = IRNode::Match(
            Box::new(IRNode::Variant("Ok".into(), vec![IRNode::Int(42)])),
            vec![
                IRArm {
                    pattern: IRPattern::Variant("Ok".into(), vec![IRPattern::Bind("v".into())]),
                    body: IRNode::Load("v".into()),
                },
                IRArm {
                    pattern: IRPattern::Wild,
                    body: IRNode::Int(0),
                },
            ],
        );
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(42));
    }

    #[test]
    fn test_lambda() {
        let mut vm = IrVm::new();
        let node = IRNode::CallExpr(
            Box::new(IRNode::Lambda(
                vec!["x".into()],
                Box::new(IRNode::Call("+".into(), vec![IRNode::Load("x".into()), IRNode::Int(1)])),
            )),
            vec![IRNode::Int(10)],
        );
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(11));
    }

    #[test]
    fn test_tco_no_overflow() {
        let mut vm = IrVm::new();
        let body = IRNode::If(
            Box::new(IRNode::Call("=".into(), vec![IRNode::Load("n".into()), IRNode::Int(0)])),
            Box::new(IRNode::Int(0)),
            Box::new(IRNode::Call("count-down".into(), vec![
                IRNode::Call("-".into(), vec![IRNode::Load("n".into()), IRNode::Int(1)]),
            ])),
        );
        vm.exec_program(&[IRNode::Func("count-down".into(), vec!["n".into()], Box::new(body))]).unwrap();
        let result = vm.exec(&IRNode::Call("count-down".into(), vec![IRNode::Int(100_000)])).unwrap();
        assert_eq!(result, Value::Int(0));
    }

    #[test]
    fn test_builtin_arithmetic() {
        let mut vm = IrVm::new();
        let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
        assert_eq!(vm.exec(&node).unwrap(), Value::Int(7));
    }
}
