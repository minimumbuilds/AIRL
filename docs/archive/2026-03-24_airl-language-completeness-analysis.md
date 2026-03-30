# AIRL Language & Stdlib Completeness Analysis (v2)

**Date:** 2026-03-24
**Reviewer:** The Critic
**Subject:** AIRL v0.2.1 — language feature completeness, stdlib gaps, duplication audit
**Supersedes:** Previous analysis from same date (was written against v0.2.0; most "critical gaps" have since been resolved)

---

## Verdict

AIRL v0.2.1 is a substantially complete language for its stated purpose: AI-generated, contract-verified, agent-interoperable computation. The core language (types, control flow, ownership, contracts, pattern matching) is solid. The stdlib covers collections, math, strings, maps, sets, results, and error handling. File I/O went from 3 builtins to 13, HTTP went from POST-only to all verbs, float math went from nonexistent to 15 builtins, and character-level string ops were added. The previous analysis identified 30+ missing functions; all critical and medium-severity gaps have been closed. What remains are missing *categories* of functionality — regex, crypto, concurrency, testing, path manipulation — and some design constraints that limit expressiveness (string-only map keys, no streaming I/O, no iterators). These are real gaps, but they're the kind of gaps a v0.2 language has, not the kind that make it unusable.

---

## Critical Issues

### 1. `http-post` is dead code that should be removed

**Where:** `crates/airl-runtime/src/builtins.rs:1625-1658` (registration at line 1360)

**What:** `http-post` is explicitly marked `// deprecated: use http-request` in the registration comment and has a doc comment saying the same. But it's still registered and callable. It duplicates `http-request "POST"` with a slightly different signature (3 args vs 4 args — no method parameter).

**Why it matters:** Two builtins that do the same thing with different signatures. Any AIRL code written against `http-post` will break when it's eventually removed, and any code written against `http-request` won't know `http-post` exists. The spec mentions both, compounding the confusion. This is the *only* confirmed duplication in the entire builtin set.

**Fix:** Remove `http-post` from the builtin registry and delete `builtin_http_post`. Update the spec and any example code. If backward compatibility matters, have `http-post` delegate to `http-request` internally (but it doesn't — it's a fully separate implementation with its own `ureq` agent construction, which is also a maintenance burden).

### 2. Spec-implementation divergence on float types

**Where:** Spec §2.1 lists `f16`, `f32`, `f64`, `bf16`. Runtime type system (`airl-types/checker.rs`) recognizes these. But:

**What:** The float math builtins (`sqrt`, `sin`, `cos`, etc. at `builtins.rs:1370-1395`) operate on `f64` exclusively. The stdlib `math.airl` is i64-only. Tensor ops use `f32` internally. There is no runtime path to perform float math on `f16` or `bf16` values outside of tensor operations.

**Why it matters:** The spec promises four float types. The implementation delivers one (`f64` for scalar math, `f32` for tensors). Code that declares `bf16` variables and tries to call `sqrt` on them will hit type errors or silent coercion at runtime — behavior that varies by execution path (bytecode VM vs JIT vs AOT). This is a correctness issue, not a convenience gap.

**Fix:** Either (a) document that scalar float math is f64-only and `f16`/`bf16` are tensor-only types, or (b) implement coercion builtins (`f16-to-f64`, `bf16-to-f32`, etc.) so scalar math works on all advertised types.

### 3. No test coverage for float math builtins

**Where:** `crates/airl-runtime/src/builtins.rs:1370-1395` (15 builtins). Test section starts at line ~2300.

**What:** The float math functions (`sqrt`, `sin`, `cos`, `tan`, `log`, `exp`, `floor`, `ceil`, `round`, `float-to-int`, `int-to-float`, `infinity`, `nan`, `is-nan?`, `is-infinite?`) are registered but have zero test coverage in the builtin test suite. No `.airl` test fixtures exercise them either.

**Why it matters:** These are numeric operations where edge cases matter (negative sqrt, log(0), tan(π/2), NaN propagation, infinity arithmetic). Without tests, you don't know if they work. Given that the runtime boxes everything as `RtValue`, there's a real risk that float values are being silently truncated to i64 somewhere in the boxing/unboxing path.

**Fix:** Add at minimum: `sqrt(4.0) = 2.0`, `sin(0.0) = 0.0`, `floor(3.7) = 3`, `is-nan?(nan()) = true`, `float-to-int(3.14) = 3`, `log(1.0) = 0.0`. These are not exhaustive — they're smoke tests to prove the plumbing works.

---

## Significant Problems

### 4. No streaming/line-by-line file read

**Where:** `read-file` at `builtins.rs:1095` reads the entire file into a single `Value::Str`.

**What:** There is no `read-lines`, `read-bytes`, or any streaming I/O primitive. Processing a 100MB log file requires loading it entirely into memory as a string, then splitting with `(lines (read-file path))` — which doubles memory usage (original string + list of line strings).

**Why it matters:** For a language targeting agent workloads that process data, this is a bottleneck. The `lines` stdlib function exists (`stdlib/string.airl`) but it operates on an already-loaded string, not a file handle.

**Fix:** Add `read-lines` builtin that returns a list of strings directly from the file (one allocation pass, not two). Or, more ambitiously, add a `file-handle` type with `read-line` for true streaming.

### 5. No path manipulation

**Where:** Nowhere — completely absent.

**What:** No `path-join`, `path-parent`, `path-filename`, `path-extension`, `path-normalize`, `path-separator`, `is-absolute?`. The file I/O builtins all take string paths, but there's no way to construct or decompose paths safely.

**Why it matters:** Cross-platform path construction via string concatenation (`(str dir "/" file)`) breaks on Windows and is error-prone everywhere. The sandbox validation in `validate_sandboxed_path` handles normalization internally, but user code can't do the same.

**Fix:** Add 5-6 path builtins: `path-join`, `path-parent`, `path-filename`, `path-extension`, `is-absolute?`, `path-separator`.

### 6. No regex support

**Where:** Nowhere — completely absent.

**What:** No regular expression compilation, matching, replacement, or splitting. All string searching is literal substring matching (`contains`, `index-of`, `split`).

**Why it matters:** Pattern matching on strings is fundamental to text processing, log parsing, input validation, and data extraction — all core agent workloads. Without regex, complex string matching requires manual character-by-character parsing.

**Fix:** Add `regex-match`, `regex-find-all`, `regex-replace`, `regex-split` backed by Rust's `regex` crate. This is ~50 lines of builtin code.

### 7. No crypto/hashing

**Where:** Nowhere — completely absent.

**What:** No SHA-256, MD5, HMAC, random bytes, base64 encode/decode. Agent communication in the spec describes trust levels and capability verification, but the language can't compute a hash.

**Why it matters:** Any agent-to-agent protocol that needs to verify message integrity, generate tokens, or sign payloads is impossible without shelling out to `openssl` via `shell-exec`.

**Fix:** Add `sha256`, `hmac-sha256`, `base64-encode`, `base64-decode`, `random-bytes` backed by Rust's `sha2`/`hmac`/`base64`/`rand` crates.

### 8. No concurrency primitives beyond agent messaging

**Where:** Agent builtins (`spawn-agent`, `send`, `send-async`, `await`, `parallel`) exist but are process-level, not thread-level.

**What:** No threads, no async/await within a single program, no channels, no mutexes, no atomics. The only parallelism is spawning separate AIRL agent processes and exchanging messages.

**Why it matters:** Fine-grained parallelism within a computation (parallel map over a dataset, concurrent HTTP requests, producer-consumer pipelines) requires spawning entire agent processes. The overhead makes it impractical for anything smaller than a multi-second task.

**Fix:** This is a design decision, not a bug. If AIRL intends to keep parallelism at the agent level, document it explicitly. If fine-grained concurrency is planned, `channel-new`, `channel-send`, `channel-recv` would be the minimal set.

### 9. String-only map keys and set elements

**Where:** `builtins.rs` map implementation uses `BTreeMap<String, Value>`. `stdlib/set.airl` implements sets as maps with `true` values.

**What:** Map keys must be strings. Set elements must be strings (because sets are maps). No integer keys, no composite keys, no enum keys.

**Why it matters:** `(map-set m 42 "value")` silently coerces `42` to `"42"` (or fails — behavior depends on the code path). Grouping by integer, indexing by tuple, or building frequency tables of non-string values all require manual `int-to-string` wrappers.

**Fix:** Extend map keys to accept any hashable value type (`i64`, `bool`, `String`). This requires changing the Rust backing from `BTreeMap<String, Value>` to `BTreeMap<Value, Value>` with a `Hash`/`Ord` impl on `Value`.

### 10. `length` on strings returns byte count, not character count

**Where:** `builtins.rs` — `length` dispatches on type; for strings it returns `s.len()` (byte length).

**What:** `(length "café")` returns 5, not 4. The only way to get character count is `(length (chars "café"))`, which allocates a list of single-character strings just to count them.

**Why it matters:** Every string algorithm that uses `length` for bounds checking will be wrong on non-ASCII input. The `pad-left`/`pad-right` stdlib functions also use byte-based `length`, so padding is wrong for multibyte characters.

**Fix:** Either (a) change `length` on strings to return char count (breaking change, but correct), or (b) add `char-count` builtin and document that `length` is byte-based.

---

## Minor Issues

### 11. No testing framework

**What:** `assert` exists as a builtin, but there's no test runner, no `deftest` form, no test discovery, no test isolation. The tests in `tests/` are Rust integration tests that shell out to `airl run`.

**Fix:** Add `(deftest name body)` that registers tests, and `airl test <file>` that discovers and runs them with pass/fail reporting.

### 12. No `format` / `sprintf`

**What:** All string formatting is manual: `(str "Value: " (int-to-string x) " of " (int-to-string total))`. No format strings, no interpolation.

**Fix:** Add `(format "Value: {} of {}" x total)` with `{}` placeholder substitution.

### 13. `pow` in stdlib is O(n), not O(log n)

**Where:** `stdlib/math.airl` — `pow` uses naive recursion: `(if (= exp 0) 1 (* base (pow base (- exp 1))))`.

**Fix:** Use exponentiation by squaring: O(log n) instead of O(n). Three more lines of code.

### 14. No `exit` / `exit-code` builtin

**What:** No way to exit with a specific exit code from AIRL code. `panic` aborts with an error, but there's no clean exit with code 0 or any specific status.

**Fix:** Add `(exit code)` builtin.

### 15. No date/time arithmetic

**What:** `time-now` returns epoch millis, `format-time` formats it, `sleep` pauses. But there's no duration arithmetic, no timezone handling, no date parsing.

**Fix:** Low priority for v0.2. Document that time is epoch-millis and leave higher-level date handling to future work.

### 16. `xor` exists as builtin but isn't in the spec

**Where:** `builtins.rs` registers `"xor"`. The spec §4.3 lists `and`, `or`, `not` but not `xor`.

**Fix:** Add `xor` to the spec's logical operators section, or remove it from builtins.

### 17. Spec lists `concat` as "not in AIRL" but stdlib has it

**Where:** Spec's "What DOES NOT exist" section lists `concat` among constructs that will cause errors. But `stdlib/prelude.airl` defines `(defn concat ...)` and it works.

**Fix:** Remove `concat` from the "does not exist" list in the spec.

---

## What Actually Works

The remediation from v0.2.0 to v0.2.1 was thorough and well-executed. Specifically:

**File I/O went from 3 functions to 13.** `append-file` uses OS-level `OpenOptions::append(true)` — atomic append, not read-concat-write. `create-dir` uses `create_dir_all` (recursive). `read-dir` returns sorted entries. All new file builtins go through `validate_sandboxed_path`, maintaining the security model. This is the kind of gap closure that's easy to do badly and was done correctly.

**`http-request` covers all HTTP verbs with a single, clean interface.** GET, POST, PUT, DELETE, PATCH, HEAD — dispatched via a method string parameter. Uses `ureq` with a 300-second timeout. Returns `Result` variants. The only complaint is that `http-post` should be removed now that the generic function exists.

**The stdlib is well-structured and internally consistent.** Every function has contracts (`:requires`, `:ensures`). Naming follows a single convention (`kebab-case` throughout). No function duplicates another. The `prelude.airl` → `math.airl` → `string.airl` → `map.airl` → `result.airl` → `set.airl` layering is clean, with each module depending only on builtins and the modules before it.

**The bootstrap compiler is a genuine achievement.** A self-hosting compiler written in the language it compiles, verified at fixpoint (compiled compiler produces identical IR to interpreted version), linking against `libairl_rt.a` for primitive builtins. This is a strong proof of language completeness for symbolic computation.

---

## Complete Builtin & Stdlib Inventory (v0.2.1)

| Category | Rust Builtins | Stdlib (AIRL) | Total |
|----------|--------------|---------------|-------|
| Arithmetic | `+`, `-`, `*`, `/`, `%` | `abs`, `min`, `max`, `clamp`, `sign`, `even?`, `odd?`, `pow`, `gcd`, `lcm`, `sum-list`, `product-list` | 17 |
| Comparison | `=`, `!=`, `<`, `>`, `<=`, `>=` | — | 6 |
| Logic | `and`, `or`, `not`, `xor` | — | 4 |
| List (core) | `length`, `at`, `at-or`, `set-at`, `head`, `tail`, `cons`, `append`, `empty?`, `list-contains?` | — | 10 |
| List (higher-order) | — | `map`, `filter`, `fold`, `reverse`, `concat`, `zip`, `flatten`, `range`, `take`, `drop`, `any`, `all`, `find`, `sort`, `merge`, `unique`, `enumerate`, `group-by` | 18 |
| String (core) | `str`, `char-at`, `char-code`, `char-from-code`, `substring`, `split`, `join`, `contains`, `starts-with`, `ends-with`, `trim`, `to-upper`, `to-lower`, `replace`, `index-of`, `chars` | — | 16 |
| String (stdlib) | — | `words`, `unwords`, `lines`, `unlines`, `repeat-str`, `pad-left`, `pad-right`, `is-empty-str`, `reverse-str`, `count-occurrences` | 10 |
| Map (core) | `map-new`, `map-from`, `map-get`, `map-get-or`, `map-set`, `map-has`, `map-remove`, `map-keys`, `map-values`, `map-size` | — | 10 |
| Map (stdlib) | — | `map-entries`, `map-from-entries`, `map-merge`, `map-map-values`, `map-filter`, `map-update`, `map-update-or`, `map-count` | 8 |
| Set (stdlib) | — | `set-new`, `set-from`, `set-add`, `set-remove`, `set-contains?`, `set-size`, `set-to-list`, `set-union`, `set-intersection`, `set-difference`, `set-subset?` | 11 |
| Result (stdlib) | — | `is-ok?`, `is-err?`, `unwrap-or`, `map-ok`, `map-err`, `and-then`, `or-else`, `ok-or` | 8 |
| File I/O | `read-file`, `write-file`, `append-file`, `file-exists?`, `delete-file`, `delete-dir`, `rename-file`, `create-dir`, `read-dir`, `file-size`, `is-dir?`, `get-args` | — | 12 |
| Float math | `sqrt`, `sin`, `cos`, `tan`, `log`, `exp`, `floor`, `ceil`, `round`, `float-to-int`, `int-to-float`, `infinity`, `nan`, `is-nan?`, `is-infinite?` | — | 15 |
| Type conversion | `int-to-string`, `float-to-string`, `string-to-int`, `string-to-float`, `char-code`, `char-from-code`, `type-of` | — | 7 |
| Error handling | `panic`, `assert`, `valid` | — | 3 |
| System | `shell-exec`, `time-now`, `sleep`, `format-time`, `getenv`, `get-args` | — | 6 |
| Network/JSON | `http-request`, ~~`http-post`~~ *(deprecated)*, `json-parse`, `json-stringify` | — | 4 |
| Output | `print`, `println` | — | 2 |
| Tensor | `tensor.zeros`, `tensor.ones`, `tensor.rand`, `tensor.identity`, `tensor.add`, `tensor.mul`, `tensor.matmul`, `tensor.reshape`, `tensor.transpose`, `tensor.softmax`, `tensor.sum`, `tensor.max`, `tensor.slice` | — | 13 |
| Agent | `spawn-agent`, `send`, `send-async`, `await`, `parallel`, `broadcast`, `retry`, `escalate`, `any-agent` | — | 9 |
| Special | `shape`, `run-bytecode`, `compile-to-executable` | — | 3 |

**Grand total: ~192 functions** (103 Rust builtins + ~89 stdlib functions)

---

## Missing Feature Categories (Not Yet Addressable)

| Category | Impact | Effort to Add |
|----------|--------|---------------|
| **Regex** | High — text processing, validation, parsing | Low (wrap Rust `regex` crate) |
| **Path manipulation** | Medium — safe cross-platform file handling | Low (5-6 builtins wrapping `std::path`) |
| **Crypto/hashing** | Medium — agent message integrity, tokens | Low (wrap `sha2`/`base64` crates) |
| **Streaming I/O** | Medium — large file processing | Medium (needs file handle type or lazy list) |
| **Testing framework** | Medium — developer experience | Medium (needs `deftest` form + runner) |
| **Format strings** | Low — developer convenience | Low (simple `{}` substitution builtin) |
| **Concurrency** | Low for agent use; High for compute | High (needs runtime threading model) |
| **Non-string map keys** | Medium — data modeling flexibility | Medium (needs `Hash`/`Ord` on `Value`) |
| **Date/time arithmetic** | Low | Low (wrap `chrono` crate) |
| **Exit codes** | Low | Trivial (one builtin) |

---

## Duplication Audit

| Item | Status | Action |
|------|--------|--------|
| `http-post` vs `http-request "POST"` | **Confirmed duplicate.** `http-post` is deprecated, has separate implementation. | Remove `http-post`. |
| `list-contains?` (builtin) vs `(any (fn [x] (= x target)) xs)` | Not duplication — builtin is O(n) native, stdlib composition is O(n) interpreted. Builtin is justified for performance. | Keep both. |
| `at-or` (builtin) vs manual `(if (< i (length xs)) (at xs i) default)` | Same — native vs interpreted performance difference justifies the builtin. | Keep. |
| `println` vs `(print (str x "\n"))` | Convenience, not duplication. | Keep. |
| All other builtins | No duplicates found. Each builtin has a unique capability not replicated elsewhere. | — |

---

## Code Review Skill Compatibility Note

The `code-review:code-review` skill was tested against this project. **It does not work for local codebase review.** The skill is designed exclusively for GitHub pull requests — it requires a PR URL, uses `gh` CLI commands to fetch diffs and post comments, and its entire 8-step workflow (eligibility check → file list → summary → 5-way parallel review → scoring → filtering → re-check → comment) assumes a PR context. To use it with AIRL, the code would need to be in a PR on GitHub. For local/offline codebase review, use The Critic methodology directly.
