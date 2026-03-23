// crates/airl-runtime/src/bytecode_compiler.rs
use std::collections::HashMap;
use crate::ir::*;
use crate::value::Value;
use crate::bytecode::*;

pub struct BytecodeCompiler {
    instructions: Vec<Instruction>,
    constants: Vec<Value>,
    locals: HashMap<String, u16>,       // variable name → register slot
    next_reg: u16,
    max_reg: u16,
    lambda_counter: usize,              // unique lambda name counter
    compiled_lambdas: Vec<BytecodeFunc>, // lambdas compiled during expression compilation
}

impl BytecodeCompiler {
    pub fn new() -> Self {
        BytecodeCompiler {
            instructions: Vec::new(),
            constants: Vec::new(),
            locals: HashMap::new(),
            next_reg: 0,
            max_reg: 0,
            lambda_counter: 0,
            compiled_lambdas: Vec::new(),
        }
    }

    fn alloc_reg(&mut self) -> u16 {
        let r = self.next_reg;
        self.next_reg += 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        r
    }

    fn free_reg_to(&mut self, r: u16) {
        self.next_reg = r;
    }

    fn emit(&mut self, op: Op, dst: u16, a: u16, b: u16) {
        self.instructions.push(Instruction::new(op, dst, a, b));
    }

    fn add_constant(&mut self, val: Value) -> u16 {
        // Reuse existing constant if identical
        for (i, c) in self.constants.iter().enumerate() {
            if c == &val {
                return i as u16;
            }
        }
        let idx = self.constants.len() as u16;
        self.constants.push(val);
        idx
    }

    /// Compile an IRNode expression, placing the result in `dst`.
    pub fn compile_expr(&mut self, node: &IRNode, dst: u16) {
        match node {
            IRNode::Int(v) => {
                let idx = self.add_constant(Value::Int(*v));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Float(v) => {
                let idx = self.add_constant(Value::Float(*v));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Str(s) => {
                let idx = self.add_constant(Value::Str(s.clone()));
                self.emit(Op::LoadConst, dst, idx, 0);
            }
            IRNode::Bool(true) => self.emit(Op::LoadTrue, dst, 0, 0),
            IRNode::Bool(false) => self.emit(Op::LoadFalse, dst, 0, 0),
            IRNode::Nil => self.emit(Op::LoadNil, dst, 0, 0),

            IRNode::Load(name) => {
                if let Some(&slot) = self.locals.get(name) {
                    if slot != dst {
                        self.emit(Op::Move, dst, slot, 0);
                    }
                } else {
                    // Will be handled in Task 3 (function refs, builtins)
                    // For now, treat as constant string lookup
                    let idx = self.add_constant(Value::Str(name.clone()));
                    self.emit(Op::LoadConst, dst, idx, 0);
                }
            }

            IRNode::Do(exprs) => {
                if exprs.is_empty() {
                    self.emit(Op::LoadNil, dst, 0, 0);
                } else {
                    let save = self.next_reg;
                    for (i, expr) in exprs.iter().enumerate() {
                        if i == exprs.len() - 1 {
                            self.compile_expr(expr, dst);
                        } else {
                            let tmp = self.alloc_reg();
                            self.compile_expr(expr, tmp);
                        }
                    }
                    self.free_reg_to(save.max(dst + 1));
                }
            }

            IRNode::List(items) => {
                let start = self.next_reg;
                for item in items {
                    let r = self.alloc_reg();
                    self.compile_expr(item, r);
                }
                self.emit(Op::MakeList, dst, start, items.len() as u16);
                self.free_reg_to(start.max(dst + 1));
            }

            IRNode::If(cond, then_, else_) => {
                // Compile condition
                let cond_reg = self.alloc_reg();
                self.compile_expr(cond, cond_reg);
                // JumpIfFalse to else
                let jump_to_else = self.instructions.len();
                self.emit(Op::JumpIfFalse, 0, cond_reg, 0); // offset patched later
                self.free_reg_to(cond_reg.max(dst + 1));
                // Then branch
                self.compile_expr(then_, dst);
                let jump_to_end = self.instructions.len();
                self.emit(Op::Jump, 0, 0, 0); // offset patched later
                // Else branch
                let else_start = self.instructions.len();
                self.compile_expr(else_, dst);
                let end = self.instructions.len();
                // Patch jumps
                self.instructions[jump_to_else].b = (else_start as i16 - jump_to_else as i16 - 1) as u16;
                self.instructions[jump_to_end].a = (end as i16 - jump_to_end as i16 - 1) as u16;
            }

            IRNode::Let(bindings, body) => {
                let save_regs = self.next_reg;
                // Binding registers must not overlap with dst; start allocation above dst
                if self.next_reg <= dst {
                    self.next_reg = dst + 1;
                    if self.next_reg > self.max_reg { self.max_reg = self.next_reg; }
                }
                for binding in bindings {
                    let r = self.alloc_reg();
                    self.compile_expr(&binding.expr, r);
                    self.locals.insert(binding.name.clone(), r);
                }
                self.compile_expr(body, dst);
                // Remove bindings from locals
                for binding in bindings {
                    self.locals.remove(&binding.name);
                }
                self.free_reg_to(save_regs.max(dst + 1));
            }

            IRNode::Call(name, args) => {
                // Check if it's a known arithmetic/comparison builtin for direct opcodes
                let direct_op = match name.as_str() {
                    "+" => Some(Op::Add),
                    "-" => Some(Op::Sub),
                    "*" => Some(Op::Mul),
                    "/" => Some(Op::Div),
                    "%" => Some(Op::Mod),
                    "=" => Some(Op::Eq),
                    "!=" => Some(Op::Ne),
                    "<" => Some(Op::Lt),
                    "<=" => Some(Op::Le),
                    ">" => Some(Op::Gt),
                    ">=" => Some(Op::Ge),
                    "not" => Some(Op::Not),
                    _ => None,
                };

                if let Some(op) = direct_op {
                    if args.len() == 2 {
                        let a_reg = self.alloc_reg();
                        self.compile_expr(&args[0], a_reg);
                        let b_reg = self.alloc_reg();
                        self.compile_expr(&args[1], b_reg);
                        self.emit(op, dst, a_reg, b_reg);
                        self.free_reg_to(a_reg.max(dst + 1));
                    } else if args.len() == 1 {
                        let a_reg = self.alloc_reg();
                        self.compile_expr(&args[0], a_reg);
                        self.emit(op, dst, a_reg, 0);
                        self.free_reg_to(a_reg.max(dst + 1));
                    }
                } else {
                    // General function call
                    // Place args in consecutive registers starting at dst+1
                    let arg_start = dst + 1;
                    let save = self.next_reg;
                    self.next_reg = arg_start;
                    if self.next_reg > self.max_reg { self.max_reg = self.next_reg; }
                    for arg in args {
                        let r = self.alloc_reg();
                        self.compile_expr(arg, r);
                    }
                    // Determine if user-defined or builtin
                    if self.locals.contains_key(name) {
                        // Closure/funcref in register
                        let callee_reg = *self.locals.get(name).unwrap();
                        self.emit(Op::CallReg, dst, callee_reg, args.len() as u16);
                    } else {
                        // Try as named function first, fall back to builtin
                        let name_idx = self.add_constant(Value::Str(name.clone()));
                        self.emit(Op::Call, dst, name_idx, args.len() as u16);
                    }
                    self.free_reg_to(save.max(dst + 1));
                }
            }

            IRNode::CallExpr(callee, args) => {
                let callee_reg = self.alloc_reg();
                self.compile_expr(callee, callee_reg);
                let arg_start = dst + 1;
                let save = self.next_reg;
                self.next_reg = arg_start;
                if self.next_reg > self.max_reg { self.max_reg = self.next_reg; }
                for arg in args {
                    let r = self.alloc_reg();
                    self.compile_expr(arg, r);
                }
                self.emit(Op::CallReg, dst, callee_reg, args.len() as u16);
                self.free_reg_to(save.max(dst + 1));
            }

            IRNode::Variant(tag, args) => {
                let tag_idx = self.add_constant(Value::Str(tag.clone()));
                if args.is_empty() {
                    self.emit(Op::MakeVariant0, dst, tag_idx, 0);
                } else if args.len() == 1 {
                    let a_reg = self.alloc_reg();
                    self.compile_expr(&args[0], a_reg);
                    self.emit(Op::MakeVariant, dst, tag_idx, a_reg);
                    self.free_reg_to(a_reg.max(dst + 1));
                } else {
                    // Multi-arg variant: wrap in list
                    let start = self.next_reg;
                    for arg in args {
                        let r = self.alloc_reg();
                        self.compile_expr(arg, r);
                    }
                    let list_reg = self.alloc_reg();
                    self.emit(Op::MakeList, list_reg, start, args.len() as u16);
                    self.emit(Op::MakeVariant, dst, tag_idx, list_reg);
                    self.free_reg_to(start.max(dst + 1));
                }
            }

            IRNode::Lambda(params, body) => {
                // Compile lambda body as a named function stored in a side table.
                let lambda_name = format!("__lambda_{}", self.lambda_counter);
                self.lambda_counter += 1;

                // Captured variables become additional parameters prepended before user params.
                let captured_names: Vec<(String, u16)> = self.locals.iter()
                    .map(|(k, &v)| (k.clone(), v))
                    .collect();

                let mut all_params: Vec<String> = captured_names.iter().map(|(n, _)| n.clone()).collect();
                all_params.extend(params.iter().cloned());

                let func = self.compile_function(&lambda_name, &all_params, body);
                self.compiled_lambdas.push(func);

                // Emit MakeClosure: copy captured values to consecutive regs, then emit opcode
                let capture_start = self.next_reg;
                for (_, slot) in &captured_names {
                    let r = self.alloc_reg();
                    self.emit(Op::Move, r, *slot, 0);
                }
                let name_idx = self.add_constant(Value::Str(lambda_name));
                self.emit(Op::MakeClosure, dst, name_idx, capture_start);
                self.free_reg_to(capture_start.max(dst + 1));
            }

            IRNode::Match(scrutinee, arms) => {
                let scr_reg = self.alloc_reg();
                self.compile_expr(scrutinee, scr_reg);

                let mut end_jumps = Vec::new();

                for arm in arms {
                    match &arm.pattern {
                        IRPattern::Wild => {
                            self.emit(Op::MatchWild, dst, scr_reg, 0);
                            // No jump needed — wildcard always matches
                            self.compile_expr(&arm.body, dst);
                        }
                        IRPattern::Bind(name) => {
                            // Bind scrutinee to name
                            self.locals.insert(name.clone(), scr_reg);
                            self.compile_expr(&arm.body, dst);
                            self.locals.remove(name);
                        }
                        IRPattern::Lit(val) => {
                            let val_reg = self.alloc_reg();
                            let idx = self.add_constant(val.clone());
                            self.emit(Op::LoadConst, val_reg, idx, 0);
                            self.emit(Op::Eq, val_reg, scr_reg, val_reg);
                            let skip = self.instructions.len();
                            self.emit(Op::JumpIfFalse, 0, val_reg, 0); // patch later
                            self.free_reg_to(val_reg.max(dst + 1));
                            self.compile_expr(&arm.body, dst);
                            end_jumps.push(self.instructions.len());
                            self.emit(Op::Jump, 0, 0, 0); // jump to end, patch later
                            let here = self.instructions.len();
                            self.instructions[skip].b = (here as i16 - skip as i16 - 1) as u16;
                        }
                        IRPattern::Variant(tag, sub_pats) => {
                            let tag_idx = self.add_constant(Value::Str(tag.clone()));
                            let inner_reg = self.alloc_reg();
                            self.emit(Op::MatchTag, inner_reg, scr_reg, tag_idx);
                            let skip = self.instructions.len();
                            self.emit(Op::JumpIfNoMatch, 0, 0, 0); // patch later
                            // Bind sub-patterns
                            if sub_pats.len() == 1 {
                                if let IRPattern::Bind(name) = &sub_pats[0] {
                                    self.locals.insert(name.clone(), inner_reg);
                                    self.compile_expr(&arm.body, dst);
                                    self.locals.remove(name);
                                } else if let IRPattern::Wild = &sub_pats[0] {
                                    self.compile_expr(&arm.body, dst);
                                } else {
                                    // Nested pattern — compile body with inner bound
                                    self.compile_expr(&arm.body, dst);
                                }
                            } else if sub_pats.is_empty() {
                                self.compile_expr(&arm.body, dst);
                            } else {
                                // Multi-field variant destructuring not common, handle basic case
                                self.compile_expr(&arm.body, dst);
                            }
                            self.free_reg_to(inner_reg.max(dst + 1));
                            end_jumps.push(self.instructions.len());
                            self.emit(Op::Jump, 0, 0, 0); // jump to end
                            let here = self.instructions.len();
                            self.instructions[skip].a = (here as i16 - skip as i16 - 1) as u16;
                        }
                    }
                }
                // Patch all end jumps
                let end = self.instructions.len();
                for j in end_jumps {
                    self.instructions[j].a = (end as i16 - j as i16 - 1) as u16;
                }
                self.free_reg_to(scr_reg.max(dst + 1));
            }

            IRNode::Try(expr) => {
                let src = self.alloc_reg();
                self.compile_expr(expr, src);
                let _err_jump = self.instructions.len();
                self.emit(Op::TryUnwrap, dst, src, 0); // err_offset patched in context
                self.free_reg_to(src.max(dst + 1));
            }

            // Func nodes are handled at the program level, not as expressions
            IRNode::Func(_, _, _) => {
                self.emit(Op::LoadNil, dst, 0, 0);
            }
        }
    }

    /// Compile a top-level function definition.
    pub fn compile_function(&mut self, name: &str, params: &[String], body: &IRNode) -> BytecodeFunc {
        let mut compiler = BytecodeCompiler::new();
        // Bind params to first N registers
        for (i, param) in params.iter().enumerate() {
            compiler.locals.insert(param.clone(), i as u16);
            compiler.next_reg = (i as u16) + 1;
            compiler.max_reg = compiler.next_reg;
        }
        let dst = compiler.alloc_reg();
        compiler.compile_expr(body, dst);
        compiler.emit(Op::Return, 0, dst, 0);

        BytecodeFunc {
            name: name.to_string(),
            arity: params.len() as u16,
            register_count: compiler.max_reg,
            capture_count: 0,
            instructions: compiler.instructions,
            constants: compiler.constants,
        }
    }

    /// Compile a list of top-level IRNodes into a list of BytecodeFuncs + a main function.
    pub fn compile_program(&mut self, nodes: &[IRNode]) -> (Vec<BytecodeFunc>, BytecodeFunc) {
        let mut functions = Vec::new();
        let mut main_nodes = Vec::new();

        for node in nodes {
            match node {
                IRNode::Func(name, params, body) => {
                    let func = self.compile_function(name, params, body);
                    functions.push(func);
                }
                _ => main_nodes.push(node.clone()),
            }
        }

        // Compile remaining top-level expressions as __main__
        let main_body = if main_nodes.is_empty() {
            IRNode::Nil
        } else if main_nodes.len() == 1 {
            main_nodes.into_iter().next().unwrap()
        } else {
            IRNode::Do(main_nodes)
        };

        let main_func = self.compile_function("__main__", &[], &main_body);
        // Collect any lambdas compiled during function/main compilation
        functions.extend(self.compiled_lambdas.drain(..));
        (functions, main_func)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_int() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Int(42), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Int(42));
    }

    #[test]
    fn test_compile_float() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Float(3.14), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Float(3.14));
    }

    #[test]
    fn test_compile_str() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Str("hello".to_string()), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Str("hello".to_string()));
    }

    #[test]
    fn test_compile_bool() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Bool(true), 0);
        assert_eq!(c.instructions[0].op, Op::LoadTrue);

        let mut c2 = BytecodeCompiler::new();
        c2.compile_expr(&IRNode::Bool(false), 0);
        assert_eq!(c2.instructions[0].op, Op::LoadFalse);
    }

    #[test]
    fn test_compile_nil() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Nil, 0);
        assert_eq!(c.instructions[0].op, Op::LoadNil);
    }

    #[test]
    fn test_compile_load_known() {
        let mut c = BytecodeCompiler::new();
        c.locals.insert("x".to_string(), 0);
        c.next_reg = 1;
        c.max_reg = 1;
        // Load x into reg 1 — should emit Move
        c.compile_expr(&IRNode::Load("x".to_string()), 1);
        assert_eq!(c.instructions[0].op, Op::Move);
        assert_eq!(c.instructions[0].dst, 1);
        assert_eq!(c.instructions[0].a, 0);
    }

    #[test]
    fn test_compile_load_same_reg() {
        let mut c = BytecodeCompiler::new();
        c.locals.insert("x".to_string(), 0);
        c.next_reg = 1;
        c.max_reg = 1;
        // Load x into reg 0 — already there, no Move needed
        c.compile_expr(&IRNode::Load("x".to_string()), 0);
        assert_eq!(c.instructions.len(), 0);
    }

    #[test]
    fn test_compile_load_unknown() {
        let mut c = BytecodeCompiler::new();
        // Unknown variable: falls back to LoadConst string
        c.compile_expr(&IRNode::Load("foo".to_string()), 0);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Str("foo".to_string()));
    }

    #[test]
    fn test_compile_do() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![IRNode::Int(1), IRNode::Int(2)]), 0);
        // Should have LoadConst for 1 (temp), LoadConst for 2 (dst=0)
        assert!(c.instructions.len() >= 2);
    }

    #[test]
    fn test_compile_do_empty() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![]), 0);
        assert_eq!(c.instructions[0].op, Op::LoadNil);
    }

    #[test]
    fn test_compile_do_single() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::Do(vec![IRNode::Int(99)]), 0);
        assert_eq!(c.instructions.len(), 1);
        assert_eq!(c.instructions[0].op, Op::LoadConst);
        assert_eq!(c.constants[0], Value::Int(99));
    }

    #[test]
    fn test_compile_list() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::List(vec![IRNode::Int(1), IRNode::Int(2), IRNode::Int(3)]), 0);
        // Last instruction should be MakeList
        let last = c.instructions.last().unwrap();
        assert_eq!(last.op, Op::MakeList);
        assert_eq!(last.b, 3); // count = 3
    }

    #[test]
    fn test_compile_list_empty() {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(&IRNode::List(vec![]), 0);
        let last = c.instructions.last().unwrap();
        assert_eq!(last.op, Op::MakeList);
        assert_eq!(last.b, 0);
    }

    #[test]
    fn test_compile_function() {
        let mut c = BytecodeCompiler::new();
        // (defn id [x] x)
        let func = c.compile_function("id", &["x".to_string()], &IRNode::Load("x".into()));
        assert_eq!(func.name, "id");
        assert_eq!(func.arity, 1);
        // Should have Move (x→dst) + Return, or just Return if x is already dst
        assert!(func.instructions.len() >= 1);
        // Last instruction must be Return
        assert_eq!(func.instructions.last().unwrap().op, Op::Return);
    }

    #[test]
    fn test_compile_function_literal_body() {
        let mut c = BytecodeCompiler::new();
        // (defn const42 [] 42)
        let func = c.compile_function("const42", &[], &IRNode::Int(42));
        assert_eq!(func.arity, 0);
        assert_eq!(func.instructions[0].op, Op::LoadConst);
        assert_eq!(func.constants[0], Value::Int(42));
        assert_eq!(func.instructions.last().unwrap().op, Op::Return);
    }

    #[test]
    fn test_constant_deduplication() {
        let mut c = BytecodeCompiler::new();
        // Adding the same constant twice should reuse the same slot
        let idx1 = c.add_constant(Value::Int(10));
        let idx2 = c.add_constant(Value::Int(10));
        assert_eq!(idx1, idx2);
        assert_eq!(c.constants.len(), 1);
    }

    #[test]
    fn test_compile_program_no_funcs() {
        let mut c = BytecodeCompiler::new();
        let nodes = vec![IRNode::Int(1), IRNode::Int(2)];
        let (funcs, main) = c.compile_program(&nodes);
        assert!(funcs.is_empty());
        assert_eq!(main.name, "__main__");
    }

    #[test]
    fn test_compile_program_with_func() {
        let mut c = BytecodeCompiler::new();
        let nodes = vec![
            IRNode::Func(
                "double".to_string(),
                vec!["x".to_string()],
                Box::new(IRNode::Load("x".to_string())),
            ),
            IRNode::Int(42),
        ];
        let (funcs, main) = c.compile_program(&nodes);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "double");
        assert_eq!(main.name, "__main__");
    }

    #[test]
    fn test_compile_if() {
        let mut c = BytecodeCompiler::new();
        let node = IRNode::If(
            Box::new(IRNode::Bool(true)),
            Box::new(IRNode::Int(1)),
            Box::new(IRNode::Int(2)),
        );
        c.compile_expr(&node, 0);
        // Should have: LoadTrue, JumpIfFalse, LoadConst(1), Jump, LoadConst(2)
        assert!(c.instructions.len() >= 5);
    }

    #[test]
    fn test_compile_let() {
        let mut c = BytecodeCompiler::new();
        let node = IRNode::Let(
            vec![IRBinding { name: "x".into(), expr: IRNode::Int(42) }],
            Box::new(IRNode::Load("x".into())),
        );
        c.compile_expr(&node, 0);
        assert!(c.instructions.len() >= 2);
    }

    #[test]
    fn test_compile_call_add() {
        let mut c = BytecodeCompiler::new();
        let node = IRNode::Call("+".into(), vec![IRNode::Int(3), IRNode::Int(4)]);
        c.compile_expr(&node, 0);
        // Should use direct Add opcode, not CallBuiltin
        let has_add = c.instructions.iter().any(|i| i.op == Op::Add);
        assert!(has_add, "arithmetic should compile to direct opcode");
    }
}
