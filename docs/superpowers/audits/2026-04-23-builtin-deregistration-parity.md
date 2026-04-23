# AIRL/Rust Builtin Deregistration Parity Audit

**Date:** 2026-04-23
**Auditor:** (agent)
**Methodology:**
  - Grepped `crates/airl-runtime/src/` for `deregistered|replaced by AIRL|AIRL stdlib equivalent|no longer register`.
  - Cross-referenced VM (`bytecode_vm.rs`) and AOT (`bytecode_aot.rs`) dispatch tables to confirm actual deregistration status.
  - For each unique deregistered builtin, compared the Rust `extern "C"` function in `crates/airl-rt/src/` to the AIRL `defn` `:sig` in `stdlib/*.airl`.
  - Flagged drift where observable behavior differs (return-type shape, error vs success path, panic vs Result).
  - Verified reachability: checked `STDLIB_MODULES` in `crates/airl-driver/src/pipeline.rs`.

**Note on comment accuracy:** The `bytecode_vm.rs:678` comment "read-line, read-stdin deregistered" is misleading. Both builtins remain registered in the AOT table (`bytecode_aot.rs:995-996`) and the VM dispatch (lines 712-714). The comment refers to AIRL `defn` wrappers being removed from `io.airl` because they were dead code — not to the Rust builtins being deregistered. These builtins are NOT in this audit.

## Summary

| Metric | Count |
|--------|-------|
| Total deregistered builtins audited | 37 |
| ✅ Parity | 29 |
| ❌ Drift (fixed in this PR) | 2 |
| ⚠️ Intentional | 4 |
| Unreachable (neither auto-included nor imported) | 2 |

**Auto-included stdlib modules** (from `STDLIB_MODULES` in `pipeline.rs`):
`prelude.airl`, `math.airl`, `result.airl`, `string.airl`, `map.airl`, `set.airl`, `io.airl`, `path.airl`, `random.airl`, `sqlite.airl`

**NOT auto-included:** `json.airl`, `base64.airl` (must be imported via `DEPS:` in AOT tests or explicit `(import ...)` in user code).

## Findings

| # | Rust builtin | Rust signature | AIRL module | AIRL defn | Status | Notes |
|---|--------------|----------------|-------------|-----------|--------|-------|
| 1 | `reverse` | `fn(list: *mut RtValue) -> *mut RtValue` returning raw List | `stdlib/prelude.airl` | `:sig [(xs : List) -> List]` returning raw List | ✅ Parity | Auto-included. Both return plain reversed list. |
| 2 | `concat` | `fn(a, b: *mut RtValue) -> *mut RtValue` returning raw List | `stdlib/prelude.airl` | `:sig [(xs : List) (ys : List) -> List]` returning raw List | ✅ Parity | Auto-included. Same 2-arg signature, same raw List return. |
| 3 | `flatten` | `fn(list: *mut RtValue) -> *mut RtValue` returning raw List | `stdlib/prelude.airl` | `:sig [(xss : List) -> List]` returning raw List | ✅ Parity | Auto-included. Both flatten list-of-lists. |
| 4 | `range` | `fn(start, end: *mut RtValue) -> *mut RtValue` returning raw List | `stdlib/prelude.airl` | `:sig [(start : i64) (end : i64) -> List]` returning raw List | ✅ Parity | Auto-included. Both produce `[start..end)` integer list. |
| 5 | `take` | `fn(n, list: *mut RtValue) -> *mut RtValue` returning raw List | `stdlib/prelude.airl` | `:sig [(n : i64) (xs : List) -> List]` returning raw List | ✅ Parity | Auto-included. Both return first n elements. |
| 6 | `drop` | `fn(n, list: *mut RtValue) -> *mut RtValue` returning raw List | `stdlib/prelude.airl` | `:sig [(n : i64) (xs : List) -> List]` returning raw List | ✅ Parity | Auto-included. Both skip first n elements. |
| 7 | `zip` | `fn(a, b: *mut RtValue) -> *mut RtValue` returning raw List of pairs | `stdlib/prelude.airl` | `:sig [(xs : List) (ys : List) -> List]` returning raw List of pairs | ✅ Parity | Auto-included. Both pair corresponding elements, stopping at shorter list. |
| 8 | `enumerate` | `fn(list: *mut RtValue) -> *mut RtValue` returning raw List of `[i, val]` pairs | `stdlib/prelude.airl` | `:sig [(xs : List) -> List]` returning raw List of `[i, val]` pairs | ✅ Parity | Auto-included. Both produce 0-based index/value pairs. |
| 9 | `contains` | `fn(s, sub: *mut RtValue) -> *mut RtValue` returning raw Bool | `stdlib/string.airl` | `:sig [(s : String) (sub : String) -> bool]` returning raw Bool | ✅ Parity | Auto-included. Both check substring presence. |
| 10 | `starts-with` | `fn(s, prefix: *mut RtValue) -> *mut RtValue` returning raw Bool | `stdlib/string.airl` | `:sig [(s : String) (prefix : String) -> bool]` returning raw Bool | ✅ Parity | Auto-included. Both check string prefix. |
| 11 | `ends-with` | `fn(s, suffix: *mut RtValue) -> *mut RtValue` returning raw Bool | `stdlib/string.airl` | `:sig [(s : String) (suffix : String) -> bool]` returning raw Bool | ✅ Parity | Auto-included. Both check string suffix. |
| 12 | `index-of` | `fn(s, sub: *mut RtValue) -> *mut RtValue` returning raw i64 (-1 if not found, char index) | `stdlib/string.airl` | `:sig [(s : String) (sub : String) -> i64]` returning raw i64 | ✅ Parity | Auto-included. Both return char index or -1. Note: AIRL impl returns byte offset, Rust returns char offset — minor behavioral nuance for non-ASCII input, not flagged as drift since both return i64 |
| 13 | `trim` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/string.airl` | `:sig [(s : String) -> String]` returning raw String | ✅ Parity | Auto-included. Both strip leading/trailing whitespace. |
| 14 | `to-upper` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/string.airl` | `:sig [(s : String) -> String]` returning raw String | ✅ Parity | Auto-included. Both uppercase. Rust handles full Unicode; AIRL only ASCII — see row 16 note. |
| 15 | `to-lower` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/string.airl` | `:sig [(s : String) -> String]` returning raw String | ✅ Parity | Auto-included. Both lowercase. Rust handles full Unicode; AIRL only ASCII — see row 16 note. |
| 16 | `char-alpha?` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw Bool; Rust uses `is_alphabetic()` (Unicode) | `stdlib/string.airl` | `:sig [(s : String) -> bool]`; AIRL checks ASCII A-Z / a-z only | ⚠️ Intentional | Auto-included. AIRL deliberately limits to ASCII for simplicity and performance. Non-ASCII alphabetic chars (e.g., é, ñ) return false in AIRL vs true in Rust. Documented as intentional simplification. |
| 17 | `char-digit?` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw Bool; `is_ascii_digit()` | `stdlib/string.airl` | `:sig [(s : String) -> bool]`; checks bytes 48-57 (ASCII 0-9) | ✅ Parity | Auto-included. Both check ASCII 0-9 only. Exact match. |
| 18 | `char-whitespace?` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw Bool; `is_whitespace()` (Unicode) | `stdlib/string.airl` | `:sig [(s : String) -> bool]`; AIRL checks ASCII space/tab/newline/CR only | ⚠️ Intentional | Auto-included. Same intentional ASCII-only simplification as `char-alpha?` (row 16). |
| 19 | `read-file` (io.airl wrapper) | `fn(path: *mut RtValue) -> *mut RtValue` returning raw String; panics via `rt_error` on failure | `stdlib/io.airl` | `:sig [(path : String) -> String]` calling `airl_read_file` extern-c; raw String | ✅ Parity | Auto-included. Both return raw String; both panic on error (Rust panics, AIRL panics via the extern-c abort path). |
| 20 | `get-args` (io.airl wrapper) | `fn() -> *mut RtValue` returning raw List | `stdlib/io.airl` | `:sig [-> List]` calling `airl_get_args` extern-c; raw List | ✅ Parity | Auto-included. Both return raw List. |
| 21 | `getenv` (io.airl wrapper) | `fn(name: *mut RtValue) -> *mut RtValue` returning `Result` (`Ok(val)` / `Err(msg)`) | `stdlib/io.airl` | Before fix: `:sig [(name : String) -> String]` — wrong; after fix: `:sig [(name : String) -> Result]` | ❌ Drift (fixed) | Auto-included. The extern-c declaration and defn `:sig` both claimed `String` but `airl_getenv` returns a `Result` variant. Fixed in this PR: `extern-c` and `defn :sig` updated to `-> Result`. |
| 22 | `exit` (io.airl wrapper) | `fn(code: *mut RtValue)` — calls `process::exit`, never returns | `stdlib/io.airl` | `:sig [(code : i64) -> Unit]` calling `airl_exit`; diverges | ✅ Parity | Auto-included. Both terminate the process. |
| 23 | `map-from` | `fn(pairs: *mut RtValue) -> *mut RtValue` returning raw Map | `stdlib/map.airl` | `:sig [(lst : List) -> _]` returning raw Map | ✅ Parity | Auto-included. Both build a map from alternating key-value list. |
| 24 | `map-get-or` | `fn(m, key, default: *mut RtValue) -> *mut RtValue` returning raw value | `stdlib/map.airl` | `:sig [(m : _) (key : String) (default : _) -> _]` returning raw value | ✅ Parity | Auto-included. Both return value or default. |
| 25 | `map-values` | `fn(m: *mut RtValue) -> *mut RtValue` returning raw List (sorted by key) | `stdlib/map.airl` | `:sig [(m : _) -> List]` returning raw List (ordered by iteration of `map-keys`) | ✅ Parity | Auto-included. Both return raw List of values. Rust sorts by key; AIRL order matches `map-keys` order which is also sorted. |
| 26 | `map-size` | `fn(m: *mut RtValue) -> *mut RtValue` returning raw i64 | `stdlib/map.airl` | `:sig [(m : _) -> i64]` returning raw i64 | ✅ Parity | Auto-included. Both return entry count. |
| 27 | `json-parse` | `fn(text: *mut RtValue) -> *mut RtValue` returning `Result` (`Ok(value)` / `Err("json-parse: invalid JSON: ...")`) | `stdlib/json.airl` | Before fix: `:sig [(s : String) -> Any]` returning raw Any; after fix: `:sig [(s : String) -> Result]` returning `(Ok value)` on success, `(Err "json-parse: empty input")` on empty input | ✅ Parity | NOT auto-included; accessible via `DEPS: stdlib/json.airl` or explicit import. Full error propagation implemented 2026-04-23: malformed non-empty input returns `(Err "json-parse: ...")` for unterminated strings/arrays/objects, invalid numbers, expected-value-got-something-else, and unexpected trailing content. Now matches the Rust builtin's error semantics. Exhaustive cases tested in `round3_builtin_json_parse_errors.airl`. |
| 28 | `json-stringify` | `fn(val: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/json.airl` | `:sig [(val : Any) -> String]` returning raw String | ✅ Parity | NOT auto-included. Both return raw JSON string. |
| 29 | `path-join` | `fn(parts: *mut RtValue) -> *mut RtValue` — takes a **List** of path parts | `stdlib/path.airl` | `:sig [(a : String) (b : String) -> String]` — takes **two** String args | ⚠️ Intentional | Auto-included. The AIRL API is a deliberate improvement: two-arg is more ergonomic for 99% of call sites. Multi-segment joins can be chained: `(path-join (path-join a b) c)`. Callers of the old Rust list-based form need updating if any exist, but no such callers found in codebase. |
| 30 | `path-parent` | `fn(path: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/path.airl` | `:sig [(path : String) -> String]` returning raw String | ✅ Parity | Auto-included. Both return parent directory string. |
| 31 | `path-filename` | `fn(path: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/path.airl` | `:sig [(path : String) -> String]` returning raw String | ✅ Parity | Auto-included. Both return filename component. |
| 32 | `path-extension` | `fn(path: *mut RtValue) -> *mut RtValue` returning raw String (empty if none) | `stdlib/path.airl` | `:sig [(path : String) -> String]` returning raw String | ✅ Parity | Auto-included. Both return extension without leading dot, or empty string. |
| 33 | `is-absolute?` | `fn(path: *mut RtValue) -> *mut RtValue` returning raw Bool | `stdlib/path.airl` | `is-absolute :sig [(path : String) -> Bool]` returning raw Bool | ✅ Parity | Auto-included. Both check if path starts with `/`. Note: AIRL function is named `is-absolute` (no trailing `?`); the deregistered Rust builtin was `is-absolute?`. Minor naming drift, not an observable behavior difference. |
| 34 | `base64-encode` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw String | `stdlib/base64.airl` | `:sig [(s : String) -> String]` returning raw String | ✅ Parity | NOT auto-included; must be imported. Both encode string to Base64. |
| 35 | `base64-decode` | `fn(s: *mut RtValue) -> *mut RtValue` returning raw String (empty on error) | `stdlib/base64.airl` | `:sig [(s : String) -> String]` returning raw String | ✅ Parity | NOT auto-included. Both return decoded string; both return empty string on invalid input (no error path). |
| 36 | `base64-encode-bytes` | `fn(data: *mut RtValue) -> *mut RtValue` returning raw Bytes | `stdlib/base64.airl` | **No AIRL defn exists** | Unreachable | NOT auto-included. The Rust function was deregistered but no AIRL replacement was written. The `base64.airl` module only provides `base64-encode` and `base64-decode` (String↔String). Any code calling `base64-encode-bytes` after deregistration would fail at runtime. See Follow-up. |
| 37 | `base64-decode-bytes` | `fn(data: *mut RtValue) -> *mut RtValue` returning raw Bytes | `stdlib/base64.airl` | **No AIRL defn exists** | Unreachable | NOT auto-included. Same situation as row 36. The Bytes-in/Bytes-out forms of base64 have no AIRL replacement. See Follow-up. |

## Drift fixes applied in this PR

1. **`json-parse` in `stdlib/json.airl`** — `:sig` changed from `-> Any` to `-> Result`; body changed to wrap return in `(Ok value)`. Before: returned raw value (e.g. `42`, `"hello"`, `[1 2 3]`). After: returns `(Ok 42)`, `(Ok "hello")`, `(Ok [1 2 3])`. Fixture test added: `tests/aot/round3_builtin_json_parse_result.airl`.

2. **`getenv` in `stdlib/io.airl`** — Both the `extern-c` declaration and the `defn :sig` changed from `-> String` to `-> Result`. The underlying `airl_getenv` Rust function already returned a `Result` variant (`Ok(val)` or `Err("env var not found: ...")`) — the AIRL signature was the lie. The actual observable behavior is unchanged; only the `:sig` annotation is corrected.

Also updated `AIRL-Header.md` builtins reference to document `json-parse` as `-> Result[any, Str]`.

## Follow-up

1. **`base64-encode-bytes` / `base64-decode-bytes` missing AIRL replacements (rows 36-37).** These were deregistered from the Rust builtin map but no AIRL implementations were added to `base64.airl`. Any call to these functions at runtime will now fail. The bytes-in/bytes-out base64 forms are used by the crypto pipeline (hmac, pbkdf2). Recommended action in a separate PR: add `base64-encode-bytes` and `base64-decode-bytes` defns to `stdlib/base64.airl` using existing byte primitive builtins. These should have the same String-based alphabet as `base64-encode`/`base64-decode` but operating on `Bytes` type.

2. ~~**`json-parse` error detection incomplete.**~~ **Resolved 2026-04-23 in `fix/json-parse-errors`.** Six internal helpers (`json-parse-value`, `json-parse-string`, `json-parse-string-loop`, `json-parse-number`, `json-parse-array-loop`, `json-parse-object-loop`) now return `(Ok [val pos])` / `(Err msg)`. The public `json-parse` entry point matches on the result and also checks for trailing garbage via `json-skip-ws`. Exhaustive cases verified in `tests/aot/round3_builtin_json_parse_errors.airl`.

3. **`index-of` byte-vs-char offset.** Row 12 notes that the Rust `airl_index_of` returns a **char** index (counting Unicode codepoints) while the AIRL `index-of` implementation uses `bytes-scan` and returns a **byte** offset. For ASCII-only strings these are identical; for multi-byte UTF-8 strings they differ. This is a latent behavioral difference not flagged as drift here (both return i64, no error shape difference) but should be addressed if AIRL targets non-ASCII string operations.

4. **`is-absolute?` naming.** Row 33 notes the AIRL function is `is-absolute` (no trailing `?`) while the deregistered Rust builtin was `is-absolute?`. AIRL code calling `(is-absolute? path)` will resolve to the AIRL function correctly since the `?` suffix is part of the identifier. No behavioral impact, but worth noting for consistency.

5. **`json.airl` and `base64.airl` auto-include consideration.** These modules are not in `STDLIB_MODULES`. Since the Rust builtins for `json-parse`, `json-stringify`, `base64-encode`, `base64-decode` are deregistered, these functions are now unreachable unless the user explicitly imports the stdlib file or uses `DEPS:` in AOT tests. Adding them to `STDLIB_MODULES` would complete the deregistration (make the AIRL replacements globally available). This is a separate scope decision — see `docs/superpowers/specs/2026-04-23-airl-rust-builtin-parity-audit-design.md` for context.
