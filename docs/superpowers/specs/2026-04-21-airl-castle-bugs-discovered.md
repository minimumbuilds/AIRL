# AIRL Bugs Surfaced During AIRL_castle Memory Measurement

**Date:** 2026-04-21
**Status:** Discovered, not yet fixed (out of scope for current memory work)
**Context:** Surfaced while running `AIRL_castle/make test-binary` against
the post-Spec-3 g3 to measure peak RSS.

**Scope of this doc:** bugs in the **AIRL repo** (this repo). Bugs in
the AIRL_castle repo are documented separately at
`AIRL_castle/docs/superpowers/specs/2026-04-21-memory-measurement-blockers.md`.

---

## Bug A — Z3 disproves `stdlib/sha256.airl :: u32` (BLOCKER)

### Symptom

With the current (spec-3-complete) g3, `make test-binary` in AIRL_castle
aborts at the first file with:

```
[g3] err:  Z3 contract violation: function 'u32' has a disproven contract
  counterexample: result -> 1
```

Prior to this session's spec-1 revert (`7721980`), this was silenced
by `AIRL_NO_Z3_BODIES=1`. Post-revert the error surfaces for real.

### The function

`stdlib/sha256.airl`:

```airl
(defn u32
  :sig [(x : i64) -> i64]
  :intent "Mask value to 32 bits"
  :requires [(valid x)]
  :ensures [(>= result 0) (<= result 4294967295)]
  :body (bitwise-and x 4294967295))
```

The contract is trivially true mathematically: `bitwise-and` with
`0xFFFFFFFF` produces a non-negative value ≤ 2³²−1 on any i64 input.

### Root cause

`crates/airl-solver/src/translate.rs` has **no bitwise-op translation.**
Grep for `bitwise`, `bv`, `bit_and` inside the solver crate returns
nothing. So `(bitwise-and x 4294967295)` translates to an *unsupported
operation*: Z3 treats `result` as unconstrained. With `result` free,
the negated ensures conjunction is trivially SAT, and Z3 reports
"disproven."

The reported counterexample `result -> 1` is misleading UX — it's an
incidental model value that happens to satisfy the ensures; the actual
disproof is about the *negated* ensures being satisfiable when `result`
is unconstrained.

### Fix options

**A1. Treat untranslatable bodies as `Unknown` (immediate unblocker)**

When the translator encounters an operation it can't model, the body
doesn't constrain `result`, so a disproof under that condition is
meaningless. Change the translator to flag "body contains unsupported
op" → the pipeline treats the verification as `Unknown` /
`TranslationError` (which falls back to runtime checking, not a hard
error). Same path already used when the translator encounters
lists, maps, or other unmodelled types.

Cheap, sound, and exactly the behaviour the pipeline already has for
every *other* untranslatable construct.

**A2. Add bit-vector translation (proper fix)**

Translate `bitwise-and/or/xor/shl/shr/not` using Z3's built-in
`BV64` theory. Each AIRL bitwise op has a direct SMT-LIB equivalent
(`bvand`, `bvor`, `bvxor`, `bvshl`, `bvlshr`, `bvnot`). Under that
translation the `u32` contract becomes trivially provable.

Follow-up to A1; improves stdlib/sha256 + stdlib/hmac + stdlib/pbkdf2
+ stdlib/base64 coverage.

**A3. Counterexample UX polish**

Separately from A1 and A2, when Z3 reports "disproven," the currently-
printed counterexample sometimes shows values that satisfy the ensures
(as above). That's because we print arbitrary model values without
filtering against the ensures. Clean up the extraction so the shown
model is a *witness* to the disproof, not a random feasible point.

### Recommendation

Land A1 immediately — unblocks AIRL_castle compile without weakening
any real verification. A2 as follow-up work (improves actual coverage).
A3 can be bundled with A2 or done independently.

### Prior art (2026-04-22 update — A1 superseded)

This spec's A1 turned out to be a *regression rediscovery*, not a new
fix. Git archaeology after the A1 landing surfaced `a03e980`
(2026-04-16, "fix(solver): add Z3 bitwise axioms — unblock sha256/crypto
contract verification") which had already solved this problem — using
a stronger approach than A1 — a week earlier. That axiom machinery was
then silently deleted on 2026-04-20 by `89f5e0f` ("feat(z3): add
inductive verification to prover.rs and translate.rs") as unrelated
collateral damage in a large refactor. The commit message did not
mention bitwise or axioms, and the 59-line regression went unnoticed
until `AIRL_castle` hit the `u32` contract error that triggered this
spec on 2026-04-21.

**A1 is superseded** by restoring `a03e980`'s bounded-fresh-variable
axiom approach. Tracked under
`artifacts/spec-airl-bitwise-axioms-restore.md` (priority 10):

- `bitwise-and x mask` ⇒ fresh Int with axiom `0 ≤ fresh ≤ mask` (when
  `mask` is a non-negative constant), else `fresh ≥ 0`
- `bitwise-or/xor/shr` ⇒ fresh Int with axiom `fresh ≥ 0`
- `bitwise-shl`, `bitwise-not` ⇒ no axiom (overflow/two's-complement)
- Prover drains axioms after body-binding, before the ensures check.

**A2 (BV64) has since landed** and supersedes the restored axioms.
Tracked under `artifacts/spec-airl-z3-bv64-bitwise.md`. BV64 produces
bit-precise results: `(bitwise-not 0)` proves to exactly `-1`,
`(bitwise-xor x x)` proves to exactly `0`, `(bitwise-shl x 0) = x`.
The axiom infrastructure (`pending_axioms`, `drain_axioms`,
`make_bitwise_fresh`) was removed in favor of direct Int↔BV64 conversion
via `z3::ast::BV::from_int` / `ast::Int::from_bv`. The bitwise tests
were upgraded from range-bound assertions to exact-value assertions
where applicable; four new BV-exclusive tests cover cases the axiom
approach couldn't prove at all (`bitwise-not`, self-xor, shl identity,
small-mask exact range).

**Lessons for future reviews.** The A1-era review missed the
regression because the 2026-04-21 investigation started from "bitwise
ops don't work" rather than "bitwise ops used to work — what changed?"
When a supposedly-new bug affects a well-understood area, check
`git log -- <path> -S<symbol>` before writing a new spec. Had we done
that on 2026-04-21, the spec would have opened with "restore
`a03e980`" and never shipped the weaker A1.

---

## Bug B — AIRL parser rejects `(let (name : _ symbol-expr) (do ...))`

### Symptom

When running AIRL_castle under the **old** g3 (pre-Spec-1-revert, so
Bug A is bypassed), the compile gets past stdlib and fails at
`AIRL_castle/kafka/client.airl:88:20`:

```
Compile error: AIRL_castle/kafka/client.airl:88:20:
  let binding requires (name : Type value)
```

### Context

Line 87–88 of `kafka/client.airl`:

```airl
(Ok handle) (let (handle-ro : _ handle)
    (do
      (tcp-set-timeout handle-ro 30000)
      ...
```

The form is `(let (handle-ro : _ handle) (do ...))` — a single binding
with `_` (inferred) type, where the *value* is a bare symbol reference
(`handle`) rather than a call expression, and the body is a `(do ...)`.

### Root cause (hypothesis)

The AIRL parser's multi-binding let accepts any number of
`(name : Type value)` binding tuples before the body. With a single
binding where the value is a bare symbol (not a parenthesized call),
the parser seems to look greedily for more bindings and
misidentifies `(do ...)` as another binding attempt — which fails
the `(name : Type value)` shape check, producing the observed error.

Stdlib uses `(let (k : _ (at ks i)) ...)` successfully — the value
there is a parenthesized call, which may be what tips the parser
out of binding-greedy mode.

Needs a focused reproducer + parser test case in
`crates/airl-syntax/` to confirm.

### Fix

Either:

**B1.** Make the parser's let-binding disambiguation deterministic and
  documented — e.g., a single-binding let is fully resolved when the
  first binding tuple is followed by a non-`(name :` form.
**B2.** Add a test fixture `tests/fixtures/valid/let_single_binding_symbol.airl`
  exercising the failing shape, drive the parser fix from there.

### Why it's not in scope right now

AIRL_castle has a pre-existing compile-time bug of its own (see
AIRL_castle-side doc) that also blocks the build; fixing this parser
edge case is necessary but not sufficient for AIRL_castle to compile
through. The memory-measurement work needs both repos to land their
fixes.

---

## Summary

| Bug | Repo | Severity | Fix complexity |
|---|---|---|---|
| A — Z3 disproves u32 (translator gap) | **AIRL** | Blocker for castle compile | Small (A1) or Medium (A2) |
| B — Parser rejects single-binding let with symbol value | **AIRL** | Blocks castle compile past stdlib | Small once reproducer is in place |

Companion bugs filed against AIRL_castle (Makefile env-var staleness +
kafka/client.airl contract workaround) are documented in
`AIRL_castle/docs/superpowers/specs/2026-04-21-memory-measurement-blockers.md`.

Only after both AIRL Bugs A + B are resolved (and the AIRL_castle-side
workarounds are applied) can the real AIRL_castle peak-RSS measurement
complete end-to-end.
