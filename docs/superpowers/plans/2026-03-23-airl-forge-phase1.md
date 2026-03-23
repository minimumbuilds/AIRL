# AIRL-Forge Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the AIRL-Forge Rust crate (builtins) and Layer 1 AIRL modules (provider, schema, tools, codec) plus Layer 2 chain/validate, with tests using a mock provider.

**Architecture:** Separate git repository (`airl-forge/`) depending on the AIRL project via path dependencies. Rust crate provides 6 builtins (`json-parse`, `json-encode`, `http-request`, `fn-metadata`, `env`, `to-string`). AIRL modules provide the toolkit and composition layers. All data records use Maps (no product type field access).

**Tech Stack:** Rust (reqwest for HTTP, serde_json for JSON), AIRL (stdlib: map operations, collections, string, result combinators)

**Spec:** `docs/superpowers/specs/2026-03-23-airl-forge-design.md`

**AIRL Language Reminders:**
- **`let` syntax:** `(let (name : Type value) body)` — always parenthesized with type. Use `_` for inferred type. Nest for multiple: `(let (a : _ 1) (let (b : _ 2) body))`
- **String concatenation:** `(+ "a" "b")` — NOT `(concat a b)` (concat is for lists)
- **`+` takes 2 args:** For 3+ parts: `(+ (+ "a" "b") "c")`
- **No `to-string` in stdlib** — we add it as a Forge builtin
- **Eager `and`/`or`** — use nested `if` for short-circuit
- **`join` arg order:** `(join list separator)` — list first
- **`try`** unwraps Ok/propagates Err — does NOT catch runtime errors
- **No field access** — use `map-get`/`map-set`
- **`(valid x)`** — builtin that always returns true; used as placeholder contract

---

## Prerequisites

### Task 0: Add Extension Points to AIRL Core

**Files:**
- Modify: `crates/airl-runtime/src/builtins.rs`
- Modify: `crates/airl-runtime/src/eval.rs`

- [ ] **Step 1: Write the test**

In `crates/airl-runtime/src/builtins.rs`, add to the existing `#[cfg(test)]` module:

```rust
#[test]
fn test_register_external_builtin() {
    let mut builtins = Builtins::new();
    assert!(!builtins.has("test-external"));
    builtins.register_external("test-external", |_args| {
        Ok(Value::Str("external".into()))
    });
    assert!(builtins.has("test-external"));
    let f = builtins.get("test-external").unwrap();
    let result = f(&[]).unwrap();
    assert_eq!(result, Value::Str("external".into()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p airl-runtime test_register_external_builtin`
Expected: FAIL — `register_external` not found

- [ ] **Step 3: Implement**

In `crates/airl-runtime/src/builtins.rs`, add to `impl Builtins`:

```rust
pub fn register_external(&mut self, name: &str, f: BuiltinFnPtr) {
    self.fns.insert(name.to_string(), f);
}
```

In `crates/airl-runtime/src/eval.rs`, add to `impl Interpreter`:

```rust
pub fn builtins_mut(&mut self) -> &mut Builtins {
    &mut self.builtins
}
```

- [ ] **Step 4: Run test — expect PASS**

Run: `cargo test -p airl-runtime test_register_external_builtin`

- [ ] **Step 5: Run full suite — expect no regressions**

Run: `cargo test --workspace --exclude airl-mlir`

- [ ] **Step 6: Commit**

```bash
git add crates/airl-runtime/src/builtins.rs crates/airl-runtime/src/eval.rs
git commit -m "feat(runtime): add register_external and builtins_mut for library extensions"
```

---

## Repository Scaffold

### Task 1: Create airl-forge Repository

**Files:**
- Create: `../airl-forge/Cargo.toml`, `../airl-forge/crates/airl-forge/Cargo.toml`
- Create: `../airl-forge/crates/airl-forge/src/lib.rs`
- Create: directory tree under `../airl-forge/lib/` and `../airl-forge/tests/`

- [ ] **Step 1: Create directories**

```bash
mkdir -p ../airl-forge/crates/airl-forge/src
mkdir -p ../airl-forge/lib/{core,compose,forge}
mkdir -p ../airl-forge/tests/{core,compose,forge,integration}
```

- [ ] **Step 2: Create workspace Cargo.toml**

`../airl-forge/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/airl-forge"]
```

- [ ] **Step 3: Create crate Cargo.toml**

`../airl-forge/crates/airl-forge/Cargo.toml`:

```toml
[package]
name = "airl-forge"
version = "0.1.0"
edition = "2021"

[dependencies]
airl-runtime = { path = "../../../AIRL/crates/airl-runtime" }
airl-syntax = { path = "../../../AIRL/crates/airl-syntax" }
airl-types = { path = "../../../AIRL/crates/airl-types" }
airl-driver = { path = "../../../AIRL/crates/airl-driver" }
serde_json = "1"
reqwest = { version = "0.12", features = ["blocking", "json"] }
```

- [ ] **Step 4: Create lib.rs and module stubs**

`../airl-forge/crates/airl-forge/src/lib.rs`:

```rust
pub mod json;
pub mod env;
pub mod to_string;
pub mod http;
pub mod introspect;
pub mod pipeline;

use airl_runtime::Builtins;

pub fn register_forge_builtins(builtins: &mut Builtins) {
    builtins.register_external("json-parse", json::builtin_json_parse);
    builtins.register_external("json-encode", json::builtin_json_encode);
    builtins.register_external("env", env::builtin_env);
    builtins.register_external("to-string", to_string::builtin_to_string);
    builtins.register_external("http-request", http::builtin_http_request);
    builtins.register_external("fn-metadata", introspect::builtin_fn_metadata);
}
```

Create stub files for each module with `todo!()` implementations (same pattern as before). Verify `cargo build` compiles.

- [ ] **Step 5: Init git and commit**

```bash
cd ../airl-forge && git init && echo "target/" > .gitignore && git add -A
git commit -m "chore: scaffold airl-forge repository"
```

---

## Rust Builtins

### Task 2: Implement `json-parse` Builtin

**Files:** `../airl-forge/crates/airl-forge/src/json.rs`

- [ ] **Step 1: Write tests** (same as before — `test_parse_string`, `test_parse_integer`, `test_parse_float`, `test_parse_boolean`, `test_parse_null`, `test_parse_array`, `test_parse_object`, `test_parse_nested`, `test_parse_invalid_json`)

- [ ] **Step 2: Verify tests fail** — Run: `cd ../airl-forge && cargo test -p airl-forge json::tests`

- [ ] **Step 3: Implement**

```rust
use airl_runtime::{Value, RuntimeError};
use std::collections::HashMap;

fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::Int(i) }
            else if let Some(f) = n.as_f64() { Value::Float(f) }
            else { Value::Nil }
        }
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(arr) => Value::List(arr.iter().map(json_to_value).collect()),
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj { map.insert(k.clone(), json_to_value(v)); }
            Value::Map(map)
        }
    }
}

pub fn builtin_json_parse(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::Custom(format!("json-parse expects 1 argument, got {}", args.len())));
    }
    match &args[0] {
        Value::Str(s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(json) => Ok(Value::Variant("Ok".into(), Box::new(json_to_value(&json)))),
            Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e.to_string())))),
        },
        other => Err(RuntimeError::TypeError(format!("json-parse expects String, got {:?}", other))),
    }
}
```

- [ ] **Step 4: Verify tests pass** — Run: `cd ../airl-forge && cargo test -p airl-forge json::tests`

- [ ] **Step 5: Commit** — `cd ../airl-forge && git add -A && git commit -m "feat: implement json-parse builtin"`

---

### Task 3: Implement `json-encode` Builtin

**Files:** `../airl-forge/crates/airl-forge/src/json.rs`

- [ ] **Step 1: Write tests** (`test_encode_string`, `test_encode_integer`, `test_encode_float`, `test_encode_boolean`, `test_encode_null`, `test_encode_array`, `test_encode_object`, `test_encode_variant`, `test_roundtrip`)

- [ ] **Step 2: Verify new tests fail**

- [ ] **Step 3: Implement**

```rust
fn value_to_json(val: &Value) -> Result<serde_json::Value, String> {
    match val {
        Value::Nil | Value::Unit => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Int(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .ok_or_else(|| format!("Cannot encode float: {}", f)),
        Value::Str(s) => Ok(serde_json::Value::String(s.clone())),
        Value::List(items) => Ok(serde_json::Value::Array(
            items.iter().map(value_to_json).collect::<Result<Vec<_>, _>>()?
        )),
        Value::Map(m) => {
            let mut obj = serde_json::Map::new();
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            for k in keys { obj.insert(k.clone(), value_to_json(m.get(k).unwrap())?); }
            Ok(serde_json::Value::Object(obj))
        }
        Value::Variant(tag, inner) => {
            let mut obj = serde_json::Map::new();
            obj.insert("tag".into(), serde_json::Value::String(tag.clone()));
            obj.insert("value".into(), value_to_json(inner)?);
            Ok(serde_json::Value::Object(obj))
        }
        other => Err(format!("Cannot JSON-encode: {:?}", other)),
    }
}

pub fn builtin_json_encode(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 1 {
        return Err(RuntimeError::Custom(format!("json-encode expects 1 argument, got {}", args.len())));
    }
    match value_to_json(&args[0]) {
        Ok(json) => Ok(Value::Variant("Ok".into(), Box::new(Value::Str(serde_json::to_string(&json).unwrap())))),
        Err(e) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(e)))),
    }
}
```

- [ ] **Step 4: Verify all json tests pass**

- [ ] **Step 5: Commit** — `git commit -m "feat: implement json-encode builtin"`

---

### Task 4: Implement `env` and `to-string` Builtins

**Files:** `../airl-forge/crates/airl-forge/src/env.rs`, `../airl-forge/crates/airl-forge/src/to_string.rs`

- [ ] **Step 1: Implement `env`**

```rust
pub fn builtin_env(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 1 { return Err(RuntimeError::Custom(format!("env expects 1 arg, got {}", args.len()))); }
    match &args[0] {
        Value::Str(name) => match std::env::var(name) {
            Ok(val) => Ok(Value::Variant("Ok".into(), Box::new(Value::Str(val)))),
            Err(_) => Ok(Value::Variant("Err".into(), Box::new(Value::Str(format!("not set: {}", name))))),
        },
        other => Err(RuntimeError::TypeError(format!("env expects String, got {:?}", other))),
    }
}
```

- [ ] **Step 2: Implement `to-string`**

```rust
pub fn builtin_to_string(args: &[Value]) -> Result<Value, RuntimeError> {
    if args.len() != 1 { return Err(RuntimeError::Custom(format!("to-string expects 1 arg, got {}", args.len()))); }
    let s = match &args[0] {
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.clone(),
        Value::Nil => "nil".to_string(),
        Value::Unit => "unit".to_string(),
        Value::List(items) => {
            let strs: Vec<String> = items.iter()
                .map(|v| match builtin_to_string(&[v.clone()]) { Ok(Value::Str(s)) => s, _ => format!("{:?}", v) })
                .collect();
            format!("[{}]", strs.join(" "))
        }
        Value::Variant(tag, inner) => {
            let inner_s = match builtin_to_string(&[*inner.clone()]) { Ok(Value::Str(s)) => s, _ => format!("{:?}", inner) };
            if inner_s == "unit" { format!("({})", tag) } else { format!("({} {})", tag, inner_s) }
        }
        other => format!("{:?}", other),
    };
    Ok(Value::Str(s))
}
```

- [ ] **Step 3: Write tests for both** (env: test existing PATH, missing var, wrong type; to-string: int, float, bool, string, nil, list, variant)

- [ ] **Step 4: Run tests, commit** — `git commit -m "feat: implement env and to-string builtins"`

---

### Task 5: Implement `http-request` Builtin

**Files:** `../airl-forge/crates/airl-forge/src/http.rs`

Same implementation as previously specified (reqwest blocking client, 4 args: method, url, headers, body). Returns `Result[Map String]` with `"status"`, `"headers"`, `"body"` keys. Non-network tests only (wrong arity, wrong type); network test marked `#[ignore]`.

- [ ] **Step 1: Implement with tests**
- [ ] **Step 2: Run tests, commit** — `git commit -m "feat: implement http-request builtin"`

---

### Task 6: Implement `fn-metadata` Builtin

**Files:** `../airl-forge/crates/airl-forge/src/introspect.rs`

Thread-local `FN_REGISTRY` with `populate_fn_registry()`. Same implementation as previously specified.

- [ ] **Step 1: Implement with tests**
- [ ] **Step 2: Run tests, commit** — `git commit -m "feat: implement fn-metadata builtin"`

---

### Task 7: Pipeline Wrapper

**Files:** `../airl-forge/crates/airl-forge/src/pipeline.rs`

- [ ] **Step 1: Implement pipeline**

```rust
use airl_runtime::{Interpreter, Value};
use crate::introspect;
use std::collections::HashMap;

pub fn create_forge_interpreter() -> Result<Interpreter, String> {
    let mut interp = Interpreter::new();
    crate::register_forge_builtins(interp.builtins_mut());
    airl_driver::pipeline::eval_prelude(&mut interp);
    // Forge AIRL modules loaded here once they exist (Tasks 8-13)
    Ok(interp)
}

/// Populate fn-metadata registry from interpreter's environment bindings.
fn sync_fn_registry(interp: &Interpreter) {
    let mut fns = HashMap::new();
    for (name, slot) in interp.env.iter_bindings() {
        if let airl_runtime::Value::Function(fv) = &slot.value {
            fns.insert(name.to_string(), fv.def.clone());
        }
    }
    introspect::populate_fn_registry(&fns);
}

pub fn run_forge_source(source: &str) -> Result<Value, String> {
    let mut interp = create_forge_interpreter()?;

    let tokens = airl_syntax::lexer::lex(source)
        .map_err(|e| format!("Lex error: {:?}", e))?;
    let top_levels = airl_syntax::parser::parse(&tokens)
        .map_err(|e| format!("Parse error: {:?}", e))?;

    // Evaluate user code
    let mut last = Value::Nil;
    for tl in &top_levels {
        last = interp.eval_top_level(tl)
            .map_err(|e| format!("Eval error: {:?}", e))?;
    }

    // Sync fn registry for fn-metadata calls (e.g., discover-tools)
    sync_fn_registry(&interp);

    Ok(last)
}
```

- [ ] **Step 2: Write integration tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parse_from_airl() {
        let result = run_forge_source(r#"
            (match (json-parse "{\"x\": 1}")
                (Ok m) (map-get m "x")
                (Err _) -1)
        "#).unwrap();
        assert_eq!(result, Value::Int(1));
    }

    #[test]
    fn test_env_from_airl() {
        let result = run_forge_source(r#"
            (match (env "PATH") (Ok _) "found" (Err _) "missing")
        "#).unwrap();
        assert_eq!(result, Value::Str("found".into()));
    }

    #[test]
    fn test_to_string_from_airl() {
        let result = run_forge_source(r#"(to-string 42)"#).unwrap();
        assert_eq!(result, Value::Str("42".into()));
    }
}
```

- [ ] **Step 3: Run tests, commit** — `git commit -m "feat: pipeline wrapper with fn-metadata sync"`

---

## AIRL Modules

### Task 8: `core/codec.airl`

**Files:** `../airl-forge/lib/core/codec.airl`

- [ ] **Step 1: Write implementation**

```lisp
;; codec.airl — JSON <-> AIRL Value marshalling with validation

(deftype CodecError (|
  (InvalidJson String)
  (MissingKeys String)))

(defn decode-as
  :sig [(json : String) (expected-keys : List[String]) -> Result[Map CodecError]]
  :intent "Parse JSON string into a Map, validating expected keys are present"
  :requires [(not (is-empty-str json))]
  :ensures [(match result
              (Ok v) (all (fn (k) (map-has v k)) expected-keys)
              (Err _) true)]
  :body
    (match (json-parse json)
      (Ok val)
        (let (missing : List (filter (fn (k) (not (map-has val k))) expected-keys))
          (if (empty? missing)
            (Ok val)
            (Err (MissingKeys (join missing ", ")))))
      (Err msg) (Err (InvalidJson msg))))

(defn encode-value
  :sig [(value : _) -> Result[String CodecError]]
  :intent "Encode an AIRL value as a JSON string"
  :requires [(valid value)]
  :ensures [(match result (Ok s) (not (is-empty-str s)) (Err _) true)]
  :body
    (match (json-encode value)
      (Ok s) (Ok s)
      (Err msg) (Err (InvalidJson msg))))

(defn identity
  :sig [(x : _) -> _]
  :requires [(valid x)]
  :ensures [(valid result)]
  :body x)
```

- [ ] **Step 2: Add `include_str!` to pipeline, write Rust integration tests**

```rust
#[test]
fn test_codec_decode_as() {
    let result = run_forge_source(r#"
        (match (decode-as "{\"name\": \"test\", \"val\": 42}" ["name" "val"])
            (Ok m) (map-get m "val")
            (Err _) -1)
    "#).unwrap();
    assert_eq!(result, Value::Int(42));
}

#[test]
fn test_codec_missing_key() {
    let result = run_forge_source(r#"
        (match (decode-as "{\"name\": \"test\"}" ["name" "missing"])
            (Ok _) "bad" (Err _) "caught")
    "#).unwrap();
    assert_eq!(result, Value::Str("caught".into()));
}
```

- [ ] **Step 3: Run tests, commit** — `git commit -m "feat: core/codec.airl"`

---

### Task 9: `core/schema.airl`

**Files:** `../airl-forge/lib/core/schema.airl`

- [ ] **Step 1: Write implementation**

```lisp
;; schema.airl — AIRL type string -> JSON Schema

(defn type-to-schema
  :sig [(type-str : String) -> Map]
  :intent "Convert an AIRL type string to a JSON Schema map"
  :requires [(not (is-empty-str type-str))]
  :ensures [(map-has result "type")]
  :body
    (if (= type-str "String")
      (map-from-entries [["type" "string"]])
    (if (= type-str "i64")
      (map-from-entries [["type" "integer"]])
    (if (= type-str "f64")
      (map-from-entries [["type" "number"]])
    (if (= type-str "Bool")
      (map-from-entries [["type" "boolean"]])
    (if (= type-str "Map")
      (map-from-entries [["type" "object"] ["additionalProperties" true]])
    (if (starts-with type-str "List[")
      (let (inner-type : String (substring type-str 5 (- (length type-str) 1)))
        (map-from-entries [["type" "array"] ["items" (type-to-schema inner-type)]]))
    (map-from-entries
      [["type" "string"]
       ["description" (+ "AIRL type: " type-str)]]))))))))

(defn params-to-schema
  :sig [(params : List[Map]) -> Map]
  :intent "Convert fn-metadata params list to JSON Schema properties"
  :requires [(valid params)]
  :ensures [(map-has result "type")]
  :body
    (let (properties : Map (fold
      (fn (acc p)
        (map-set acc
          (map-get-or p "name" "unknown")
          (type-to-schema (map-get-or p "type" "String"))))
      (map-new)
      params))
    (let (required : List (map (fn (p) (map-get-or p "name" "unknown")) params))
      (map-from-entries
        [["type" "object"]
         ["properties" properties]
         ["required" required]]))))
```

- [ ] **Step 2: Add to pipeline, write tests**

```rust
#[test]
fn test_schema_string() {
    let result = run_forge_source(r#"(map-get (type-to-schema "String") "type")"#).unwrap();
    assert_eq!(result, Value::Str("string".into()));
}

#[test]
fn test_schema_integer() {
    let result = run_forge_source(r#"(map-get (type-to-schema "i64") "type")"#).unwrap();
    assert_eq!(result, Value::Str("integer".into()));
}
```

- [ ] **Step 3: Run tests, commit** — `git commit -m "feat: core/schema.airl"`

---

### Task 10: `core/tools.airl`

**Files:** `../airl-forge/lib/core/tools.airl`

- [ ] **Step 1: Write implementation**

```lisp
;; tools.airl — Tool registry via :intent convention

(defn build-tool-schema
  :sig [(meta : Map) -> Map]
  :requires [(map-has meta "name")]
  :ensures [(map-has result "name")]
  :body
    (let (params-list : List (map-get-or meta "params" []))
      (map-from-entries
        [["name" (map-get meta "name")]
         ["intent" (map-get-or meta "intent" "")]
         ["parameters" (params-to-schema params-list)]
         ["preconditions" (map-get-or meta "requires" [])]])))

(defn discover-tool
  :sig [(fn-name : String) -> List[Map]]
  :requires [(not (is-empty-str fn-name))]
  :ensures [(valid result)]
  :body
    (match (fn-metadata fn-name)
      (Ok meta)
        (if (is-empty-str (map-get-or meta "intent" ""))
          []
          [(build-tool-schema meta)])
      (Err _) []))

(defn discover-tools
  :sig [(fn-names : List[String]) -> List[Map]]
  :intent "Discover all functions with :intent annotations and build tool schemas"
  :requires [(not (empty? fn-names))]
  :ensures [(all (fn (t) (not (is-empty-str (map-get-or t "intent" "")))) result)]
  :body (flatten (map discover-tool fn-names)))
```

- [ ] **Step 2: Add to pipeline, commit** — `git commit -m "feat: core/tools.airl"`

---

### Task 11: `core/provider.airl`

**Files:** `../airl-forge/lib/core/provider.airl`

- [ ] **Step 1: Write implementation**

```lisp
;; provider.airl — LLM provider abstraction

(deftype LLMMessage (|
  (System String)
  (User String)
  (Assistant String)
  (ToolCall String String)
  (ToolResult String String)))

(deftype ProviderError (|
  (HttpError String)
  (ParseError String)
  (AuthError String)
  (RateLimited i64)))

(defn http-provider
  :sig [(name : String) (api-key : String) (model : String) -> Map]
  :intent "Create a provider config for a known HTTP API provider"
  :requires [(not (is-empty-str api-key)) (not (is-empty-str model))]
  :ensures [(= (map-get result "name") name)]
  :body
    (let (api-url : String
      (if (= name "anthropic")
        "https://api.anthropic.com/v1/messages"
      (if (= name "openai")
        "https://api.openai.com/v1/chat/completions"
        "")))
    (map-from-entries
      [["name" name] ["api-url" api-url]
       ["api-key" api-key] ["model" model]])))

(defn mock-provider
  :sig [(response : Map) -> Map]
  :intent "Create a mock provider that returns a canned response"
  :requires [(map-has response "content")]
  :ensures [(= (map-get result "name") "mock")]
  :body
    (map-from-entries
      [["name" "mock"] ["api-url" ""] ["api-key" "mock-key"]
       ["model" "mock-model"] ["mock-response" response]]))

(defn format-message-anthropic
  :sig [(msg : LLMMessage) -> Map]
  :requires [(valid msg)]
  :ensures [(valid result)]
  :body
    (match msg
      (System s) (map-from-entries [["role" "user"] ["content" (+ "[SYSTEM] " s)]])
      (User s) (map-from-entries [["role" "user"] ["content" s]])
      (Assistant s) (map-from-entries [["role" "assistant"] ["content" s]])
      (ToolCall name args) (map-from-entries
        [["role" "assistant"]
         ["content" (+ (+ "Tool call: " name) (+ " " args))]])
      (ToolResult name r) (map-from-entries
        [["role" "user"]
         ["content" (+ (+ "Tool result for " name) (+ ": " r))]])))

(defn extract-system-anthropic
  :sig [(messages : List[LLMMessage]) -> String]
  :requires [(valid messages)]
  :ensures [(valid result)]
  :body
    (let (sys-msgs : List (filter (fn (m) (match m (System _) true _ false)) messages))
      (if (empty? sys-msgs) ""
        (match (head sys-msgs) (System s) s _ ""))))

(defn non-system-messages
  :sig [(messages : List[LLMMessage]) -> List[LLMMessage]]
  :requires [(valid messages)]
  :ensures [(valid result)]
  :body (filter (fn (m) (match m (System _) false _ true)) messages))

(defn tool-to-anthropic-format
  :sig [(tool : Map) -> Map]
  :requires [(map-has tool "name")]
  :ensures [(map-has result "name")]
  :body
    (map-from-entries
      [["name" (map-get tool "name")]
       ["description" (map-get-or tool "intent" "")]
       ["input_schema" (map-get-or tool "parameters"
          (map-from-entries [["type" "object"] ["properties" (map-new)]]))]]))

(defn call-provider
  :sig [(config : Map) (messages : List[LLMMessage])
        (tools : List[Map]) -> Result[Map ProviderError]]
  :intent "Send a message list to an LLM provider and return the response"
  :requires [(not (is-empty-str (map-get-or config "api-key" "")))
             (not (empty? messages))]
  :ensures [(match result (Ok r) (map-has r "content") (Err _) true)]
  :body
    (let (provider-name : String (map-get-or config "name" ""))
      (if (= provider-name "mock")
        (call-provider-mock config)
      (if (= provider-name "anthropic")
        (call-provider-anthropic config messages tools)
        (Err (HttpError (+ "unknown provider: " provider-name)))))))

(defn call-provider-mock
  :sig [(config : Map) -> Result[Map ProviderError]]
  :requires [(valid config)]
  :ensures [(match result (Ok r) (map-has r "content") (Err _) true)]
  :body
    (let (resp : Map (map-get-or config "mock-response" (map-new)))
      (if (map-has resp "content")
        (Ok (map-from-entries
          [["content" (map-get resp "content")]
           ["tool-calls" (map-get-or resp "tool-calls" [])]
           ["usage" (map-from-entries [["input" 0] ["output" 0]])]]))
        (Err (ParseError "mock response missing content")))))

(defn call-provider-anthropic
  :sig [(config : Map) (messages : List[LLMMessage])
        (tools : List[Map]) -> Result[Map ProviderError]]
  :requires [(valid config)]
  :ensures [(match result (Ok r) (map-has r "content") (Err _) true)]
  :body
    (let (api-key : String (map-get config "api-key"))
    (let (model : String (map-get config "model"))
    (let (api-url : String (map-get config "api-url"))
    (let (system-prompt : String (extract-system-anthropic messages))
    (let (msgs : List (map format-message-anthropic (non-system-messages messages)))
    (let (body-map : Map (map-from-entries
        [["model" model] ["max_tokens" 4096] ["messages" msgs]]))
    (let (body-map : Map (if (is-empty-str system-prompt)
        body-map
        (map-set body-map "system" system-prompt)))
    (let (body-map : Map (if (empty? tools)
        body-map
        (map-set body-map "tools" (map tool-to-anthropic-format tools))))
    (let (headers : Map (map-from-entries
        [["x-api-key" api-key]
         ["anthropic-version" "2023-06-01"]
         ["content-type" "application/json"]]))
      (match (encode-value body-map)
        (Ok body-json)
          (match (http-request "POST" api-url headers body-json)
            (Ok resp)
              (let (status : i64 (map-get resp "status"))
                (if (= status 200)
                  (match (decode-as (map-get resp "body") ["content"])
                    (Ok parsed) (parse-anthropic-response parsed)
                    (Err e) (Err (ParseError "failed to parse response body")))
                  (if (= status 429)
                    (Err (RateLimited 60))
                    (Err (HttpError (+ "HTTP " (to-string status)))))))
            (Err msg) (Err (HttpError msg)))
        (Err _) (Err (ParseError "failed to encode request body")))))))))))))

(defn parse-anthropic-response
  :sig [(parsed : Map) -> Result[Map ProviderError]]
  :requires [(valid parsed)]
  :ensures [(match result (Ok r) (map-has r "content") (Err _) true)]
  :body
    (let (content-blocks : List (map-get-or parsed "content" []))
    (let (text-blocks : List (filter
        (fn (b) (= (map-get-or b "type" "") "text"))
        content-blocks))
    (let (text : String (if (empty? text-blocks) ""
        (map-get-or (head text-blocks) "text" "")))
    (let (tool-blocks : List (filter
        (fn (b) (= (map-get-or b "type" "") "tool_use"))
        content-blocks))
    (let (tool-calls : List (map
        (fn (b) (map-from-entries
          [["id" (map-get-or b "id" "")]
           ["name" (map-get-or b "name" "")]
           ["arguments" (match (encode-value (map-get-or b "input" (map-new)))
                          (Ok s) s (Err _) "{}")]]))
        tool-blocks))
    (let (usage-map : Map (map-get-or parsed "usage" (map-new)))
      (Ok (map-from-entries
        [["content" text]
         ["tool-calls" tool-calls]
         ["usage" (map-from-entries
            [["input" (map-get-or usage-map "input_tokens" 0)]
             ["output" (map-get-or usage-map "output_tokens" 0)]])]])))))))))
```

- [ ] **Step 2: Write mock provider test**

```rust
#[test]
fn test_mock_provider() {
    let result = run_forge_source(r#"
        (let (config : Map (mock-provider (map-from-entries [["content" "Hello"]])))
          (match (call-provider config [(User "hi")] [])
            (Ok resp) (map-get resp "content")
            (Err _) "error"))
    "#).unwrap();
    assert_eq!(result, Value::Str("Hello".into()));
}
```

- [ ] **Step 3: Run tests, commit** — `git commit -m "feat: core/provider.airl with Anthropic and mock"`

---

### Task 12: `compose/chain.airl`

**Files:** `../airl-forge/lib/compose/chain.airl`

- [ ] **Step 1: Write implementation**

```lisp
;; chain.airl — Pipeline combinator

(deftype ChainError (|
  (StepFailed String String)
  (EmptyPipeline)))

(defn step
  :sig [(name : String) (f : _) (transform : _) -> Map]
  :intent "Create a pipeline step"
  :requires [(not (is-empty-str name))]
  :ensures [(map-has result "name")]
  :body (map-from-entries [["name" name] ["fn" f] ["transform" transform]]))

(defn run-step
  :sig [(s : Map) (input : _) -> Result[_ ChainError]]
  :requires [(map-has s "name")]
  :ensures [(valid result)]
  :body
    (let (f : _ (map-get s "fn"))
    (let (transform : _ (map-get s "transform"))
    (let (step-name : String (map-get s "name"))
      (match (f input)
        (Ok val) (Ok (transform val))
        (Err msg) (Err (StepFailed step-name (to-string msg))))))))

(defn chain-loop
  :sig [(steps : List[Map]) (current : _) -> Result[_ ChainError]]
  :requires [(valid steps)]
  :ensures [(valid result)]
  :body
    (if (empty? steps)
      (Ok current)
      (match (run-step (head steps) current)
        (Ok next-val) (chain-loop (tail steps) next-val)
        (Err e) (Err e))))

(defn chain
  :sig [(steps : List[Map]) (input : _) -> Result[_ ChainError]]
  :intent "Execute pipeline steps, threading results with short-circuit on error"
  :requires [(not (empty? steps))]
  :ensures [(valid result)]
  :body (chain-loop steps input))

(defn fan-out-loop
  :sig [(steps : List[Map]) (input : _) (acc : List[_]) -> Result[List[_] ChainError]]
  :requires [(valid steps)]
  :ensures [(valid result)]
  :body
    (if (empty? steps)
      (Ok (reverse acc))
      (match (run-step (head steps) input)
        (Ok val) (fan-out-loop (tail steps) input (cons val acc))
        (Err e) (Err e))))

(defn fan-out
  :sig [(steps : List[Map]) (input : _) -> Result[List[_] ChainError]]
  :intent "Execute steps on same input, collect all results"
  :requires [(not (empty? steps))]
  :ensures [(match result (Ok rs) (= (length rs) (length steps)) (Err _) true)]
  :body (fan-out-loop steps input []))
```

- [ ] **Step 2: Write tests**

```rust
#[test]
fn test_chain_single() {
    let result = run_forge_source(r#"
        (defn inc :sig [(x : _) -> Result[i64 String]]
          :requires [(valid x)] :ensures [(valid result)]
          :body (Ok (+ x 1)))
        (match (chain [(step "inc" inc identity)] 5) (Ok v) v (Err _) -1)
    "#).unwrap();
    assert_eq!(result, Value::Int(6));
}

#[test]
fn test_chain_multi() {
    let result = run_forge_source(r#"
        (defn inc :sig [(x : _) -> Result[i64 String]]
          :requires [(valid x)] :ensures [(valid result)] :body (Ok (+ x 1)))
        (defn dbl :sig [(x : _) -> Result[i64 String]]
          :requires [(valid x)] :ensures [(valid result)] :body (Ok (* x 2)))
        (match (chain [(step "inc" inc identity) (step "dbl" dbl identity)] 5)
          (Ok v) v (Err _) -1)
    "#).unwrap();
    assert_eq!(result, Value::Int(12));
}

#[test]
fn test_chain_short_circuit() {
    let result = run_forge_source(r#"
        (defn fail-fn :sig [(x : _) -> Result[i64 String]]
          :requires [(valid x)] :ensures [(valid result)] :body (Err "fail"))
        (match (chain [(step "f" fail-fn identity)] 5) (Ok _) "bad" (Err _) "caught")
    "#).unwrap();
    assert_eq!(result, Value::Str("caught".into()));
}
```

- [ ] **Step 3: Run tests, commit** — `git commit -m "feat: compose/chain.airl"`

---

### Task 13: `compose/validate.airl`

**Files:** `../airl-forge/lib/compose/validate.airl`

- [ ] **Step 1: Write implementation**

```lisp
;; validate.airl — Output validation with retry

(deftype ValidationResult (|
  (Valid _)
  (Invalid String _)))

(defn validate-output
  :sig [(value : _) (tool-name : String) -> ValidationResult]
  :intent "Check if a tool result is Ok or Err"
  :requires [(not (is-empty-str tool-name))]
  :ensures [(valid result)]
  :body
    (match value
      (Ok v) (Valid v)
      (Err e) (Invalid (to-string e) value)
      _ (Valid value)))

(defn validate-retry-loop
  :sig [(f : _) (args : _) (max-retries : i64) (attempt : i64) -> Result[_ String]]
  :requires [(valid f)]
  :ensures [(valid result)]
  :body
    (if (>= attempt max-retries)
      (Err "max retries exceeded")
      (match (f args)
        (Ok v) (Ok v)
        (Err reason) (validate-retry-loop f args max-retries (+ attempt 1)))))

(defn validate-with-retry
  :sig [(f : _) (args : _) (max-retries : i64) -> Result[_ String]]
  :intent "Call function, retry on Err up to max-retries times"
  :requires [(> max-retries 0)]
  :ensures [(match result (Ok v) (valid v) (Err _) true)]
  :body (validate-retry-loop f args max-retries 0))
```

- [ ] **Step 2: Write test**

```rust
#[test]
fn test_validate_with_retry() {
    let result = run_forge_source(r#"
        (defn ok-fn :sig [(x : _) -> Result[i64 String]]
          :requires [(valid x)] :ensures [(valid result)] :body (Ok (+ x 1)))
        (match (validate-with-retry ok-fn 5 3) (Ok v) v (Err _) -1)
    "#).unwrap();
    assert_eq!(result, Value::Int(6));
}
```

- [ ] **Step 3: Run tests, commit** — `git commit -m "feat: compose/validate.airl"`

---

### Task 14: Final Pipeline Integration

**Files:** `../airl-forge/crates/airl-forge/src/pipeline.rs`

- [ ] **Step 1: Wire all `include_str!` module loading**

Update `create_forge_interpreter` to load all 6 AIRL modules in order:

```rust
const CODEC_SOURCE: &str = include_str!("../../../lib/core/codec.airl");
const SCHEMA_SOURCE: &str = include_str!("../../../lib/core/schema.airl");
const TOOLS_SOURCE: &str = include_str!("../../../lib/core/tools.airl");
const PROVIDER_SOURCE: &str = include_str!("../../../lib/core/provider.airl");
const CHAIN_SOURCE: &str = include_str!("../../../lib/compose/chain.airl");
const VALIDATE_SOURCE: &str = include_str!("../../../lib/compose/validate.airl");

const FORGE_MODULES: &[(&str, &str)] = &[
    ("codec", CODEC_SOURCE),
    ("schema", SCHEMA_SOURCE),
    ("tools", TOOLS_SOURCE),
    ("provider", PROVIDER_SOURCE),
    ("chain", CHAIN_SOURCE),
    ("validate", VALIDATE_SOURCE),
];
```

Load each module in `create_forge_interpreter` after `eval_prelude`:

```rust
for (name, source) in FORGE_MODULES {
    let tokens = airl_syntax::lexer::lex(source)
        .map_err(|e| format!("Forge {} lex error: {:?}", name, e))?;
    let top_levels = airl_syntax::parser::parse(&tokens)
        .map_err(|e| format!("Forge {} parse error: {:?}", name, e))?;
    for tl in &top_levels {
        interp.eval_top_level(tl)
            .map_err(|e| format!("Forge {} eval error: {:?}", name, e))?;
    }
}
```

- [ ] **Step 2: Write full integration test**

```rust
#[test]
fn test_full_integration() {
    let result = run_forge_source(r#"
        (let (config : Map (mock-provider (map-from-entries [["content" "test response"]])))
          (match (call-provider config [(User "hello")] [])
            (Ok resp)
              (let (content : String (map-get resp "content"))
                (match (encode-value (map-from-entries [["answer" content]]))
                  (Ok json)
                    (match (decode-as json ["answer"])
                      (Ok m) (map-get m "answer")
                      (Err _) "decode-err")
                  (Err _) "encode-err"))
            (Err _) "provider-err"))
    "#).unwrap();
    assert_eq!(result, Value::Str("test response".into()));
}
```

- [ ] **Step 3: Run all tests**

Run: `cd ../airl-forge && cargo test`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
cd ../airl-forge && git add -A && git commit -m "feat: complete Phase 1 — all modules integrated"
```

---

## Task Dependency Graph

| Task | Description | Depends On |
|------|-------------|------------|
| 0 | AIRL core extension points | — |
| 1 | Repo scaffold | 0 |
| 2 | `json-parse` | 1 |
| 3 | `json-encode` | 2 (same file) |
| 4 | `env` + `to-string` | 1 |
| 5 | `http-request` | 1 |
| 6 | `fn-metadata` | 1 |
| 7 | Pipeline wrapper | 1 |
| 8 | `core/codec.airl` | 2, 3, 7 |
| 9 | `core/schema.airl` | 7 |
| 10 | `core/tools.airl` | 6, 9 |
| 11 | `core/provider.airl` | 4, 5, 8 |
| 12 | `compose/chain.airl` | 4, 7, 8 |
| 13 | `compose/validate.airl` | 4, 7 |
| 14 | Final integration | 8-13 |

### Parallelizable After Task 1

- **Group A:** Tasks 2→3 (JSON, sequential)
- **Group B:** Task 4 (env + to-string)
- **Group C:** Task 5 (http)
- **Group D:** Task 6 (fn-metadata)
- **Group E:** Task 7 (pipeline)

### Phase 2 Preview

After Phase 1: `compose/loop.airl` (ReAct agent loop), `forge/forge.airl` (create-forge, ask, ask-with-tools), `forge/serve.airl` (HTTP tool server), `start-server` Rust builtin.
