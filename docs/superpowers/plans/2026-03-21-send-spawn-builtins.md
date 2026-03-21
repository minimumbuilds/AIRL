# send/spawn-agent Builtins Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable AIRL programs to spawn worker agents and dispatch tasks to them via `spawn-agent` and `send` builtins.

**Architecture:** New `agent_client.rs` in `airl-runtime` handles framing and protocol (duplicated from `airl-agent` to avoid circular dep). `spawn-agent` launches child processes via stdio. `send` dispatches synchronous tasks by agent name or endpoint. Agent runtime updated to support stdio listeners.

**Tech Stack:** Rust, std process/networking, existing AIRL crates.

**Spec:** `docs/superpowers/specs/2026-03-21-send-spawn-builtins-design.md`

---

## File Map

```
crates/
├── airl-runtime/src/
│   ├── agent_client.rs     # NEW — framing, task/result serialization for client side
│   ├── eval.rs             # MODIFY — add agents list, spawn-agent/send builtins
│   └── lib.rs              # MODIFY — export agent_client
│
├── airl-agent/src/
│   └── runtime.rs          # MODIFY — add stdio support to run_agent_loop
│
├── airl-driver/src/
│   └── main.rs             # MODIFY — handle --listen stdio in cmd_agent
│
tests/fixtures/
├── valid/
│   └── orchestrator.airl   # NEW — spawn + send demo
```

---

## Task 1: Agent Client Module (`agent_client.rs`)

**Files:**
- Create: `crates/airl-runtime/src/agent_client.rs`
- Modify: `crates/airl-runtime/src/lib.rs`

Duplicate the minimal framing and protocol logic from `airl-agent` to avoid circular dependency.

- [ ] **Step 1: Implement framing and protocol**

```rust
use std::io::{self, Read, Write};
use crate::value::Value;

/// Write a length-prefixed frame: [u32 BE length][UTF-8 payload].
pub fn write_frame(writer: &mut dyn Write, payload: &str) -> io::Result<()> {
    let bytes = payload.as_bytes();
    let len = bytes.len() as u32;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

/// Read a length-prefixed frame.
pub fn read_frame(reader: &mut dyn Read) -> io::Result<String> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Format a task message as an AIRL S-expression.
pub fn format_task(id: &str, fn_name: &str, args: &[Value]) -> String {
    let args_str: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
    format!(
        r#"(task "{}" :from "self" :call "{}" :args [{}])"#,
        id, fn_name, args_str.join(" ")
    )
}

/// Parse a result message. Returns Ok(value) on success, Err(message) on failure.
pub fn parse_result_message(response: &str) -> Result<Value, String> {
    // Parse as S-expression
    let mut lexer = airl_syntax::Lexer::new(response);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    let sexprs = airl_syntax::parse_sexpr_all(&tokens).map_err(|d| d.message)?;

    if sexprs.is_empty() {
        return Err("empty response".into());
    }

    let items = match &sexprs[0] {
        airl_syntax::sexpr::SExpr::List(items, _) => items,
        _ => return Err("expected list".into()),
    };

    // Walk items looking for :status and :payload/:message
    let mut status_complete = false;
    let mut payload: Option<Value> = None;
    let mut error_msg: Option<String> = None;

    let mut i = 0;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "status" => {
                    if i + 1 < items.len() {
                        if let Some(s) = items[i + 1].as_keyword() {
                            status_complete = s == "complete";
                        }
                        i += 1;
                    }
                }
                "payload" => {
                    if i + 1 < items.len() {
                        payload = sexpr_to_value(&items[i + 1]).ok();
                        i += 1;
                    }
                }
                "message" => {
                    if i + 1 < items.len() {
                        if let airl_syntax::sexpr::SExpr::Atom(a) = &items[i + 1] {
                            if let airl_syntax::sexpr::AtomKind::Str(s) = &a.kind {
                                error_msg = Some(s.clone());
                            }
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    if status_complete {
        Ok(payload.unwrap_or(Value::Unit))
    } else {
        Err(error_msg.unwrap_or_else(|| "unknown error".into()))
    }
}

/// Convert an S-expression to a Value (for parsing result payloads).
fn sexpr_to_value(sexpr: &airl_syntax::sexpr::SExpr) -> Result<Value, String> {
    use airl_syntax::sexpr::{SExpr, AtomKind};
    match sexpr {
        SExpr::Atom(a) => match &a.kind {
            AtomKind::Integer(v) => Ok(Value::Int(*v)),
            AtomKind::Float(v) => Ok(Value::Float(*v)),
            AtomKind::Str(v) => Ok(Value::Str(v.clone())),
            AtomKind::Bool(v) => Ok(Value::Bool(*v)),
            AtomKind::Nil => Ok(Value::Nil),
            AtomKind::Symbol(s) => Ok(Value::Str(s.clone())),
            AtomKind::Keyword(k) => Ok(Value::Str(format!(":{}", k))),
            AtomKind::Arrow => Ok(Value::Str("->".into())),
        }
        SExpr::List(items, _) => {
            // Check for variant: (Ok 42)
            if let Some(SExpr::Atom(a)) = items.first() {
                if let AtomKind::Symbol(name) = &a.kind {
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) && items.len() == 2 {
                        let inner = sexpr_to_value(&items[1])?;
                        return Ok(Value::Variant(name.clone(), Box::new(inner)));
                    }
                }
            }
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
        SExpr::BracketList(items, _) => {
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_round_trip() {
        let msg = "hello";
        let mut buf = Vec::new();
        write_frame(&mut buf, msg).unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(read_frame(&mut cursor).unwrap(), "hello");
    }

    #[test]
    fn format_task_message() {
        let msg = format_task("t-1", "add", &[Value::Int(3), Value::Int(4)]);
        assert!(msg.contains("task"));
        assert!(msg.contains("t-1"));
        assert!(msg.contains(":call"));
        assert!(msg.contains("add"));
        assert!(msg.contains("3"));
        assert!(msg.contains("4"));
    }

    #[test]
    fn parse_success_result() {
        let response = r#"(result "t-1" :status :complete :payload 42)"#;
        let val = parse_result_message(response).unwrap();
        assert_eq!(val, Value::Int(42));
    }

    #[test]
    fn parse_error_result() {
        let response = r#"(result "t-1" :status :error :message "not found")"#;
        let err = parse_result_message(response).unwrap_err();
        assert!(err.contains("not found"));
    }
}
```

- [ ] **Step 2: Update lib.rs**

Add `pub mod agent_client;` to `crates/airl-runtime/src/lib.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p airl-runtime -- agent_client`
Expected: all 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/airl-runtime/src/agent_client.rs crates/airl-runtime/src/lib.rs
git commit -m "feat(runtime): add agent_client module for framing and protocol"
```

---

## Task 2: Stdio Support in Agent Runtime

**Files:**
- Modify: `crates/airl-agent/src/runtime.rs`
- Modify: `crates/airl-driver/src/main.rs`

- [ ] **Step 1: Add stdio handling to run_agent_loop**

In `crates/airl-agent/src/runtime.rs`, update the `match endpoint` in `run_agent_loop` to handle `Endpoint::Stdio`:

```rust
match endpoint {
    Endpoint::Tcp(addr) => {
        // ... existing TCP code ...
    }
    Endpoint::Stdio => {
        eprintln!("Agent listening on stdio");
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut reader = std::io::BufReader::new(stdin.lock());
        let mut writer = std::io::BufWriter::new(stdout.lock());

        // Use a simple wrapper that implements the same loop
        handle_stdio_connection(&mut reader, &mut writer, &mut interp);
        Ok(())
    }
    _ => Err(AgentError::Protocol("unsupported endpoint type".into())),
}
```

Add a `handle_stdio_connection` function that uses `read_frame`/`write_frame` directly on the reader/writer (since StdioTransport spawns a child, but here we ARE the child):

```rust
fn handle_stdio_connection(
    reader: &mut dyn std::io::Read,
    writer: &mut dyn std::io::Write,
    interp: &mut Interpreter,
) {
    use crate::transport::{read_frame, write_frame};

    loop {
        let frame = match read_frame(reader) {
            Ok(f) => f,
            Err(_) => break,
        };

        let result_msg = match parse_task(&frame) {
            Ok(task) => {
                eprintln!("Task {}: calling {}({:?})", task.id, task.call, task.args);
                match interp.call_by_name(&task.call, task.args) {
                    Ok(value) => ResultMessage {
                        id: task.id, success: true, payload: Some(value), error: None,
                    },
                    Err(e) => ResultMessage {
                        id: task.id, success: false, payload: None, error: Some(format!("{}", e)),
                    },
                }
            }
            Err(e) => ResultMessage {
                id: "unknown".into(), success: false, payload: None,
                error: Some(format!("protocol error: {}", e)),
            },
        };

        let response = serialize_result(&result_msg);
        if write_frame(writer, &response).is_err() {
            break;
        }
    }
}
```

- [ ] **Step 2: Update cmd_agent to accept --listen stdio**

In `crates/airl-driver/src/main.rs`, the `cmd_agent` function already calls `parse_endpoint` which recognizes "stdio" — but we need to handle it. Update `parse_endpoint` in `airl-agent/src/runtime.rs` to accept "stdio":

```rust
pub fn parse_endpoint(s: &str) -> Result<Endpoint, String> {
    if s == "stdio" {
        Ok(Endpoint::Stdio)
    } else if let Some(addr_str) = s.strip_prefix("tcp:") {
        // ... existing ...
    } else if let Some(path_str) = s.strip_prefix("unix:") {
        // ... existing ...
    } else {
        Err(format!("unknown endpoint format: '{}' (expected tcp:HOST:PORT, unix:/path, or stdio)", s))
    }
}
```

- [ ] **Step 3: Write test**

```rust
#[test]
fn parse_stdio_endpoint() {
    let ep = parse_endpoint("stdio").unwrap();
    assert!(matches!(ep, Endpoint::Stdio));
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/airl-agent/src/runtime.rs crates/airl-driver/src/main.rs
git commit -m "feat(agent): add stdio support to agent receive loop"
```

---

## Task 3: spawn-agent and send Builtins

**Files:**
- Modify: `crates/airl-runtime/src/eval.rs`

This is the core task — add the agent list to Interpreter and implement both builtins.

- [ ] **Step 1: Add agent tracking to Interpreter**

```rust
use std::io::{BufReader, BufWriter};
use std::process::{Child, Command, Stdio};

struct LiveAgent {
    name: String,
    writer: BufWriter<std::process::ChildStdin>,
    reader: BufReader<std::process::ChildStdout>,
    child: Child,
}

pub struct Interpreter {
    pub env: Env,
    builtins: Builtins,
    pub jit: Option<airl_codegen::JitCache>,
    pub tensor_jit: Option<airl_codegen::TensorJit>,
    agents: Vec<LiveAgent>,
    next_agent_id: u32,
    next_send_id: u32,
}
```

Initialize in `new()`:
```rust
agents: Vec::new(),
next_agent_id: 0,
next_send_id: 0,
```

Register builtin names in `register_builtin_symbols`:
```rust
"spawn-agent", "send",
```

- [ ] **Step 2: Implement builtin_spawn_agent**

```rust
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
        writer: BufWriter::new(stdin),
        reader: BufReader::new(stdout),
        child,
    });

    // Give agent a moment to load
    std::thread::sleep(std::time::Duration::from_millis(100));

    Ok(Value::Str(name))
}
```

- [ ] **Step 3: Implement builtin_send**

```rust
fn builtin_send(&mut self, args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() < 2 {
        return Err(RuntimeError::TypeError("send requires at least 2 args: target, function, [args...]".into()));
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
        // Direct connection
        self.send_to_endpoint(&target, &task_msg)
    } else {
        // Agent name lookup
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
    let agent = self.agents.iter_mut().find(|a| a.name == name)
        .ok_or_else(|| RuntimeError::Custom(format!("unknown agent: {}", name)))?;

    crate::agent_client::write_frame(&mut agent.writer, task_msg)
        .map_err(|e| RuntimeError::Custom(format!("send to {} failed: {}", name, e)))?;
    let response = crate::agent_client::read_frame(&mut agent.reader)
        .map_err(|e| RuntimeError::Custom(format!("recv from {} failed: {}", name, e)))?;

    crate::agent_client::parse_result_message(&response)
        .map_err(|e| RuntimeError::Custom(e))
}
```

- [ ] **Step 4: Wire into FnCall**

In the FnCall arm, before the tensor JIT check, add:

```rust
if let Value::BuiltinFn(ref name) = callee_val {
    match name.as_str() {
        "spawn-agent" => {
            let result = self.builtin_spawn_agent(&arg_vals);
            // Release borrows
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
        _ => {}
    }
}
```

- [ ] **Step 5: Implement Drop for cleanup**

```rust
impl Drop for Interpreter {
    fn drop(&mut self) {
        for agent in &mut self.agents {
            let _ = agent.child.kill();
            let _ = agent.child.wait();
        }
    }
}
```

- [ ] **Step 6: Write tests**

```rust
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

        let frame = crate::agent_client::read_frame(&mut reader).unwrap();
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
```

Note: Testing `spawn-agent` is harder because it requires the binary to be built. A full E2E test is in Task 4.

- [ ] **Step 7: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): add spawn-agent and send builtins"
```

---

## Task 4: E2E Demo and Fixture

**Files:**
- Create: `tests/fixtures/valid/orchestrator.airl`

- [ ] **Step 1: Create orchestrator fixture**

`tests/fixtures/valid/orchestrator.airl`:
```clojure
;; EXPECT: 30
;; Spawns a worker agent and sends it a task
(let (w (spawn-agent "tests/fixtures/agent/worker_module.airl"))
  (send w "add" 10 20))
```

Note: This fixture requires the `airl` binary to be built, so it won't work in the fixture runner (which uses `run_source`, not the binary). This is a manual demo fixture.

- [ ] **Step 2: Manual E2E test**

```bash
cargo build
cargo run -- run tests/fixtures/valid/orchestrator.airl
# Expected: 30
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test --workspace`
Expected: all 422+ tests pass

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/valid/orchestrator.airl
git commit -m "feat: add orchestrator fixture — spawn + send demo"
```

---

## Task 5: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`

- [ ] **Step 2: Demo: spawn + send**

```bash
cargo run -- run tests/fixtures/valid/orchestrator.airl
# Expected: 30
```

- [ ] **Step 3: Demo: send to TCP agent**

Terminal 1: `cargo run -- agent tests/fixtures/agent/worker_module.airl --listen tcp:127.0.0.1:9876`
Terminal 2: `cargo run -- run -c '(send "tcp:127.0.0.1:9876" "multiply" 6 7)'`
(Or test via `airl call` which already works)

- [ ] **Step 4: Commit**

```bash
git commit -m "chore: send/spawn-agent builtins complete — programmatic agent orchestration"
```
