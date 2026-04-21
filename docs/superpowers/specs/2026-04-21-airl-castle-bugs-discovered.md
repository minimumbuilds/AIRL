# Bugs Discovered During AIRL_castle Memory Measurement

**Date:** 2026-04-21
**Status:** Discovered, not yet fixed (out of scope for current memory work)
**Context:** Surfaced while running `AIRL_castle/make test-binary` against the
post-Spec-3 g3 to measure peak RSS.

---

## Bug 1 — Z3 disproves `u32` contract in `stdlib/sha256.airl` (BLOCKER)

### Symptom

Running `make test-binary` in AIRL_castle with the current (spec-3-complete) g3:

```
[g3] err:  Z3 contract violation: function 'u32' has a disproven contract
  counterexample: result -> 1
```

The compile aborts at the first file (`stdlib/sha256.airl`); no later
file is reached. Prior to my spec 1 revert, this was silently bypassed
by `AIRL_NO_Z3_BODIES=1` (see commit `86b301c`); post-revert it fires
for real.

### The function and contract

```airl
(defn u32
  :sig [(x : i64) -> i64]
  :intent "Mask value to 32 bits"
  :requires [(valid x)]
  :ensures [(>= result 0) (<= result 4294967295)]
  :body (bitwise-and x 4294967295))
```

Mathematically the contract is trivially true: `bitwise-and` with
`0xFFFFFFFF` on any i64 produces a non-negative value ≤ 2³²−1. Z3 should
prove this.

### Root cause

`crates/airl-solver/src/translate.rs` contains **no `bitwise-and`
translation.** Grep for `bitwise`, `bv`, `bit_and` returns nothing —
bitwise operations are not modelled. So the body `(bitwise-and x 0xFFFFFFFF)`
translates to an unsupported operation: Z3 treats `result` as unconstrained.
With `result` free, the negation of the ensures conjunction is
trivially SAT, and Z3 reports "disproven."

(The reported counterexample `result -> 1` is incidental — Z3 picks
*some* model and `result = 1` happens to satisfy the ensures anyway;
the disproof is about the *ensures conjunction negated* being
satisfiable, which it is whenever `result` is free. The counterexample
extraction is probably showing a non-witness value. Separate UX issue.)

### Fix options

**A. Teach the Z3 translator about bitwise ops** (proper fix)
- Translate `bitwise-and/or/xor/shl/shr/not` via Z3's bit-vector theory
  (`BV64` or similar). Each op has a direct SMT-LIB equivalent.
- All of stdlib/sha256, stdlib/hmac, stdlib/pbkdf2, stdlib/base64
  become provable.
- Touches `crates/airl-solver/src/translate.rs` only.

**B. Return `Unknown` instead of `Disproven` for untranslatable bodies**
- When the translator encounters an operation it can't model, the
  resulting `result` is unconstrained — and a disproof under that
  condition is meaningless. Change the translator to flag "body contains
  unsupported op" → pipeline treats as `Unknown`/`TranslationError`
  (which falls back to runtime checking, not a hard error).
- Cheap; preserves semantic soundness (a missing translation is
  exactly the case where runtime check is the right answer).

**C. `:verify trusted` on stdlib modules that exercise bitwise ops**
- Declarative escape hatch: `(module sha256 :verify trusted ...)`.
- Documents the user's intent ("this module can't be Z3-verified
  today") instead of silently reducing to Unknown.

Recommended: **B first** as the immediate unblocker (it's what the
pipeline already does for lists/HashMaps that the translator doesn't
model), then **A** as follow-up for actual provability.

### Why it's not in scope right now

Fixing it properly is a Z3-translator enhancement. The memory
investigation explicitly shouldn't touch Z3 strictness — the user's
hard rule from the session start was *don't weaken Z3 verification
to hide real issues*. This u32 issue surfaces a **translator gap**,
not a genuinely violable contract — option B above resolves the
immediate block without weakening anything real.

---

## Bug 2 — `AIRL_castle/Makefile` still sets removed env var `AIRL_NO_Z3_BODIES=1`

### Symptom

`Makefile` line 93 (in `run-test` macro):

```makefile
cd $(AIRL_DIR) && AIRL_NO_Z3_BODIES=1 $(G3) -- $(ALL_SOURCES) ...
```

### Root cause

`AIRL_NO_Z3_BODIES` was the env var introduced by `86b301c` and
removed by the revert `7721980` in this session. The Makefile
references it but no AIRL code reads it anymore — it's a silent no-op.

### Impact

Cosmetic. The Makefile *behaves* as if the bypass were active (because
it historically was), but the bypass doesn't exist anymore, so Bug 1's
Z3 error now surfaces.

### Fix

Remove `AIRL_NO_Z3_BODIES=1` from the Makefile. Once Bug 1 is resolved
(translator fixed or stdlib marked `:verify trusted`), the Makefile is
clean regardless.

---

## Bug 3 — Parse error in `AIRL_castle/kafka/client.airl:88`

### Symptom

When running AIRL_castle under the *old* g3 (pre-Spec-1 revert, which
still honored `AIRL_NO_Z3_BODIES=1` so Bug 1 was bypassed), the compile
gets past stdlib and fails at:

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

`(let (handle-ro : _ handle) <body>)` *is* valid AIRL syntax (single
binding with unspecified type `_`). The parser rejecting it at column 20
of line 88 (the `(do` line) suggests the parser lost track of where
the bindings end and the body begins, possibly because of the `_` type
placeholder in a single-binding form.

### Impact

AIRL_castle doesn't compile on *any* g3 (old or new). Independent of
the memory work; pre-existing. Wasn't previously caught because the
build has been OOMing before reaching it.

### Fix

Either (a) the AIRL parser treats `(let (n : _ v) body)` consistently
with the multi-binding form, or (b) `kafka/client.airl:87` is rewritten
to avoid the ambiguity — e.g., give `handle-ro` a concrete type, or
use the multi-binding form `(let (handle-ro : _ handle) () body)`.

This wants a focused reproducer + parser test case. Not in scope for
memory work.

---

## Summary & recommendation

All three bugs pre-date this session's memory work. They surfaced only
because Specs 1–3 removed the scaffolding that was hiding them:

- **Bug 1** exposed by the spec-1 revert (no more `AIRL_NO_Z3_BODIES`
  bypass).
- **Bug 2** is the same — a now-stale Makefile directive.
- **Bug 3** was always reachable in theory but the build OOMed before
  getting there; Step 3 per-file emit now gets there (via old g3 with
  bypass) or bails earlier (via new g3 at Bug 1).

Suggested resolution order:
1. **Bug 1 fix B** (Z3 translator: untranslatable bodies → Unknown,
   not Disproven). Unblocks AIRL_castle compile without weakening real
   verification.
2. **Bug 3**: parser/client.airl fix. Required for AIRL_castle to
   actually build through.
3. **Bug 2**: cosmetic Makefile cleanup.
4. **Bug 1 fix A** (bit-vector translation) — improves stdlib coverage,
   separate effort.

Only after (1) and (3) land can we get the real AIRL_castle RSS
measurement that the memory-investigation cycle is aiming for.
