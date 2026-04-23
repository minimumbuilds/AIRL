# json-parse Error Propagation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `json-parse` return `(Err msg)` for malformed non-empty input. Currently the pure-AIRL parser silently produces `(Ok <garbage>)` for invalid JSON, defeating the Result contract.

**Architecture:** Thread Result-wrapped returns through six internal helpers in `stdlib/json.airl`: `json-parse-value`, `json-parse-string`, `json-parse-string-loop`, `json-parse-number`, `json-parse-array-loop`, `json-parse-object-loop`. Add trailing-garbage check to public `json-parse`. Add fixture.

**Spec:** `docs/superpowers/specs/2026-04-23-json-parse-error-propagation-design.md`

---

## Task 1: Add failing fixture

**Files:**
- Create: `tests/aot/round3_builtin_json_parse_errors.airl`

- [ ] **Step 1: Write the fixture**

```clojure
;; EXPECT: empty:ok|garbage:ok|trailing:ok|unterminated-arr:ok|unterminated-obj:ok|valid:ok
;; DEPS: stdlib/json.airl
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

- [ ] **Step 2: Run it and confirm it FAILS**

```
rm -rf tests/aot/cache/round3_builtin_json_parse_errors
bash tests/aot/run_aot_tests.sh 2>&1 | grep "round3_builtin_json_parse_errors"
```

Expected: `FAIL: round3_builtin_json_parse_errors` — the garbage/trailing/unterminated cases currently return `(Ok ...)` not `(Err ...)`, so the output will have `bad:unexpected-ok` for several of them. The `empty:ok` and `valid:ok` parts will pass because those paths already work.

If the fixture PASSES before Task 2 is applied, you've misread the audit's claim. Double-check: inspect `r2`, `r3`, `r4`, `r5` output in a REPL. If they return `(Err ...)` today, something already propagates errors and this plan is obsolete — report BLOCKED.

- [ ] **Step 3: DO NOT commit yet.** Fixture + implementation land together.

---

## Task 2: Refactor helpers to return Result

**Files:**
- Modify: `stdlib/json.airl`

The helpers below must change return shape from raw `[value, new-pos]` to `(Ok [value, new-pos])` / `(Err "message")`. Each caller must match on the result and either extract the pair on Ok or propagate the Err.

- [ ] **Step 1: Catalog call sites before editing**

```
grep -n "json-parse-value\|json-parse-string\|json-parse-number\|json-parse-array-loop\|json-parse-object-loop" stdlib/json.airl
```

Record: which functions call each helper, and how the returned pair is currently destructured (e.g. `(let (pair (json-parse-value src p len))) ...`). You'll need to update each call site.

- [ ] **Step 2: Refactor leaf helpers first**

Start with the leaves that don't call other parser helpers:

**`json-parse-number`**: Returns `[value, new-pos]` on valid input. Currently has no error path — a string like `"abc"` starting with a non-digit returns whatever it finds. After refactor: return `(Ok [value, new-pos])` when parsing succeeds (digit consumed at least once), `(Err "json-parse: invalid number")` otherwise.

**`json-parse-string`** (and internally `json-parse-string-loop`): Currently parses a quoted string until the closing quote, or until end-of-input (in which case it returns whatever it had). After refactor: return `(Ok [string, new-pos])` if closing quote found, `(Err "json-parse: unterminated string")` otherwise.

For each, wrap the successful return in `(Ok ...)` and add the error return at the "end of input reached without finding what we needed" branches.

- [ ] **Step 3: Refactor `json-parse-value`**

`json-parse-value` dispatches on the first character to one of: parse-string, parse-array-loop, parse-object-loop, boolean, null, number. It must now:

```clojure
(match (json-parse-string src (+ p 1) len)
  (Ok pair) ...extract v, p'...
  (Err e)   (Err e))
```

For each arm. The result of `json-parse-value` itself is `(Ok [v p'])` or `(Err msg)`.

For the `true` / `false` / `null` literal arms, also check that the bytes fully match the keyword (e.g. `t` must be followed by `rue`) — bad `tab`-like prefixes currently parse as `true` because `json-parse-value` reads only the first byte. Add explicit length checks:

```clojure
;; Old:
(if (= b 116) ;; t -> true
  [true (+ p 4)]
  ...)
;; New:
(if (= b 116) ;; t
  (if (bytes-match src (+ p 1) (bytes-from-string "rue"))
    (Ok [true (+ p 4)])
    (Err "json-parse: expected 'true'"))
  ...)
```

If `bytes-match` doesn't exist, add it as a local helper (not pub) at the top of the file. It compares three bytes at an offset against a three-byte expected pattern.

- [ ] **Step 4: Refactor array/object loops**

`json-parse-array-loop` reads values separated by commas until `]`. If it reaches `len` without finding `]`, that's an unterminated array. Similarly for `json-parse-object-loop` and `}`.

Each recursive call to `json-parse-value` must match on its Result. Each recursive call to the loop itself also propagates Errors.

- [ ] **Step 5: Check compiles**

```
cargo build --features aot
```

Expected: clean. Type errors indicate a callsite missed a `match` wrap.

---

## Task 3: Update `json-parse` entry point

**Files:**
- Modify: `stdlib/json.airl`

- [ ] **Step 1: Thread `json-parse-value` result through `json-parse`**

Current body shape:

```clojure
(if (= len 0)
  (Err "json-parse: empty input")
  (Ok (at (json-parse-value src 0 len) 0)))
```

New body:

```clojure
(if (= len 0)
  (Err "json-parse: empty input")
  (match (json-parse-value src 0 len)
    (Ok pair)
      (let (v (at pair 0))
           (end-pos : i64 (at pair 1))
           (trailing-pos : i64 (json-skip-ws src end-pos len)))
        (if (= trailing-pos len)
          (Ok v)
          (Err "json-parse: unexpected trailing content after value")))
    (Err e) (Err e)))
```

`json-skip-ws` already exists (see line 108 of the current file) and returns the position after whitespace. Reuse it.

- [ ] **Step 2: Bump embed hash anchor**

Source of `stdlib/json.airl` has changed, so `stdlib_embed_hash()` value changes. The `stdlib_embed_hash_is_stable` test (if present — it's from the consolidation refactor, which this branch does NOT include) would fail. Check if this worktree's `crates/airl-driver/src/pipeline.rs` has the test:

```
grep "stdlib_embed_hash_is_stable" crates/airl-driver/src/pipeline.rs
```

If present: capture the new hash via the print-test technique and update the anchor. If absent: no action needed on this branch; merging with consolidation later will surface the hash update.

---

## Task 4: Regression + commit

- [ ] **Step 1: Run the failing fixture — it should now PASS**

```
rm -rf tests/aot/cache
bash tests/aot/run_aot_tests.sh 2>&1 | grep -E "round3_builtin_json_parse_errors|round3_builtin_json_parse_result"
```

Expected: both pass.

- [ ] **Step 2: Full AOT suite**

```
bash tests/aot/run_aot_tests.sh 2>&1 | tail -5
```

Expected: all tests pass except possibly `round3_builtin_json_full` which has a known pre-existing COMPILE_FAIL on this branch (audit branch predates the json-autoinclude fix). That failure is unchanged — document it as pre-existing, not a regression from this PR.

- [ ] **Step 3: Full Rust suite**

```
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
```

Expected: all pass.

- [ ] **Step 4: Update audit document**

Open `docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md`. Find row 27 (`json-parse`). Update Notes column — replace the paragraph that says "pure-AIRL parser cannot detect malformed non-empty JSON at parse-value level, so invalid non-empty input still returns (Ok ...) with a best-effort value. This is a known limitation noted in Follow-up." with:

```
Full error propagation implemented 2026-04-23: malformed non-empty input returns (Err "json-parse: ...") for unterminated strings/arrays/objects, invalid numbers, expected-value-got-something-else, and unexpected trailing content. Now matches the Rust builtin's error semantics. Exhaustive cases tested in round3_builtin_json_parse_errors.airl.
```

Also update the Follow-up section's item 2 (json-parse error detection incomplete) — remove it or mark it as resolved.

- [ ] **Step 5: Commit**

```bash
git add stdlib/json.airl \
        tests/aot/round3_builtin_json_parse_errors.airl \
        docs/superpowers/audits/2026-04-23-builtin-deregistration-parity.md \
        crates/airl-driver/src/pipeline.rs  # only if stability-test anchor was updated
git commit -m "$(cat <<'EOF'
fix(stdlib): json-parse returns Err for malformed non-empty input

The pure-AIRL recursive-descent parser in stdlib/json.airl silently
produced (Ok <garbage>) for invalid non-empty JSON, making the Result
contract unreliable. The audit's Follow-up #2 flagged this.

Fix: Six internal helpers (json-parse-value, json-parse-string,
json-parse-string-loop, json-parse-number, json-parse-array-loop,
json-parse-object-loop) now return (Ok [val pos]) / (Err msg).
Errors propagate up through each recursive call. The public
json-parse entry point also checks for unexpected trailing content
after the parsed value.

New fixture round3_builtin_json_parse_errors.airl exercises six
cases: empty, garbage prefix, trailing garbage, unterminated array,
unterminated object, and a valid control — all pass.

Audit row 27 updated; Follow-up #2 marked resolved.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Do NOT push. Do NOT merge.

---

## Self-Review

**Spec coverage:**
- Six helpers refactored (Task 2) ✓
- json-parse trailing-garbage check (Task 3) ✓
- New fixture (Task 1) ✓
- Audit document updated (Task 4 Step 4) ✓

**Placeholder scan:** no TBDs. One implementation-deferred decision: does `bytes-match` exist as a builtin? If not, the implementer adds it as a local helper in Task 2 Step 3.

**Recursion correctness:** The refactor preserves the original recursion structure. Only the return shape changes. Stack depth is identical to pre-refactor.

**Scope:** No new public APIs, no changes to json-stringify, no Rust-side changes.
