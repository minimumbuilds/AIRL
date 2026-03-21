# AIRL Agent End-to-End Wiring Design

**Date:** 2026-03-21
**Status:** Approved
**Depends on:** Phase 1 Implementation + Hardening (360 tests)

---

## Overview

Wire the agent infrastructure into a working end-to-end demo: two processes exchanging typed, contract-verified AIRL tasks over TCP. Agent B loads a module file with function definitions, Agent A (via `airl call`) sends tasks specifying which function to call with what arguments.

---

## 1. Message Protocol

Two message types flow over the existing length-prefixed transport (u32 BE + UTF-8 AIRL S-expression):

### Task (Request)

```clojure
(task "task-id-001"
  :from "orchestrator"
  :call "add"
  :args [3 4])
```

Fields:
- Task ID (string) — unique identifier
- `:from` — sender agent name
- `:call` — function name to invoke in receiver's module
- `:args` — list of argument values as AIRL literals

### Result (Response)

```clojure
(result "task-id-001"
  :status :complete
  :payload 7)
```

Or on error:
```clojure
(result "task-id-001"
  :status :error
  :message "UseAfterMove: `x` was moved at 3:5")
```

Fields:
- Task ID (string) — matches the request
- `:status` — `:complete` or `:error`
- `:payload` — return value (on success)
- `:message` — error description (on failure)

Both are valid AIRL S-expressions parsed by the existing lexer/S-expr parser. No new grammar needed — fields are extracted by keyword matching on the parsed SExpr tree.

---

## 2. Agent B — Worker

```
airl agent worker.airl --listen tcp:127.0.0.1:9001
```

### Startup

1. **Load module** — Parse the `.airl` file, evaluate all top-level forms with the interpreter. This registers function definitions in the environment (defn) and type definitions (deftype). Bare expressions are evaluated but their results discarded (side-effect only).

2. **Parse endpoint** — Parse `--listen` argument. Format: `tcp:HOST:PORT` or `unix:/path/to/socket`.

3. **Bind transport** — Create a TCP listener (or Unix socket listener) on the specified address.

### Receive Loop

```
loop {
    accept connection → transport
    loop {
        frame = transport.recv_message()
        if Err(Io(UnexpectedEof)) or Err(Disconnected): break inner loop

        sexpr = parse(frame)
        task_id, fn_name, args = extract_task_fields(sexpr)

        result = look up fn_name in interpreter env
                 → call it with args (contracts checked)
                 → on success: (result id :status :complete :payload value)
                 → on error:   (result id :status :error :message err_string)

        transport.send_message(serialize(result))
    }
    // connection closed, accept next
}
```

Single connection at a time (no concurrency). When a connection closes, go back to accepting.

### Function Lookup and Execution

When a task arrives with `:call "add" :args [3 4]`:

1. Look up `"add"` in the interpreter's environment → must be `Value::Function(FnValue)`
2. Parse each arg from the S-expression into a `Value` (integer literals → `Value::Int`, strings → `Value::Str`, etc.)
3. Call the function via a new public method `interpreter.call_by_name(name, args)` — this looks up the function, checks `:requires`, executes the body, checks `:ensures`. Note: `call_fn` is currently private; we add `call_by_name` as the public entry point rather than exposing `call_fn` directly.
4. On `Ok(value)` → serialize as success result
5. On `Err(RuntimeError)` → serialize as error result

---

## 3. Agent A — Client (`airl call`)

```
airl call tcp:127.0.0.1:9001 add 3 4
```

One-shot client:

1. **Parse endpoint** — Same format as agent: `tcp:HOST:PORT` or `unix:/path`
2. **Connect** — Create transport connection to the endpoint
3. **Build task** — Serialize `(task "call-0" :from "cli" :call "add" :args [3 4])`
   - Function name from first positional arg
   - Remaining positional args parsed as AIRL literals (integers, floats, strings in quotes)
4. **Send and wait** — Send the task frame, read the result frame
5. **Print result** — Parse the result S-expression, extract `:payload` or `:message`, print to stdout
6. **Exit** — Exit 0 on success, exit 1 on error

### Argument Parsing

CLI arguments are parsed as AIRL literals:
- Bare numbers → integers (`3` → `Value::Int(3)`)
- Numbers with dots → floats (`3.14` → `Value::Float(3.14)`)
- Quoted strings → strings (`"hello"` → `Value::Str("hello")`)
- `true`/`false` → booleans
- `nil` → nil

---

## 4. Endpoint Parsing

Shared utility for both `--listen` and positional endpoint arg. Reuses the existing `Endpoint` type from `airl_agent::identity`:

```
tcp:127.0.0.1:9001  → Endpoint::Tcp(SocketAddr)
unix:/tmp/airl.sock → Endpoint::Unix(PathBuf)
```

Add a `parse_endpoint(s: &str) -> Result<Endpoint, String>` function to `airl-agent`.

**CLI argument parsing for `cmd_agent`:** Since the project uses raw `std::env::args` (no clap), find `--listen` in the args slice, take the next arg as the endpoint string. First positional arg (before `--listen`) is the module file path.

---

## 5. Value Serialization

To send results back over the wire, we need `Value → S-expression string`:

- `Value::Int(42)` → `"42"`
- `Value::Float(3.14)` → `"3.14"`
- `Value::Bool(true)` → `"true"`
- `Value::Str("hello")` → `"\"hello\""`
- `Value::Nil` → `"nil"`
- `Value::Unit` → `"()"` (current Display impl; receiver treats as nil)
- `Value::Variant("Ok", box Int(42))` → `"(Ok 42)"`
- `Value::List([1, 2, 3])` → `"[1 2 3]"`

This uses the `Display` impl for `Value` — which already exists. We use `format!("{}", value)` for the result payload.

**Supported types over the wire:** Int, UInt, Float, Bool, Str, Nil, Unit, Variant, List. Unsupported types (Struct, Tensor, Function, Lambda, BuiltinFn) will serialize via their Display impl but may not round-trip through the parser. For Phase 1 this is acceptable — the demo only uses primitive types. A future iteration should add proper S-expression serialization for all Value types.

For deserializing args from the task message, parse each element of the `:args` bracket list as a `Value` using the existing SExpr → literal mapping.

---

## 6. Files to Modify/Create

| File | Change |
|---|---|
| `crates/airl-agent/src/protocol.rs` | **NEW** — `TaskMessage`, `ResultMessage` types; `serialize_task`, `parse_task`, `serialize_result`, `parse_result` functions; `sexpr_to_value` for arg deserialization |
| `crates/airl-agent/src/runtime.rs` | Add `run_agent_loop(module_path, endpoint)` — loads module, binds, runs receive loop |
| `crates/airl-agent/src/lib.rs` | Export `protocol` module |
| `crates/airl-driver/src/main.rs` | Implement `cmd_agent` (replace stub) and add `cmd_call` |

**Unchanged:**
- Transport trait and implementations (TCP, Unix, Stdio)
- Framing protocol (u32 BE length prefix)
- Interpreter and evaluator
- Lexer and S-expression parser

---

## 7. Testing

### Unit Tests (protocol.rs)
- Serialize/parse task message round-trip
- Serialize/parse result message (success + error) round-trip
- `sexpr_to_value` for each literal type

### Integration Tests
- Spawn `airl agent` as child process on a port, then `airl call` connects and gets result
- Call a function that succeeds → verify correct payload
- Call a nonexistent function → verify error response
- Call a function that triggers a contract violation → verify error response

### Fixture
- `tests/fixtures/agent/worker_module.airl` — a simple module with `add` and `multiply` functions

---

## 8. Demo

```
# Terminal 1: Start worker
$ airl agent tests/fixtures/agent/worker_module.airl --listen tcp:127.0.0.1:9001
Listening on tcp:127.0.0.1:9001...

# Terminal 2: Send tasks
$ airl call tcp:127.0.0.1:9001 add 3 4
7

$ airl call tcp:127.0.0.1:9001 multiply 6 7
42

$ airl call tcp:127.0.0.1:9001 nonexistent 1 2
error: function `nonexistent` not found
```

---

## 9. Future Work (B — `send`/`await` Builtins)

Not in scope for this iteration, but noted for the next:

- **`send` builtin** — From within an AIRL program, create a transport connection and dispatch a task. Returns a task ID for tracking.
- **`await` builtin** — Block on a pending task result with timeout. Invoke `:on-result` or `:on-timeout` callbacks.
- **`spawn-agent` builtin** — Launch a child process running `airl agent`, connect via stdio transport.
- **`parallel` builtin** — Fan-out multiple tasks, collect results, apply `:merge` function.
- **Circular dependency resolution** — The interpreter needs access to the agent runtime (for send/await), but the agent runtime needs the interpreter (for task execution). Resolve via trait object, callback, or shared ownership (`Arc<Mutex<>>`).
- **Concurrent connections** — The receive loop currently handles one connection at a time. Future work: thread-per-connection or async runtime.
- **Trust-level-dependent validation** — Validate results differently based on sender's trust level (none/verified/proven).
- **Deadline enforcement** — Background thread monitors elapsed time, cancels task on timeout.

---

## Not In Scope

- `send`/`await`/`spawn-agent`/`parallel` builtins (future work B)
- Concurrent connections
- Deadline enforcement
- Trust-level validation
- Stdio transport demo (works but not demonstrated)
- TLS or authentication
