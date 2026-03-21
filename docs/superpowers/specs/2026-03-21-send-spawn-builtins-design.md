# `send`/`spawn-agent` Builtins Design

**Date:** 2026-03-21
**Status:** Approved
**Depends on:** Agent Wiring (TCP task exchange working, 422 tests)

---

## Overview

Add two builtins to the AIRL interpreter so programs can programmatically spawn worker agents and dispatch tasks to them. `spawn-agent` launches a child process running `airl agent` via stdio. `send` dispatches a synchronous task to a named agent or endpoint and returns the result.

---

## 1. `spawn-agent` Builtin

```clojure
(spawn-agent "path/to/module.airl")
;; Returns: "agent-0" (agent name string)
```

### Behavior

1. Extract module path from first argument (`Value::Str`)
2. Find the `airl` binary via `std::env::current_exe()`
3. Spawn child process: `airl agent <module_path> --listen stdio`
   - stdin: piped (parent writes to child)
   - stdout: piped (parent reads from child)
   - stderr: inherited (child logs visible to parent)
4. Generate agent name: `"agent-N"` (incrementing counter)
5. Store `LiveAgent { name, stdin_writer, stdout_reader, child }` in interpreter's agent list
6. Return `Value::Str(name)`

### LiveAgent

```rust
struct LiveAgent {
    name: String,
    writer: std::io::BufWriter<std::process::ChildStdin>,
    reader: std::io::BufReader<std::process::ChildStdout>,
    child: std::process::Child,
}
```

Uses `BufWriter`/`BufReader` for buffered I/O over the child's stdin/stdout.

### Cleanup

When `LiveAgent` is dropped (or interpreter is dropped), call `child.kill()` and `child.wait()` to clean up the child process.

---

## 2. `send` Builtin

```clojure
(send "agent-0" "add" 3 4)              ;; by agent name
(send "tcp:127.0.0.1:9001" "add" 3 4)  ;; by endpoint
;; Returns: the result value (synchronous)
```

### Behavior

1. First arg: agent name or endpoint string (`Value::Str`)
2. Second arg: function name (`Value::Str`)
3. Remaining args: function arguments (any `Value`)
4. Resolve transport:
   - If starts with `tcp:` → parse address, create `TcpStream`, use directly
   - If starts with `unix:` → parse path, create `UnixStream`, use directly
   - Otherwise → look up in `self.agents` by name → use stored writer/reader
5. Build task message: `(task "send-N" :from "self" :call "fn" :args [...])`
6. Write frame (u32 BE length + UTF-8 payload)
7. Read response frame
8. Parse result message, extract payload or error
9. For TCP/Unix: close the stream after the response
10. Return `Value` payload, or `RuntimeError` on failure

### Task/Result Serialization

Duplicated in `airl-runtime` to avoid circular dependency with `airl-agent`. The format is identical:

```clojure
;; Request
(task "send-0" :from "self" :call "add" :args [3 4])

;; Response (success)
(result "send-0" :status :complete :payload 7)

;; Response (error)
(result "send-0" :status :error :message "UndefinedSymbol: `foo`")
```

The serialization is simple string formatting. The parsing uses the existing lexer + S-expression parser to extract fields by keyword.

---

## 3. Agent Registry on Interpreter

```rust
pub struct Interpreter {
    pub env: Env,
    builtins: Builtins,
    pub jit: Option<JitCache>,
    pub tensor_jit: Option<TensorJit>,
    agents: Vec<LiveAgent>,        // NEW
    next_agent_id: u32,            // NEW
    next_send_id: u32,             // NEW — for task ID generation
}
```

`agents` is a simple `Vec` — lookup by name is O(n) but the number of agents is small.

---

## 4. Stdio Support in Agent Runtime

The existing `run_agent_loop` in `airl-agent/src/runtime.rs` only supports TCP listeners. Update it to handle `Endpoint::Stdio`:

- When endpoint is `Stdio`: use `std::io::stdin()` and `std::io::stdout()` directly
- Same framing protocol, same receive loop
- No listener accept loop — just one connection (stdin/stdout)

The CLI `airl agent module.airl --listen stdio` uses this path.

---

## 5. Circular Dependency Solution

`airl-runtime` cannot import from `airl-agent` (agent depends on runtime). The framing and protocol are duplicated in a new file `airl-runtime/src/agent_client.rs`:

```rust
// Minimal framing: write/read length-prefixed frames
pub fn write_frame(writer: &mut dyn Write, payload: &str) -> io::Result<()>
pub fn read_frame(reader: &mut dyn Read) -> io::Result<String>

// Task/result message formatting
pub fn format_task(id: &str, fn_name: &str, args: &[Value]) -> String
pub fn parse_result(response: &str) -> Result<Value, String>
```

This is ~50 lines of code. The duplication is intentional — it avoids restructuring the crate dependency graph.

---

## 6. FnCall Integration

In eval.rs's FnCall arm, `spawn-agent` and `send` are handled before the regular builtin dispatch (same pattern as tensor JIT):

```rust
Value::BuiltinFn(ref name) => {
    match name.as_str() {
        "spawn-agent" => return self.builtin_spawn_agent(&arg_vals),
        "send" => return self.builtin_send(&arg_vals),
        // tensor JIT checks...
        // regular builtin dispatch...
    }
}
```

These are methods on `Interpreter` (not free functions) because they need `&mut self` to access the agent list.

---

## 7. Testing

### Unit tests (agent_client.rs)
- Frame round-trip: write + read
- Task message formatting: verify S-expression structure
- Result parsing: success and error cases

### Integration tests (eval.rs)
- `spawn_agent_and_send`: spawn worker, send add task, verify result is 7
- `send_to_tcp`: start agent on TCP (background thread), send task, verify
- `send_to_unknown_agent`: send to nonexistent name → error
- `spawn_agent_bad_path`: spawn with nonexistent module → error

### E2E demo
```clojure
;; orchestrator.airl
(let (w (spawn-agent "tests/fixtures/agent/worker_module.airl"))
  (let (result (send w "add" 10 20))
    result))
;; Expected output: 30
```

---

## 8. Files

| File | Change |
|---|---|
| `crates/airl-runtime/src/agent_client.rs` | **NEW** — framing, task/result serialization |
| `crates/airl-runtime/src/eval.rs` | Add agents list, spawn-agent/send builtins, register builtin names |
| `crates/airl-runtime/src/lib.rs` | Export agent_client module |
| `crates/airl-agent/src/runtime.rs` | Add stdio support to run_agent_loop |
| `crates/airl-driver/src/main.rs` | Handle `--listen stdio` in cmd_agent |
| `tests/fixtures/valid/orchestrator.airl` | **NEW** — spawn + send demo fixture |

---

## 9. Not In Scope

- `await` (async task dispatch)
- `parallel` / `broadcast`
- Timeouts
- Trust-level validation on results
- Agent capability negotiation
- Multiple concurrent connections
