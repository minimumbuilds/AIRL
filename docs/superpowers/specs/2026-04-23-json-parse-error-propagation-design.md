# json-parse Error Propagation — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** Make `stdlib/json.airl`'s `json-parse` produce `(Err msg)` for all malformed input — not just empty strings. Currently the pure-AIRL recursive-descent parser silently returns best-effort `(Ok ...)` for invalid non-empty JSON, defeating the Result contract the function advertises.

## Background

The parity-audit fixes (commits `221b339` and `713fe87`) changed `json-parse` to return a `Result`:

```
(json-parse "")        ;; → (Err "json-parse: empty input")
(json-parse "42")      ;; → (Ok 42)
(json-parse "\"hi\"")  ;; → (Ok "hi")
```

That matches the interface of the deregistered Rust builtin `airl_json_parse`. But the audit itself noted a gap:

> **json-parse error detection incomplete.** The pure-AIRL recursive-descent parser (`json-parse-value`) does not propagate parse errors; it always returns a best-effort value. The `(Err ...)` branch of the `Result` returned by `json-parse` is never actually produced by the AIRL implementation (unlike the Rust version which returned `Err` for truly invalid JSON). A future PR could add an explicit validation pass or add error propagation to `json-parse-value`.

Concrete consequences today:

```
(json-parse "not-json")        ;; → (Ok <garbage>)       — should be (Err ...)
(json-parse "[1, 2,")          ;; → (Ok [1 2])           — should be (Err ...)
(json-parse "{\"foo\":")       ;; → (Ok {...})           — should be (Err ...)
(json-parse "true false")      ;; → (Ok true)            — trailing garbage ignored
```

Callers matching on `(Err e)` never see those cases, so defensive code is broken: you can't tell "parse succeeded with this value" from "parse failed and you got silent garbage".

The Rust `airl_json_parse` returned `(Err "json-parse: invalid JSON: <input>")` for all of the above. Parity requires AIRL to do the same.

## Goals

1. Change `json-parse-value` (and its helpers `json-parse-string-loop`, `json-parse-number`, `json-parse-array-loop`, `json-parse-object-loop`) so that every parse failure — unterminated array, unexpected token, incomplete object — propagates up as an `(Err msg)` Result.
2. Have `json-parse` (the public entry point) return `(Err msg)` whenever any sub-parser fails, with a message that is at least as informative as the Rust builtin's (`"json-parse: invalid JSON: ..."`).
3. Detect and error on trailing garbage — `(json-parse "42 x")` returns `(Err ...)` not `(Ok 42)`.

## Non-Goals

- Detailed error positions (line/column numbers). The Rust version didn't track positions either; staying at "invalid JSON" parity is sufficient.
- JSON5 or relaxed-JSON syntax.
- Performance optimization. The error-propagation refactor may slow the parser by 5-15% due to additional Result unwrapping at every recursion boundary; that's acceptable.
- Rewriting the parser to a monadic style or otherwise reshaping its structure. The change is local — add error-return paths to existing helpers.

## Architecture

### The current parser shape

`stdlib/json.airl` currently has helpers that return raw parse results:

```
json-parse-value  :  src -> pos -> len -> [value, new-pos]
json-parse-string :  src -> pos -> len -> [string, new-pos]
json-parse-number :  src -> pos -> len -> [number, new-pos]
json-parse-array  :  src -> pos -> len -> [array, new-pos]
json-parse-object :  src -> pos -> len -> [map, new-pos]
```

Each returns a 2-element list `[parsed-value, next-position]`. There's no error path; bad input produces a best-effort value and a position that may or may not make sense.

### Two options for propagating errors

**Option A — Wrap each helper in Result:**

Every helper returns `(Ok [value, pos])` or `(Err msg)`. Callers unwrap:

```
(json-parse-value src 0 len)
;; → (Ok [42 2]) on success
;; → (Err "unexpected end of input at position 4") on failure
```

Pros: explicit, idiomatic. Cons: significant structural churn — every site that calls a parser helper grows a `(match ... (Ok [v p]) ... (Err e) ...)` block, roughly 20-30 call sites.

**Option B — Sentinel position:**

Reserve a position value (e.g. `-1`) as "parse failed". Helpers return `[nil, -1]` on failure; callers check `(< pos 0)` to detect errors. Message is threaded through a secondary return or a global state (AIRL doesn't have global state, so secondary return).

Pros: minimal structural change. Cons: error messages become coupled to position threading; easy to forget the `< 0` check.

**Recommendation: Option A.**

The churn is real but the code is already structured for it — each helper has exactly one entry point per variant and returns in a single place. Converting raw returns to `Ok`-wrapped returns is mechanical, and the error path becomes searchable by grep (`(Err ...)`). Option B is cleverer but fragile: skipping one bounds check produces silent garbage again.

### Callsite pattern

Each helper invocation becomes:

```
(match (json-parse-value src p len)
  (Ok pair) (let (v (at pair 0))
                 (p' (at pair 1))
              ;; continue with v, p'
              )
  (Err e) (Err e))
```

Where parsing a substructure fails, the `(Err e)` passes through unchanged. This preserves the first error encountered rather than conflating multiple failures.

### Trailing-garbage detection

After `json-parse-value` returns, check that the remaining input (from the returned position to `len`) is whitespace only. If not, return `(Err "json-parse: unexpected trailing content after value")`. This is the last check in the top-level `json-parse` function.

```clojure
(defn json-parse :pub
  :sig [(s : String) -> Result]
  :intent "Parse a JSON string into an AIRL value, returning (Ok value) or (Err msg)"
  :requires [(valid s)]
  :ensures [(valid result)]
  :body (let (src : Bytes (bytes-from-string s))
             (len : i64 (length src))
          (if (= len 0)
            (Err "json-parse: empty input")
            (match (json-parse-value src 0 len)
              (Ok pair)
                (let (v (at pair 0))
                     (end-pos : i64 (at pair 1))
                     (trailing-pos : i64 (skip-whitespace src end-pos len)))
                  (if (= trailing-pos len)
                    (Ok v)
                    (Err "json-parse: unexpected trailing content after value")))
              (Err e) (Err e)))))
```

`skip-whitespace` is a new (or reused — `json-skip-ws` already exists in the file) helper that advances past any `space|tab|newline|cr` bytes.

### Error messages

Target messages (rough shape, exact wording TBD in implementation):

- Empty input: `"json-parse: empty input"` — already handled.
- Unterminated string: `"json-parse: unterminated string"`.
- Unterminated array: `"json-parse: unterminated array"`.
- Unterminated object: `"json-parse: unterminated object"`.
- Invalid number: `"json-parse: invalid number"`.
- Expected value, got something else: `"json-parse: expected value at position N"`.
- Unexpected trailing content: `"json-parse: unexpected trailing content after value"`.

Each is produced by the helper closest to the failure. The top-level caller doesn't try to enrich messages — just forwards the `Err`.

## Components

### Helpers changed

| Function | Before | After |
|----------|--------|-------|
| `json-parse-value` | `src p len -> [v p']` | `src p len -> (Ok [v p']) \| (Err msg)` |
| `json-parse-string-loop` | `src p len -> [s p']` | `src p len -> (Ok [s p']) \| (Err msg)` |
| `json-parse-string` | `src p len -> [s p']` | `src p len -> (Ok [s p']) \| (Err msg)` |
| `json-parse-number` | `src p len -> [n p']` | `src p len -> (Ok [n p']) \| (Err msg)` |
| `json-parse-array-loop` | `src p len -> [arr p']` | `src p len -> (Ok [arr p']) \| (Err msg)` |
| `json-parse-object-loop` | `src p len -> m p' -> [m p']` | `src p len m -> (Ok [m p']) \| (Err msg)` |

### Entry point changed

`json-parse` adds trailing-garbage check after successful `json-parse-value`.

### Helpers unchanged

`json-skip-ws`, `hex-digit-to-int`, `unicode-escape-to-bytes`, `json-escape-loop`, `json-escape`, `json-stringify*`. These are leaf helpers that can't fail in interesting ways OR are only used by `json-stringify` (which doesn't have error semantics).

## Testing

### New fixture

`tests/aot/round3_builtin_json_parse_errors.airl`:

```clojure
;; EXPECT: empty:ok|garbage:ok|trailing:ok|unterminated-arr:ok|unterminated-obj:ok|valid:ok
(let (r1 : Result (json-parse ""))
     (r2 : Result (json-parse "not-json-at-all"))
     (r3 : Result (json-parse "42 x"))
     (r4 : Result (json-parse "[1,2,"))
     (r5 : Result (json-parse "{\"foo\":"))
     (r6 : Result (json-parse "42"))
  (let (v1 : String (match r1 (Ok v) "unexpected-ok" (Err e) "err"))
       (v2 : String (match r2 (Ok v) "unexpected-ok" (Err e) "err"))
       (v3 : String (match r3 (Ok v) "unexpected-ok" (Err e) "err"))
       (v4 : String (match r4 (Ok v) "unexpected-ok" (Err e) "err"))
       (v5 : String (match r5 (Ok v) "unexpected-ok" (Err e) "err"))
       (v6 : String (match r6 (Ok v) (if (= v 42) "ok" "bad-val") (Err e) (str "unexpected-err:" e)))
    (print (str
      "empty:" (if (= v1 "err") "ok" (str "bad:" v1))
      "|garbage:" (if (= v2 "err") "ok" (str "bad:" v2))
      "|trailing:" (if (= v3 "err") "ok" (str "bad:" v3))
      "|unterminated-arr:" (if (= v4 "err") "ok" (str "bad:" v4))
      "|unterminated-obj:" (if (= v5 "err") "ok" (str "bad:" v5))
      "|valid:" (if (= v6 "ok") "ok" (str "bad:" v6))))))
```

Exercises six cases: three different kinds of malformed input (garbage, trailing, unterminated) plus empty and a valid control.

### Existing fixture behavior

`tests/aot/round3_builtin_json_parse_result.airl` (added in `221b339`) — still passes. Its test cases all use valid JSON or empty input, which continue to produce the correct Ok/Err responses. No updates needed.

`tests/aot/round3_builtin_json_full.airl` — assuming the json-autoinclude fix is in the tree, this continues to pass. All its inputs are well-formed JSON.

### Manual smoke

```bash
# These previously returned (Ok <garbage>); now return (Err ...).
echo '(print (json-parse "not-json"))' | cargo run --release --features aot -- run -
echo '(print (json-parse "[1,2,"))'    | cargo run --release --features aot -- run -
```

Expected: both print an `(Err ...)` value.

## Files Modified

| File | Change |
|------|--------|
| `stdlib/json.airl` | Update six helper functions and `json-parse` to use Result-wrapped returns. ~30-50 lines of structural changes. |
| `tests/aot/round3_builtin_json_parse_errors.airl` | New fixture (six test cases). |
| `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md` | Update row 27 Notes: replace "this implementation always returns (Ok ...)" with "produces (Err ...) for empty, garbage, unterminated, and trailing-content inputs". |

## Risks

- **Regression risk for callers that silently accepted garbage.** If any code in the tree relies on `(json-parse "invalid")` returning `(Ok ...)` rather than `(Err ...)`, that code will now see `(Err ...)` and must be updated. Mitigation: grep the tree for `json-parse`; every call-site uses `match r (Ok v) ... (Err e) ...` already (per the audit's parity expectation). The fix makes previously unreachable `(Err e)` branches reachable, which is a feature not a regression.
- **Subtle helper-contract drift.** The six helpers changed shape; any callers not updated will see a raw `(Ok [v p])` pair where they expected `[v p]`. Mitigation: all callers are internal to `stdlib/json.airl`; no external code calls these helpers (they're not `:pub`). The change is entirely local.
- **Error message format churn.** If a consumer greps error messages for specific substrings, this spec's new messages could break those greps. Mitigation: keep the `"json-parse: ..."` prefix consistent with the prior `"json-parse: empty input"` message. Specific substrings beyond the prefix are a fragile contract — the audit document notes this risk in the row's Notes column.
- **Recursion-depth edge cases.** Deeply nested JSON (`[[[[[[...]]]]]]]`) exercises `json-parse-value` recursion. Adding Result unwrapping doesn't add stack frames beyond what the existing recursion already does, but if the current parser was tuned to the bare minimum, adding match expressions could push stack usage by a constant factor. Test with a known-deep fixture (100 nested arrays) to confirm no new stack issues.

## Invariants Preserved

- `json-stringify` is untouched.
- `json-parse`'s Ok-path behavior for valid JSON is unchanged — every test case in the existing parity fixture still passes.
- `json-parse-value`'s result shape on success — `[value, next-pos]` — is the same. Only the wrapper changes.
- No external function's contract changes; only internal helpers change (and external `json-parse`'s error path becomes actually-reachable).
- No new dependencies.
- No changes to `stdlib_embed_hash` pinning — the consolidation spec's anchor still holds because only `stdlib/json.airl` content changes, which is already hashed into the embed.

Wait — the embed hash DOES include `stdlib/json.airl`. Any source change to that file changes the hash. The implementer must update the `stdlib_embed_hash_is_stable` anchor value in the same commit.

## Out of Scope / Future Work

- **Error position tracking.** Adding line/column to errors would help debugging but isn't Rust-builtin parity.
- **Streaming / incremental parsing.** Not in scope.
- **JSON Schema validation.** Entirely separate concern.
- **JSON5 / relaxed syntax.** Separate concern.
