# Stdlib Registry Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collapse the eight hardcoded stdlib list bodies in `crates/airl-driver/src/pipeline.rs` into a single `STDLIB_MODULES` registry. Pure refactor — behavior identical, drift impossible.

**Architecture:** Introduce `StdlibModule` struct + `STDLIB_MODULES` const array as the single source of truth. Rewrite `STDLIB_PATHS`, `stdlib_embed_hash`, `compile_stdlib_all`, `compile_to_object`, `compile_to_object_with_imports` as iterator loops over the registry. Stability-gated by a stdlib-embed-hash test.

**Tech Stack:** Rust, compile-time `include_str!`, existing test harness.

**Spec:** `docs/superpowers/specs/2026-04-23-stdlib-registry-consolidation-design.md`

---

## Task 1: Capture baseline stdlib embed hash

**Files:**
- Read: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Add a temporary test that prints the current hash**

Add at the bottom of the `#[cfg(test)] mod tests` block in `crates/airl-driver/src/pipeline.rs`:

```rust
#[test]
fn __capture_stdlib_embed_hash() {
    // Temporary — delete after capturing. Run with:
    //   cargo test -p airl-driver __capture_stdlib_embed_hash -- --nocapture
    eprintln!("STDLIB_EMBED_HASH = {}", stdlib_embed_hash());
}
```

- [ ] **Step 2: Run and capture the value**

```
cargo test -p airl-driver __capture_stdlib_embed_hash -- --nocapture 2>&1 | grep STDLIB_EMBED_HASH
```

Record the printed `u64` value. This is the anchor for the stability test that will be added in Task 4.

**Write the captured value down in this plan as a comment in Task 4, Step 1.** If you can't capture it (e.g. build fails), STOP and report.

- [ ] **Step 3: Delete the temporary test**

Remove the `__capture_stdlib_embed_hash` test you just added. It was only needed for the capture.

- [ ] **Step 4: Commit the no-op delete as part of Task 4**

Leave this step uncommitted. We'll roll it into the final commit.

---

## Task 2: Add `StdlibModule` + `STDLIB_MODULES` + helper

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Stub `SQLITE_SOURCE` on airlos**

Currently `SQLITE_SOURCE` is `#[cfg(not(target_os = "airlos"))]`-gated. Change to:

```rust
#[cfg(not(target_os = "airlos"))]
const SQLITE_SOURCE: &str = include_str!("../../../stdlib/sqlite.airl");
// On airlos, sqlite has no host bindings. Declare an empty stub so STDLIB_MODULES
// can reference SQLITE_SOURCE unconditionally; `on_airlos: false` prevents the stub
// from being used.
#[cfg(target_os = "airlos")]
const SQLITE_SOURCE: &str = "";
```

- [ ] **Step 2: Add `StdlibModule` struct and `STDLIB_MODULES` const**

Insert after the `*_SOURCE` constants (around line 60):

```rust
/// One auto-included stdlib module.
///
/// `source` is the embedded AIRL source (from `include_str!`).
/// `path` is the repo-relative path used for change-detection dependencies.
/// `name` is the bytecode compiler prefix used for symbol mangling.
/// `has_extern_c` routes AOT compilation through the extern-C-aware path.
/// `on_airlos` gates inclusion on the airlos target (currently only sqlite is excluded).
pub struct StdlibModule {
    pub source: &'static str,
    pub path: &'static str,
    pub name: &'static str,
    pub has_extern_c: bool,
    pub on_airlos: bool,
}

/// Single source of truth for every auto-included AIRL stdlib module.
/// Order matters — modules are compiled in registry order, so later modules may
/// reference names defined by earlier ones (e.g. string.airl uses prelude.airl's map).
pub const STDLIB_MODULES: &[StdlibModule] = &[
    StdlibModule { source: COLLECTIONS_SOURCE, path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/prelude.airl"), name: "collections", has_extern_c: false, on_airlos: true },
    StdlibModule { source: MATH_SOURCE,        path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/math.airl"),    name: "math",        has_extern_c: false, on_airlos: true },
    StdlibModule { source: RESULT_SOURCE,      path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/result.airl"),  name: "result",      has_extern_c: false, on_airlos: true },
    StdlibModule { source: STRING_SOURCE,      path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/string.airl"),  name: "string",      has_extern_c: false, on_airlos: true },
    StdlibModule { source: MAP_SOURCE,         path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/map.airl"),     name: "map",         has_extern_c: false, on_airlos: true },
    StdlibModule { source: SET_SOURCE,         path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/set.airl"),     name: "set",         has_extern_c: false, on_airlos: true },
    StdlibModule { source: IO_SOURCE,          path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/io.airl"),      name: "io",          has_extern_c: true,  on_airlos: true },
    StdlibModule { source: PATH_SOURCE,        path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/path.airl"),    name: "path",        has_extern_c: false, on_airlos: true },
    StdlibModule { source: RANDOM_SOURCE,      path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/random.airl"),  name: "random",      has_extern_c: false, on_airlos: true },
    StdlibModule { source: JSON_SOURCE,        path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/json.airl"),    name: "json",        has_extern_c: false, on_airlos: true },
    StdlibModule { source: SQLITE_SOURCE,      path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../../stdlib/sqlite.airl"),  name: "sqlite",      has_extern_c: true,  on_airlos: false },
];

#[inline]
fn on_current_target(m: &StdlibModule) -> bool {
    if cfg!(target_os = "airlos") {
        m.on_airlos
    } else {
        true
    }
}
```

Note: the module order above MUST match the pre-refactor compile order. Current pre-refactor order in `compile_stdlib_all` is: collections, math, result, string, map, set, io, path, random, (json on 5c7a30e), sqlite. The registry matches.

- [ ] **Step 3: Compile-check**

```
cargo build -p airl-driver
```

Expected: clean. No warnings introduced by these additions. If you see warnings about unused constants, the `STDLIB_MODULES` usage from Task 3 hasn't landed yet — that's expected until Task 3 is done.

- [ ] **Step 4: DO NOT commit yet** — we commit once at the end. This keeps the diff atomically reviewable.

---

## Task 3: Rewrite consumption sites

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Rewrite `STDLIB_PATHS`**

Currently around line 1055, `STDLIB_PATHS: &[&str]` is a const array. A const array can't directly derive from `STDLIB_MODULES` at compile time without a proc-macro. Replace with a function:

Delete the current `STDLIB_PATHS` constant. Add:

```rust
/// Stdlib source paths for change-detection. Derived from STDLIB_MODULES.
fn stdlib_paths() -> Vec<&'static str> {
    STDLIB_MODULES.iter()
        .filter(|m| on_current_target(m))
        .map(|m| m.path)
        .collect()
}
```

Update every caller of `STDLIB_PATHS` to call `stdlib_paths()` instead. Grep:

```
grep -n "STDLIB_PATHS" crates/airl-driver/src/pipeline.rs
```

If there are callers outside pipeline.rs, grep the whole crate. Update each.

- [ ] **Step 2: Rewrite `stdlib_embed_hash()`**

Replace the body:

```rust
fn stdlib_embed_hash() -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m)) {
        m.source.hash(&mut hasher);
    }
    hasher.finish()
}
```

- [ ] **Step 3: Rewrite `compile_stdlib_all()`**

Replace the function body (around line 1102):

```rust
fn compile_stdlib_all() -> Result<Vec<(Vec<BytecodeFunc>, BytecodeFunc)>, PipelineError> {
    let mut result = Vec::new();
    for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m)) {
        let tops = parse_source_stdlib(m.source, m.name)?;
        let ir_nodes: Vec<IRNode> = tops.iter().flat_map(compile_top_level).collect();
        let mut bc_compiler = BytecodeCompiler::with_prefix(m.name);
        let (funcs, main_func) = bc_compiler.compile_program(&ir_nodes);
        result.push((funcs, main_func));
    }
    Ok(result)
}
```

Delete the two `#[cfg]`-gated `stdlib_modules` arrays — they're replaced by the registry iteration.

- [ ] **Step 4: Rewrite the pure-AIRL loop in `compile_to_object()`**

Around line 1805, replace:

```rust
for (src, name) in &[
    (COLLECTIONS_SOURCE, "collections"),
    // ... (several entries)
] {
    let (funcs, _stdlib_main) = compile_source_to_bytecode(src, name)?;
    all_funcs.extend(funcs);
    mem_trace(&format!("stdlib {} compiled (total funcs={})", name, all_funcs.len()));
}
```

with:

```rust
for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m) && !m.has_extern_c) {
    let (funcs, _stdlib_main) = compile_source_to_bytecode(m.source, m.name)?;
    all_funcs.extend(funcs);
    mem_trace(&format!("stdlib {} compiled (total funcs={})", m.name, all_funcs.len()));
}
```

- [ ] **Step 5: Rewrite the extern-C loop in `compile_to_object()`**

Around line 1822, replace:

```rust
for (src, name) in &[
    (IO_SOURCE, "io"),
    (SQLITE_SOURCE, "sqlite"),
] {
    let (funcs, _stdlib_main, externs) = compile_source_to_bytecode_with_externs(src, name)?;
    all_funcs.extend(funcs);
    stdlib_extern_c_decls.extend(externs);
    mem_trace(&format!("stdlib-extern {} compiled (total funcs={})", name, all_funcs.len()));
}
```

with:

```rust
for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m) && m.has_extern_c) {
    let (funcs, _stdlib_main, externs) = compile_source_to_bytecode_with_externs(m.source, m.name)?;
    all_funcs.extend(funcs);
    stdlib_extern_c_decls.extend(externs);
    mem_trace(&format!("stdlib-extern {} compiled (total funcs={})", m.name, all_funcs.len()));
}
```

- [ ] **Step 6: Rewrite the same two loops in `compile_to_object_with_imports()`**

Around line 1988. Same shape as Step 4 + Step 5:

```rust
// Pure-AIRL
for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m) && !m.has_extern_c) {
    let (funcs, _stdlib_main) = compile_source_to_bytecode(m.source, m.name)?;
    all_funcs.extend(funcs);
}

// Extern-C
for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m) && m.has_extern_c) {
    let (funcs, _stdlib_main, externs) = compile_source_to_bytecode_with_externs(m.source, m.name)?;
    all_funcs.extend(funcs);
    extern_c_decls.extend(externs);
}
```

Note: the original `compile_to_object_with_imports` had a different variable name (`extern_c_decls`, not `stdlib_extern_c_decls`). Preserve whichever variable name the original function used.

- [ ] **Step 7: Compile**

```
cargo build -p airl-driver
```

Expected: clean. Zero warnings about unused constants/functions.

If you see `unused const *_SOURCE` warnings, the constant is used only via `STDLIB_MODULES` — that's fine, the const references prevent the warning. If you see `STDLIB_PATHS was never constructed` or similar for code removed via refactor, delete those fragments.

---

## Task 4: Stability test + cleanup + commit

**Files:**
- Modify: `crates/airl-driver/src/pipeline.rs`

- [ ] **Step 1: Add the stability test**

At the bottom of the test module, add:

```rust
#[test]
fn stdlib_embed_hash_is_stable() {
    // Anchor value captured before the consolidation refactor.
    // A mismatch here means the refactor changed iteration order or included/excluded
    // sources — both are regressions, not intended behavior changes.
    const EXPECTED: u64 = <VALUE FROM TASK 1 STEP 2>;
    assert_eq!(stdlib_embed_hash(), EXPECTED, "stdlib embed hash changed — refactor has a drift bug");
}
```

Replace `<VALUE FROM TASK 1 STEP 2>` with the actual `u64` literal captured in Task 1.

- [ ] **Step 2: Add registry invariant tests**

```rust
#[test]
fn stdlib_registry_has_no_duplicate_names() {
    let mut seen = std::collections::HashSet::new();
    for m in STDLIB_MODULES {
        assert!(seen.insert(m.name), "duplicate module name: {}", m.name);
    }
}

#[test]
fn stdlib_registry_has_no_duplicate_paths() {
    let mut seen = std::collections::HashSet::new();
    for m in STDLIB_MODULES {
        assert!(seen.insert(m.path), "duplicate module path: {}", m.path);
    }
}

#[test]
fn stdlib_registry_sources_are_non_empty_on_target() {
    for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m)) {
        assert!(!m.source.is_empty(), "empty source for {} on current target", m.name);
    }
}

#[test]
fn stdlib_registry_airlos_excludes_sqlite() {
    let sqlite_entry = STDLIB_MODULES.iter().find(|m| m.name == "sqlite");
    assert!(sqlite_entry.is_some(), "sqlite module missing from registry");
    assert_eq!(sqlite_entry.unwrap().on_airlos, false);
}
```

- [ ] **Step 3: Run test suite**

```
cargo test -p airl-driver 2>&1 | tail -15
```

Expected: all pass, including the new stability test. If `stdlib_embed_hash_is_stable` fails, the refactor changed the hash — most likely cause is iteration-order drift between `STDLIB_MODULES` and the old hardcoded lists. Check that the registry order matches: collections, math, result, string, map, set, io, path, random, json, sqlite.

- [ ] **Step 4: Full Rust suite regression**

```
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver 2>&1 | grep "test result:" | head -20
```

Expected: every line `ok. N passed; 0 failed`.

- [ ] **Step 5: AOT suite regression**

```
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh 2>&1 | tail -5
```

Expected: `Results: 68 passed, 0 failed, 0 compile errors, 0 skipped`.

- [ ] **Step 6: `airl compile` smoke for JSON**

```
rm -f /tmp/json_smoke
cargo run --release --features aot -- compile tests/aot/round3_builtin_json_full.airl -o /tmp/json_smoke && /tmp/json_smoke
```

Expected: `int:ok|str:ok|bool:ok|null:ok|roundtrip-int:ok|roundtrip-str:ok|stringify-list:ok`.

- [ ] **Step 7: Commit**

```bash
git add crates/airl-driver/src/pipeline.rs
git commit -m "$(cat <<'EOF'
refactor(driver): consolidate stdlib auto-include into STDLIB_MODULES

Replaces eight hardcoded stdlib list bodies in pipeline.rs with a
single STDLIB_MODULES registry + iterator queries. Drift between
lists (which caused the json.airl and random.airl gaps) is now
impossible by construction.

Sites collapsed:
- STDLIB_PATHS → stdlib_paths() query
- stdlib_embed_hash() iterator
- compile_stdlib_all() airlos + non-airlos arrays → one filter expression
- compile_to_object() pure-AIRL + extern-C arrays
- compile_to_object_with_imports() pure-AIRL + extern-C arrays

All use `on_current_target()` for platform gating and `has_extern_c`
for AOT compilation-path routing. The empty-stub SQLITE_SOURCE on
airlos is filtered out by `on_airlos: false`.

Stability test: stdlib_embed_hash_is_stable pins the hash value
captured pre-refactor so iteration-order drift is caught in CI.

Pure refactor — no behavior change. All Rust tests green, AOT suite
68/68, `airl compile` on the JSON fixture produces identical output.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review

**Spec coverage:**
- Registry structure (StdlibModule + STDLIB_MODULES + on_current_target) → Task 2.
- Airlos sqlite stub → Task 2 Step 1.
- All eight consumption sites rewritten → Task 3 (6 steps covering all sites).
- Stability test → Task 4 Step 1.
- Registry invariant tests → Task 4 Step 2.
- Pure refactor verification → Task 4 Steps 4-6 (Rust + AOT + smoke).

**Placeholder scan:** `<VALUE FROM TASK 1 STEP 2>` is the only placeholder and it's an explicit capture-and-substitute instruction — the implementer fills it after running the capture test.

**Type consistency:** `StdlibModule` fields (`source`, `path`, `name`, `has_extern_c`, `on_airlos`) are used identically across all steps. `on_current_target()` signature matches the stated helper. `STDLIB_MODULES` is the only registry name referenced. The variable `stdlib_extern_c_decls` vs `extern_c_decls` difference between the two AOT functions is explicitly called out in Task 3 Step 6.

**Out-of-scope guard:** No new features, no behavior changes, no tests beyond registry invariants + stability. The implementer is expected to refuse scope creep (e.g. "while I'm here, let me also move SOURCE constants to a separate file").
