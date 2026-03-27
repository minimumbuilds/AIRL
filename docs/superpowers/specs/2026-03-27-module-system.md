# AIRL Module System — Design Specification

**Version:** 1.0 draft
**Status:** Approved for implementation
**Branch:** `feat/module-system` (worktree: `.worktrees/module-system/`)

## Overview

Add a module system to AIRL with hierarchical namespaces, pub/private visibility, file-path based resolution, and circular dependency rejection. Whole-program compilation (no separate `.o` files in v1).

## Syntax

### Import

```clojure
;; Import with hierarchical access — prefix is filename stem
(import "lib/math.airl")
(math.abs -5)            ;; → 5
(math.min 3 7)           ;; → 3

;; Import with alias
(import "lib/math.airl" :as m)
(m.abs -5)               ;; → 5

;; Selective import — no prefix needed
(import "lib/math.airl" :only [abs min max])
(abs -5)                 ;; → 5

;; Multiple imports
(import "lib/math.airl")
(import "lib/string-utils.airl" :as su)
(import "lib/result.airl" :only [unwrap-or])
```

`import` is a top-level form (same level as `defn`, `deftype`). Not valid inside expressions.

### Export

`:pub` annotation on `defn` and `deftype`. Private by default.

```clojure
;; Public — visible to importers
(defn abs :pub
  :sig [(x : i64) -> i64]
  :requires [(valid x)]
  :ensures [(>= result 0)]
  :body (if (< x 0) (- 0 x) x))

;; Private — only visible within this module (default)
(defn clamp-helper
  :sig [(x : i64) -> i64]
  :requires [(valid x)]
  :ensures [(valid result)]
  :body (* x 2))

;; Public type
(deftype Color :pub
  :body (Sum (Red []) (Green []) (Blue [])))
```

### Path Resolution

Paths are relative to the importing file's directory:

```
project/
  app.airl              ;; (import "lib/math.airl")
  lib/
    math.airl           ;; (import "utils.airl")  → resolves to lib/utils.airl
    utils.airl
```

- No absolute paths (sandbox constraint)
- No `..` traversal (sandbox constraint)
- `.airl` extension required in import path
- Module name (for prefix) is the filename stem: `"lib/math.airl"` → `math`

### Circular Dependency Rejection

Detected at import resolution time. If A imports B and B imports A (directly or transitively), compilation fails with:

```
error: circular dependency detected
  app.airl imports lib/math.airl
  lib/math.airl imports ../app.airl
```

The resolver tracks the import stack and checks before loading each new file.

## Semantics

### Name Resolution Order

When resolving a symbol `foo`:
1. Local scope (let bindings, function parameters)
2. Module scope (defns in the current file)
3. Imported names (from `:only` imports, no prefix)
4. Stdlib (prelude, math, result, string, map, set — always available, no import needed)

When resolving a qualified symbol `math.foo`:
1. Find the module imported as `math` (or with that filename stem)
2. Look up `foo` in that module's public exports
3. Error if not found or private

### Visibility Rules

- `:pub` functions/types are visible to importers
- Non-`:pub` functions/types are module-private
- Stdlib functions are always public (no change)
- Top-level expressions (non-defn) in an imported module are NOT executed — only defns are imported. The importing module's own top-level expressions run as `__main__`.

### Backward Compatibility

- Files without `import` work exactly as before (all defns visible, no namespacing)
- Existing stdlib remains auto-loaded with no import needed
- The `:pub` annotation is optional — without it, defns are private, but files without any imports don't enforce visibility (everything is effectively public in single-file mode)

## Implementation Plan

### Phase 1: Parser (AST changes)

**Files:** `crates/airl-syntax/src/parser.rs`, `crates/airl-syntax/src/ast.rs`

1. Add `Import` variant to `TopLevel` enum:
   ```rust
   Import {
       path: String,           // "lib/math.airl"
       alias: Option<String>,  // :as name
       only: Option<Vec<String>>, // :only [names]
       span: Span,
   }
   ```

2. Add `:pub` support to `FnDef` and `TypeDef` AST nodes:
   ```rust
   pub is_public: bool,  // true if :pub present
   ```

3. Parse `(import ...)` as a top-level form
4. Parse `:pub` keyword in defn/deftype

### Phase 2: Import Resolver

**Files:** New `crates/airl-driver/src/resolver.rs`

1. `resolve_imports(entry_path: &str) -> Result<Vec<ResolvedModule>, Error>`
2. Reads the entry file, finds all `import` statements
3. Recursively resolves imported files
4. Detects circular dependencies via import stack
5. Returns modules in dependency order (leaves first)
6. Each `ResolvedModule` has: path, source, AST, public symbol list

### Phase 3: Compilation Pipeline Changes

**Files:** `crates/airl-driver/src/pipeline.rs`, `crates/airl-runtime/src/bytecode_compiler.rs`

1. `compile_to_object` takes resolved module list instead of flat file list
2. Each module compiled as a separate bytecode unit with its own `__main__` filtered out
3. Qualified name resolution: `math.abs` → look up `abs` in math module's function table
4. Visibility enforcement: error if accessing non-`:pub` symbol from another module
5. Final linking combines all modules' BCFuncs into one list (same as today)

### Phase 4: G3 Bootstrap Compiler

**Files:** `bootstrap/g3_compiler.airl`, `bootstrap/parser.airl`

1. Bootstrap parser: add `(import ...)` parsing → `ASTImport` variant
2. Bootstrap parser: add `:pub` parsing in `parse-defn`
3. G3 compiler: resolve imports, compile in dependency order
4. Same whole-program linking as today

### Phase 5: Type Checker Integration

**Files:** `crates/airl-types/src/checker.rs`

1. Track which module each symbol belongs to
2. Validate qualified references (`math.abs` exists and is `:pub`)
3. Error on private symbol access across modules

## Testing Strategy

1. **Parser tests:** `(import "foo.airl")`, `(import "foo.airl" :as f)`, `(import "foo.airl" :only [a b])`
2. **Resolver tests:** linear deps, diamond deps, circular rejection
3. **Visibility tests:** public access works, private access errors
4. **Qualified name tests:** `math.abs` resolves, `math.helper` rejects (private)
5. **Integration tests:** multi-file project compiles and runs correctly
6. **Backward compat tests:** existing single-file programs unchanged

## Not in v1

- Separate compilation / incremental builds
- Re-exports (`(import "a.airl" :re-export [foo])`)
- Wildcard imports (`(import "math.airl" :all)`)
- Package manager / dependency resolution beyond local files
- Module-level constants or state
- Conditional imports
