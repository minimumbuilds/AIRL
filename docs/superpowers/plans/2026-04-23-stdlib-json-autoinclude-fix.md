# stdlib/json.airl Auto-Include Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `round3_builtin_json_full` AOT compile failure by adding `stdlib/json.airl` to the driver's auto-include registry, so `json-parse` / `json-stringify` resolve at link time.

**Architecture:** Single-file edit to `crates/airl-driver/src/pipeline.rs`. Add `JSON_SOURCE` constant, thread it through four existing use sites (the hash, the path-list, and both airlos / non-airlos `stdlib_modules` arrays in `compile_stdlib_all`). No new files, no API changes, no behavioral changes beyond "json.airl now compiles into every AIRL program's stdlib like prelude.airl does".

**Tech Stack:** Rust, `include_str!` macro, existing AIRL test harness (`tests/aot/run_aot_tests.sh`).

**Spec:** `docs/superpowers/specs/2026-04-23-stdlib-json-autoinclude-fix.md`

---

## File Structure

Exactly one Rust file is modified: `crates/airl-driver/src/pipeline.rs`. No new files. The file responsibility is unchanged — it still orchestrates stdlib inclusion; this change just extends its registry by one module.

Four distinct regions of that file need edits, listed by line number (values are from the current tip of `main` at `9bc44c9`; subagent should grep if line numbers have drifted):

| Region | Purpose |
|--------|---------|
| `~line 57-59` | `include_str!` constants — one per stdlib file |
| `~line 1057-1070` | `STDLIB_PATHS` — change-detection path list |
| `~line 1071-1077` | `stdlib_embed_hash()` — hash function |
| `~line 1104-1127` | `compile_stdlib_all()` — two `stdlib_modules` arrays (airlos / non-airlos) — the load-bearing compilation list |

The fourth region is the one the spec under-emphasized: **it is the list that actually compiles each stdlib module into bytecode**. Without adding json.airl here, the `include_str!` constant would be unused and the fix would have no effect.

---

## Task 1: Wire `JSON_SOURCE` through pipeline.rs

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`

This single logical change touches four co-located regions of the file. Group them into one commit because they share a single purpose (registering `stdlib/json.airl` as auto-included) and must land together to preserve consistency.

- [ ] **Step 1: Reproduce the failure (pre-fix baseline)**

```bash
cd /mnt/b6d8b397-9fc1-42ac-a0da-8664a73d4ee9/AIRL/.worktrees/stdlib-json-autoinclude
rm -rf tests/aot/cache/round3_builtin_json_full
cargo run --release --features aot -- run \
  --load bootstrap/lexer.airl \
  --load bootstrap/parser.airl \
  --load bootstrap/z3_cache.airl \
  --load bootstrap/z3_bridge_g3.airl \
  --load bootstrap/linearity.airl \
  --load bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl -- \
  tests/aot/round3_builtin_json_full.airl \
  -o tests/aot/cache/round3_builtin_json_full 2>&1 | tail -10
```

Expected: link failure with lines like

```
/usr/bin/ld: airl_program:(.text+0x9f8): undefined reference to `__airl_fn_json_parse'
/usr/bin/ld: airl_program:(.text+0x130e): undefined reference to `__airl_fn_json_stringify'
collect2: error: ld returned 1 exit status
[g3] err:  linker failed: Some(1)
```

This confirms the bug reproduces before the fix.

- [ ] **Step 2: Add the `JSON_SOURCE` constant**

Open `crates/airl-driver/src/pipeline.rs`. Find the block of `include_str!` constants around lines 49-59. It currently looks like:

```rust
const COLLECTIONS_SOURCE: &str = include_str!("../../../stdlib/prelude.airl");
const MATH_SOURCE: &str = include_str!("../../../stdlib/math.airl");
const RESULT_SOURCE: &str = include_str!("../../../stdlib/result.airl");
const STRING_SOURCE: &str = include_str!("../../../stdlib/string.airl");
const MAP_SOURCE: &str = include_str!("../../../stdlib/map.airl");
const SET_SOURCE: &str = include_str!("../../../stdlib/set.airl");
const IO_SOURCE: &str = include_str!("../../../stdlib/io.airl");
const PATH_SOURCE: &str = include_str!("../../../stdlib/path.airl");
const RANDOM_SOURCE: &str = include_str!("../../../stdlib/random.airl");
#[cfg(not(target_os = "airlos"))]
const SQLITE_SOURCE: &str = include_str!("../../../stdlib/sqlite.airl");
```

Insert a new constant immediately before the `#[cfg(not(target_os = "airlos"))]` line for `SQLITE_SOURCE`:

```rust
const RANDOM_SOURCE: &str = include_str!("../../../stdlib/random.airl");
// json.airl is auto-included (like prelude.airl) because the Rust builtins
// `json-parse` and `json-stringify` were deregistered in bytecode_aot.rs and
// bytecode_vm.rs; the AIRL implementations in stdlib/json.airl are now the
// only providers of those symbols.
const JSON_SOURCE: &str = include_str!("../../../stdlib/json.airl");
#[cfg(not(target_os = "airlos"))]
const SQLITE_SOURCE: &str = include_str!("../../../stdlib/sqlite.airl");
```

- [ ] **Step 3: Add the path to `STDLIB_PATHS`**

Find `STDLIB_PATHS` around line 1057. It currently looks like:

```rust
const STDLIB_PATHS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/prelude.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/math.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/result.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/string.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/map.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/set.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/io.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/path.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/random.airl"),
    #[cfg(not(target_os = "airlos"))]
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/sqlite.airl"),
];
```

Insert the `json.airl` path immediately before the `#[cfg(not(target_os = "airlos"))]` line:

```rust
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/random.airl"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/json.airl"),
    #[cfg(not(target_os = "airlos"))]
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/sqlite.airl"),
```

- [ ] **Step 4: Hash `JSON_SOURCE` in `stdlib_embed_hash()`**

Find `stdlib_embed_hash()` around line 1071. It currently has hash calls for every `*_SOURCE` constant:

```rust
fn stdlib_embed_hash() -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    COLLECTIONS_SOURCE.hash(&mut hasher);
    MATH_SOURCE.hash(&mut hasher);
    RESULT_SOURCE.hash(&mut hasher);
    STRING_SOURCE.hash(&mut hasher);
    MAP_SOURCE.hash(&mut hasher);
    SET_SOURCE.hash(&mut hasher);
    IO_SOURCE.hash(&mut hasher);
    PATH_SOURCE.hash(&mut hasher);
    RANDOM_SOURCE.hash(&mut hasher);
    #[cfg(not(target_os = "airlos"))]
    SQLITE_SOURCE.hash(&mut hasher);
    hasher.finish()
}
```

Insert the hash call for `JSON_SOURCE` immediately before the `#[cfg(not(target_os = "airlos"))]` line:

```rust
    RANDOM_SOURCE.hash(&mut hasher);
    JSON_SOURCE.hash(&mut hasher);
    #[cfg(not(target_os = "airlos"))]
    SQLITE_SOURCE.hash(&mut hasher);
```

- [ ] **Step 5: Register `JSON_SOURCE` in BOTH `stdlib_modules` arrays in `compile_stdlib_all()`**

This is the load-bearing edit — without it, the constant is unused and the fix has no effect. Find `compile_stdlib_all()` around line 1102. It has TWO arrays (one per `#[cfg]`):

```rust
fn compile_stdlib_all() -> Result<Vec<(Vec<BytecodeFunc>, BytecodeFunc)>, PipelineError> {
    #[cfg(target_os = "airlos")]
    let stdlib_modules: &[(&str, &str)] = &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (IO_SOURCE, "io"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
    ];
    #[cfg(not(target_os = "airlos"))]
    let stdlib_modules: &[(&str, &str)] = &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (IO_SOURCE, "io"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
        (SQLITE_SOURCE, "sqlite"),
    ];
```

In the `airlos` array, append at the end:

```rust
    #[cfg(target_os = "airlos")]
    let stdlib_modules: &[(&str, &str)] = &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (IO_SOURCE, "io"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
        (JSON_SOURCE, "json"),
    ];
```

In the `not(target_os = "airlos")` array, insert `(JSON_SOURCE, "json"),` immediately before the `SQLITE_SOURCE` entry:

```rust
    #[cfg(not(target_os = "airlos"))]
    let stdlib_modules: &[(&str, &str)] = &[
        (COLLECTIONS_SOURCE, "collections"),
        (MATH_SOURCE, "math"),
        (RESULT_SOURCE, "result"),
        (STRING_SOURCE, "string"),
        (MAP_SOURCE, "map"),
        (SET_SOURCE, "set"),
        (IO_SOURCE, "io"),
        (PATH_SOURCE, "path"),
        (RANDOM_SOURCE, "random"),
        (JSON_SOURCE, "json"),
        (SQLITE_SOURCE, "sqlite"),
    ];
```

The module name `"json"` is the `BytecodeCompiler` prefix used for name-mangling symbols — it does NOT need to match the file stem. Convention in this file is to use a short lowercase name (e.g. `"collections"` for `prelude.airl`, `"result"` for `result.airl`); `"json"` is the obvious choice.

- [ ] **Step 6: Build**

```bash
cargo build --features aot 2>&1 | tail -10
```

Expected: clean build with only pre-existing warnings (`unused_mut`, `unreachable_patterns`, `airlos` cfg in crates other than `airl-driver`). No new warnings. Any compile error means an edit was mis-placed or has a typo.

- [ ] **Step 7: Run the Rust test suite**

```bash
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver 2>&1 | grep "test result:" | head -20
```

Expected: every `test result:` line shows `ok. N passed; 0 failed`. No regressions. (Stdlib embed changes invalidate cached stdlib bytecode across the suite — tests that rely on the cache will regenerate it, but none should fail.)

- [ ] **Step 8: Confirm the regression is fixed**

```bash
rm -rf tests/aot/cache/round3_builtin_json_full
cargo run --release --features aot -- run \
  --load bootstrap/lexer.airl \
  --load bootstrap/parser.airl \
  --load bootstrap/z3_cache.airl \
  --load bootstrap/z3_bridge_g3.airl \
  --load bootstrap/linearity.airl \
  --load bootstrap/bc_compiler.airl \
  bootstrap/g3_compiler.airl -- \
  tests/aot/round3_builtin_json_full.airl \
  -o tests/aot/cache/round3_builtin_json_full 2>&1 | tail -5
```

Expected: no linker errors. Last lines of output should NOT contain `undefined reference to __airl_fn_json_*`. The command may still emit Z3/warning output; what matters is successful link.

Then run the compiled binary and verify expected output:

```bash
./tests/aot/cache/round3_builtin_json_full
```

Expected stdout (from the `;; EXPECT:` header in the test file):

```
int:ok|str:ok|bool:ok|null:ok|roundtrip-int:ok|roundtrip-str:ok|stringify-list:ok
```

- [ ] **Step 9: Run the full AOT suite**

```bash
rm -rf tests/aot/cache
bash tests/aot/run_aot_tests.sh 2>&1 | tail -15
```

Expected last line: `Results: 68 passed, 0 failed, 0 compile errors, 0 skipped`. (Prior baseline was 67/68 with `round3_builtin_json_full` as the one compile error.)

If you see `COMPILE_FAIL` on any OTHER test, per project memory (`project_aot_test_flakiness.md`): retry that test individually with `rm -rf tests/aot/cache/<test-name>` and re-run; transient failures are a known phenomenon. If the retry also fails, that's a regression and should be reported — do NOT attempt to fix it here (this plan's scope is strictly the json fix).

- [ ] **Step 10: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs
git commit -m "$(cat <<'EOF'
fix(driver): auto-include stdlib/json.airl

stdlib/json.airl was defined (332 lines) and the Rust builtins json-parse
and json-stringify were deregistered ("AIRL stdlib equivalents take over"
per comments in bytecode_aot.rs and bytecode_vm.rs), but json.airl was
never added to the driver's auto-include registry. Result: linker errors
for __airl_fn_json_parse and __airl_fn_json_stringify in any program
using JSON, visible as round3_builtin_json_full COMPILE_FAIL.

Fix wires JSON_SOURCE through four co-located regions of pipeline.rs:
the include_str! constants block, STDLIB_PATHS, stdlib_embed_hash(),
and both airlos / non-airlos arrays in compile_stdlib_all(). All other
auto-included stdlib files land in all four regions; this one was
uniformly absent.

Regression test: round3_builtin_json_full transitions from COMPILE_FAIL
to PASS (68/68 AOT tests now pass).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review

**Spec coverage:** The spec lists three required edits (add `JSON_SOURCE`, add to `STDLIB_PATHS`, hash in `stdlib_embed_hash`). The plan covers those PLUS a fourth load-bearing edit (register in both `stdlib_modules` arrays in `compile_stdlib_all`) that the spec under-emphasized. The plan is strictly a superset of the spec's requirements; the additional edit is necessary for the fix to actually work. Regression test is covered by Step 9.

**Placeholder scan:** No TBD, TODO, "fill in", "similar to", or other placeholder markers. Every step has exact code or exact commands with expected output.

**Type consistency:** Identifier usage is consistent across all five Rust regions: `JSON_SOURCE` as the constant name, `"json"` as the module prefix in `stdlib_modules`, `stdlib/json.airl` as the file path. No naming drift.

**Out-of-scope guard:** Step 9 explicitly tells the implementer NOT to fix unrelated AOT regressions; this plan is scoped strictly to json.airl.
