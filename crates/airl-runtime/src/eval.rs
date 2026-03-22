use crate::value::{Value, FnValue, LambdaValue};
use crate::error::RuntimeError;
use crate::env::{Env, FrameKind};
use crate::builtins::Builtins;
use crate::pattern::try_match;
use airl_syntax::ast::*;

use std::collections::HashMap;
use std::io::{BufReader, BufWriter};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};

struct LiveAgent {
    name: String,
    writer: Arc<Mutex<BufWriter<std::process::ChildStdin>>>,
    reader: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    child: Child,
}

pub struct Interpreter {
    pub env: Env,
    builtins: Builtins,
    pub jit: Option<airl_codegen::JitCache>,
    pub tensor_jit: Option<airl_codegen::TensorJit>,
    agents: Vec<LiveAgent>,
    pending_results: HashMap<String, mpsc::Receiver<Result<Value, String>>>,
    next_agent_id: u32,
    next_send_id: u32,
    recursion_depth: usize,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Interpreter {
            env: Env::new(),
            builtins: Builtins::new(),
            jit: airl_codegen::JitCache::new().ok(),
            tensor_jit: airl_codegen::TensorJit::new().ok(),
            agents: Vec::new(),
            pending_results: HashMap::new(),
            next_agent_id: 0,
            next_send_id: 0,
            recursion_depth: 0,
        };
        // Register all builtin names in the environment so symbol lookups resolve them
        interp.register_builtin_symbols();
        interp
    }

    fn register_builtin_symbols(&mut self) {
        let names = [
            "+", "-", "*", "/", "%",
            "=", "!=", "<", ">", "<=", ">=",
            "and", "or", "not", "xor",
            "tensor.zeros", "tensor.ones", "tensor.rand", "tensor.identity",
            "tensor.add", "tensor.mul", "tensor.matmul", "tensor.reshape",
            "tensor.transpose", "tensor.softmax", "tensor.sum", "tensor.max",
            "tensor.slice",
            "length", "at", "append", "head", "tail", "empty?", "cons",
            "print", "type-of", "shape", "valid",
            "spawn-agent", "send", "send-async", "await", "parallel",
            "char-at", "substring", "split", "join", "contains",
            "starts-with", "ends-with", "trim", "to-upper", "to-lower",
            "replace", "index-of", "chars",
            "map-new", "map-from", "map-get", "map-get-or", "map-set",
            "map-has", "map-remove", "map-keys", "map-values", "map-size",
        ];
        for name in &names {
            self.env.bind(name.to_string(), Value::BuiltinFn(name.to_string()));
        }
    }

    pub fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match &expr.kind {
            ExprKind::IntLit(v) => Ok(Value::Int(*v)),
            ExprKind::FloatLit(v) => Ok(Value::Float(*v)),
            ExprKind::BoolLit(v) => Ok(Value::Bool(*v)),
            ExprKind::StrLit(v) => Ok(Value::Str(v.clone())),
            ExprKind::NilLit => Ok(Value::Nil),
            ExprKind::KeywordLit(k) => Ok(Value::Str(format!(":{}", k))),

            ExprKind::SymbolRef(name) => {
                // Check builtins first (in case env was modified)
                self.env.get(name).cloned()
            }

            ExprKind::If(cond, then_branch, else_branch) => {
                let cond_val = self.eval(cond)?;
                if is_truthy(&cond_val) {
                    self.eval(then_branch)
                } else {
                    self.eval(else_branch)
                }
            }

            ExprKind::Let(bindings, body) => {
                self.env.push_frame(FrameKind::Let);
                for binding in bindings {
                    let val = self.eval(&binding.value)?;
                    self.env.bind(binding.name.clone(), val);
                }
                let result = self.eval(body);
                self.env.pop_frame();
                result
            }

            ExprKind::Do(exprs) => {
                let mut result = Value::Unit;
                for e in exprs {
                    result = self.eval(e)?;
                }
                Ok(result)
            }

            ExprKind::Match(scrutinee, arms) => {
                let val = self.eval(scrutinee)?;
                for arm in arms {
                    if let Some(bindings) = try_match(&arm.pattern, &val) {
                        self.env.push_frame(FrameKind::Match);
                        for (name, v) in bindings {
                            self.env.bind(name, v);
                        }
                        let result = self.eval(&arm.body);
                        self.env.pop_frame();
                        return result;
                    }
                }
                Err(RuntimeError::NonExhaustiveMatch {
                    value: format!("{}", val),
                })
            }

            ExprKind::Forall(param, where_clause, body) => {
                self.eval_quantifier(&param.name, where_clause.as_deref(), body, true)
            }

            ExprKind::Exists(param, where_clause, body) => {
                self.eval_quantifier(&param.name, where_clause.as_deref(), body, false)
            }

            ExprKind::Lambda(params, body) => {
                // Capture the current environment bindings
                let captures = self.capture_env();
                Ok(Value::Lambda(LambdaValue {
                    params: params.clone(),
                    body: (**body).clone(),
                    captures,
                }))
            }

            ExprKind::FnCall(callee, args) => {
                let callee_val = self.eval(callee)?;
                let mut arg_vals = Vec::with_capacity(args.len());

                // Get parameter ownership from callee if it's a known function
                let param_ownerships = match &callee_val {
                    Value::Function(f) => f.def.params.iter().map(|p| p.ownership).collect::<Vec<_>>(),
                    _ => vec![Ownership::Default; args.len()],
                };

                // Track borrows for this call so we can release them after
                let mut borrow_ledger: Vec<(String, bool)> = Vec::new(); // (name, is_mutable)

                for (i, arg) in args.iter().enumerate() {
                    let val = self.eval(arg)?;
                    arg_vals.push(val);

                    let ownership = param_ownerships.get(i).copied().unwrap_or(Ownership::Default);

                    // Only track ownership for symbol references (not literals/expressions)
                    if let ExprKind::SymbolRef(ref name) = arg.kind {
                        // Skip builtins
                        if let Ok(v) = self.env.get(name) {
                            if matches!(v, Value::BuiltinFn(_)) { continue; }
                        }

                        match ownership {
                            Ownership::Own => {
                                // Explicit own: mark source as moved
                                self.env.mark_moved(name, arg.span)?;
                            }
                            Ownership::Ref => {
                                self.env.borrow_immutable(name)?;
                                borrow_ledger.push((name.clone(), false));
                            }
                            Ownership::Mut => {
                                self.env.borrow_mutable(name)?;
                                borrow_ledger.push((name.clone(), true));
                            }
                            Ownership::Copy => {
                                // Verify type supports Copy (primitives except String)
                                if let Ok(v) = self.env.get(name) {
                                    let is_copy = matches!(v,
                                        Value::Int(_) | Value::UInt(_) | Value::Float(_) |
                                        Value::Bool(_) | Value::Unit | Value::Nil
                                    );
                                    if !is_copy {
                                        return Err(RuntimeError::Custom(format!(
                                            "cannot copy `{}` — type does not implement Copy", name
                                        )));
                                    }
                                }
                            }
                            Ownership::Default => {
                                // Default: clone without move (no tracking)
                            }
                        }
                    }
                }

                // Call the function
                // Handle spawn-agent and send builtins (need &mut self)
                if let Value::BuiltinFn(ref name) = callee_val {
                    match name.as_str() {
                        "spawn-agent" => {
                            let result = self.builtin_spawn_agent(&arg_vals);
                            for (bname, is_mutable) in &borrow_ledger {
                                if *is_mutable { self.env.release_mutable_borrow(bname); }
                                else { self.env.release_immutable_borrow(bname); }
                            }
                            return result;
                        }
                        "send" => {
                            let result = self.builtin_send(&arg_vals);
                            for (bname, is_mutable) in &borrow_ledger {
                                if *is_mutable { self.env.release_mutable_borrow(bname); }
                                else { self.env.release_immutable_borrow(bname); }
                            }
                            return result;
                        }
                        "send-async" => {
                            let result = self.builtin_send_async(&arg_vals);
                            for (bname, is_mutable) in &borrow_ledger {
                                if *is_mutable { self.env.release_mutable_borrow(bname); }
                                else { self.env.release_immutable_borrow(bname); }
                            }
                            return result;
                        }
                        "await" => {
                            let result = self.builtin_await(&arg_vals);
                            for (bname, is_mutable) in &borrow_ledger {
                                if *is_mutable { self.env.release_mutable_borrow(bname); }
                                else { self.env.release_immutable_borrow(bname); }
                            }
                            return result;
                        }
                        "parallel" => {
                            let result = self.builtin_parallel(&arg_vals);
                            for (bname, is_mutable) in &borrow_ledger {
                                if *is_mutable { self.env.release_mutable_borrow(bname); }
                                else { self.env.release_immutable_borrow(bname); }
                            }
                            return result;
                        }
                        _ => {}
                    }
                }

                // Try tensor JIT for supported ops before regular dispatch
                if let Value::BuiltinFn(ref name) = callee_val {
                    if matches!(name.as_str(), "tensor.add" | "tensor.mul" | "tensor.matmul") {
                        if let Some(mut tjit) = self.tensor_jit.take() {
                            let result = try_tensor_jit(&mut tjit, name, &arg_vals);
                            self.tensor_jit = Some(tjit);
                            match result {
                                Ok(Some(val)) => {
                                    // Release borrows before returning
                                    for (bname, is_mutable) in &borrow_ledger {
                                        if *is_mutable {
                                            self.env.release_mutable_borrow(bname);
                                        } else {
                                            self.env.release_immutable_borrow(bname);
                                        }
                                    }
                                    return Ok(val);
                                }
                                Err(e) => {
                                    for (bname, is_mutable) in &borrow_ledger {
                                        if *is_mutable {
                                            self.env.release_mutable_borrow(bname);
                                        } else {
                                            self.env.release_immutable_borrow(bname);
                                        }
                                    }
                                    return Err(e);
                                }
                                Ok(None) => {} // fall through to interpreted builtin
                            }
                        }
                    }
                }

                let result = match callee_val {
                    Value::BuiltinFn(ref name) => {
                        let f = self.builtins.get(name).ok_or_else(|| {
                            RuntimeError::UndefinedSymbol(name.clone())
                        })?;
                        f(&arg_vals)
                    }
                    Value::Function(ref fn_val) => {
                        let fn_val = fn_val.clone();
                        self.call_fn(&fn_val, arg_vals)
                    }
                    Value::Lambda(ref lam) => {
                        let lam = lam.clone();
                        self.call_lambda(&lam, arg_vals)
                    }
                    other => Err(RuntimeError::NotCallable(format!("{}", other))),
                };

                // Release borrows taken for this call
                for (name, is_mutable) in &borrow_ledger {
                    if *is_mutable {
                        self.env.release_mutable_borrow(name);
                    } else {
                        self.env.release_immutable_borrow(name);
                    }
                }

                result
            }

            ExprKind::Try(inner) => {
                let val = self.eval(inner)?;
                match val {
                    Value::Variant(ref name, ref inner_val) if name == "Ok" => {
                        Ok(inner_val.as_ref().clone())
                    }
                    Value::Variant(ref name, ref inner_val) if name == "Err" => {
                        Err(RuntimeError::Custom(format!("Err: {}", inner_val)))
                    }
                    other => Err(RuntimeError::TryOnNonResult(format!("{}", other))),
                }
            }

            ExprKind::VariantCtor(name, args) => {
                let inner = if args.is_empty() {
                    Value::Unit
                } else if args.len() == 1 {
                    self.eval(&args[0])?
                } else {
                    let mut vals = Vec::new();
                    for a in args {
                        vals.push(self.eval(a)?);
                    }
                    Value::Tuple(vals)
                };
                Ok(Value::Variant(name.clone(), Box::new(inner)))
            }

            ExprKind::StructLit(_name, fields) => {
                let mut map = std::collections::BTreeMap::new();
                for (field_name, field_expr) in fields {
                    map.insert(field_name.clone(), self.eval(field_expr)?);
                }
                Ok(Value::Struct(map))
            }

            ExprKind::ListLit(items) => {
                let mut vals = Vec::with_capacity(items.len());
                for item in items {
                    vals.push(self.eval(item)?);
                }
                Ok(Value::List(vals))
            }
        }
    }

    fn call_fn(&mut self, fn_val: &FnValue, args: Vec<Value>) -> Result<Value, RuntimeError> {
        if self.recursion_depth >= 50_000 {
            return Err(RuntimeError::TypeError(
                "maximum recursion depth (50000) exceeded".into(),
            ));
        }
        self.recursion_depth += 1;

        let def = &fn_val.def;

        // 1. Push Function frame
        self.env.push_frame(FrameKind::Function);

        // 2. Bind params to arg values
        for (i, param) in def.params.iter().enumerate() {
            let val = args.get(i).cloned().unwrap_or(Value::Nil);
            self.env.bind(param.name.clone(), val);
        }

        // 3. Check :requires contracts
        for contract in &def.requires {
            let contract_result = self.eval(contract)?;
            if contract_result != Value::Bool(true) {
                self.recursion_depth -= 1;
                self.env.pop_frame();
                return Err(RuntimeError::ContractViolation(
                    airl_contracts::violation::ContractViolation {
                        function: fn_val.name.clone(),
                        contract_kind: airl_contracts::violation::ContractKind::Requires,
                        clause_source: format!("{:?}", contract.kind),
                        bindings: vec![],
                        evaluated: format!("{}", contract_result),
                        span: contract.span,
                    },
                ));
            }
        }

        // 4. Try JIT path
        if let Some(ref mut jit) = self.jit {
            let raw_args: Result<Vec<_>, _> = args.iter().map(|val| {
                value_to_raw(val)
            }).collect();

            if let Ok(raw_args) = raw_args {
                match jit.try_call(def, &raw_args) {
                    Ok(Some(raw_result)) => {
                        let result_val = raw_to_value(raw_result, &def.return_type);
                        self.env.bind("result".to_string(), result_val.clone());
                        // Check :invariant contracts
                        for contract in &def.invariants {
                            let contract_result = self.eval(contract)?;
                            if contract_result != Value::Bool(true) {
                                self.recursion_depth -= 1;
                                self.env.pop_frame();
                                return Err(RuntimeError::ContractViolation(
                                    airl_contracts::violation::ContractViolation {
                                        function: fn_val.name.clone(),
                                        contract_kind: airl_contracts::violation::ContractKind::Invariant,
                                        clause_source: format!("{:?}", contract.kind),
                                        bindings: vec![],
                                        evaluated: format!("{}", contract_result),
                                        span: contract.span,
                                    },
                                ));
                            }
                        }
                        // Check :ensures contracts
                        for contract in &def.ensures {
                            let contract_result = self.eval(contract)?;
                            if contract_result != Value::Bool(true) {
                                self.recursion_depth -= 1;
                                self.env.pop_frame();
                                return Err(RuntimeError::ContractViolation(
                                    airl_contracts::violation::ContractViolation {
                                        function: fn_val.name.clone(),
                                        contract_kind: airl_contracts::violation::ContractKind::Ensures,
                                        clause_source: format!("{:?}", contract.kind),
                                        bindings: vec![],
                                        evaluated: format!("{}", contract_result),
                                        span: contract.span,
                                    },
                                ));
                            }
                        }
                        self.recursion_depth -= 1;
                        self.env.pop_frame();
                        return Ok(result_val);
                    }
                    Ok(None) => {} // not compilable, fall through to interpreter
                    Err(_e) => {
                        // JIT error, fall through to interpreter silently
                    }
                }
            }
        }

        // 5. Eval body (interpreted path)
        let result = self.eval(&def.body);

        match result {
            Ok(result_val) => {
                // 5. Bind `result` for contract checking
                self.env.bind("result".to_string(), result_val.clone());

                // 6. Check :invariant contracts
                for contract in &def.invariants {
                    let contract_result = self.eval(contract)?;
                    if contract_result != Value::Bool(true) {
                        self.recursion_depth -= 1;
                        self.env.pop_frame();
                        return Err(RuntimeError::ContractViolation(
                            airl_contracts::violation::ContractViolation {
                                function: fn_val.name.clone(),
                                contract_kind: airl_contracts::violation::ContractKind::Invariant,
                                clause_source: format!("{:?}", contract.kind),
                                bindings: vec![],
                                evaluated: format!("{}", contract_result),
                                span: contract.span,
                            },
                        ));
                    }
                }

                // 7. Check :ensures contracts
                for contract in &def.ensures {
                    let contract_result = self.eval(contract)?;
                    if contract_result != Value::Bool(true) {
                        self.recursion_depth -= 1;
                        self.env.pop_frame();
                        return Err(RuntimeError::ContractViolation(
                            airl_contracts::violation::ContractViolation {
                                function: fn_val.name.clone(),
                                contract_kind: airl_contracts::violation::ContractKind::Ensures,
                                clause_source: format!("{:?}", contract.kind),
                                bindings: vec![],
                                evaluated: format!("{}", contract_result),
                                span: contract.span,
                            },
                        ));
                    }
                }

                // 7. Pop frame
                self.recursion_depth -= 1;
                self.env.pop_frame();

                // 8. Return result
                Ok(result_val)
            }
            Err(e) => {
                self.recursion_depth -= 1;
                self.env.pop_frame();
                Err(e)
            }
        }
    }

    fn call_lambda(&mut self, lam: &LambdaValue, args: Vec<Value>) -> Result<Value, RuntimeError> {
        self.env.push_frame(FrameKind::Function);

        // Restore captures
        for (name, val) in &lam.captures {
            self.env.bind(name.clone(), val.clone());
        }

        // Bind params
        for (i, param) in lam.params.iter().enumerate() {
            let val = args.get(i).cloned().unwrap_or(Value::Nil);
            self.env.bind(param.name.clone(), val);
        }

        let result = self.eval(&lam.body);
        self.env.pop_frame();
        result
    }

    fn eval_quantifier(
        &mut self,
        var_name: &str,
        where_clause: Option<&Expr>,
        body: &Expr,
        is_forall: bool,
    ) -> Result<Value, RuntimeError> {
        const MAX_ITERATIONS: i64 = 10_000;

        for i in 0..MAX_ITERATIONS {
            self.env.push_frame(FrameKind::Let);
            self.env.bind(var_name.to_string(), Value::Int(i));

            let in_domain = if let Some(guard) = where_clause {
                let guard_val = self.eval(guard)?;
                is_truthy(&guard_val)
            } else {
                true
            };

            if in_domain {
                let result = self.eval(body)?;
                let holds = is_truthy(&result);
                self.env.pop_frame();

                if is_forall && !holds {
                    return Ok(Value::Bool(false));
                }
                if !is_forall && holds {
                    return Ok(Value::Bool(true));
                }
            } else {
                self.env.pop_frame();
            }
        }

        if is_forall {
            Ok(Value::Bool(true))
        } else {
            Ok(Value::Bool(false))
        }
    }

    fn capture_env(&self) -> Vec<(String, Value)> {
        // Capture all visible bindings from current frames
        // For Phase 1, we use dynamic scoping — lambdas rely on
        // the environment being present at call time.
        // This works since most lambdas are used immediately in let bindings.
        Vec::new()
    }

    /// Call a named function with the given arguments.
    /// Used by the agent runtime to execute tasks.
    pub fn call_by_name(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        let fn_val = match self.env.get(name)? {
            Value::Function(f) => f.clone(),
            Value::BuiltinFn(ref bname) => {
                let f = self.builtins.get(bname).ok_or_else(|| {
                    RuntimeError::UndefinedSymbol(bname.to_string())
                })?;
                return f(&args);
            }
            other => return Err(RuntimeError::NotCallable(format!(
                "`{}` is {}, not a function", name, other
            ))),
        };
        self.call_fn(&fn_val, args)
    }

    fn builtin_spawn_agent(&mut self, args: &[Value]) -> Result<Value, RuntimeError> {
        let module_path = match args.first() {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(RuntimeError::TypeError("spawn-agent requires a string path".into())),
        };

        let exe = std::env::current_exe()
            .map_err(|e| RuntimeError::Custom(format!("cannot find airl binary: {}", e)))?;

        let mut child = Command::new(&exe)
            .args(["agent", &module_path, "--listen", "stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| RuntimeError::Custom(format!("cannot spawn agent: {}", e)))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| RuntimeError::Custom("cannot get child stdin".into()))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| RuntimeError::Custom("cannot get child stdout".into()))?;

        let name = format!("agent-{}", self.next_agent_id);
        self.next_agent_id += 1;

        self.agents.push(LiveAgent {
            name: name.clone(),
            writer: Arc::new(Mutex::new(BufWriter::new(stdin))),
            reader: Arc::new(Mutex::new(BufReader::new(stdout))),
            child,
        });

        // Give agent a moment to load
        std::thread::sleep(std::time::Duration::from_millis(100));

        Ok(Value::Str(name))
    }

    fn builtin_send(&mut self, args: &[Value]) -> Result<Value, RuntimeError> {
        if args.len() < 2 {
            return Err(RuntimeError::TypeError(
                "send requires at least 2 args: target, function, [args...]".into(),
            ));
        }

        let target = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(RuntimeError::TypeError("send target must be a string".into())),
        };
        let fn_name = match &args[1] {
            Value::Str(s) => s.clone(),
            _ => return Err(RuntimeError::TypeError("send function name must be a string".into())),
        };
        let fn_args = &args[2..];

        let task_id = format!("send-{}", self.next_send_id);
        self.next_send_id += 1;
        let task_msg = crate::agent_client::format_task(&task_id, &fn_name, fn_args);

        if target.starts_with("tcp:") || target.starts_with("unix:") {
            self.send_to_endpoint(&target, &task_msg)
        } else {
            self.send_to_agent(&target, &task_msg)
        }
    }

    fn send_to_endpoint(&mut self, endpoint: &str, task_msg: &str) -> Result<Value, RuntimeError> {
        use std::net::TcpStream;

        if let Some(addr_str) = endpoint.strip_prefix("tcp:") {
            let addr: std::net::SocketAddr = addr_str.parse()
                .map_err(|e| RuntimeError::Custom(format!("invalid address: {}", e)))?;
            let mut stream = TcpStream::connect(addr)
                .map_err(|e| RuntimeError::Custom(format!("cannot connect: {}", e)))?;

            crate::agent_client::write_frame(&mut stream, task_msg)
                .map_err(|e| RuntimeError::Custom(format!("send failed: {}", e)))?;
            let response = crate::agent_client::read_frame(&mut stream)
                .map_err(|e| RuntimeError::Custom(format!("recv failed: {}", e)))?;

            crate::agent_client::parse_result_message(&response)
                .map_err(|e| RuntimeError::Custom(e))
        } else {
            Err(RuntimeError::Custom(format!("unsupported endpoint: {}", endpoint)))
        }
    }

    fn send_to_agent(&mut self, name: &str, task_msg: &str) -> Result<Value, RuntimeError> {
        let agent = self.agents.iter().find(|a| a.name == name)
            .ok_or_else(|| RuntimeError::Custom(format!("unknown agent: {}", name)))?;

        let mut writer = agent.writer.lock()
            .map_err(|_| RuntimeError::Custom("agent writer lock poisoned".into()))?;
        let mut reader = agent.reader.lock()
            .map_err(|_| RuntimeError::Custom("agent reader lock poisoned".into()))?;

        crate::agent_client::write_frame(&mut *writer, task_msg)
            .map_err(|e| RuntimeError::Custom(format!("send to {} failed: {}", name, e)))?;
        let response = crate::agent_client::read_frame(&mut *reader)
            .map_err(|e| RuntimeError::Custom(format!("recv from {} failed: {}", name, e)))?;

        crate::agent_client::parse_result_message(&response)
            .map_err(|e| RuntimeError::Custom(e))
    }

    /// send-async: dispatch a task to an agent without waiting for the result.
    /// Returns a task ID string that can be passed to `await`.
    fn builtin_send_async(&mut self, args: &[Value]) -> Result<Value, RuntimeError> {
        if args.len() < 2 {
            return Err(RuntimeError::TypeError(
                "send-async requires at least 2 args: target, function, [args...]".into(),
            ));
        }

        let target = match &args[0] {
            Value::Str(s) => s.clone(),
            _ => return Err(RuntimeError::TypeError("send-async target must be a string".into())),
        };
        let fn_name = match &args[1] {
            Value::Str(s) => s.clone(),
            _ => return Err(RuntimeError::TypeError("send-async function name must be a string".into())),
        };
        let fn_args = &args[2..];

        let task_id = format!("send-{}", self.next_send_id);
        self.next_send_id += 1;
        let task_msg = crate::agent_client::format_task(&task_id, &fn_name, fn_args);

        // Find the agent and get Arc handles to its reader/writer
        let agent = self.agents.iter().find(|a| a.name == target)
            .ok_or_else(|| RuntimeError::Custom(format!("unknown agent: {}", target)))?;
        let writer_arc = Arc::clone(&agent.writer);
        let reader_arc = Arc::clone(&agent.reader);
        let agent_name = agent.name.clone();

        // Write the task frame (synchronous — fast, just writes to pipe buffer)
        {
            let mut writer = writer_arc.lock()
                .map_err(|_| RuntimeError::Custom("agent writer lock poisoned".into()))?;
            crate::agent_client::write_frame(&mut *writer, &task_msg)
                .map_err(|e| RuntimeError::Custom(format!("send-async to {} failed: {}", agent_name, e)))?;
        }

        // Spawn background thread to read the response
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let mut reader = reader_arc.lock()
                    .map_err(|_| "agent reader lock poisoned".to_string())?;
                let response = crate::agent_client::read_frame(&mut *reader)
                    .map_err(|e| format!("recv from {} failed: {}", agent_name, e))?;
                crate::agent_client::parse_result_message(&response)
            })();
            let _ = tx.send(result);
        });

        self.pending_results.insert(task_id.clone(), rx);
        Ok(Value::Str(task_id))
    }

    /// await: block until an async task completes, with optional timeout in milliseconds.
    /// Usage: (await task-id) or (await task-id 5000)
    fn builtin_await(&mut self, args: &[Value]) -> Result<Value, RuntimeError> {
        let task_id = match args.first() {
            Some(Value::Str(s)) => s.clone(),
            _ => return Err(RuntimeError::TypeError("await requires a task ID string".into())),
        };

        let rx = self.pending_results.remove(&task_id)
            .ok_or_else(|| RuntimeError::Custom(format!("unknown task ID: {}", task_id)))?;

        // Optional timeout in milliseconds (second argument)
        let result = match args.get(1) {
            Some(Value::Int(ms)) => {
                let timeout = std::time::Duration::from_millis(*ms as u64);
                rx.recv_timeout(timeout)
                    .map_err(|e| RuntimeError::Custom(format!("await {} timed out: {}", task_id, e)))?
            }
            _ => {
                // No timeout — block indefinitely
                rx.recv()
                    .map_err(|e| RuntimeError::Custom(format!("await {} failed: {}", task_id, e)))?
            }
        };

        result.map_err(|e| RuntimeError::Custom(e))
    }

    /// parallel: collect results from multiple async tasks.
    /// Usage: (parallel [task-id-1 task-id-2 ...]) or (parallel [task-id-1 ...] timeout-ms)
    /// Returns a list of results in the same order as the task IDs.
    fn builtin_parallel(&mut self, args: &[Value]) -> Result<Value, RuntimeError> {
        let task_ids = match args.first() {
            Some(Value::List(ids)) => {
                let mut result = Vec::new();
                for id in ids {
                    match id {
                        Value::Str(s) => result.push(s.clone()),
                        _ => return Err(RuntimeError::TypeError(
                            "parallel requires a list of task ID strings".into()
                        )),
                    }
                }
                result
            }
            _ => return Err(RuntimeError::TypeError(
                "parallel requires a list of task IDs".into()
            )),
        };

        // Optional timeout in milliseconds (second argument)
        let timeout = match args.get(1) {
            Some(Value::Int(ms)) => Some(std::time::Duration::from_millis(*ms as u64)),
            _ => None,
        };

        // Collect all results
        let mut results = Vec::new();
        for task_id in &task_ids {
            let rx = self.pending_results.remove(task_id)
                .ok_or_else(|| RuntimeError::Custom(format!("unknown task ID: {}", task_id)))?;

            let result = match timeout {
                Some(t) => rx.recv_timeout(t)
                    .map_err(|e| RuntimeError::Custom(
                        format!("parallel: task {} timed out: {}", task_id, e)
                    ))?,
                None => rx.recv()
                    .map_err(|e| RuntimeError::Custom(
                        format!("parallel: task {} failed: {}", task_id, e)
                    ))?,
            };

            results.push(result.map_err(|e| RuntimeError::Custom(e))?);
        }

        Ok(Value::List(results))
    }

    pub fn eval_top_level(&mut self, top: &TopLevel) -> Result<Value, RuntimeError> {
        match top {
            TopLevel::Defn(f) => {
                let fn_val = Value::Function(FnValue {
                    name: f.name.clone(),
                    def: f.clone(),
                });
                self.env.bind(f.name.clone(), fn_val);
                Ok(Value::Unit)
            }
            TopLevel::Expr(e) => self.eval(e),
            TopLevel::DefType(_) => Ok(Value::Unit),
            TopLevel::Module(m) => {
                for item in &m.body {
                    self.eval_top_level(item)?;
                }
                Ok(Value::Unit)
            }
            TopLevel::UseDecl(_) => Ok(Value::Unit),
            TopLevel::Task(_) => Ok(Value::Unit),
        }
    }
}

impl Drop for Interpreter {
    fn drop(&mut self) {
        for agent in &mut self.agents {
            let _ = agent.child.kill();
            let _ = agent.child.wait();
        }
    }
}

fn value_to_raw(val: &Value) -> Result<airl_codegen::RawValue, ()> {
    match val {
        Value::Int(v) => Ok(airl_codegen::RawValue::from_i64(*v)),
        Value::Float(v) => Ok(airl_codegen::RawValue::from_f64(*v)),
        Value::Bool(v) => Ok(airl_codegen::RawValue::from_bool(*v)),
        _ => Err(()),
    }
}

fn raw_to_value(raw: airl_codegen::RawValue, ty: &airl_syntax::ast::AstType) -> Value {
    match &ty.kind {
        airl_syntax::ast::AstTypeKind::Named(name) => match name.as_str() {
            "i32" => Value::Int(raw.to_i32() as i64),
            "i64" => Value::Int(raw.to_i64()),
            "f32" => Value::Float(raw.to_f32() as f64),
            "f64" => Value::Float(raw.to_f64()),
            "bool" => Value::Bool(raw.to_bool()),
            _ => Value::Int(raw.to_i64()),
        },
        _ => Value::Int(raw.to_i64()),
    }
}

fn try_tensor_jit(
    tjit: &mut airl_codegen::TensorJit,
    op: &str,
    args: &[Value],
) -> Result<Option<Value>, RuntimeError> {
    match op {
        "tensor.add" | "tensor.mul" => {
            if args.len() != 2 { return Ok(None); }
            let (a, b) = match (&args[0], &args[1]) {
                (Value::Tensor(a), Value::Tensor(b)) => (a.as_ref(), b.as_ref()),
                _ => return Ok(None),
            };
            if a.shape != b.shape {
                return Err(RuntimeError::ShapeMismatch {
                    expected: a.shape.clone(), got: b.shape.clone(),
                });
            }
            let mut out = vec![0.0f64; a.data.len()];
            let r = if op == "tensor.add" {
                tjit.add(&a.data, &b.data, &mut out)
            } else {
                tjit.mul(&a.data, &b.data, &mut out)
            };
            r.map_err(|e| RuntimeError::Custom(e))?;
            Ok(Some(Value::Tensor(Box::new(crate::tensor::TensorValue {
                dtype: a.dtype, shape: a.shape.clone(), data: out,
            }))))
        }
        "tensor.matmul" => {
            if args.len() != 2 { return Ok(None); }
            let (a, b) = match (&args[0], &args[1]) {
                (Value::Tensor(a), Value::Tensor(b)) => (a.as_ref(), b.as_ref()),
                _ => return Ok(None),
            };
            if a.shape.len() != 2 || b.shape.len() != 2 { return Ok(None); }
            let (m, k1) = (a.shape[0], a.shape[1]);
            let (k2, n) = (b.shape[0], b.shape[1]);
            if k1 != k2 {
                return Err(RuntimeError::ShapeMismatch {
                    expected: vec![m, k1], got: vec![k2, n],
                });
            }
            let mut out = vec![0.0f64; m * n];
            tjit.matmul(&a.data, &b.data, &mut out, m, k1, n)
                .map_err(|e| RuntimeError::Custom(e))?;
            Ok(Some(Value::Tensor(Box::new(crate::tensor::TensorValue {
                dtype: a.dtype, shape: vec![m, n], data: out,
            }))))
        }
        _ => Ok(None),
    }
}

fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Nil => false,
        Value::Unit => false,
        Value::Int(0) => false,
        Value::UInt(0) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_str(input: &str) -> Value {
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
        let mut diags = airl_syntax::Diagnostics::new();
        let mut interp = Interpreter::new();
        let mut result = Value::Unit;
        for sexpr in &sexprs {
            let top = airl_syntax::parser::parse_top_level(sexpr, &mut diags);
            match top {
                Ok(top) => result = interp.eval_top_level(&top).unwrap(),
                Err(_) => {
                    let expr = airl_syntax::parser::parse_expr(sexpr, &mut diags).unwrap();
                    result = interp.eval(&expr).unwrap();
                }
            }
        }
        result
    }

    #[test]
    fn eval_integer_literal() {
        assert_eq!(eval_str("42"), Value::Int(42));
    }

    #[test]
    fn eval_arithmetic() {
        assert_eq!(eval_str("(+ 1 2)"), Value::Int(3));
    }

    #[test]
    fn eval_nested_arithmetic() {
        assert_eq!(eval_str("(+ (* 2 3) 4)"), Value::Int(10));
    }

    #[test]
    fn eval_let_binding() {
        assert_eq!(eval_str("(let (x : i32 42) x)"), Value::Int(42));
    }

    #[test]
    fn eval_if_true() {
        assert_eq!(eval_str("(if true 1 2)"), Value::Int(1));
    }

    #[test]
    fn eval_if_false() {
        assert_eq!(eval_str("(if false 1 2)"), Value::Int(2));
    }

    #[test]
    fn eval_nested_let() {
        assert_eq!(
            eval_str("(let (x : i32 1) (y : i32 2) (+ x y))"),
            Value::Int(3)
        );
    }

    #[test]
    fn eval_do_block() {
        assert_eq!(eval_str("(do 1 2 3)"), Value::Int(3));
    }

    #[test]
    fn eval_comparison() {
        assert_eq!(eval_str("(> 5 3)"), Value::Bool(true));
    }

    #[test]
    fn eval_logic() {
        assert_eq!(eval_str("(and true false)"), Value::Bool(false));
    }

    #[test]
    fn eval_string() {
        assert_eq!(eval_str(r#""hello""#), Value::Str("hello".into()));
    }

    #[test]
    fn eval_list() {
        let v = eval_str("[1 2 3]");
        assert_eq!(
            v,
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn eval_variant() {
        assert!(matches!(
            eval_str("(Ok 42)"),
            Value::Variant(ref name, _) if name == "Ok"
        ));
    }

    #[test]
    fn eval_match() {
        assert_eq!(
            eval_str("(match (Ok 42) (Ok v) v (Err e) 0)"),
            Value::Int(42)
        );
    }

    #[test]
    fn eval_lambda() {
        assert_eq!(
            eval_str("(let (f : fn (fn [x] (+ x 1))) (f 5))"),
            Value::Int(6)
        );
    }

    #[test]
    fn eval_defn_and_call() {
        let input = r#"
            (defn add-one
              :sig [(x : i32) -> i32]
              :intent "add one"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body (+ x 1))
            (add-one 5)
        "#;
        assert_eq!(eval_str(input), Value::Int(6));
    }

    #[test]
    fn eval_try_ok() {
        assert_eq!(
            eval_str("(match (Ok 42) (Ok v) v (Err e) 0)"),
            Value::Int(42)
        );
    }

    #[test]
    fn eval_nil() {
        assert_eq!(eval_str("nil"), Value::Nil);
    }

    #[test]
    fn eval_bool_true() {
        assert_eq!(eval_str("true"), Value::Bool(true));
    }

    #[test]
    fn eval_float() {
        assert_eq!(eval_str("3.14"), Value::Float(3.14));
    }

    #[test]
    fn eval_subtraction() {
        assert_eq!(eval_str("(- 10 3)"), Value::Int(7));
    }

    #[test]
    fn eval_multiplication() {
        assert_eq!(eval_str("(* 6 7)"), Value::Int(42));
    }

    #[test]
    fn eval_division() {
        assert_eq!(eval_str("(/ 10 3)"), Value::Int(3));
    }

    #[test]
    fn eval_not() {
        assert_eq!(eval_str("(not true)"), Value::Bool(false));
    }

    #[test]
    fn eval_or() {
        assert_eq!(eval_str("(or false true)"), Value::Bool(true));
    }

    #[test]
    fn eval_eq() {
        assert_eq!(eval_str("(= 1 1)"), Value::Bool(true));
        assert_eq!(eval_str("(= 1 2)"), Value::Bool(false));
    }

    #[test]
    fn eval_if_with_comparison() {
        assert_eq!(eval_str("(if (> 3 2) 10 20)"), Value::Int(10));
    }

    #[test]
    fn eval_let_with_arithmetic() {
        assert_eq!(
            eval_str("(let (x : i32 (+ 1 2)) (* x x))"),
            Value::Int(9)
        );
    }

    #[test]
    fn call_by_name_success() {
        let mut interp = Interpreter::new();
        let input = r#"
            (defn double
              :sig [(x : i32) -> i32]
              :intent "double"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body (* x 2))
        "#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
        let mut diags = airl_syntax::Diagnostics::new();
        for sexpr in &sexprs {
            let top = airl_syntax::parser::parse_top_level(sexpr, &mut diags).unwrap();
            interp.eval_top_level(&top).unwrap();
        }
        let result = interp.call_by_name("double", vec![Value::Int(21)]).unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn call_by_name_not_found() {
        let mut interp = Interpreter::new();
        let result = interp.call_by_name("nonexistent", vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn call_by_name_builtin() {
        let mut interp = Interpreter::new();
        let result = interp.call_by_name("+", vec![Value::Int(3), Value::Int(4)]).unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn eval_use_after_move_errors() {
        let input = r#"
            (defn consume
              :sig [(own x : i32) -> i32]
              :intent "consume x"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body x)
            (let (v : i32 42)
              (do (consume v) v))
        "#;
        let mut lexer = airl_syntax::Lexer::new(input);
        let tokens = lexer.lex_all().unwrap();
        let sexprs = airl_syntax::parse_sexpr_all(&tokens).unwrap();
        let mut diags = airl_syntax::Diagnostics::new();
        let mut interp = Interpreter::new();
        let mut result: Result<Value, RuntimeError> = Ok(Value::Unit);
        for sexpr in &sexprs {
            match airl_syntax::parser::parse_top_level(sexpr, &mut diags) {
                Ok(top) => result = interp.eval_top_level(&top),
                Err(_) => {
                    let expr = airl_syntax::parser::parse_expr(sexpr, &mut diags).unwrap();
                    result = interp.eval(&expr);
                }
            }
        }
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("UseAfterMove") || err.contains("moved"));
    }

    #[test]
    fn jit_transparent_same_result() {
        let input = r#"
            (defn add-nums
              :sig [(a : i64) (b : i64) -> i64]
              :intent "add" :requires [(valid a) (valid b)]
              :ensures [(valid result)]
              :body (+ a b))
            (add-nums 100 200)
        "#;
        assert_eq!(eval_str(input), Value::Int(300));
    }

    #[test]
    fn jit_with_if_expression() {
        let input = r#"
            (defn abs-val
              :sig [(x : i64) -> i64]
              :intent "absolute value" :requires [(valid x)]
              :ensures [(valid result)]
              :body (if (< x 0) (- 0 x) x))
            (abs-val -42)
        "#;
        assert_eq!(eval_str(input), Value::Int(42));
    }

    #[test]
    fn non_jit_function_still_works() {
        // String params -> not JIT eligible, falls back to interpreter
        let input = r#"
            (defn greet
              :sig [(name : String) -> String]
              :intent "greet" :requires [(valid name)]
              :ensures [(valid result)]
              :body name)
            (greet "world")
        "#;
        assert_eq!(eval_str(input), Value::Str("world".into()));
    }

    #[test]
    fn tensor_jit_add_transparent() {
        let input = r#"
            (let (a : tensor (tensor.ones [4]))
              (let (b : tensor (tensor.ones [4]))
                (tensor.add a b)))
        "#;
        let result = eval_str(input);
        if let Value::Tensor(t) = result {
            assert_eq!(t.data, vec![2.0, 2.0, 2.0, 2.0]);
        } else {
            panic!("expected Tensor");
        }
    }

    #[test]
    fn tensor_jit_matmul_transparent() {
        let input = r#"
            (let (a : tensor (tensor.identity 3))
              (let (b : tensor (tensor.identity 3))
                (tensor.matmul a b)))
        "#;
        let result = eval_str(input);
        if let Value::Tensor(t) = result {
            assert_eq!(t.shape, vec![3, 3]);
            assert_eq!(t.data[0], 1.0); // diagonal
            assert_eq!(t.data[4], 1.0);
            assert_eq!(t.data[8], 1.0);
            assert_eq!(t.data[1], 0.0); // off-diagonal
        } else {
            panic!("expected Tensor");
        }
    }

    #[test]
    fn send_to_tcp_agent() {
        use std::net::TcpListener;
        use std::thread;

        // Start a mini agent on TCP in a background thread
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(&stream);
            let mut writer = std::io::BufWriter::new(&stream);

            let _frame = crate::agent_client::read_frame(&mut reader).unwrap();
            // Parse task, respond with result
            let response = format!(r#"(result "t" :status :complete :payload 42)"#);
            crate::agent_client::write_frame(&mut writer, &response).unwrap();
        });

        let mut interp = Interpreter::new();
        let result = interp.builtin_send(&[
            Value::Str(format!("tcp:{}", addr)),
            Value::Str("add".into()),
            Value::Int(3),
            Value::Int(4),
        ]).unwrap();

        assert_eq!(result, Value::Int(42));
        handle.join().unwrap();
    }
}
