# AIRL-Forge Design Spec

**Date:** 2026-03-23
**Status:** Draft

## Overview

AIRL-Forge is a standalone library built on AIRL that provides a bidirectional AI function framework. AIRL programs can orchestrate LLMs (call providers, validate outputs, chain tools), and external LLMs can discover and call AIRL functions (with contract-based validation at the boundary).

The library leverages AIRL's existing strengths — mandatory contracts, sum types with exhaustive matching, and the `:intent` annotation — rather than reinventing validation or schema systems.

## Goals

- **Bidirectional:** AIRL calls LLMs, LLMs call AIRL tools
- **Contract-validated:** LLM outputs are untrusted input, validated against `:requires`/`:ensures`
- **Convention-based discovery:** Functions with `:intent` are automatically discoverable as tools
- **Transport-agnostic:** Abstract provider interface with HTTP and agent-protocol backends
- **Hybrid Rust/AIRL:** Rust for system boundaries (HTTP, JSON), AIRL for orchestration logic
- **Layered:** Toolkit → Composition → Framework, each independently useful

## Non-Goals

- Z3 proof integration for LLM output validation (can be layered on later)
- DAG-based workflow orchestration (phase 2, after pipelines and ReAct loops)
- Modifying AIRL core — this is a separate repository

## Architecture

```
┌─────────────────────────────────────┐
│  Layer 3: Forge (framework)         │  create-forge, ask, ask-with-tools, serve-tools
├─────────────────────────────────────┤
│  Layer 2: Compose (orchestration)   │  chain, fan-out, run-loop, validate-output
├─────────────────────────────────────┤
│  Layer 1: Core (toolkit)            │  provider, schema, tools, codec
├─────────────────────────────────────┤
│  Rust crate: airl-forge             │  http-request, json-parse, json-encode,
│                                     │  start-server, fn-metadata
└─────────────────────────────────────┘
```

**Dependency:** The Rust crate depends on `airl-runtime` and `airl-types` from the AIRL repo as external crate dependencies.

## Data Representation: Maps Over Product Types

AIRL has product types (`deftype ... (&  ...)`) in its AST and type system, but the runtime has no field accessor syntax (e.g., no `(. struct field)`). Since Forge needs to read and write record fields pervasively, all data records in Forge use **AIRL Maps** with string keys, accessed via `map-get`, `map-get-or`, and `map-set`.

This trades static type checking for runtime flexibility — a pragmatic choice that avoids requiring changes to AIRL core. Contracts on function inputs/outputs compensate: each function validates the expected shape of its Map arguments via `:requires` clauses.

**Convention:** Record constructors are helper functions that return Maps with known keys:

```lisp
(defn make-provider-config
  :sig [(name : String) (api-url : String) (api-key : String) (model : String) -> Map]
  :intent "Create a provider configuration record"
  :requires [(not (is-empty-str name)) (not (is-empty-str api-key))]
  :ensures [(if (map-has result "name")
              (if (map-has result "api-key")
                (if (map-has result "model")
                  true
                  false)
                false)
              false)]
  :body (map-from-entries
    [["name" name] ["api-url" api-url] ["api-key" api-key] ["model" model]]))
```

**Sum types** (tagged unions via `deftype ... (| ...)`) are used normally — they work well with AIRL's pattern matching and do not need field access.

## Repository Structure

```
airl-forge/
  Cargo.toml
  crates/airl-forge/          # Rust crate: system boundary builtins
    src/
      lib.rs
      http.rs                 # http-request builtin
      json.rs                 # json-parse, json-encode builtins
      server.rs               # start-server builtin
      introspect.rs           # fn-metadata builtin
      env.rs                  # env builtin
  lib/
    core/                     # Layer 1: Toolkit
      provider.airl           # Provider abstraction + contracts
      schema.airl             # Type → JSON Schema conversion
      tools.airl              # Tool registry (auto-discover :intent fns)
      codec.airl              # JSON ↔ AIRL Value marshalling
    compose/                  # Layer 2: Composition
      chain.airl              # Pipeline combinator (chain, fan-out)
      loop.airl               # ReAct conversational agent loop
      validate.airl           # Contract-based output validation + retry
    forge/                    # Layer 3: Framework
      forge.airl              # High-level agent builder (create-forge, ask, ask-with-tools)
      serve.airl              # Expose tools to external LLMs via HTTP
  tests/
    core/                     # Unit tests per module
    compose/
    forge/
    integration/              # End-to-end tests with mock provider
```

## Library Loading

Forge's AIRL modules are loaded via the same `include_str!()` + `eval_prelude()` pattern as the AIRL stdlib. The Rust crate embeds all `.airl` files from `lib/` and evaluates them in order (core → compose → forge) before user code runs. Load order within each layer follows dependency: `codec.airl` before `tools.airl` (tools uses codec), `chain.airl` and `validate.airl` before `loop.airl` (loop uses both).

## Rust Crate — Builtins

The Rust crate provides six builtins. These handle what AIRL cannot do natively (I/O, serialization, introspection, environment access):

### `http-request`

```
(http-request method url headers body) -> Result[Map String]
```

Makes an HTTP request. Returns a Map with keys `"status"` (i64), `"headers"` (Map of String→String), and `"body"` (String). On failure, returns `(Err reason-string)`.

### `json-parse` / `json-encode`

```
(json-parse json-string) -> Result[Value String]
(json-encode value)      -> Result[String String]
```

Bidirectional JSON ↔ AIRL Value marshalling. Maps JSON objects to AIRL Maps, arrays to Lists, numbers to Int (i64) or Float (f64), strings to Str, booleans to Bool, null to Nil.

### `start-server`

```
(start-server config handler) -> Result[Nil String]
```

Starts an HTTP server on a background thread that routes requests to an AIRL handler function. The handler receives a Map with keys `"method"`, `"path"`, `"headers"`, `"body"` and returns a Map with `"status"`, `"headers"`, `"body"`. The calling AIRL thread blocks until the server is shut down (via a `"shutdown"` key in the config, or process exit).

### `fn-metadata`

```
(fn-metadata fn-name) -> Result[Map String]
```

Runtime introspection: given a function name string, looks up the `FnDef` in the interpreter's function registry and returns a Map with:
- `"name"` — function name (String)
- `"intent"` — `:intent` string, or `""` if absent (String)
- `"params"` — list of Maps, each with `"name"` (String) and `"type"` (String representation of type)
- `"return-type"` — String representation of return type
- `"requires"` — list of `:requires` clause strings
- `"ensures"` — list of `:ensures` clause strings

Returns `(Err "function not found")` if the name is not in the registry. This is new Rust code that reads `Interpreter.functions` — it does not exist in AIRL today and must be implemented in this crate. Implementation follows the existing builtin dispatch pattern: builtins that need interpreter state are handled directly in the `FnCall` arm of `eval.rs`, receiving `&self` access to the interpreter.

### `env`

```
(env var-name) -> Result[String String]
```

Reads an environment variable. Returns `(Ok value)` if the variable is set, `(Err "not set: VAR_NAME")` otherwise. Used by Forge to read API keys and auth tokens without hardcoding them.

## Layer 1: Core Toolkit

### Record Constructors

Since all data records are Maps (see "Data Representation" above), each logical record type has a constructor function and accessor helpers. The "type definitions" below document the expected Map shapes:

```lisp
;; ProviderConfig: Map with keys "name", "api-url", "api-key", "model"
;; LLMResponse: Map with keys "content", "tool-calls", "usage"
;; ToolCallRequest: Map with keys "id", "name", "arguments"
;; TokenUsage: Map with keys "input", "output" (both i64)
;; ToolSchema: Map with keys "name", "intent", "parameters", "preconditions"
;; Step: Map with keys "name", "fn", "transform"
;; LoopConfig: Map with keys "provider", "tools", "system", "max-turns", "error-strategy"
;; Forge: Map with keys "provider", "tools", "system", "max-turns", "error-strategy", "history"
;; ServerConfig: Map with keys "host", "port", "tools", "auth-token"
```

Sum types are used where pattern matching is needed:

```lisp
;; LLM message types — pattern matched in provider formatting
(deftype LLMMessage (|
  (System String)
  (User String)
  (Assistant String)
  (ToolCall String String)
  (ToolResult String String)))

;; Error strategies — pattern matched in loop logic
(deftype ErrorStrategy (| Retry Skip Abort))

;; Provider errors
(deftype ProviderError (|
  (HttpError String)
  (ParseError String)
  (AuthError String)
  (RateLimited i64)))

;; Codec errors
(deftype CodecError (|
  (InvalidJson String)
  (TypeMismatch String String)))

;; Chain errors
(deftype ChainError (|
  (StepFailed String String)
  (EmptyPipeline)))

;; Loop errors
(deftype LoopError (|
  (MaxTurnsExceeded i64)
  (ProviderFailed ProviderError)
  (ToolFailed String String)
  (ContractViolation String String)))

;; Forge errors (wraps lower-layer errors)
(deftype ForgeError (|
  (ProviderErr ProviderError)
  (ChainErr ChainError)
  (LoopErr LoopError)
  (SetupErr String)))

;; Validation results
(deftype ValidationResult (|
  (Valid Value)
  (Invalid String Value)))

;; Schema errors
(deftype SchemaError (|
  (UnsupportedType String)))
```

### `core/provider.airl`

```lisp
(defn call-provider
  :sig [(config : Map) (messages : List[LLMMessage])
        (tools : List[Map]) -> Result[Map ProviderError]]
  :intent "Send a message list to an LLM provider and return the response"
  :requires [(not (is-empty-str (map-get-or config "api-key" "")))
             (not (empty? messages))]
  :ensures [(match result (Ok r) (map-has r "content") (Err _) true)]
  :body ...)

(defn http-provider
  :sig [(name : String) (api-key : String) (model : String) -> Map]
  :intent "Create a provider config for a known HTTP API provider"
  :requires [(not (is-empty-str api-key)) (not (is-empty-str model))]
  :ensures [(= (map-get result "name") name)]
  :body ...)
```

Provider-specific request/response formatting (Anthropic vs OpenAI message formats) is handled by internal helper functions dispatched on `(map-get config "name")`.

### `core/schema.airl`

```lisp
(defn type-to-schema
  :sig [(type-str : String) -> Map]
  :intent "Convert an AIRL type string representation to a JSON Schema map"
  :requires [(not (is-empty-str type-str))]
  :ensures [(map-has result "type")]
  :body ...)
```

Type strings come from `fn-metadata`'s `"type"` fields. Mapping rules:
- `"String"` → `{"type": "string"}`
- `"i64"` → `{"type": "integer"}`
- `"f64"` → `{"type": "number"}`
- `"Bool"` → `{"type": "boolean"}`
- `"List[T]"` → `{"type": "array", "items": <schema-of-T>}`
- `"Map"` → `{"type": "object", "additionalProperties": true}`
- Product type strings → `{"type": "object", "properties": {...}, "required": [...]}`
- Sum type strings → `{"oneOf": [...]}` with a `"tag"` discriminator

### `core/tools.airl`

```lisp
(defn discover-tools
  :sig [(fn-names : List[String]) -> List[Map]]
  :intent "Discover all functions with :intent annotations and build tool schemas"
  :requires [(not (empty? fn-names))]
  :ensures [(all (fn (t) (not (is-empty-str (map-get-or t "intent" "")))) result)]
  :body ...)
```

For each function name, calls `fn-metadata`. If the function has a non-empty `"intent"`, builds a tool schema Map with its parameter types converted to JSON Schema via `type-to-schema`. Functions without `:intent` are silently skipped.

### `core/codec.airl`

```lisp
(defn decode-as
  :sig [(json : String) (expected-keys : List[String]) -> Result[Map CodecError]]
  :intent "Parse JSON string into a Map, validating expected keys are present"
  :requires [(not (is-empty-str json))]
  :ensures [(match result
              (Ok v) (all (fn (k) (map-has v k)) expected-keys)
              (Err _) true)]
  :body ...)

(defn encode-value
  :sig [(value : Value) -> Result[String CodecError]]
  :intent "Encode an AIRL value as a JSON string"
  :requires [(valid value)]
  :ensures [(match result (Ok s) (not (is-empty-str s)) (Err _) true)]
  :body ...)
```

`decode-as` is the critical trust boundary: untrusted JSON from LLM responses is parsed via `json-parse`, then validated for expected keys. Type mismatches produce `CodecError` with details about what was expected vs received.

## Layer 2: Composition

### `compose/chain.airl`

```lisp
(defn step
  :sig [(name : String) (f : (-> [Value] Result[Value String]))
        (transform : (-> [Value] Value)) -> Map]
  :intent "Create a pipeline step with a name, function, and output transform"
  :requires [(not (is-empty-str name))]
  :ensures [(map-has result "name")]
  :body (map-from-entries [["name" name] ["fn" f] ["transform" transform]]))

(defn chain
  :sig [(steps : List[Map]) (input : Value) -> Result[Value ChainError]]
  :intent "Execute a pipeline of steps, threading results through with short-circuit on error"
  :requires [(not (empty? steps))]
  :ensures [(match result (Ok _) true (Err _) true)]
  :body ...)

(defn fan-out
  :sig [(steps : List[Map]) (input : Value) -> Result[List[Value] ChainError]]
  :intent "Execute steps sequentially on the same input, collect all results"
  :requires [(not (empty? steps))]
  :ensures [(match result (Ok rs) (= (length rs) (length steps)) (Err _) true)]
  :body ...)
```

`chain` is a left fold over steps, threading `Result`. On `Err`, it captures the failed step name. `fan-out` executes each step on the same input and collects results. Phase 1 executes sequentially; parallel execution via `send-async`/`await` can be added when the concurrency model is proven.

### `compose/loop.airl`

```lisp
(defn run-loop
  :sig [(config : Map) (prompt : String) -> Result[String LoopError]]
  :intent "Run a ReAct agent loop: LLM reasons, calls tools, validates results, repeats until done"
  :requires [(not (is-empty-str prompt))
             (> (map-get-or config "max-turns" 0) 0)]
  :ensures [(match result (Ok s) (not (is-empty-str s)) (Err _) true)]
  :body ...)
```

Loop algorithm:
1. Build initial messages: `[(System system-prompt) (User prompt)]`
2. Call `call-provider` with messages and tool schemas
3. If response has `"tool-calls"` (non-empty list):
   a. For each tool call: look up in registry → `decode-as` arguments → call function → contracts validate input/output automatically → `encode-value` result
   b. On contract violation: pattern match on `ErrorStrategy` — `Retry` re-calls with violation message as feedback, `Skip` returns nil result, `Abort` returns error
   c. Append `(ToolResult name result-json)` messages, go to step 2
4. If response `"content"` is non-empty → return content
5. If turn count exceeds `"max-turns"` → return `(Err (MaxTurnsExceeded turns))`

### `compose/validate.airl`

```lisp
(defn validate-output
  :sig [(value : Value) (tool-name : String) -> ValidationResult]
  :intent "Validate a tool call result against the tool function's contracts"
  :requires [(not (is-empty-str tool-name))]
  :ensures [(valid result)]
  :body ...)

(defn validate-with-retry
  :sig [(config : Map) (tool-name : String) (args : Value)
        (max-retries : i64) -> Result[Value String]]
  :intent "Call a tool and retry up to max-retries times if contract validation fails, feeding violation details back to the LLM"
  :requires [(> max-retries 0)]
  :ensures [(match result (Ok v) (valid v) (Err _) true)]
  :body ...)
```

`validate-with-retry` is the contract-as-feedback loop: when an LLM's output fails validation, the violation message (including the specific clause and variable bindings from AIRL's `capture_bindings()`) is sent back to the LLM as a `ToolResult` error, giving it the information to self-correct.

## Layer 3: Forge Framework

### `forge/forge.airl`

```lisp
(defn create-forge
  :sig [(provider : Map) (system : String)
        (tool-fns : List[String]) -> Result[Map ForgeError]]
  :intent "Create a forge agent with auto-discovered tools and validated configuration"
  :requires [(not (is-empty-str system))
             (not (empty? tool-fns))]
  :ensures [(match result
              (Ok f) (if (not (empty? (map-get-or f "tools" [])))
                       (= (map-get f "system") system)
                       false)
              (Err _) true)]
  :body ...)
```

`create-forge` calls `discover-tools`, sets defaults (`"max-turns": 10`, `"error-strategy": Retry`), and returns a ready-to-use Forge Map.

Convenience functions:

```lisp
(defn ask
  :sig [(forge : Map) (prompt : String) -> Result[String ForgeError]]
  :intent "Single-turn: send prompt, get response, no tool use"
  :requires [(not (is-empty-str prompt))]
  :ensures [(match result (Ok s) (not (is-empty-str s)) (Err _) true)]
  :body ...)

(defn ask-with-tools
  :sig [(forge : Map) (prompt : String) -> Result[String ForgeError]]
  :intent "Multi-turn: run ReAct loop with all registered tools"
  :requires [(not (is-empty-str prompt))]
  :ensures [(match result (Ok s) (not (is-empty-str s)) (Err _) true)]
  :body ...)

(defn ask-chain
  :sig [(forge : Map) (steps : List[Map]) (input : Value) -> Result[Value ForgeError]]
  :intent "Execute a tool pipeline using the forge's provider and tools"
  :requires [(not (empty? steps))]
  :ensures [(match result (Ok _) true (Err _) true)]
  :body ...)
```

### `forge/serve.airl`

```lisp
(defn serve-tools
  :sig [(config : Map) -> Result[Nil String]]
  :intent "Start an HTTP server exposing AIRL tools for external LLM consumption"
  :requires [(> (map-get-or config "port" 0) 0)
             (not (empty? (map-get-or config "tools" [])))]
  :ensures [(match result (Ok _) true (Err _) true)]
  :body ...)
```

Endpoints:
- `GET /tools` — returns tool schemas in OpenAI function-calling JSON format
- `POST /call/:tool-name` — JSON body → `decode-as` → function call → contract validation → JSON response
- `GET /health` — `{"status": "ok"}`

Contract violations on inbound calls return structured JSON errors:
```json
{
  "error": "contract_violation",
  "tool": "summarize",
  "clause": ":requires",
  "violation": "(> max-words 0)",
  "bindings": {"max-words": -5}
}
```

## Full Usage Example

```lisp
;; Define tools — :intent makes them discoverable
(defn web-search
  :sig [(query : String) -> Result[List[Map] String]]
  :intent "Search the web for information matching the query"
  :requires [(not (is-empty-str query))]
  :ensures [(match result (Ok rs) (>= (length rs) 0) (Err _) true)]
  :body ...)

(defn summarize
  :sig [(text : String) (max-words : i64) -> Result[String String]]
  :intent "Summarize text to at most max-words words"
  :requires [(not (is-empty-str text)) (> max-words 0)]
  :ensures [(match result (Ok s) (<= (length (words s)) max-words) (Err _) true)]
  :body ...)

;; Create forge — auto-discovers tools via :intent
(let forge (create-forge
  (http-provider "anthropic" (env "ANTHROPIC_API_KEY") "claude-sonnet-4-6")
  "You are a research assistant. Be thorough and cite sources."
  ["web-search" "summarize"]))

;; Single prompt — LLM decides which tools to call
(match forge
  (Ok f) (match (ask-with-tools f "Find and summarize recent advances in formal verification")
           (Ok answer) (print answer)
           (Err e) (print "Error in agent loop"))
  (Err e) (print "Failed to create forge"))

;; Or explicit pipeline
(match forge
  (Ok f) (match (ask-chain f
           [(step "search" web-search identity)
            (step "summarize" summarize (fn (results) (join (map (fn (r) (map-get-or r "snippet" "")) results) "\n")))]
           "formal verification 2026")
           (Ok summary) (print summary)
           (Err e) (print "Chain failed"))
  (Err e) (print "Failed to create forge"))

;; Or serve tools for external LLMs to call
(match forge
  (Ok f) (serve-tools (map-from-entries
           [["host" "0.0.0.0"] ["port" 8080]
            ["tools" (map-get f "tools")]
            ["auth-token" (env "AUTH_TOKEN")]]))
  (Err e) (print "Failed to create forge"))
```

## Phased Delivery

### Phase 1: Core + Pipelines
- Rust crate (`http-request`, `json-parse`/`json-encode`, `fn-metadata`, `env`)
- Layer 1 complete (provider, schema, tools, codec)
- `compose/chain.airl` (chain, fan-out — sequential)
- `compose/validate.airl`
- Utility functions: `identity`
- Tests with mock provider (AIRL function that returns canned responses, injected in place of `call-provider`)

### Phase 2: Agent Loop + Framework
- Rust crate addition: `start-server`
- `compose/loop.airl` (ReAct loop)
- Layer 3 complete (forge, serve)
- Integration tests with real LLM provider

### Phase 3: DAG Orchestration (future)
- `compose/dag.airl` — declarative workflow graphs
- Dependency resolution, parallel execution of independent steps
- Concurrent `fan-out` via `send-async`/`await`

## Testing Strategy

- **Unit tests:** Each AIRL module has a corresponding test file in `tests/`
- **Mock provider:** A test provider that returns canned responses, for deterministic testing of chains and loops
- **Contract tests:** Verify that contract violations on LLM output are caught and produce correct error messages
- **Integration tests:** End-to-end tests with real provider (gated behind env var for API key)
- **Fixture tests:** Standard AIRL fixture patterns for codec (JSON ↔ Value round-trips) and schema (type → JSON Schema snapshots)

## AIRL Language Constraints

Implementers must observe these AIRL-specific constraints:
- **Eager `and`/`or`:** Both operands always evaluate. Use nested `if` for guard-style short-circuit logic (e.g., check a key exists before reading its value).
- **No mixed int/float:** All integers are `i64` at runtime. Use `int-to-float` for arithmetic mixing.
- **No field access syntax:** Product types have no `(. struct field)` accessor. This spec uses Maps with `map-get`/`map-set` throughout.
- **Function types:** Use `(-> [ArgTypes] RetType)` syntax in `:sig`, not bare `Fn`.
- **No `Value` type in AIRL:** Where this spec uses `Value` in signatures, it means "any AIRL value" — the dynamically-typed boundary between JSON and AIRL. In actual AIRL code, use a type variable or `_` for inference. The Forge library should define a `ForgeValue` sum type if explicit typing is needed: `(deftype ForgeValue (| (FStr String) (FInt i64) (FFloat f64) (FBool Bool) (FList List[ForgeValue]) (FMap Map) FNil))`.
- **Functions in Maps:** The `step` constructor stores closures in Map values. Calling a function retrieved from a map works in AIRL: `((map-get step-map "fn") input)` is valid because `map-get` returns a `Value::Closure` which is callable.
- **`fn-metadata` and `env` are new:** Must be implemented in the Rust crate; they do not exist in AIRL today.
- **`identity` helper:** Not in the AIRL stdlib. Forge should define it: `(defn identity :sig [(x : _) -> _] :requires [(valid x)] :ensures [(valid result)] :body x)`
- **`join` argument order:** `(join list separator)` — list first, separator second.
