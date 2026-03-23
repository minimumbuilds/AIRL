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

            // Stubs for remaining node types — implemented in Tasks 3-5
            _ => {
                self.emit(Op::LoadNil, dst, 0, 0); // placeholder
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
}
