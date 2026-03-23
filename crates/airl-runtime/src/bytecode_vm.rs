// crates/airl-runtime/src/bytecode_vm.rs
use std::collections::HashMap;
use crate::bytecode::*;
use crate::value::Value;
use crate::builtins::Builtins;
use crate::error::RuntimeError;

struct CallFrame {
    registers: Vec<Value>,
    func_name: String,
    ip: usize,
    /// Register in the CALLER's frame where the return value should be stored.
    /// Ignored for the bottom-most frame (returns via Ok(result)).
    return_reg: u16,
    match_flag: bool,
}

pub struct BytecodeVm {
    pub functions: HashMap<String, BytecodeFunc>,
    builtins: Builtins,
    call_stack: Vec<CallFrame>,
    recursion_depth: usize,
}

impl BytecodeVm {
    pub fn new() -> Self {
        BytecodeVm {
            functions: HashMap::new(),
            builtins: Builtins::new(),
            call_stack: Vec::new(),
            recursion_depth: 0,
        }
    }

    pub fn load_function(&mut self, func: BytecodeFunc) {
        self.functions.insert(func.name.clone(), func);
    }

    /// Execute a function by name with no arguments. Used to run __main__.
    pub fn exec_main(&mut self) -> Result<Value, RuntimeError> {
        self.push_frame("__main__", &[], 0)
            .and_then(|_| self.run())
    }

    /// Push a new call frame for the named function with the given args.
    /// `return_reg` is the register in the CALLER's frame where the return value goes.
    fn push_frame(&mut self, name: &str, args: &[Value], return_reg: u16) -> Result<(), RuntimeError> {
        let func = self.functions.get(name)
            .ok_or_else(|| RuntimeError::UndefinedSymbol(name.to_string()))?;

        self.recursion_depth += 1;
        if self.recursion_depth > 50_000 {
            self.recursion_depth -= 1;
            return Err(RuntimeError::Custom("stack overflow".into()));
        }

        let mut registers = vec![Value::Nil; func.register_count as usize];
        for (i, arg) in args.iter().enumerate() {
            if i < registers.len() {
                registers[i] = arg.clone();
            }
        }

        self.call_stack.push(CallFrame {
            registers,
            func_name: name.to_string(),
            ip: 0,
            return_reg,
            match_flag: false,
        });
        Ok(())
    }

    fn run(&mut self) -> Result<Value, RuntimeError> {
        loop {
            let (func_name, ip, func_len) = {
                let frame = self.call_stack.last().unwrap();
                (frame.func_name.clone(), frame.ip, {
                    let f = self.functions.get(&frame.func_name).unwrap();
                    f.instructions.len()
                })
            };

            if ip >= func_len {
                // Implicit return nil
                let return_reg = self.call_stack.last().unwrap().return_reg;
                self.call_stack.pop();
                self.recursion_depth = self.recursion_depth.saturating_sub(1);
                if self.call_stack.is_empty() {
                    return Ok(Value::Nil);
                }
                let caller = self.call_stack.last_mut().unwrap();
                caller.registers[return_reg as usize] = Value::Nil;
                continue;
            }

            let instr = {
                let f = self.functions.get(&func_name).unwrap();
                f.instructions[ip]
            };
            self.call_stack.last_mut().unwrap().ip += 1;

            match instr.op {
                Op::LoadConst => {
                    let val = {
                        let f = self.functions.get(&func_name).unwrap();
                        f.constants[instr.a as usize].clone()
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = val;
                }
                Op::LoadNil => {
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Nil;
                }
                Op::LoadTrue => {
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Bool(true);
                }
                Op::LoadFalse => {
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::Bool(false);
                }
                Op::Move => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = val;
                }

                // Arithmetic
                Op::Add => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x + y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                        (Value::Str(x), Value::Str(y)) => Value::Str(format!("{}{}", x, y)),
                        _ => return Err(RuntimeError::TypeError("add: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Sub => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x - y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x - y),
                        _ => return Err(RuntimeError::TypeError("sub: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Mul => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x * y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x * y),
                        _ => return Err(RuntimeError::TypeError("mul: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Div => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(_), Value::Int(0)) => return Err(RuntimeError::DivisionByZero),
                        (Value::Int(x), Value::Int(y)) => Value::Int(x / y),
                        (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
                        _ => return Err(RuntimeError::TypeError("div: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Mod => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Int(x % y),
                        _ => return Err(RuntimeError::TypeError("mod: incompatible types".into()))
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Neg => {
                    let a = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let result = match &a {
                        Value::Int(x) => Value::Int(-x),
                        Value::Float(x) => Value::Float(-x),
                        _ => return Err(RuntimeError::TypeError("neg: expected number".into())),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }

                // Comparison
                Op::Eq => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = Value::Bool(a == b);
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Ne => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = Value::Bool(a != b);
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Lt => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x < y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x < y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Le => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x <= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x <= y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Gt => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x > y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x > y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Ge => {
                    let frame = self.call_stack.last().unwrap();
                    let a = frame.registers[instr.a as usize].clone();
                    let b = frame.registers[instr.b as usize].clone();
                    let result = match (&a, &b) {
                        (Value::Int(x), Value::Int(y)) => Value::Bool(x >= y),
                        (Value::Float(x), Value::Float(y)) => Value::Bool(x >= y),
                        _ => Value::Bool(false),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }
                Op::Not => {
                    let a = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let result = match &a {
                        Value::Bool(b) => Value::Bool(!b),
                        _ => return Err(RuntimeError::TypeError("not: expected bool".into())),
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                }

                // Control flow
                Op::Jump => {
                    let offset = instr.a as i16;
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = (frame.ip as i32 + offset as i32) as usize;
                }
                Op::JumpIfFalse => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    if let Value::Bool(false) = val {
                        let offset = instr.b as i16;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::JumpIfTrue => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    if let Value::Bool(true) = val {
                        let offset = instr.b as i16;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }

                // Data
                Op::MakeList => {
                    let start = instr.a as usize;
                    let count = instr.b as usize;
                    let items: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (start..start+count).map(|i| frame.registers[i].clone()).collect()
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = Value::List(items);
                }
                Op::MakeVariant => {
                    let tag = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                        }
                    };
                    let inner = self.call_stack.last().unwrap().registers[instr.b as usize].clone();
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] =
                        Value::Variant(tag, Box::new(inner));
                }
                Op::MakeVariant0 => {
                    let tag = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("variant tag must be string".into())),
                        }
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] =
                        Value::Variant(tag, Box::new(Value::Nil));
                }

                // Pattern matching
                Op::MatchTag => {
                    let tag = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.b as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("match tag must be string".into())),
                        }
                    };
                    let scr = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let frame = self.call_stack.last_mut().unwrap();
                    match scr {
                        Value::Variant(ref vtag, ref inner) if *vtag == tag => {
                            frame.registers[instr.dst as usize] = *inner.clone();
                            frame.match_flag = true;
                        }
                        _ => {
                            frame.match_flag = false;
                        }
                    }
                }
                Op::JumpIfNoMatch => {
                    let matched = self.call_stack.last().unwrap().match_flag;
                    if !matched {
                        let offset = instr.a as i16;
                        let frame = self.call_stack.last_mut().unwrap();
                        frame.ip = (frame.ip as i32 + offset as i32) as usize;
                    }
                }
                Op::MatchWild => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.registers[instr.dst as usize] = val;
                    frame.match_flag = true;
                }

                // Try
                Op::TryUnwrap => {
                    let val = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    match val {
                        Value::Variant(ref tag, ref inner) if tag == "Ok" => {
                            let inner = *inner.clone();
                            self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = inner;
                        }
                        Value::Variant(ref tag, ref inner) if tag == "Err" => {
                            return Err(RuntimeError::Custom(format!("{}", inner)));
                        }
                        _ => return Err(RuntimeError::TryOnNonResult(format!("{}", val))),
                    }
                }

                // Function calls — push new frame and let the main loop execute it
                Op::Call => {
                    let name = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("call: func name must be string".into())),
                        }
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (0..argc).map(|i| frame.registers[instr.dst as usize + 1 + i].clone()).collect()
                    };

                    // Try builtin first
                    if let Some(f) = self.builtins.get(&name) {
                        let result = f(&args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    } else {
                        // Push a new frame; return_reg is instr.dst in the current frame
                        self.push_frame(&name, &args, instr.dst)?;
                        // The main loop continues executing the new frame
                    }
                }
                Op::CallBuiltin => {
                    let name = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("callbuiltin: name must be string".into())),
                        }
                    };
                    let argc = instr.b as usize;
                    let args: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (0..argc).map(|i| frame.registers[instr.dst as usize + 1 + i].clone()).collect()
                    };
                    if let Some(f) = self.builtins.get(&name) {
                        let result = f(&args)?;
                        self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                    } else {
                        return Err(RuntimeError::UndefinedSymbol(name));
                    }
                }
                Op::CallReg => {
                    let callee = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let argc = instr.b as usize;
                    let args: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (0..argc).map(|i| frame.registers[instr.dst as usize + 1 + i].clone()).collect()
                    };
                    match callee {
                        Value::BytecodeClosure(ref closure) => {
                            let mut full_args = closure.captured.clone();
                            full_args.extend(args);
                            let name = closure.func_name.clone();
                            self.push_frame(&name, &full_args, instr.dst)?;
                        }
                        Value::IRFuncRef(ref name) => {
                            let name = name.clone();
                            if let Some(f) = self.builtins.get(&name) {
                                let result = f(&args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else {
                                self.push_frame(&name, &args, instr.dst)?;
                            }
                        }
                        Value::BuiltinFn(ref name) => {
                            let name = name.clone();
                            if let Some(f) = self.builtins.get(&name) {
                                let result = f(&args)?;
                                self.call_stack.last_mut().unwrap().registers[instr.dst as usize] = result;
                            } else {
                                return Err(RuntimeError::UndefinedSymbol(name));
                            }
                        }
                        _ => return Err(RuntimeError::NotCallable(format!("{}", callee))),
                    }
                }
                Op::TailCall => {
                    // Reset ip to 0 for self-recursion (args already rebound by compiler)
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = 0;
                }

                Op::Return => {
                    let result = self.call_stack.last().unwrap().registers[instr.a as usize].clone();
                    let return_reg = self.call_stack.last().unwrap().return_reg;
                    self.call_stack.pop();
                    self.recursion_depth = self.recursion_depth.saturating_sub(1);
                    if self.call_stack.is_empty() {
                        return Ok(result);
                    }
                    let caller = self.call_stack.last_mut().unwrap();
                    caller.registers[return_reg as usize] = result;
                }

                Op::MakeClosure => {
                    let func_name_const = {
                        let f = self.functions.get(&func_name).unwrap();
                        match &f.constants[instr.a as usize] {
                            Value::Str(s) => s.clone(),
                            _ => return Err(RuntimeError::TypeError("closure: func name must be string".into())),
                        }
                    };
                    let capture_count = self.functions.get(&func_name_const)
                        .map(|f| f.capture_count as usize)
                        .unwrap_or(0);
                    let capture_start = instr.b as usize;
                    let captured: Vec<Value> = {
                        let frame = self.call_stack.last().unwrap();
                        (capture_start..capture_start + capture_count)
                            .map(|i| frame.registers[i].clone())
                            .collect()
                    };
                    self.call_stack.last_mut().unwrap().registers[instr.dst as usize] =
                        Value::BytecodeClosure(BytecodeClosureValue {
                            func_name: func_name_const,
                            captured,
                        });
                }
            }
        }
    }

    /// Load functions and execute __main__
    pub fn exec_program(&mut self, functions: Vec<BytecodeFunc>, main_func: BytecodeFunc) -> Result<Value, RuntimeError> {
        for func in functions {
            self.load_function(func);
        }
        self.load_function(main_func);
        self.exec_main()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode_compiler::BytecodeCompiler;
    use crate::ir::*;

    fn compile_and_run(nodes: &[IRNode]) -> Value {
        let mut compiler = BytecodeCompiler::new();
        let (funcs, main_func) = compiler.compile_program(nodes);
        let mut vm = BytecodeVm::new();
        vm.exec_program(funcs, main_func).unwrap()
    }

    #[test]
    fn test_int_literal() {
        assert_eq!(compile_and_run(&[IRNode::Int(42)]), Value::Int(42));
    }

    #[test]
    fn test_bool_literal() {
        assert_eq!(compile_and_run(&[IRNode::Bool(true)]), Value::Bool(true));
    }

    #[test]
    fn test_arithmetic() {
        let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
        assert_eq!(compile_and_run(&[node]), Value::Int(7));
    }

    #[test]
    fn test_if_true() {
        let node = IRNode::If(
            Box::new(IRNode::Bool(true)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        assert_eq!(compile_and_run(&[node]), Value::Int(1));
    }

    #[test]
    fn test_if_false() {
        let node = IRNode::If(
            Box::new(IRNode::Bool(false)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        assert_eq!(compile_and_run(&[node]), Value::Int(2));
    }

    #[test]
    fn test_let() {
        let node = IRNode::Let(
            vec![IRBinding { name: "x".into(), expr: IRNode::Int(42) }],
            Box::new(IRNode::Load("x".into())),
        );
        assert_eq!(compile_and_run(&[node]), Value::Int(42));
    }

    #[test]
    fn test_function_call() {
        let nodes = vec![
            IRNode::Func("double".into(), vec!["x".into()],
                Box::new(IRNode::Call("*".into(), vec![IRNode::Load("x".into()), IRNode::Int(2)]))),
            IRNode::Call("double".into(), vec![IRNode::Int(21)]),
        ];
        assert_eq!(compile_and_run(&nodes), Value::Int(42));
    }

    #[test]
    fn test_recursion() {
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
        let nodes = vec![
            IRNode::Func("fact".into(), vec!["n".into()], Box::new(fact_body)),
            IRNode::Call("fact".into(), vec![IRNode::Int(5)]),
        ];
        assert_eq!(compile_and_run(&nodes), Value::Int(120));
    }

    #[test]
    fn test_match_variant() {
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
        assert_eq!(compile_and_run(&[node]), Value::Int(42));
    }

    #[test]
    fn test_list() {
        let node = IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]);
        match compile_and_run(&[node]) {
            Value::List(items) => assert_eq!(items.len(), 3),
            _ => panic!("expected list"),
        }
    }
}
