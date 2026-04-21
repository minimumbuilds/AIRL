# Z3 Context Per-File Lifecycle

**Date:** 2026-04-21
**Status:** Proposed
**Priority:** High — largest suspected contributor to the G3/AIRL_castle memory baseline after Step 2.5 pools landed
**Scope:** `bootstrap/z3_bridge_g3.airl`, `bootstrap/z3_cache.airl`, `crates/airl-rt/` (new Z3 context builtin), `crates/airl-solver/` (Rust-side also)

## Problem

After Step 2.5 pools (small-int, short-string) and Step 3 per-file emit, the G3 bootstrap build still OOMs at 6.2 GiB during `g3_compiler.airl` (file 7 of 7). Per-file .o emission works — `.stdlib.o`, `.f0-.f5.o` land on disk — but peak RSS did not drop materially. Diagnostic (`AIRL_RT_TRACE=1`) showed 1.6 M alive `list` RtValues at OOM and 500 K alive `int` despite the small-int pool.

The list count is suspicious. Across 6 processed files, the fold accumulator (now just obj-path strings) cannot account for 1.6 M lists. The retention is structural.

The remaining suspects, ordered by likelihood:

1. **Z3 C-library term pool**. Each file's verification creates a Z3 `Context` via `airl_z3_mk_context`. The Z3 C library pools term memory per-context and never returns to the OS on drop. Across 6 files with 21/50/131/38/17/31 = 288 contract verifications, the Z3 pool accumulates GBs of term memory. On the 33-file AIRL_castle run this is multiplied proportionally.

2. **Z3 context objects retained in the AIRL-level cache**. `bootstrap/z3_cache.airl` may hold Z3 AST handles inside the proof-map, keeping the owning context reachable and preventing the Rust `Z3Prover` drop.

3. **AIRL bootstrap state**. `linearity-check-module`, the Z3 translator, and the `z3c-verify-ast-cached` all build internal maps that may be retained across invocations.

This spec targets (1) and (2). (3) is covered by a separate spec on AIRL-side state isolation.

## Current architecture

### Rust side (`crates/airl-solver/`)

- `Z3Prover::new()` creates a new `Context` and `Solver` per call. When the `Z3Prover` is dropped (end of `verify_defn`), the Context's Rust wrapper drops — but the underlying Z3 C library's `Z3_context` is only freed via `Z3_del_context`. If `Z3Prover`'s Drop impl does the right thing, the C-side context is destroyed — BUT the Z3 allocator pool is process-scoped and doesn't return memory to the OS even after `Z3_del_context`. See `src/compiler/api/c++/context.cpp` in Z3 upstream: `memory::finalize` only runs at process exit.

### AIRL bootstrap side (`bootstrap/z3_bridge_g3.airl`)

- `z3-verify-function` creates a context via `airl_z3_mk_context`, runs proofs, then calls `airl_z3_del_context`. The context is short-lived per function.
- `z3-verify-ast-nodes` wraps multiple `z3-verify-function` calls. Each creates and destroys its own context.
- **Suspected leak**: the `proof-map` built by `z3-verify-ast-nodes` may store Z3 AST handles (pointers into the already-destroyed context). After `del_context` runs, these handles dangle; the AIRL RtValue holding them stays alive.

### AIRL cache side (`bootstrap/z3_cache.airl`)

- `z3c-verify-ast-cached` maintains an AIRL `map-new`-keyed cache. The cache stores serialized proof verdicts (not Z3 AST handles), so this is probably safe.
- `z3c-save` persists to disk. Fine.

## Proposed design

### 1. Audit the proof-map for dangling Z3 handles

Read `bootstrap/z3_bridge_g3.airl` carefully. Identify every value placed into the `proof-map`. Confirm each is either a primitive (Int / Bool / String) or a stable AIRL value — no raw Z3 handles. If any Z3 handle is stored, convert to a primitive verdict (`"proven" | "disproven" | "unknown"`) before map insertion.

### 2. Force a Z3 process respawn between files for G3 builds

This is the nuclear option and the surest way to reclaim Z3 C-library memory. Rather than trying to coax `Z3_del_context` into actually returning pages to the OS, fork a fresh `airl-solver` subprocess per file, have it verify that file, then exit. The OS reclaims all memory.

**Implementation sketch:**

- New Rust-side binary: `airl-z3-verify-once`. Takes a serialized AST + cache on stdin, writes proof-map to stdout, exits.
- `crates/airl-solver/src/lib.rs` gains `verify_ast_in_subprocess(ast: &[TopLevel], cache: &DiskCache) -> ProofCache` which spawns the subprocess and pipes.
- `bootstrap/z3_bridge_g3.airl`'s `z3-verify-ast-nodes` gains an alternate path keyed on `AIRL_Z3_SUBPROCESS=1`: instead of in-process verification, call out via a new `airl_z3_verify_in_subprocess` builtin.
- Per-file overhead: ~50 ms process spawn, not a concern when compile time is 10-60 s per file.

**Gain:** All Z3 C-pool memory returned to OS per file. Expected peak RSS reduction: 40-60% for programs that hit Z3 on every file.

### 3. Alternatively: recreate Z3 Context per function inside one process

Cheaper than subprocess but less effective. Ensures we hit `Z3_del_context` more frequently (each function verification gets a fresh context). Works IF the Z3 per-context pool actually is scoped correctly. Quick win if (2) proves expensive.

**Implementation:**

- `crates/airl-solver/src/prover.rs`: `verify_defn` already creates a fresh `Context`. Verify with a memory-leak test that each `Context` drop reclaims ~all of its allocated term memory.
- If it does not (likely, given upstream Z3 behavior), fall through to Option 2.

### 4. Measurement plan

Before each option, add `AIRL_MEM_TRACE` snapshots at each Z3 context creation/destruction. Run against `bc_compiler.airl` (131 misses — the biggest Z3 file). Record peak RSS delta.

Expected results:
- Option 3 alone: 20-40% reduction (hits the "Z3 drops correctly" case).
- Option 2 (subprocess): 40-60% reduction (guarantees OS reclaim).

## Files to modify

| File | Change | Phase |
|---|---|---|
| `crates/airl-solver/src/prover.rs` | Add context lifecycle instrumentation | 4 |
| `crates/airl-solver/src/subprocess.rs` (new) | Fork-based verify | 2 |
| `crates/airl-driver/src/bin/airl-z3-verify-once.rs` (new) | Subprocess entry point | 2 |
| `crates/airl-rt/src/z3.rs` | New `airl_z3_verify_in_subprocess` extern "C" | 2 |
| `bootstrap/z3_bridge_g3.airl` | Audit proof-map; optional subprocess branch | 1, 2 |
| `bootstrap/z3_cache.airl` | Verify no Z3 handles stored | 1 |

## Non-goals

- **Do not weaken Z3 verification.** This spec preserves all contract checks, just reclaims memory between them.
- **Do not introduce a "skip Z3" flag.** That was the wrong fix in `86b301c`; this spec is the right one.

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Subprocess spawn overhead dominates small files | Low | Spawn cost ~50 ms; small files take 2+ s in G3 compile. |
| IPC serialization itself has a memory overhead | Medium | Use a streaming format (bincode-of-Value); free after write. |
| Z3 C-library actually doesn't retain memory between context creations | Low | Worth measuring first. If false, option 3 is sufficient. |
