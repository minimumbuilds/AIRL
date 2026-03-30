# Spec: Trap Cleanup & Short-Circuit `and`/`or`

**Date:** 2026-03-28
**Version:** 0.7.1
**Status:** Approved
**Scope:** Language semantics (1 breaking change), documentation cleanup, minor stdlib additions

---

## Motivation

AIRL-Header.md documents 19 "traps" — gotchas that LLMs and humans hit when writing AIRL. An audit against the current codebase (v0.7.0) found:

- **3 traps are factually wrong** (fixed bugs still listed, implemented features listed as missing)
- **7 traps are not traps at all** (normal syntax rules inflating perceived complexity)
- **1 trap is a genuine language semantics issue** (eager `and`/`or`)
- **1 trap is an unnecessary asymmetry** (list-only `empty?`)
- **7 traps are intentional design choices** that just need clearer framing

The "19 traps" count overstates complexity. Most items are either stale, normal syntax rules, or intentional design decisions that belong in reference documentation rather than a warning section. This spec prunes the list to accurately reflect the current language.

**Note on verification architecture:** Z3 contract verification (informational/advisory), opt-in linearity checking, and runtime-typed builtins are **intentional Phase 1 design decisions** (see `docs/superpowers/specs/2026-03-28-verification-gaps-assessment.md`). Runtime contract enforcement (`AssertRequires`/`AssertEnsures`/`AssertInvariant` opcodes) always executes and is the primary safety mechanism. These are not gaps — they are stated scope boundaries with runtime enforcement as the backstop.

---

## Changes

### Change 1: Short-Circuit `and`/`or` (CRITICAL)

**Problem:** `and`/`or` evaluate both arguments eagerly. This means guard patterns crash:

```clojure
;; CRASHES on empty list — (head xs) evaluated even when (empty? xs) is true
(and (not (empty? xs)) (> (head xs) 0))
```

Every Lisp since Scheme (1975) makes `and`/`or` short-circuit. Eager evaluation is the single most dangerous trap in AIRL — it produces silent correctness bugs in code that looks correct to any Lisp-literate LLM or human.

**Fix:** Compile `and`/`or` as special forms with conditional branching, not as eager function calls.

**Semantics after change:**
- `(and a b)` — evaluate `a`; if falsy, return `false` without evaluating `b`
- `(or a b)` — evaluate `a`; if truthy, return `true` without evaluating `b`
- Both remain binary (no variadic change needed)
- Return type remains `Bool`

**This is not a breaking change for correct programs.** Short-circuit is strictly more permissive than eager — any program that works with eager evaluation also works with short-circuit. Programs that relied on side effects in both branches of `and`/`or` were already buggy (AIRL is functional; side effects in boolean expressions are a code smell).

**Files to modify:**

1. **`crates/airl-runtime/src/bytecode_compiler.rs`** — Detect `and`/`or` calls during compilation. Instead of emitting a normal `Call` opcode, emit conditional branch sequences:
   - `and`: evaluate LHS → if false, jump to result=false; else evaluate RHS → result=RHS
   - `or`: evaluate LHS → if true, jump to result=true; else evaluate RHS → result=RHS
   - Pattern is identical to how `if` is compiled, just with boolean coercion

2. **`crates/airl-runtime/src/bytecode_aot.rs`** — Same conditional branch pattern for AOT compilation. The AOT compiler already compiles `if` to Cranelift `brif` instructions — reuse that pattern.

3. **`crates/airl-rt/src/logic.rs`** — Keep `airl_and`/`airl_or` as-is for backward compat, but they will no longer be called by compiled code (only by any remaining interpreter paths).

4. **`bootstrap/bc_compiler.airl`** — The bootstrap bytecode compiler must also emit short-circuit branches for `and`/`or`. Check how it currently compiles these — likely as regular function calls that need the same special-case treatment.

**Verification:**
- Add fixture `tests/fixtures/valid/short_circuit.airl`:
  ```clojure
  ;; Test: and short-circuits on false (head on empty list must NOT be evaluated)
  (defn test-and-short-circuit
    :sig [-> Bool]
    :requires [(valid true)]
    :ensures [(= result true)]
    :body (let (xs : List [])
            (if (and (not (empty? xs)) (> (head xs) 0))
              false
              true)))

  ;; Test: or short-circuits on true (head on empty list must NOT be evaluated)
  (defn test-or-short-circuit
    :sig [-> Bool]
    :requires [(valid true)]
    :ensures [(= result true)]
    :body (let (xs : List [])
            (if (or (empty? xs) (> (head xs) 0))
              true
              false)))

  (println (test-and-short-circuit))
  (println (test-or-short-circuit))
  ;; EXPECT: true
  ;; EXPECT: true
  ```
- Run full test suite (`cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver`) — no regressions expected
- Run AOT test suite (`bash tests/aot/run_aot_tests.sh`) — no regressions expected
- Run bootstrap compiler tests to verify G3 also handles short-circuit

---

### Change 2: Polymorphic `empty?` (MEDIUM)

**Problem:** `empty?` only works on lists. Calling it on a string is a runtime error. Users must remember to use `(= s "")` for strings and `(= (map-size m) 0)` for maps. This is an unnecessary asymmetry.

**Fix:** Extend `airl_empty` in `crates/airl-rt/src/list.rs` to handle multiple types:

```rust
// Current (list.rs:58-64):
match &(*val).data {
    RtData::List(xs) => rt_bool(xs.is_empty()),
    _ => rt_error("airl_empty: not a List"),
}

// After:
match &(*val).data {
    RtData::List(xs) => rt_bool(xs.is_empty()),
    RtData::IntList(xs) => rt_bool(xs.is_empty()),
    RtData::Str(s) => rt_bool(s.is_empty()),
    RtData::Map(m) => rt_bool(m.is_empty()),
    _ => rt_error("airl_empty: expected List, String, or Map"),
}
```

**Files to modify:**
1. **`crates/airl-rt/src/list.rs`** — Extend `airl_empty` match arms as shown above

**Verification:**
- Add fixture `tests/fixtures/valid/empty_polymorphic.airl`:
  ```clojure
  (println (empty? []))        ;; true
  (println (empty? [1]))       ;; false
  (println (empty? ""))        ;; true
  (println (empty? "hi"))      ;; false
  (println (empty? (map-new))) ;; true
  ;; EXPECT: true
  ;; EXPECT: false
  ;; EXPECT: true
  ;; EXPECT: false
  ;; EXPECT: true
  ```

---

### Change 3: Delete Stale Traps from AIRL-Header.md

Remove 3 traps that are factually wrong:

**Trap #7 (defn requires :sig + :body + contract) — INACCURATE:**
- `:sig` is optional (parser defaults params/return type)
- `:body` is optional (defaults to NilLit, parser.rs line 657)
- Only contracts (`:requires` or `:ensures`) are actually mandatory (parser.rs lines 642-646)
- **Action:** Rewrite to: "Every `defn` needs at least one of `:requires`/`:ensures`. `:sig` and `:body` are optional but recommended."

**Trap #15 (length = byte count) — FIXED:**
- `builtin_length` for strings was changed to `s.chars().count()` (character count) in v0.5.0
- The separate `char-count` builtin still exists and is redundant
- **Action:** Remove trap. Optionally deprecate `char-count` since `length` now does the same thing.

**Trap #19 (try doesn't exist) — WRONG:**
- `try` is implemented (`parse_try_expr`, parser.rs line 51, lines 267-275)
- `import` exists as a top-level form (v0.7.0 module system)
- **Action:** Remove `try` and `import` from the "don't exist" list. Keep the rest (`nil?`, `null?`, `list`, `catch`, `throw`, `typeof`, `instanceof`, `require`, `begin`, `progn`).

**Files to modify:**
1. **`AIRL-Header.md`** — Edit traps #7, #15, #19 as described above

---

### Change 4: Reclassify Non-Traps

Move 7 items from the TRAPS section to the SYNTAX section (where they belong as normal reference documentation):

| # | Current Trap | Why It's Not a Trap | Destination |
|---|-------------|-------------------|-------------|
| 5 | `let` requires type + body | Normal syntax rule | SYNTAX → Forms |
| 6 | Multi-binding let | Feature documentation | SYNTAX → Forms |
| 8 | `result` only in ensures | Standard DbC semantics | SYNTAX → Contracts |
| 10 | Uppercase variant constructors | Convention (same as Haskell/Rust) | SYNTAX → Patterns |
| 12 | Lambda params have no types | Parser rule | SYNTAX → Forms |
| 17 | Tensor.rand requires seed | API documentation | SIGS → Tensor |
| 18 | Use `char-code` not `ord` | Naming convention | SIGS → Type conversion |

**Files to modify:**
1. **`AIRL-Header.md`** — Move these 7 items into their appropriate SYNTAX/SIGS subsections. Add them as inline notes where the relevant syntax is already documented.

---

### Change 5: Rename and Restructure Remaining Traps

After changes 1-4, 5 genuine items remain. Rename the section from "TRAPS" to "KEY DIFFERENCES" and rewrite for clarity:

```markdown
## KEY DIFFERENCES

1. NO loops/mutation: no `while`/`for`/`set!`/`var`/`return`. USE `fold`/`map`/`filter`/recursion.
2. `if` has EXACTLY 3 forms: `(if cond then else)`. Both branches required. Multi-expr branch: wrap in `do`.
3. No mixed int/float: `(+ 1 1.0)` errors. Use `(+ 1.0 1.0)` or `int-to-float`.
4. Map keys are STRING ONLY. Use `int-to-string` for numeric keys. Same for Set elements.
5. Keywords are strings: `:foo` evaluates to `":foo"`.
```

These are real design choices that differ from mainstream languages. They deserve documentation. They are not "traps."

**Files to modify:**
1. **`AIRL-Header.md`** — Replace TRAPS section with KEY DIFFERENCES containing only these 5 items

---

## Execution Order

Changes are independent except that Change 5 depends on Changes 3 and 4.

| Order | Change | Risk | Effort |
|-------|--------|------|--------|
| 1 | **Short-circuit `and`/`or`** | Medium (touches bytecode compiler, AOT, and bootstrap) | ~2 hours |
| 2 | **Polymorphic `empty?`** | Low (single function, additive) | ~15 min |
| 3 | **Delete stale traps** | None (docs only) | ~10 min |
| 4 | **Reclassify non-traps** | None (docs only) | ~15 min |
| 5 | **Rename section** | None (docs only) | ~5 min |

---

## Verification Plan

1. `cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver` — full Rust test suite (572 tests)
2. `bash tests/aot/run_aot_tests.sh` — full AOT test suite (67 tests)
3. New fixtures: `short_circuit.airl`, `empty_polymorphic.airl`
4. Bootstrap compiler tests: `cargo run --release --features aot -- run bootstrap/lexer_test.airl` (and other bootstrap tests) — verify G3 compatibility
5. Manual smoke test: write an AIRL program using `(and (not (empty? xs)) (> (head xs) 0))` pattern, confirm it works without crashing on empty list

---

## Documentation Updates

After implementation, update:
1. **`AIRL-Header.md`** — All changes above
2. **`CLAUDE.md`** — Add to Completed Tasks: "Short-circuit `and`/`or`, polymorphic `empty?`, trap documentation cleanup"
3. **`AIRL-LLM-Guide.md`** — If it references eager `and`/`or` behavior, update to reflect short-circuit semantics
4. **`stdlib/*.md`** — No changes needed (stdlib functions unaffected)

---

## What This Does NOT Change

- **No mixed int/float auto-coercion.** This was considered but rejected — strict numeric typing prevents precision-loss bugs and is consistent with Rust. The verbosity cost is real but acceptable.
- **No variadic `and`/`or`.** Binary-only is sufficient. Variadic can be added later if needed via nested expansion.
- **No `match` syntax change.** Flat pairs are unusual but consistent with S-expression philosophy.
- **No generic map keys.** String-only keys are a real limitation but fixing it requires changes to the map representation (`BTreeMap<String, ...>` → `BTreeMap<Value, ...>`), hash implementation, and equality semantics. Out of scope for this spec.
