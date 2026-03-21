# Agent End-to-End Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable two AIRL agent processes to exchange typed, contract-verified tasks over TCP — `airl agent` (worker) and `airl call` (client).

**Architecture:** New `protocol.rs` handles message serialization. `runtime.rs` gets a receive loop that loads a module, accepts connections, and executes tasks. The driver gets `cmd_agent` and `cmd_call` implementations. Everything runs against the existing `Transport` trait.

**Tech Stack:** Rust, existing AIRL workspace crates, std-only networking.

**Spec:** `docs/superpowers/specs/2026-03-21-agent-wiring-design.md`

---

## File Map

```
crates/
├── airl-agent/src/
│   ├── protocol.rs       # NEW — task/result message serialization
│   ├── runtime.rs        # MODIFY — add run_agent_loop, endpoint parsing
│   └── lib.rs            # MODIFY — export protocol module
│
├── airl-runtime/src/
│   └── eval.rs           # MODIFY — add pub call_by_name method
│
├── airl-driver/src/
│   └── main.rs           # MODIFY — implement cmd_agent, add cmd_call
│
tests/fixtures/agent/
│   └── worker_module.airl  # NEW — test module for agent demo
```

---

## Task 1: Add `call_by_name` to Interpreter

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs`

The agent runtime needs to call functions by name from outside the evaluator. Currently `call_fn` is private.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn call_by_name_success() {
    let mut interp = Interpreter::new();
    // Define a function
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
    // Call by name
    let result = interp.call_by_name("double", vec![Value::Int(21)]).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn call_by_name_not_found() {
    let mut interp = Interpreter::new();
    let result = interp.call_by_name("nonexistent", vec![]);
    assert!(result.is_err());
}
```

- [ ] **Step 2: Implement call_by_name**

Add to `impl Interpreter`:
```rust
/// Call a named function with the given arguments.
/// Used by the agent runtime to execute tasks.
pub fn call_by_name(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    let fn_val = match self.env.get(name)? {
        Value::Function(f) => f.clone(),
        Value::BuiltinFn(name) => {
            let f = self.builtins.get(name).ok_or_else(|| {
                RuntimeError::UndefinedSymbol(name.to_string())
            })?;
            return f(&args);
        }
        other => return Err(RuntimeError::NotCallable(format!(
            "`{}` is {}, not a function", name, other
        ))),
    };
    self.call_fn(&fn_val, args)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-runtime -- call_by_name`
Expected: both tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): add pub call_by_name method to Interpreter"
```

---

## Task 2: Protocol Module — Message Serialization

**Files:**
- Create: `crates/airl-agent/src/protocol.rs`
- Modify: `crates/airl-agent/src/lib.rs`

- [ ] **Step 1: Define message types and write tests first**

```rust
use airl_runtime::value::Value;

/// A task request sent from client to worker.
#[derive(Debug, Clone)]
pub struct TaskMessage {
    pub id: String,
    pub from: String,
    pub call: String,
    pub args: Vec<Value>,
}

/// A result response sent from worker to client.
#[derive(Debug, Clone)]
pub struct ResultMessage {
    pub id: String,
    pub success: bool,
    pub payload: Option<Value>,
    pub error: Option<String>,
}
```

Tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_task_message() {
        let msg = TaskMessage {
            id: "t-001".into(),
            from: "cli".into(),
            call: "add".into(),
            args: vec![Value::Int(3), Value::Int(4)],
        };
        let s = serialize_task(&msg);
        assert!(s.contains("task"));
        assert!(s.contains("t-001"));
        assert!(s.contains(":call"));
        assert!(s.contains("add"));
    }

    #[test]
    fn parse_task_round_trip() {
        let msg = TaskMessage {
            id: "t-002".into(),
            from: "cli".into(),
            call: "multiply".into(),
            args: vec![Value::Int(6), Value::Int(7)],
        };
        let s = serialize_task(&msg);
        let parsed = parse_task(&s).unwrap();
        assert_eq!(parsed.id, "t-002");
        assert_eq!(parsed.call, "multiply");
        assert_eq!(parsed.args.len(), 2);
    }

    #[test]
    fn serialize_result_success() {
        let msg = ResultMessage {
            id: "t-001".into(),
            success: true,
            payload: Some(Value::Int(7)),
            error: None,
        };
        let s = serialize_result(&msg);
        assert!(s.contains("result"));
        assert!(s.contains(":complete"));
        assert!(s.contains("7"));
    }

    #[test]
    fn serialize_result_error() {
        let msg = ResultMessage {
            id: "t-001".into(),
            success: false,
            payload: None,
            error: Some("function not found".into()),
        };
        let s = serialize_result(&msg);
        assert!(s.contains(":error"));
        assert!(s.contains("function not found"));
    }

    #[test]
    fn parse_result_success_round_trip() {
        let msg = ResultMessage {
            id: "t-003".into(),
            success: true,
            payload: Some(Value::Int(42)),
            error: None,
        };
        let s = serialize_result(&msg);
        let parsed = parse_result(&s).unwrap();
        assert_eq!(parsed.id, "t-003");
        assert!(parsed.success);
        assert_eq!(parsed.payload, Some(Value::Int(42)));
    }

    #[test]
    fn sexpr_to_value_integers() {
        assert_eq!(sexpr_to_value_str("42").unwrap(), Value::Int(42));
    }

    #[test]
    fn sexpr_to_value_string() {
        assert_eq!(sexpr_to_value_str(r#""hello""#).unwrap(), Value::Str("hello".into()));
    }

    #[test]
    fn sexpr_to_value_bool() {
        assert_eq!(sexpr_to_value_str("true").unwrap(), Value::Bool(true));
    }
}
```

- [ ] **Step 2: Implement serialization functions**

```rust
/// Serialize a task message to an AIRL S-expression string.
pub fn serialize_task(msg: &TaskMessage) -> String {
    let args_str: Vec<String> = msg.args.iter().map(|v| format!("{}", v)).collect();
    format!(
        r#"(task "{}" :from "{}" :call "{}" :args [{}])"#,
        msg.id, msg.from, msg.call, args_str.join(" ")
    )
}

/// Serialize a result message to an AIRL S-expression string.
pub fn serialize_result(msg: &ResultMessage) -> String {
    if msg.success {
        let payload_str = msg.payload.as_ref()
            .map(|v| format!("{}", v))
            .unwrap_or_else(|| "nil".into());
        format!(
            r#"(result "{}" :status :complete :payload {})"#,
            msg.id, payload_str
        )
    } else {
        let err_str = msg.error.as_deref().unwrap_or("unknown error");
        format!(
            r#"(result "{}" :status :error :message "{}")"#,
            msg.id, err_str.replace('"', "\\\"")
        )
    }
}
```

- [ ] **Step 3: Implement parsing functions**

Parse task and result messages by lexing/parsing as S-expressions, then extracting fields by keyword:

```rust
use airl_syntax::{Lexer, parse_sexpr_all, sexpr::{SExpr, AtomKind}};

/// Parse a task message from an AIRL S-expression string.
pub fn parse_task(input: &str) -> Result<TaskMessage, String> {
    let sexprs = lex_and_parse(input)?;
    let list = match &sexprs[0] {
        SExpr::List(items, _) => items,
        _ => return Err("expected list".into()),
    };
    // First element should be symbol "task"
    // Second element is the task ID (string)
    // Then keyword-value pairs: :from, :call, :args
    // ... extract fields by walking items and matching keywords
}

/// Parse a result message from an AIRL S-expression string.
pub fn parse_result(input: &str) -> Result<ResultMessage, String> {
    // Similar: first "result", then id, then :status, :payload/:message
}

/// Convert an S-expression atom to a Value.
pub fn sexpr_to_value(sexpr: &SExpr) -> Result<Value, String> {
    match sexpr {
        SExpr::Atom(atom) => match &atom.kind {
            AtomKind::Integer(v) => Ok(Value::Int(*v)),
            AtomKind::Float(v) => Ok(Value::Float(*v)),
            AtomKind::Str(v) => Ok(Value::Str(v.clone())),
            AtomKind::Bool(v) => Ok(Value::Bool(*v)),
            AtomKind::Nil => Ok(Value::Nil),
            AtomKind::Symbol(s) => {
                // Capitalized symbols might be variant constructors
                Ok(Value::Str(s.clone())) // treat as string for now
            }
            AtomKind::Keyword(k) => Ok(Value::Str(format!(":{}", k))),
            AtomKind::Arrow => Ok(Value::Str("->".into())),
        }
        SExpr::List(items, _) => {
            // Could be a variant: (Ok 42)
            if let Some(SExpr::Atom(a)) = items.first() {
                if let AtomKind::Symbol(name) = &a.kind {
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) && items.len() == 2 {
                        let inner = sexpr_to_value(&items[1])?;
                        return Ok(Value::Variant(name.clone(), Box::new(inner)));
                    }
                }
            }
            // Otherwise treat as a list
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
        SExpr::BracketList(items, _) => {
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
    }
}

/// Convenience: parse a single value from a string.
pub fn sexpr_to_value_str(input: &str) -> Result<Value, String> {
    let sexprs = lex_and_parse(input)?;
    sexpr_to_value(&sexprs[0])
}

fn lex_and_parse(input: &str) -> Result<Vec<SExpr>, String> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    parse_sexpr_all(&tokens).map_err(|d| d.message)
}
```

- [ ] **Step 4: Update lib.rs**

Add `pub mod protocol;` to `crates/airl-agent/src/lib.rs`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p airl-agent -- protocol`
Expected: all protocol tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/airl-agent/src/protocol.rs crates/airl-agent/src/lib.rs
git commit -m "feat(agent): add protocol module for task/result message serialization"
```

---

## Task 3: Endpoint Parsing Utility

**Files:**
- Modify: `crates/airl-agent/src/runtime.rs`

- [ ] **Step 1: Write tests**

```rust
#[test]
fn parse_tcp_endpoint() {
    let ep = parse_endpoint("tcp:127.0.0.1:9001").unwrap();
    assert!(matches!(ep, Endpoint::Tcp(addr) if addr.port() == 9001));
}

#[test]
fn parse_unix_endpoint() {
    let ep = parse_endpoint("unix:/tmp/airl.sock").unwrap();
    assert!(matches!(ep, Endpoint::Unix(ref p) if p.to_str().unwrap() == "/tmp/airl.sock"));
}

#[test]
fn parse_invalid_endpoint() {
    assert!(parse_endpoint("garbage").is_err());
}
```

- [ ] **Step 2: Implement parse_endpoint**

```rust
use crate::identity::Endpoint;
use std::net::SocketAddr;
use std::path::PathBuf;

pub fn parse_endpoint(s: &str) -> Result<Endpoint, String> {
    if let Some(addr_str) = s.strip_prefix("tcp:") {
        let addr: SocketAddr = addr_str.parse()
            .map_err(|e| format!("invalid TCP address '{}': {}", addr_str, e))?;
        Ok(Endpoint::Tcp(addr))
    } else if let Some(path_str) = s.strip_prefix("unix:") {
        Ok(Endpoint::Unix(PathBuf::from(path_str)))
    } else {
        Err(format!("unknown endpoint format: '{}' (expected tcp:HOST:PORT or unix:/path)", s))
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git commit -m "feat(agent): add endpoint parsing utility"
```

---

## Task 4: Agent Receive Loop

**Files:**
- Modify: `crates/airl-agent/src/runtime.rs`

This is the core of the worker. It loads a module, binds a listener, and processes tasks.

- [ ] **Step 1: Implement run_agent_loop**

```rust
use std::net::TcpListener;
use crate::tcp_transport::TcpTransport;
use crate::transport::Transport;
use crate::protocol::{parse_task, serialize_result, sexpr_to_value, ResultMessage};
use airl_runtime::eval::Interpreter;
use airl_runtime::value::Value;

/// Load an AIRL module file and start listening for tasks.
pub fn run_agent_loop(module_path: &str, endpoint: &Endpoint) -> Result<(), AgentError> {
    // 1. Load module
    let source = std::fs::read_to_string(module_path)
        .map_err(|e| AgentError::Protocol(format!("cannot read {}: {}", module_path, e)))?;

    let mut interp = Interpreter::new();
    load_module(&source, &mut interp)?;

    eprintln!("Agent loaded: {}", module_path);

    // 2. Bind listener
    match endpoint {
        Endpoint::Tcp(addr) => {
            let listener = TcpListener::bind(addr)
                .map_err(|e| AgentError::Protocol(format!("cannot bind {}: {}", addr, e)))?;
            eprintln!("Listening on tcp:{}", addr);

            // 3. Accept loop
            loop {
                let (stream, peer) = listener.accept()
                    .map_err(|e| AgentError::Protocol(format!("accept error: {}", e)))?;
                eprintln!("Connection from {}", peer);

                let mut transport = TcpTransport::from_stream(stream);
                handle_connection(&mut transport, &mut interp);
                eprintln!("Connection closed from {}", peer);
            }
        }
        _ => Err(AgentError::Protocol("only TCP listeners supported in Phase 1".into())),
    }
}

fn load_module(source: &str, interp: &mut Interpreter) -> Result<(), AgentError> {
    let mut lexer = airl_syntax::Lexer::new(source);
    let tokens = lexer.lex_all()
        .map_err(|d| AgentError::Protocol(format!("parse error: {}", d.message)))?;
    let sexprs = airl_syntax::parse_sexpr_all(&tokens)
        .map_err(|d| AgentError::Protocol(format!("parse error: {}", d.message)))?;
    let mut diags = airl_syntax::Diagnostics::new();

    for sexpr in &sexprs {
        match airl_syntax::parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => {
                interp.eval_top_level(&top)
                    .map_err(|e| AgentError::Protocol(format!("module error: {}", e)))?;
            }
            Err(_) => {} // skip unparseable forms
        }
    }
    Ok(())
}

fn handle_connection(transport: &mut dyn Transport, interp: &mut Interpreter) {
    loop {
        let frame = match transport.recv_message() {
            Ok(f) => f,
            Err(_) => break, // disconnected or error → close connection
        };

        let result_msg = match parse_task(&frame) {
            Ok(task) => {
                eprintln!("Task {}: calling {}({:?})", task.id, task.call, task.args);
                match interp.call_by_name(&task.call, task.args) {
                    Ok(value) => ResultMessage {
                        id: task.id,
                        success: true,
                        payload: Some(value),
                        error: None,
                    },
                    Err(e) => ResultMessage {
                        id: task.id,
                        success: false,
                        payload: None,
                        error: Some(format!("{}", e)),
                    },
                }
            }
            Err(e) => ResultMessage {
                id: "unknown".into(),
                success: false,
                payload: None,
                error: Some(format!("protocol error: {}", e)),
            },
        };

        let response = serialize_result(&result_msg);
        if transport.send_message(&response).is_err() {
            break;
        }
    }
}
```

- [ ] **Step 2: Write integration test**

```rust
#[test]
fn agent_loop_integration() {
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    // Write a temp module file
    let dir = std::env::temp_dir().join("airl-test-agent");
    std::fs::create_dir_all(&dir).ok();
    let module_path = dir.join("worker.airl");
    std::fs::write(&module_path, r#"
        (defn add
          :sig [(a : i32) (b : i32) -> i32]
          :intent "add"
          :requires [(valid a) (valid b)]
          :ensures [(= result (+ a b))]
          :body (+ a b))
    "#).unwrap();

    // Find a free port
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let module_path_str = module_path.to_str().unwrap().to_string();
    let endpoint = Endpoint::Tcp(addr);

    // Start agent in background thread
    let handle = thread::spawn(move || {
        // run_agent_loop blocks, so we just let it run
        let _ = run_agent_loop(&module_path_str, &endpoint);
    });

    // Give agent time to bind
    thread::sleep(Duration::from_millis(100));

    // Connect as client
    let mut client = TcpTransport::connect(addr).unwrap();
    let task_str = r#"(task "t-1" :from "test" :call "add" :args [3 4])"#;
    client.send_message(task_str).unwrap();
    let response = client.recv_message().unwrap();
    client.close().ok();

    // Parse response
    let result = parse_result(&response).unwrap();
    assert!(result.success);
    assert_eq!(result.payload, Some(Value::Int(7)));

    // Cleanup
    std::fs::remove_file(&module_path).ok();
    std::fs::remove_dir(&dir).ok();
    // Note: agent thread will exit when test drops
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-agent -- agent_loop`
Expected: pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-agent/src/runtime.rs
git commit -m "feat(agent): add agent receive loop with module loading"
```

---

## Task 5: CLI — `cmd_agent` and `cmd_call`

**Files:**
- Modify: `crates/airl-driver/src/main.rs`

- [ ] **Step 1: Implement cmd_agent**

Replace the stub:
```rust
fn cmd_agent(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: airl agent <file.airl> --listen <endpoint>");
        std::process::exit(1);
    }

    let module_path = &args[0];
    let endpoint_str = find_flag(args, "--listen").unwrap_or_else(|| {
        eprintln!("error: --listen <endpoint> required (e.g., --listen tcp:127.0.0.1:9001)");
        std::process::exit(1);
    });

    let endpoint = airl_agent::runtime::parse_endpoint(&endpoint_str).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1);
    });

    if let Err(e) = airl_agent::runtime::run_agent_loop(module_path, &endpoint) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    for (i, arg) in args.iter().enumerate() {
        if arg == flag {
            return args.get(i + 1).cloned();
        }
    }
    None
}
```

- [ ] **Step 2: Implement cmd_call**

```rust
fn cmd_call(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: airl call <endpoint> <function> [args...]");
        std::process::exit(1);
    }

    let endpoint_str = &args[0];
    let fn_name = &args[1];
    let fn_args = &args[2..];

    let endpoint = airl_agent::runtime::parse_endpoint(endpoint_str).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1);
    });

    // Parse CLI args to Values
    let arg_values: Vec<airl_runtime::value::Value> = fn_args.iter().map(|s| {
        parse_cli_arg(s)
    }).collect();

    // Build task message
    let task = airl_agent::protocol::TaskMessage {
        id: "call-0".into(),
        from: "cli".into(),
        call: fn_name.clone(),
        args: arg_values,
    };
    let task_str = airl_agent::protocol::serialize_task(&task);

    // Connect and send
    match endpoint {
        airl_agent::identity::Endpoint::Tcp(addr) => {
            let mut transport = airl_agent::tcp_transport::TcpTransport::connect(addr)
                .unwrap_or_else(|e| {
                    eprintln!("error: cannot connect to {}: {}", addr, e);
                    std::process::exit(1);
                });
            transport.send_message(&task_str).unwrap_or_else(|e| {
                eprintln!("error: send failed: {}", e);
                std::process::exit(1);
            });
            let response = transport.recv_message().unwrap_or_else(|e| {
                eprintln!("error: recv failed: {}", e);
                std::process::exit(1);
            });
            transport.close().ok();

            // Parse and display result
            match airl_agent::protocol::parse_result(&response) {
                Ok(result) => {
                    if result.success {
                        if let Some(payload) = result.payload {
                            println!("{}", payload);
                        }
                    } else {
                        eprintln!("error: {}", result.error.unwrap_or_default());
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("error: bad response: {}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("error: only TCP endpoints supported for `airl call`");
            std::process::exit(1);
        }
    }
}

fn parse_cli_arg(s: &str) -> airl_runtime::value::Value {
    if let Ok(i) = s.parse::<i64>() {
        return airl_runtime::value::Value::Int(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return airl_runtime::value::Value::Float(f);
    }
    match s {
        "true" => airl_runtime::value::Value::Bool(true),
        "false" => airl_runtime::value::Value::Bool(false),
        "nil" => airl_runtime::value::Value::Nil,
        _ => {
            // Strip quotes if present
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                airl_runtime::value::Value::Str(s[1..s.len()-1].to_string())
            } else {
                airl_runtime::value::Value::Str(s.to_string())
            }
        }
    }
}
```

- [ ] **Step 3: Add "call" to main match**

```rust
match args.get(1).map(|s| s.as_str()) {
    Some("run") => cmd_run(&args[2..]),
    Some("check") => cmd_check(&args[2..]),
    Some("repl") => cmd_repl(),
    Some("agent") => cmd_agent(&args[2..]),
    Some("call") => cmd_call(&args[2..]),    // NEW
    Some("fmt") => cmd_fmt(&args[2..]),
    Some("--version") | Some("-V") => println!("airl 0.1.0"),
    _ => print_usage(),
}
```

Update `print_usage` to include `call`.

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass, binary compiles

- [ ] **Step 5: Commit**

```bash
git add crates/airl-driver/src/main.rs
git commit -m "feat(driver): implement airl agent and airl call CLI commands"
```

---

## Task 6: Worker Module Fixture and Integration Test

**Files:**
- Create: `tests/fixtures/agent/worker_module.airl`
- Modify: `crates/airl-driver/tests/fixtures.rs` (optional — add agent fixture)

- [ ] **Step 1: Create worker module fixture**

`tests/fixtures/agent/worker_module.airl`:
```clojure
;; Worker module for agent integration testing

(defn add
  :sig [(a : i32) (b : i32) -> i32]
  :intent "Add two integers"
  :requires [(valid a) (valid b)]
  :ensures [(= result (+ a b))]
  :body (+ a b))

(defn multiply
  :sig [(a : i32) (b : i32) -> i32]
  :intent "Multiply two integers"
  :requires [(valid a) (valid b)]
  :ensures [(= result (* a b))]
  :body (* a b))

(defn greet
  :sig [(name : String) -> String]
  :intent "Greet by name"
  :requires [(valid name)]
  :ensures [(valid result)]
  :body name)
```

- [ ] **Step 2: Write end-to-end test using the fixture**

In `crates/airl-agent/tests/e2e.rs` (or add to existing test file):
```rust
#[test]
fn e2e_agent_call_add() {
    // Uses the worker_module.airl fixture
    // Start agent, connect, send add task, verify result is 7
}

#[test]
fn e2e_agent_call_nonexistent() {
    // Call a function that doesn't exist
    // Verify error result
}

#[test]
fn e2e_agent_multiple_calls() {
    // Send add then multiply on same connection
    // Verify both results
}
```

- [ ] **Step 3: Run tests, commit**

```bash
git add tests/fixtures/agent/worker_module.airl crates/airl-agent/tests/
git commit -m "test: add agent worker module fixture and E2E tests"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all tests pass (360 existing + new tests)

- [ ] **Step 2: Manual demo**

Terminal 1:
```bash
cargo run -- agent tests/fixtures/agent/worker_module.airl --listen tcp:127.0.0.1:9001
```

Terminal 2:
```bash
cargo run -- call tcp:127.0.0.1:9001 add 3 4
# Expected: 7

cargo run -- call tcp:127.0.0.1:9001 multiply 6 7
# Expected: 42

cargo run -- call tcp:127.0.0.1:9001 nonexistent 1 2
# Expected: error message
```

- [ ] **Step 3: Commit**

```bash
git commit -m "chore: agent wiring complete — end-to-end task exchange working"
```

---

## Future Work Notes (B — `send`/`await` Builtins)

When implementing `send`/`await` as interpreter builtins:

1. **Circular dependency:** The interpreter needs the agent runtime (to send), the agent runtime needs the interpreter (to execute). Resolve with `Arc<Mutex<AgentRuntime>>` passed into the interpreter, or a callback trait.

2. **`send` builtin:** Takes endpoint + function name + args, creates transport, sends task, returns task ID.

3. **`await` builtin:** Takes task ID + timeout, blocks on result, returns payload or error.

4. **`spawn-agent` builtin:** Launches child process with `airl agent`, connects via StdioTransport.

5. **`parallel` builtin:** Spawns threads for each task, collects results.

6. **Concurrent connections:** Replace single-threaded accept loop with thread-per-connection or async.
