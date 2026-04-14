# Z3 Proof Caching — Design Spec

**Date:** 2026-04-14
**Status:** Draft
**Scope:** Cache Z3 verification results so unchanged functions are not re-verified on every compile, and persist the cache to disk for cross-session reuse.

## Background

Z3 creates a fresh `Solver` instance per function per `verify_function()` call (`crates/airl-solver/src/prover.rs` line 66). There is no memoization or result caching. Every `airl run`, `airl check`, or `airl compile` re-runs Z3 on every function with contracts, even if neither the function nor its contracts changed.

For projects with many contracted functions, this becomes a meaningful compile-time cost. Z3's C library is serialized behind a global mutex (`Z3_LOCK`, line 10) due to thread-safety limitations, so verification is sequential.

## Design

### In-Memory ProofCache (Phase 2B prerequisite)

The `ProofCache` type from the Phase 2B design (`2026-04-14-z3-phase2-enforcement-design.md`) stores results for the current compilation. This spec extends it with disk persistence and change detection.

### Content-Addressed Cache Key

Each function's verification result is keyed by a hash of its verification-relevant content:

```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn cache_key(def: &FnDef) -> u64 {
    let mut h = DefaultHasher::new();
    // Hash function name
    def.name.hash(&mut h);
    // Hash parameter names and types (affects Z3 variable declarations)
    for p in &def.params {
        p.name.hash(&mut h);
        format!("{:?}", p.ty.kind).hash(&mut h);
    }
    // Hash return type
    format!("{:?}", def.return_type.kind).hash(&mut h);
    // Hash contract clause source text
    for r in &def.requires { r.to_airl().hash(&mut h); }
    for e in &def.ensures { e.to_airl().hash(&mut h); }
    for i in &def.invariants { i.to_airl().hash(&mut h); }
    // Hash body (affects result binding)
    def.body.to_airl().hash(&mut h);
    h.finish()
}
```

If the hash matches a cached entry, Z3 is skipped and the cached result is returned.

### Disk Persistence

Cache file: `.airl-z3-cache` in the project root (next to `Cargo.toml`), or in `$AIRL_CACHE_DIR` if set.

Format: newline-delimited JSON (one entry per line for easy append/truncation):

```json
{"key":12345678901234,"fn":"add","clauses":[{"text":"(= result (+ a b))","kind":"ensures","result":"Proven"}]}
{"key":98765432109876,"fn":"clamp","clauses":[{"text":"(>= result lo)","kind":"ensures","result":"Proven"},{"text":"(<= result hi)","kind":"ensures","result":"Proven"}]}
```

### Cache Lifecycle

1. **Load:** At pipeline start, read `.airl-z3-cache` into a `HashMap<u64, Vec<CachedClause>>`
2. **Hit:** For each function, compute `cache_key()`. If present in cache, return stored results without calling Z3.
3. **Miss:** Call Z3, store results in both the in-memory `ProofCache` and the pending-write buffer.
4. **Write:** At pipeline end (after all verification), write updated cache to disk. Entries for functions no longer present in source are evicted.

### Cache Invalidation

The content-addressed key ensures automatic invalidation:
- Change a contract clause → different hash → cache miss → re-verify
- Change the function body → different hash → cache miss → re-verify
- Change a parameter type → different hash → cache miss → re-verify
- Change only a comment or whitespace → `to_airl()` is AST-based, not text-based → hash unchanged → cache hit

### `--no-z3-cache` Flag

Add a CLI flag to bypass the cache entirely (useful for debugging Z3 behavior).

## Changes Required

### 1. Add CacheKey computation

**File:** `crates/airl-solver/src/lib.rs`

```rust
pub fn cache_key(def: &airl_syntax::ast::FnDef) -> u64 { ... }

pub struct CachedResult {
    pub key: u64,
    pub function_name: String,
    pub ensures_results: Vec<(String, VerifyResult)>,
    pub invariants_results: Vec<(String, VerifyResult)>,
}
```

### 2. Add disk read/write

**File:** `crates/airl-solver/src/cache.rs` (new file)

```rust
pub struct DiskCache {
    entries: HashMap<u64, CachedResult>,
    dirty: bool,
}

impl DiskCache {
    pub fn load(path: &Path) -> Self { ... }
    pub fn get(&self, key: u64) -> Option<&CachedResult> { ... }
    pub fn insert(&mut self, result: CachedResult) { ... }
    pub fn write(&self, path: &Path) -> io::Result<()> { ... }
}
```

### 3. Integrate into pipeline

**File:** `crates/airl-driver/src/pipeline.rs`

```rust
let mut disk_cache = airl_solver::cache::DiskCache::load(&cache_path);

for top in &tops {
    if let TopLevel::Defn(f) = top {
        let key = airl_solver::cache_key(f);
        let verification = if let Some(cached) = disk_cache.get(key) {
            cached.to_function_verification()
        } else {
            let v = z3_prover.verify_function(f);
            disk_cache.insert(CachedResult::from(&v, key));
            v
        };
        // ... existing reporting logic ...
    }
}

disk_cache.write(&cache_path)?;
```

## Files Modified

| File | Change |
|------|--------|
| `crates/airl-solver/src/lib.rs` | Add `cache_key()`, `CachedResult` |
| `crates/airl-solver/src/cache.rs` | New file — `DiskCache` with NDJSON read/write |
| `crates/airl-driver/src/pipeline.rs` | Load cache, check before Z3 call, write on exit |
| `crates/airl-driver/src/main.rs` | Add `--no-z3-cache` CLI flag |

## Testing

1. **Cache hit test:** Verify function twice — second call should not invoke Z3 (mock or count calls)
2. **Invalidation test:** Change a contract clause, verify the function is re-checked
3. **Disk persistence test:** Write cache, reload, verify cache hit
4. **Eviction test:** Remove a function from source, verify its cache entry is evicted

## Constraints

- Zero external dependencies (NDJSON is hand-written, not a crate)
- Cache format must be forward-compatible (unknown fields ignored on load)
- Serialization of `VerifyResult::Disproven` includes counterexample values
- `.airl-z3-cache` should be in `.gitignore`
