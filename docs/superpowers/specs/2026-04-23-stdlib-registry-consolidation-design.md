# Stdlib Auto-Include Registry Consolidation — Design Spec

**Date:** 2026-04-23
**Status:** Draft
**Scope:** Replace the six hardcoded stdlib lists in `crates/airl-driver/src/pipeline.rs` with a single source-of-truth registry, so adding a new stdlib module requires one edit instead of five and drift between lists becomes impossible.

## Background

The prior spec (`2026-04-23-stdlib-json-autoinclude-fix.md`) fixed `stdlib/json.airl` being missing from the stdlib auto-include registry. Implementing it required inserting `JSON_SOURCE` into **six** separate hardcoded lists:

| Site | Line | Purpose |
|------|------|---------|
| `STDLIB_PATHS` | ~1055 | Path strings for change detection |
| `stdlib_embed_hash()` | ~1063 | Embed-hash for cache invalidation |
| `compile_stdlib_all()` — airlos | ~1103 | Module compile list, airlos target (no sqlite) |
| `compile_stdlib_all()` — non-airlos | ~1116 | Module compile list, non-airlos target (with sqlite) |
| `compile_to_object()` — pure-AIRL | ~1807 | AOT compile, modules without extern-C |
| `compile_to_object()` — extern-C | ~1826 | AOT compile, modules with extern-C (io, sqlite) |
| `compile_to_object_with_imports()` — pure-AIRL | ~1990 | Import-aware AOT, pure-AIRL |
| `compile_to_object_with_imports()` — extern-C | ~1997 | Import-aware AOT, extern-C |

That's eight list bodies to keep in sync whenever a stdlib module is added, removed, renamed, or moved between pure-AIRL and extern-C categorization.

The JSON autoinclude fix **also** turned up two bugs that existed precisely because the lists had drifted:

1. `compile_to_object_with_imports()` was missing `RANDOM_SOURCE` — a pre-existing gap that only surfaced when an audit checked cross-list consistency. Programs that used both `(import ...)` forms and `random-*` functions would fail at link time.
2. `compile_to_object()` and `compile_to_object_with_imports()` had subtly different lists even where they should have matched, and neither tracked the VM path's list.

These are the second and third instances in recent history of "adding a stdlib module required hunting through the codebase" being a class of bug, not a one-off.

## Goals

1. Define a single `STDLIB_MODULES` constant that is the only place in the driver where the set of auto-included stdlib files is enumerated.
2. Thread `STDLIB_MODULES` through all eight existing list-body sites so each site derives its view by filtering/mapping the registry, not by maintaining a parallel list.
3. Preserve existing behavior byte-for-byte — this is a refactor, not a feature change. Cache keys and compile output must remain stable (modulo the json/random parity additions, which already landed).
4. Make the registry strongly typed: each entry carries source constant, module name, extern-C flag, and airlos inclusion flag. Derived views are simple filters.

## Non-Goals

- Adding or removing stdlib modules. The set is frozen at the current 11 (collections, math, result, string, map, set, io, path, random, json, sqlite).
- Changing how stdlib modules are compiled, loaded, or cached. The registry refactor is purely about data layout.
- Changing the public API of `airl-driver`. `compile_stdlib_all`, `compile_to_object`, etc. keep their signatures.
- Dynamic stdlib discovery (e.g. scanning `stdlib/*.airl` at runtime). The registry stays a compile-time constant so `include_str!` keeps working.
- Moving module definitions into a separate crate, file, or configuration. Locality stays in `pipeline.rs`.

## Architecture

### Registry structure

```rust
/// One auto-included stdlib module.
///
/// The `source` field is the embedded AIRL source (`include_str!`).
/// The `name` field is the bytecode compiler prefix used for symbol mangling
/// (e.g. `collections` for prelude.airl, `random` for random.airl).
/// `has_extern_c` gates whether compile_to_object must route this module
/// through the extern-C-aware compilation path.
/// `on_airlos` gates per-target inclusion: modules that require host
/// facilities (currently only sqlite) are excluded from airlos builds.
pub struct StdlibModule {
    pub source: &'static str,
    pub name: &'static str,
    pub has_extern_c: bool,
    pub on_airlos: bool,
}

/// Single source of truth for every auto-included AIRL stdlib module.
/// Order matters — modules are compiled in registry order so later modules
/// may reference names defined by earlier ones (e.g. `string.airl` depends
/// on `prelude.airl` functions like `map` and `filter`).
pub const STDLIB_MODULES: &[StdlibModule] = &[
    StdlibModule { source: COLLECTIONS_SOURCE, name: "collections", has_extern_c: false, on_airlos: true },
    StdlibModule { source: MATH_SOURCE,        name: "math",        has_extern_c: false, on_airlos: true },
    StdlibModule { source: RESULT_SOURCE,      name: "result",      has_extern_c: false, on_airlos: true },
    StdlibModule { source: STRING_SOURCE,      name: "string",      has_extern_c: false, on_airlos: true },
    StdlibModule { source: MAP_SOURCE,         name: "map",         has_extern_c: false, on_airlos: true },
    StdlibModule { source: SET_SOURCE,         name: "set",         has_extern_c: false, on_airlos: true },
    StdlibModule { source: IO_SOURCE,          name: "io",          has_extern_c: true,  on_airlos: true },
    StdlibModule { source: PATH_SOURCE,        name: "path",        has_extern_c: false, on_airlos: true },
    StdlibModule { source: RANDOM_SOURCE,      name: "random",      has_extern_c: false, on_airlos: true },
    StdlibModule { source: JSON_SOURCE,        name: "json",        has_extern_c: false, on_airlos: true },
    StdlibModule { source: SQLITE_SOURCE,      name: "sqlite",      has_extern_c: true,  on_airlos: false },
];
```

The `include_str!` constants (`COLLECTIONS_SOURCE`, ..., `SQLITE_SOURCE`) remain in their current form — they are the actual embed roots. The registry references them by name.

### Platform gating

`SQLITE_SOURCE` is currently declared under `#[cfg(not(target_os = "airlos"))]` — the constant itself does not exist on airlos builds. We cannot reference it from the `STDLIB_MODULES` array unconditionally.

Two options:

**Option A — `#[cfg]` the SQLITE entry:**
```rust
pub const STDLIB_MODULES: &[StdlibModule] = &[
    // ...
    #[cfg(not(target_os = "airlos"))]
    StdlibModule { source: SQLITE_SOURCE, name: "sqlite", has_extern_c: true, on_airlos: false },
];
```

Wait — this has a subtle issue: `#[cfg]` on array elements is stable but not ideal stylistically, and `on_airlos: false` combined with `#[cfg(not(airlos))]` is double-gating. The array version on airlos would simply not contain the entry, so the `on_airlos` flag is redundant there. But the `on_airlos` flag is what lets the filter expressions stay simple and cfg-free at consumption sites.

**Option B — declare `SQLITE_SOURCE` unconditionally but stub on airlos:**
```rust
#[cfg(target_os = "airlos")]
const SQLITE_SOURCE: &str = "";  // stub; filtered out via on_airlos: false

#[cfg(not(target_os = "airlos"))]
const SQLITE_SOURCE: &str = include_str!("../../../stdlib/sqlite.airl");
```

Then the registry uses it unconditionally, and the `on_airlos` flag does the filtering.

**Recommendation: Option B.** Pros:
- The registry itself is `#[cfg]`-free — readers don't need to mentally filter.
- Consumption-site filters are simple `if on_airlos` / `if !on_airlos` checks.
- The stub empty-string `SQLITE_SOURCE` on airlos is never dereferenced because `on_airlos: false` blocks it; it exists only to satisfy the array's type.
- Unconditional reference is how the rest of the platform-gated code in this crate handles similar cases (TODO: verify).

Cons:
- Slightly surprising "why is SQLITE_SOURCE empty on airlos" — mitigated by a one-line comment.

If the reviewer prefers Option A, it works too; the difference is aesthetic.

### Consumption sites

Each of the eight current list-body sites becomes a one-line iterator over `STDLIB_MODULES` with a predicate.

**`STDLIB_PATHS`** becomes a const function or a `Vec` built lazily:
```rust
fn stdlib_paths() -> Vec<&'static str> {
    STDLIB_MODULES.iter()
        .filter(|m| m.on_airlos || !cfg!(target_os = "airlos"))
        .filter(|m| cfg!(not(target_os = "airlos")) || m.on_airlos)
        .map(|m| m.name)  // wait — STDLIB_PATHS was paths, not names
        .collect()
}
```

Actually `STDLIB_PATHS` stores filesystem paths for change detection, not module names. We'd add a `path` field to `StdlibModule`:

```rust
pub struct StdlibModule {
    pub path: &'static str,   // "stdlib/random.airl"
    pub source: &'static str, // include_str! contents
    pub name: &'static str,   // "random"
    // ...
}
```

Then `STDLIB_PATHS` becomes:
```rust
pub fn stdlib_paths() -> Vec<&'static str> {
    STDLIB_MODULES.iter()
        .filter(|m| on_current_target(m))
        .map(|m| m.path)
        .collect()
}
```

**`stdlib_embed_hash()`** iterates:
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

**`compile_stdlib_all()`** becomes:
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

**`compile_to_object()`** needs both the pure-AIRL list and the extern-C list:
```rust
// 1. Compile pure-AIRL stdlib modules
for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m) && !m.has_extern_c) {
    let (funcs, _main) = compile_source_to_bytecode(m.source, m.name)?;
    all_funcs.extend(funcs);
    mem_trace(&format!("stdlib {} compiled (total funcs={})", m.name, all_funcs.len()));
}

// 1b. Compile extern-C stdlib modules
let mut stdlib_extern_c_decls = Vec::new();
for m in STDLIB_MODULES.iter().filter(|m| on_current_target(m) && m.has_extern_c) {
    let (funcs, _main, externs) = compile_source_to_bytecode_with_externs(m.source, m.name)?;
    all_funcs.extend(funcs);
    stdlib_extern_c_decls.extend(externs);
    mem_trace(&format!("stdlib-extern {} compiled (total funcs={})", m.name, all_funcs.len()));
}
```

**`compile_to_object_with_imports()`** gets the same two loops.

**`compile_stdlib_all()` airlos vs non-airlos:** the `on_current_target` helper swallows the distinction.

### Helper

```rust
#[inline]
fn on_current_target(m: &StdlibModule) -> bool {
    if cfg!(target_os = "airlos") {
        m.on_airlos
    } else {
        true  // all modules are available on non-airlos
    }
}
```

The `true` branch is correct because currently no module is non-airlos-only; if that ever changes, add a symmetric `on_non_airlos` flag. Not worth preemptively designing for.

## Components

### 1. New types (in `pipeline.rs`)

- `StdlibModule` struct (pub so tests can construct synthetic registries if useful later)
- `STDLIB_MODULES` const array (pub — same rationale)
- `on_current_target` helper (private)

### 2. Modified sites (in `pipeline.rs`)

Each becomes a one-liner iterator over `STDLIB_MODULES`:

- `STDLIB_PATHS` → `stdlib_paths()` returning `Vec<&'static str>`, OR a `const fn` if feasible, OR a `LazyLock<Vec>` if not.
- `stdlib_embed_hash()` rewritten as iterator loop.
- `compile_stdlib_all()` rewritten as iterator loop with cfg-less filtering.
- `compile_to_object()` — two iterator loops (pure-AIRL, extern-C).
- `compile_to_object_with_imports()` — two iterator loops (same shape).

### 3. Constant declarations

All `*_SOURCE` constants stay. `SQLITE_SOURCE` gets an empty-string stub on airlos per Option B above. Add `JSON_SOURCE` to the list.

## Data flow

Before:
```
Developer adds a module
   ↓
Add *_SOURCE const
   ↓
Add to STDLIB_PATHS
   ↓
Add to stdlib_embed_hash()
   ↓
Add to compile_stdlib_all() airlos list
   ↓
Add to compile_stdlib_all() non-airlos list
   ↓
Add to compile_to_object() pure-AIRL list
   ↓
Add to compile_to_object() extern-C list if needed
   ↓
Add to compile_to_object_with_imports() pure-AIRL list
   ↓
Add to compile_to_object_with_imports() extern-C list if needed
```

After:
```
Developer adds a module
   ↓
Add *_SOURCE const
   ↓
Add one StdlibModule entry to STDLIB_MODULES
   ↓
Done.
```

## Testing

### Existence/regression

The consolidation is a pure refactor. Every existing test must continue to pass:

```bash
cargo test -p airl-syntax -p airl-types -p airl-contracts -p airl-runtime -p airl-agent -p airl-driver
rm -rf tests/aot/cache && bash tests/aot/run_aot_tests.sh
cargo run --release --features aot -- compile tests/aot/round3_builtin_json_full.airl -o /tmp/json_smoke && /tmp/json_smoke
```

Expected: Rust suite all green, AOT suite 68/68, JSON smoke test prints the standard output.

### Registry invariants (new unit tests)

Add tests in `pipeline.rs`:

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

These tests are cheap and will catch future regressions where someone adds a module with a typo'd name or forgets a cfg.

### Stability tests (critical)

`stdlib_embed_hash()` returns a stable value across refactor commits. Capture the pre-refactor hash (on main), apply the refactor, compute the post-refactor hash, assert equality. This is the acceptance test for "pure refactor":

```bash
# Before refactor
cargo run -p airl-driver -- check -- '(+ 1 1)' 2>&1 | grep "stdlib embed hash" || true
# Or dump via a temporary fn call in a test — see below
```

A one-shot test in the refactor PR:

```rust
#[test]
fn stdlib_embed_hash_is_stable() {
    // Hardcoded expected value; captured on the commit prior to refactor.
    // MUST match the value produced by the refactored implementation.
    let expected: u64 = <insert pre-refactor value>;
    assert_eq!(stdlib_embed_hash(), expected,
        "refactor changed the stdlib embed hash — cache invalidation will fire");
}
```

If the hash changes, either the source embedding changed (which it shouldn't during a pure refactor) or the iteration order changed. Both are bugs the refactor must fix.

## Files modified

| File | Change |
|------|--------|
| `crates/airl-driver/src/pipeline.rs` | Add `StdlibModule` struct + `STDLIB_MODULES` const + `on_current_target` helper. Rewrite `STDLIB_PATHS`, `stdlib_embed_hash`, `compile_stdlib_all`, `compile_to_object`, `compile_to_object_with_imports` as iterator loops. Stub `SQLITE_SOURCE` on airlos. |
| (none) | No changes to G3 (`bootstrap/g3_compiler.airl`), to `stdlib/*.airl`, to other crates. |

Exactly one file touched.

## Invariants Preserved

- Rust test suite: all currently-passing tests continue to pass.
- AOT test suite: 68/68 unchanged.
- `stdlib_embed_hash()`: produces the same value as before (stability test).
- Compilation order of stdlib modules: preserved by `STDLIB_MODULES` array order matching the pre-refactor sequence.
- Module prefix names used for symbol mangling: unchanged (`"collections"`, `"math"`, etc.).
- Extern-C compilation path for `io.airl` and `sqlite.airl`: preserved via `has_extern_c` flag.
- airlos vs non-airlos behavior: preserved via `on_airlos` flag + `on_current_target` helper.
- Public API of `airl-driver`: unchanged. Callers see the same function signatures.
- `include_str!` timing: all sources still embedded at Rust compile time. No runtime filesystem access added.

## Risks

- **Iteration-order regression.** If someone rearranges `STDLIB_MODULES`, modules that depend on earlier ones (e.g. `string.airl` depending on `prelude.airl`'s `map`) could fail to resolve names. Mitigation: the array order at refactor-time is frozen to match the pre-refactor sequence; a comment notes this dependency.
- **`on_airlos` flag confusion.** A developer could add a new module and forget the flag. Mitigation: default the field to `true` in code review checklist, or make it non-defaultable (no `Default` impl) so every new entry requires explicit flagging.
- **Airlos `SQLITE_SOURCE = ""`**: if someone accidentally compiles the empty stub on airlos (e.g. because they add a new site that forgets to filter by `on_current_target`), the parser would see an empty source and silently produce no functions. Mitigation: `stdlib_registry_sources_are_non_empty_on_target` test catches this at CI time.
- **Parallel reviewers reviewing two PRs simultaneously.** If the consolidation PR and a "add new stdlib module" PR conflict, the new-module PR has to rebase on top. Acceptable: rebase is a single-line addition to `STDLIB_MODULES` rather than six coordinated edits.

## Out of Scope / Future Work

- **AIRL/Rust builtin interface parity audit** is a separate spec (`2026-04-23-airl-rust-builtin-parity-audit.md`) addressing a different class of drift.
- **Dynamic stdlib discovery.** At some point the registry may want to auto-include every `stdlib/*.airl` file. Not today — the `include_str!` approach requires compile-time knowledge of filenames, and adding implicit discovery would change build-reproducibility guarantees.
- **Integration with the `import` mechanism.** `import` resolution is a separate subsystem and not touched here.
- **Extracting the registry to its own file.** Once `STDLIB_MODULES` exists, future work could move it (and the `*_SOURCE` constants) to `crates/airl-driver/src/stdlib_registry.rs`. Deferred — not worth the file churn until a second caller emerges.
