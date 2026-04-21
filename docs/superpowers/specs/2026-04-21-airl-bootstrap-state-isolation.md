# AIRL Bootstrap State Isolation Between Compiled Files

**Date:** 2026-04-21
**Status:** Proposed
**Priority:** High — prerequisite for reasonable memory use in bootstrap and ecosystem builds
**Scope:** `bootstrap/bc_compiler.airl`, `bootstrap/linearity.airl`, `bootstrap/z3_bridge_g3.airl`, `bootstrap/z3_cache.airl`

## Problem

Step 3's per-file emit rewrite lands each file's bytecode to a `.o` file and drops the BCFuncs locally. The fold accumulator now carries only obj-path strings. Yet peak RSS did not drop — diagnostic shows 1.6 M alive `list` RtValues at the 6 th file's OOM, unchanged from pre-Step-3.

This implies the retention is NOT in the fold accumulator but in **persistent state inside the AIRL bootstrap modules themselves**. Suspected sources, in decreasing likelihood:

1. **`bc_compiler.airl` compile-state map (`cs-*` helpers).** Every call to `bc-compile-program-with-prefix-and-proofs` threads a `compile-state` map through every step. If this map is keyed into a global cache or stashed in a module-level variable, each file's compile-state accumulates.
2. **`linearity.airl` ownership-graph state.** `linearity-check-module` builds per-function ownership maps. If it caches results in a global, or if the walker holds references to every parent node it ever visited, the retained graph equals sum-of-all-files.
3. **`z3_bridge_g3.airl` type-inference cache.** `z3-infer-type` may memoize against a module-level symbol table.
4. **`z3_cache.airl` proof-map accumulator.** The proof-map passed to `bc-compile-program-with-prefix-and-proofs` may be appended-to across files without dropping stale entries.

Without isolation, even if per-file .o emission drops bytecode, each file's **compile-time state** leaks into the next.

## Investigation plan

This spec is INVESTIGATION-FIRST. The fixes depend on what the investigation reveals.

### Step A: per-phase RSS snapshot inside the temp binary

Add AIRL-level builtins `rt-stats` (prints `[rt-trace]`-style stats) callable from the bootstrap code. Insert calls at each phase boundary within `g3-compile-source-with-z3-strict`:

```airl
(match (lex source)
  (Ok tokens)
    (do (rt-stats "after-lex")
      (match (parse-sexpr-all tokens)
        (Ok sexprs)
          (do (rt-stats "after-parse-sexprs")
            (match (parse-program sexprs)
              (Ok ast-nodes)
                (do (rt-stats "after-parse-program")
                  (match (g3-run-linearity ast-nodes label lin-mode)
                    (Ok checked-nodes)
                      (do (rt-stats "after-linearity")
                        (z3c-verify-ast-cached checked-nodes cache)
                        (rt-stats "after-z3")
                        (bc-compile-program-with-prefix-and-proofs ...)
                        (rt-stats "after-bc-compile"))))))))))
```

Run the G3 bootstrap build under this instrumentation. Which phase boundary shows the largest alive-count jump? That localizes the leak to one of the phases.

### Step B: between-file RSS snapshots

At the top of `g3-step3-fold-step`, call `rt-stats` before and after each file. Compare deltas. Fixed per-file increment (same regardless of file content) suggests a simple per-call leak. Proportional-to-file-size increment suggests state retention of parsed AST / bytecode.

### Step C: per-module alive breakdown

Extend `rt-stats` to dump the top-N allocation sites (leveraging the alloc-site-tagging spec). Identify whether leaked lists come from AIRL bootstrap code (bc_compiler, linearity) or Rust builtins (list/map).

## Probable fixes (pending Step A-C results)

### Fix 1: Drop compile-state after each file

If `bc_compiler.airl` retains compile-state in some module-level structure, ensure it's dropped at the end of `bc-compile-program-with-prefix-and-proofs`. Probably a matter of ensuring the function returns only the BCFunc list (not the surrounding state) and that the caller doesn't hold the state value.

### Fix 2: Clear linearity global cache between files

If `linearity.airl` caches per-function analysis globally, expose a `linearity-reset` builtin and call it between files in `g3-step3-fold-step`.

### Fix 3: Scope Z3 proof-map to one file

If `z3_cache.airl`'s proof-map passes through `bc-compile-program-with-prefix-and-proofs` and gets embedded in BCFuncs (as proof annotations), the accumulation is legitimate — per-file proofs need to persist to disk. BUT the in-memory map can be cleared after serialization. Audit `z3c-save` to confirm.

### Fix 4: Break ownership chains

If an AIRL closure captures `compile-state` or an AST node, that reference persists for the closure's lifetime. Search `bootstrap/*.airl` for closures inside loops; confirm captures are minimal.

## Files to investigate

| File | What to look for |
|---|---|
| `bootstrap/bc_compiler.airl` | Module-level state; closures capturing compile-state; the `cs-*` helpers — do they thread state correctly or leak intermediates? |
| `bootstrap/linearity.airl` | Global caches; per-function ownership graphs that might persist |
| `bootstrap/z3_bridge_g3.airl` | Type-inference cache; symbol table persistence across calls |
| `bootstrap/z3_cache.airl` | Proof-map growth; in-memory cache trim |
| `bootstrap/parser.airl` | AST node retention (should be released after bc-compile consumes them) |

## Phased execution

1. **Phase 1 — add `rt-stats` builtin.** Simple: call into the existing diag.rs counters. Gate on `AIRL_RT_TRACE=1`.
2. **Phase 2 — instrument the G3 compile path.** Add `rt-stats` calls at phase and file boundaries in `g3_compiler.airl`. Not a permanent change — scratch for the investigation.
3. **Phase 3 — run + analyze.** Identify which phase is the leak.
4. **Phase 4 — fix the leak.** Specific changes determined by Phase 3 findings.
5. **Phase 5 — remove instrumentation.** Leave the builtin in place but remove the investigation calls.

## Non-goals

- **Does not attempt to rewrite bc_compiler to be state-free.** Targeted leak fixes only.
- **Does not defer to a future AIRL release.** Uses existing AIRL language features throughout.

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Leak turns out to be in Rust builtins (not AIRL bootstrap) | Medium | Step C's allocation-site tagging covers this; redirect to the alloc-site-tagging-diagnostics spec. |
| Fixing retention changes compiler output (proof elision breaks) | Low | Every fix includes "AOT test suite must still pass". |
| Multiple leaks compound — fixing one doesn't reveal the others | Medium | Expect an iterative investigation; each pass fixes 1-2 leak sources. |

## Expected outcome

- AIRL_castle memory: combined with Z3-context-lifecycle, BCFunc-native, and NaN-boxing → expected peak 500 MB to 2 GB on 33 files. Down from "60 GiB OOM" originally, and down from the current "15-25 GiB" after Step 2.5.
- G3 bootstrap: fits easily in 2 GiB; can restore 6 GiB Docker sandbox without question.
